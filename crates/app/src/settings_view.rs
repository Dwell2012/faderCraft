use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme, Disableable, Icon, Selectable, Sizable,
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState},
    menu::{DropdownMenu, PopupMenuItem},
    slider::{Slider, SliderEvent, SliderState},
    switch::Switch,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use djcraft_library::Placement;
use djcraft_library::lookup::Provider;

use crate::settings::{self, Settings, StartTab, StemModel, ThemeChoice};

actions!(settings, [CloseSettings]);

/// The key context in which Escape closes the settings.
pub const KEY_CONTEXT: &str = "Settings";

pub enum SettingsEvent {
    /// A setting changed; the new values should be applied and saved.
    Changed(Settings),
    MappingSelected(Settings),
    Closed,
}

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Crossfader,
    Preferences,
    Audio,
    Controllers,
    Library,
    Assistant,
    Plugins,
    Marketplace,
    About,
}

impl Page {
    /// Pages grouped as they appear in the sidebar.
    const GROUPS: [(&'static str, &'static [Page]); 3] = [
        (
            "General",
            &[
                Page::Preferences,
                Page::Audio,
                Page::Crossfader,
                Page::Controllers,
                Page::Library,
                Page::Assistant,
            ],
        ),
        ("Extensions", &[Page::Plugins, Page::Marketplace]),
        ("App", &[Page::About]),
    ];

    fn icon(self) -> IconName {
        match self {
            Page::Crossfader => IconName::SlidersHorizontal,
            Page::Preferences => IconName::SlidersHorizontal,
            Page::Audio => IconName::Volume2,
            Page::Controllers => IconName::SlidersHorizontal,
            Page::Library => IconName::Library,
            Page::Assistant => IconName::Sparkles,
            Page::Plugins => IconName::Puzzle,
            Page::Marketplace => IconName::Store,
            Page::About => IconName::Info,
        }
    }

    fn title(self) -> &'static str {
        match self {
            Page::Crossfader => "Mixer",
            Page::Preferences => "Preferences",
            Page::Audio => "Audio",
            Page::Controllers => "Controllers",
            Page::Library => "Library",
            Page::Assistant => "Assistant",
            Page::Plugins => "Plugins",
            Page::Marketplace => "Marketplace",
            Page::About => "About",
        }
    }

    fn summary(self) -> &'static str {
        match self {
            Page::Crossfader => "Choose the fade curve and assign decks to each side.",
            Page::Preferences => "How djcraft looks and starts on this computer.",
            Page::Audio => "Where the mix is played.",
            Page::Controllers => "Choose the mapping for your MIDI controller.",
            Page::Library => "How your music library is kept up to date.",
            Page::Assistant => "A helper for organising music and preparing sets.",
            Page::Plugins => "Extensions installed on this computer.",
            Page::Marketplace => "Where plugins are found and installed from.",
            Page::About => "This copy of djcraft.",
        }
    }
}

/// The settings dialog, drawn over the whole window.
pub struct SettingsView {
    crossfader_slider: Entity<SliderState>,
    settings: Settings,
    page: Page,
    /// A description of the audio output in use.
    audio: SharedString,
    focus: FocusHandle,
    /// Text boxes for the AcoustID key and the Discogs token, made on first
    /// draw, as making them needs the window.
    key_inputs: Option<[Entity<InputState>; 2]>,
    library_notice: Option<String>,
    controller_notice: String,
    controller_present: bool,
    model_download: bool,
    model_notice: String,
    models_downloaded: [bool; 2],
}

impl SettingsView {
    fn curve_slider(sharpness: u8, cx: &mut Context<Self>) -> Entity<SliderState> {
        let slider = cx.new(|_| {
            SliderState::new()
                .min(0.)
                .max(100.)
                .step(1.)
                .default_value(sharpness as f32)
        });
        cx.subscribe(&slider, |this, _, event: &SliderEvent, cx| {
            if let SliderEvent::Change(value) = event {
                let sharpness = value.start().round() as u8;
                this.change_crossfader(|amount, _, _, _| *amount = sharpness, cx);
            }
        })
        .detach();
        slider
    }

    pub fn sync_crossfader(
        &mut self,
        curve: djcraft_protocol::CrossfaderCurve,
        routing: djcraft_protocol::CrossfaderRouting,
        cx: &mut Context<Self>,
    ) {
        if self.settings.crossfader_curve.parameters().0 != curve.parameters().0 {
            self.crossfader_slider = Self::curve_slider(curve.parameters().0, cx);
        }
        self.settings.crossfader_curve = curve;
        self.settings.crossfader_routing = routing;
        cx.notify();
    }

    pub fn new(
        settings: Settings,
        audio: SharedString,
        controller_present: bool,
        cx: &mut Context<Self>,
    ) -> Self {
        let crossfader_slider = Self::curve_slider(settings.crossfader_curve.parameters().0, cx);
        Self {
            crossfader_slider,
            settings,
            page: Page::Preferences,
            audio,
            focus: cx.focus_handle(),
            key_inputs: None,
            library_notice: None,
            controller_notice: String::new(),
            controller_present,
            model_download: false,
            model_notice: String::new(),
            models_downloaded: StemModel::ALL.map(|m| crate::stems::model_downloaded(m.name())),
        }
    }

    pub fn set_controller_present(&mut self, present: bool, cx: &mut Context<Self>) {
        if self.controller_present != present {
            self.controller_present = present;
            cx.notify();
        }
    }

    pub fn controller_notice(&mut self, message: String, cx: &mut Context<Self>) {
        self.controller_notice = message;
        cx.notify();
    }

    fn select_mapping(&mut self, selection: String, cx: &mut Context<Self>) {
        if selection != "@disabled" {
            if let Err(error) = crate::controllers::read_mapping(&selection) {
                self.controller_notice(error, cx);
                return;
            }
        }
        self.controller_notice = "Mapping selected.".into();
        self.settings.controller_mapping = selection;
        cx.emit(SettingsEvent::MappingSelected(self.settings.clone()));
        cx.notify();
    }

    fn choose_mapping(&mut self, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose a controller mapping JSON file".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            if let Some(path) = paths.first() {
                let selection = path.to_string_lossy().into_owned();
                let _ = this.update(cx, |this, cx| this.select_mapping(selection, cx));
            }
        })
        .detach();
    }

    fn download_stem_model(&mut self, model: StemModel, cx: &mut Context<Self>) {
        if self.model_download {
            return;
        }
        self.model_download = true;
        self.model_notice = format!("Downloading {} model…", model.label());
        cx.notify();
        let work = cx
            .background_executor()
            .spawn(async move { crate::stems::download_model(model.name()) });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                this.model_download = false;
                this.model_notice = match result {
                    Ok(()) => format!("{} model ready", model.label()),
                    Err(error) => error,
                };
                this.models_downloaded =
                    StemModel::ALL.map(|m| crate::stems::model_downloaded(m.name()));
                cx.notify();
            });
        })
        .detach();
    }

    fn model_download_button(&self, model: StemModel, index: usize, cx: &Context<Self>) -> Button {
        Button::new(("download-stem-model", index))
            .outline()
            .w(px(200.))
            .label(if self.models_downloaded[index] {
                "Downloaded"
            } else {
                "Download"
            })
            .disabled(self.model_download || self.models_downloaded[index])
            .on_click(cx.listener(move |this, _, _, cx| this.download_stem_model(model, cx)))
    }

    /// Makes the key and token text boxes, filled in from the settings, and
    /// saves what is typed into them.
    fn make_key_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.key_inputs.is_some() {
            return;
        }
        let values = [
            self.settings.acoustid_key.clone(),
            self.settings.discogs_token.clone(),
        ];
        let inputs = values.map(|value| {
            cx.new(|cx| {
                let mut input = InputState::new(window, cx).placeholder("Not set");
                input.set_value(value, window, cx);
                input
            })
        });
        for (slot, input) in inputs.iter().enumerate() {
            cx.subscribe(input, move |this, input, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    let value = input.read(cx).value().trim().to_string();
                    this.change(
                        |settings| match slot {
                            0 => settings.acoustid_key = value,
                            _ => settings.discogs_token = value,
                        },
                        cx,
                    );
                }
            })
            .detach();
        }
        self.key_inputs = Some(inputs);
    }

    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus
    }

    fn change(&mut self, change: impl FnOnce(&mut Settings), cx: &mut Context<Self>) {
        change(&mut self.settings);
        self.models_downloaded = StemModel::ALL.map(|m| crate::stems::model_downloaded(m.name()));
        cx.emit(SettingsEvent::Changed(self.settings.clone()));
        cx.notify();
    }

    fn choose_library(&mut self, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Choose a faderCraft library folder (local or OneDrive)".into()),
        });
        cx.spawn(async move |this,cx| {
            let Ok(Ok(Some(paths)))=paths.await else{return;};
            let Some(path)=paths.into_iter().next() else{return;};
            this.update(cx,|this,cx| {
                this.library_notice=Some(match crate::storage::choose_folder(&path) {
                    Ok(())=>"Library folder selected. Close and restart faderCraft to use it. Let OneDrive finish syncing before switching computers.".into(),
                    Err(error)=>format!("Could not select library folder: {error}"),
                });
                cx.notify();
            }).ok();
        }).detach();
    }

    fn sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (hover, muted) = (theme.secondary, theme.muted_foreground);
        div()
            .flex()
            .flex_col()
            .w(px(240.))
            .gap_4()
            .p_3()
            .border_r_1()
            .border_color(theme.border)
            .children(Page::GROUPS.map(|(group, pages)| {
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().px_2().pb_1().text_xs().text_color(muted).child(group))
                    .children(pages.iter().map(|&page| {
                        div()
                            .id(page.title())
                            .flex()
                            .items_center()
                            .gap_2()
                            .h(px(34.))
                            .px_2()
                            .rounded_md()
                            .text_sm()
                            .when(self.page == page, |this| {
                                this.bg(hover).font_weight(FontWeight::MEDIUM)
                            })
                            .hover(|style| style.bg(hover))
                            .child(Icon::new(page.icon()).small())
                            .child(page.title())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.page = page;
                                cx.notify();
                            }))
                    }))
            }))
    }

    /// A titled group of rows.
    fn section(&self, title: &'static str, rows: Vec<AnyElement>, cx: &Context<Self>) -> Div {
        let border = cx.theme().border;
        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .pb_2()
                    .border_b_1()
                    .border_color(border)
                    .font_weight(FontWeight::BOLD)
                    .child(title),
            )
            .children(rows)
    }

    /// One setting: its name and explanation on the left, its control on
    /// the right.
    fn row(
        &self,
        name: &'static str,
        detail: impl Into<SharedString>,
        control: impl IntoElement,
        cx: &Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        div()
            .flex()
            .items_center()
            .gap_4()
            .py_3()
            .border_b_1()
            .border_color(theme.border.opacity(0.5))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(name))
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child(detail.into()),
                    ),
            )
            .child(control)
            .into_any_element()
    }

    /// A dropdown showing `current`, offering every choice in `choices`.
    fn dropdown<T: Copy + PartialEq + 'static>(
        &self,
        id: &'static str,
        current: T,
        choices: &'static [T],
        label: fn(T) -> &'static str,
        set: fn(&mut Settings, T),
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<T> {
        let view = cx.weak_entity();
        Button::new(id)
            .outline()
            .w(px(200.))
            .label(label(current))
            .dropdown_caret(true)
            .dropdown_menu(move |mut menu, _, _| {
                for &choice in choices {
                    let view = view.clone();
                    menu = menu.item(
                        PopupMenuItem::new(label(choice))
                            .checked(choice == current)
                            .on_click(move |_, _, cx| {
                                view.update(cx, |view, cx| {
                                    view.change(|settings| set(settings, choice), cx)
                                })
                                .ok();
                            }),
                    );
                }
                menu
            })
    }

    fn info(&self, text: impl Into<SharedString>, cx: &Context<Self>) -> Div {
        div()
            .max_w(px(360.))
            .text_sm()
            .text_color(cx.theme().muted_foreground)
            .child(text.into())
    }

    fn change_crossfader(
        &mut self,
        edit: impl FnOnce(&mut u8, &mut bool, &mut bool, &mut bool),
        cx: &mut Context<Self>,
    ) {
        let (mut sharpness, mut power, mut reverse, mut disabled) =
            self.settings.crossfader_curve.parameters();
        edit(&mut sharpness, &mut power, &mut reverse, &mut disabled);
        self.change(
            |settings| {
                settings.crossfader_curve = djcraft_protocol::CrossfaderCurve::Adjustable {
                    sharpness,
                    power,
                    reverse,
                    disabled,
                }
            },
            cx,
        );
    }

    fn page(&self, cx: &mut Context<Self>) -> Vec<Div> {
        match self.page {
            Page::Crossfader => {
                let (_, power, reverse, disabled) = self.settings.crossfader_curve.parameters();
                let curve = div().w(px(240.)).flex().flex_col().gap_2()
                    .child(Slider::new(&self.crossfader_slider).horizontal())
                    .child(div().flex().justify_between().text_xs().child("SLOW").child("FAST"));
                let types = div().flex().gap_2().children([("Linear", false), ("Power", true)].into_iter().enumerate().map(|(i, (label, value))| {
                    Button::new(("crossfader-type", i)).outline().label(label).selected(power == value)
                        .on_click(cx.listener(move |this, _, _, cx| this.change_crossfader(|_, power, _, _| *power = value, cx)))
                }));
                let reverse = Switch::new("crossfader-reverse").checked(reverse)
                    .on_click(cx.listener(|this, checked: &bool, _, cx| this.change_crossfader(|_, _, reverse, _| *reverse = *checked, cx)));
                let disabled = Switch::new("crossfader-disabled").checked(disabled)
                    .on_click(cx.listener(|this, checked: &bool, _, cx| this.change_crossfader(|_, _, _, disabled| *disabled = *checked, cx)));
                let rows = vec![
                    self.row("Curve", "Slow gives a gradual blend. Fast gives a tight cut for scratching.", curve, cx),
                    self.row("Curve type", "Linear fades evenly. Power gives a more pronounced fade before opening fully.", types, cx),
                    self.row("Reverse", "Swap the crossfader direction without changing deck assignments.", reverse, cx),
                    self.row("Disable", "Bypass the crossfader and mix using channel volume faders.", disabled, cx),
                ];
                let mut assignments = Vec::new();
                for (deck, name) in ["Deck A", "Deck B", "Deck C", "Deck D"].into_iter().enumerate() {
                    let bit = 1 << deck;
                    let routing = self.settings.crossfader_routing;
                    let controls = div().flex().gap_2().children(
                        [("Left", routing.left & bit != 0), ("Right", routing.right & bit != 0)]
                        .into_iter().enumerate().map(|(side, (label, selected))| {
                            Button::new(("crossfader-assignment", deck * 2 + side))
                                .outline().label(label).selected(selected)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.change(|settings| {
                                        let mask = if side == 0 { &mut settings.crossfader_routing.left } else { &mut settings.crossfader_routing.right };
                                        *mask ^= bit;
                                    }, cx);
                                }))
                        })
                    );
                    assignments.push(self.row(name, "Neither side bypasses the crossfader. Both sides keep the deck audible throughout the fade.", controls, cx));
                }
                vec![self.section("Crossfader", rows, cx), self.section("Deck assignments", assignments, cx)]
            }
            Page::Preferences => {
                let theme = self.dropdown(
                    "theme",
                    self.settings.theme,
                    &ThemeChoice::ALL,
                    ThemeChoice::label,
                    |settings, theme| settings.theme = theme,
                    cx,
                );
                let start_tab = self.dropdown(
                    "start-tab",
                    self.settings.start_tab,
                    &StartTab::ALL,
                    StartTab::label,
                    |settings, tab| settings.start_tab = tab,
                    cx,
                );
                vec![
                    self.section(
                        "Appearance",
                        vec![self.row(
                            "Theme",
                            "Follow the system, or always use one look.",
                            theme,
                            cx,
                        )],
                        cx,
                    ),
                    self.section(
                        "Start-up",
                        vec![self.row(
                            "Open on",
                            "The tab shown when djcraft starts.",
                            start_tab,
                            cx,
                        )],
                        cx,
                    ),
                ]
            }
            Page::Controllers => {
                let profile = crate::controllers::read_mapping(&self.settings.controller_mapping).ok();
                let mixtrack = profile.as_ref().is_some_and(crate::controllers::is_mixtrack_profile);
                let fx1_colour = profile.as_ref().is_some_and(|profile| self.settings.controller_options_for(profile).fx1_colour);
                let fx1_key = profile.as_ref().map(crate::controllers::controller_settings_key).unwrap_or_default();
                let fx1_view = cx.weak_entity();
                let fx1 = Button::new("controller-fx1-target").outline().w(px(200.))
                    .label(if fx1_colour { "Selected CFX" } else { "Deck Effects" })
                    .dropdown_caret(true).dropdown_menu(move |mut menu, _, _| {
                        for colour in [false, true] {
                            let view = fx1_view.clone();
                            let key = fx1_key.clone();
                            menu = menu.item(PopupMenuItem::new(if colour { "Selected CFX" } else { "Deck Effects" })
                                .checked(colour == fx1_colour).on_click(move |_, _, cx| {
                                    view.update(cx, |view, cx| view.change(|settings| {
                                        settings.controller_options.entry(key.clone()).or_default().fx1_colour = colour;
                                    }, cx)).ok();
                                }));
                        }
                        menu
                    });
                let current = displayed_mapping(&self.settings.controller_mapping, self.controller_present).to_owned();
                let choices = crate::controllers::mapping_choices();
                let label = choices.iter().find(|(key, _)| key == &current).map(|(_, name)| name.clone())
                    .unwrap_or_else(|| current.clone());
                let view = cx.weak_entity();
                let mapping = Button::new("controller-mapping").outline().label(label)
                    .dropdown_caret(true).dropdown_menu(move |mut menu, _, _| {
                        let mut choices = crate::controllers::mapping_choices();
                        if !choices.iter().any(|(key, _)| key == &current) { choices.push((current.clone(), current.clone())); }
                        for (key, label) in choices {
                            let view = view.clone();
                            menu = menu.item(PopupMenuItem::new(label).checked(key == current).on_click(move |_, _, cx| {
                                let _ = view.update(cx, |view, cx| view.select_mapping(key.clone(), cx));
                            }));
                        }
                        menu
                    });
                let import = Button::new("controller-mapping-file").outline().label("Load mapping…")
                    .on_click(cx.listener(|this, _, _, cx| this.choose_mapping(cx)));
                let detail = if !self.controller_present || self.settings.controller_mapping == "@disabled" { "Controller input is disabled.".to_string() }
                    else { match crate::controllers::read_mapping(&self.settings.controller_mapping) {
                        Ok(profile) => {
                            let ports: std::collections::BTreeSet<_> = profile.mappings.iter().map(|m| m.port.as_str()).collect();
                            format!("{} — MIDI ports: {}", profile.name, ports.into_iter().collect::<Vec<_>>().join(", "))
                        }
                        Err(error) => error,
                    }};
                let mut rows = vec![
                    self.row("Mapping", "Choosing a mapping applies it immediately. One matching controller connects automatically.", mapping, cx),
                    self.row("Custom mapping", "Load a faderCraft JSON mapping. The original file is kept unchanged and must remain available.", import, cx),
                    self.info(detail, cx).into_any_element(),
                    self.info(self.controller_notice.clone(), cx).into_any_element(),
                ];
                if mixtrack {
                    rows.push(self.row("FX 1 controls", "Route the MixTrack Pro II's first FX button and encoder to deck effect slot 1 or the deck's selected Colour FX combination.", fx1, cx));
                }
                vec![self.section("Controller mapping", rows, cx)]
            }
            Page::Audio => {
                let backend = self.dropdown("audio-backend", if self.settings.audio_asio { 2u8 } else if self.settings.audio_exclusive { 1 } else { 0 }, &[0, 1, 2],
                    |mode| match mode { 1 => "WASAPI exclusive", 2 => "ASIO (Windows)", _ => "System / WASAPI shared" },
                    |s, mode| { s.audio_asio = mode == 2; s.audio_exclusive = mode == 1; s.audio_device.clear(); s.audio_buffer = 0; }, cx);
                let view = cx.weak_entity();
                let asio = self.settings.audio_asio;
                let current = self.settings.audio_device.clone();
                let device = Button::new("audio-device").outline().w(px(260.))
                    .label(if current.is_empty() { "Driver default".into() } else { current.clone() })
                    .dropdown_caret(true).dropdown_menu(move |mut menu, _, _| {
                        let entries = djcraft_engine::output_devices(asio);
                        let mut names = vec![String::new()];
                        match entries {
                            Ok(devices) => names.extend(devices),
                            Err(error) => return menu.item(PopupMenuItem::new(error.to_string()).disabled(true)),
                        }
                        for name in names {
                            let view = view.clone();
                            menu = menu.item(PopupMenuItem::new(if name.is_empty() { "Driver default".to_string() } else { name.clone() })
                                .checked(name == current).on_click(move |_, _, cx| {
                                    let name = name.clone();
                                    let _ = view.update(cx, |v, cx| v.change(|s| s.audio_device = name, cx));
                                }));
                        }
                        menu
                    });
                let buffer = self.dropdown("audio-buffer", self.settings.audio_buffer, &[0, 64, 128, 256, 512, 1024],
                    |n| match n { 64 => "64 frames", 128 => "128 frames", 256 => "256 frames", 512 => "512 frames", 1024 => "1024 frames", _ => "Driver default" },
                    |s, n| s.audio_buffer = n, cx);
                let master = self.dropdown("master-pair", self.settings.audio_master_pair, &[0, 2, 4, 6],
                    |n| match n { 2 => "3 / 4", 4 => "5 / 6", 6 => "7 / 8", _ => "1 / 2" },
                    |s, n| s.audio_master_pair = n, cx);
                let cue = self.dropdown("cue-pair", self.settings.audio_cue_pair.unwrap_or(u16::MAX), &[u16::MAX, 0, 2, 4, 6],
                    |n| match n { 0 => "1 / 2", 2 => "3 / 4", 4 => "5 / 6", 6 => "7 / 8", _ => "Disabled" },
                    |s, n| s.audio_cue_pair = if n == u16::MAX { None } else { Some(n) }, cx);
                let booth = self.dropdown("booth-pair", self.settings.audio_booth_pair.unwrap_or(u16::MAX), &[u16::MAX, 0, 2, 4, 6],
                    |n| match n { 0 => "1 / 2", 2 => "3 / 4", 4 => "5 / 6", 6 => "7 / 8", _ => "Disabled" },
                    |s, n| s.audio_booth_pair = if n == u16::MAX { None } else { Some(n) }, cx);
                let model = self.dropdown("stem-model", self.settings.stem_model, &StemModel::ALL,
                    StemModel::label, |s, model| s.stem_model = model, cx);
                let fast_download = self.model_download_button(StemModel::Fast, 0, cx);
                let detailed_download = self.model_download_button(StemModel::Detailed, 1, cx);
                let mut stem_rows = vec![
                    self.row("Separation model", "Reload a deck after switching. Each model keeps its own stem cache.", model, cx),
                    self.row("Fast model", "Demucs: quicker four-stem separation.", fast_download, cx),
                    self.row("Detailed model", "Four fine-tuned ONNX specialists (about 1.3 GB total). Slower, potentially cleaner separation.", detailed_download, cx),
                ];
                if !self.model_notice.is_empty() {
                    stem_rows.push(self.row("Download status", "", self.info(self.model_notice.clone(), cx), cx));
                }
                vec![self.section("Output", vec![
                    self.row("Currently running", "New audio selections take effect after restarting faderCraft.", self.info(self.audio.clone(), cx), cx),
                    self.row("Audio backend", "WASAPI exclusive uses the device directly and prevents other apps sharing that output. ASIO uses an installed driver.", backend, cx),
                    self.row("Output device", "Connect your audio interface before opening this menu.", device, cx),
                    self.row("Buffer size", "Smaller buffers reduce latency. Choose Driver default if the driver rejects a fixed size.", buffer, cx),
                    self.row("Master output pair", "Stereo software channels on the selected device. Changes require restart.", master, cx),
                    self.row("Headphone cue pair", "Monitors decks enabled with their headphone buttons, independently of active deck and channel faders. Use a distinct pair on the same multichannel device; ASIO recommended. Exclusive WASAPI is stereo only.", cue, cx),
                    self.row("Booth output pair", "Pre-master stereo mix with its own level. Choose a distinct pair on the same multichannel device; changes require restart.", booth, cx),
                ], cx), self.section("Stem preparation", stem_rows, cx)]
            }
            Page::Library => {
                let scan = Switch::new("scan-on-startup")
                    .checked(self.settings.scan_on_startup)
                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                        let checked = *checked;
                        this.change(|settings| settings.scan_on_startup = checked, cx);
                    }));
                let watch = Switch::new("watch-folders")
                    .checked(self.settings.watch_folders)
                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                        let checked = *checked;
                        this.change(|settings| settings.watch_folders = checked, cx);
                    }));
                let provider = self.dropdown(
                    "info-provider",
                    self.settings.info_provider,
                    &Provider::ALL,
                    Provider::label,
                    |settings, provider| settings.info_provider = provider,
                    cx,
                );
                let lowest = self.dropdown(
                    "tempo-min",
                    self.settings.tempo_min_bpm,
                    &settings::MIN_BPM_CHOICES,
                    settings::bpm_label,
                    |settings, bpm| settings.tempo_min_bpm = bpm,
                    cx,
                );
                let highest = self.dropdown(
                    "tempo-max",
                    self.settings.tempo_max_bpm,
                    &settings::MAX_BPM_CHOICES,
                    settings::bpm_label,
                    |settings, bpm| settings.tempo_max_bpm = bpm,
                    cx,
                );
                let decimals = self.dropdown(
                    "bpm-decimals",
                    self.settings.bpm_decimals,
                    &settings::BPM_DECIMAL_CHOICES,
                    settings::bpm_decimals_label,
                    |settings, decimals| settings.bpm_decimals = decimals,
                    cx,
                );
                let keep_tempo = Switch::new("keep-tagged-tempo")
                    .checked(self.settings.keep_tagged_tempo)
                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                        let checked = *checked;
                        this.change(|settings| settings.keep_tagged_tempo = checked, cx);
                    }));
                let keep_key = Switch::new("keep-tagged-key")
                    .checked(self.settings.keep_tagged_key)
                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                        let checked = *checked;
                        this.change(|settings| settings.keep_tagged_key = checked, cx);
                    }));
                let analysis = self.section(
                    "Analysis",
                    vec![
                        self.row(
                            "Lowest tempo",
                            "Tracks are given a tempo between the lowest and the highest. A beat at 70 BPM is also a beat at 140, and this range decides which one a track gets. If you change it, the whole library is analysed again.",
                            lowest,
                            cx,
                        ),
                        self.row(
                            "Highest tempo",
                            "The top of the tempo range. If you change it, the whole library is analysed again.",
                            highest,
                            cx,
                        ),
                        self.row(
                            "Show tempos with",
                            "Decimal places for tempos in lists and the info panel. The measured tempo and the beat grid stay exact.",
                            decimals,
                            cx,
                        ),
                        self.row(
                            "Keep tempo from tags",
                            "Show the tempo in a file's tags instead of the measured one, if the file has one. The beat grid still comes from the measurement.",
                            keep_tempo,
                            cx,
                        ),
                        self.row(
                            "Keep key from tags",
                            "Show the key in a file's tags instead of the detected one, if the file has one. A key that you type in is always kept.",
                            keep_key,
                            cx,
                        ),
                    ],
                    cx,
                );
                let key_place = self.dropdown(
                    "key-placement",
                    self.settings.key_placement,
                    &Placement::FOR_KEY,
                    Placement::label,
                    |settings, place| settings.key_placement = place,
                    cx,
                );
                let energy_place = self.dropdown(
                    "energy-placement",
                    self.settings.energy_placement,
                    &Placement::FOR_ENERGY,
                    Placement::label,
                    |settings, place| settings.energy_placement = place,
                    cx,
                );
                let tags = self.section(
                    "Tags",
                    vec![
                        self.row(
                            "Write the key to",
                            "Where Tag tools, Write BPM, key and energy puts the key, in Camelot notation such as 8A. Old keys it wrote there are replaced.",
                            key_place,
                            cx,
                        ),
                        self.row(
                            "Write the energy to",
                            "Where the energy level goes, written as Energy 7. Files have no field of their own for it.",
                            energy_place,
                            cx,
                        ),
                    ],
                    cx,
                );
                let key_box = |slot: usize| {
                    div()
                        .w(px(260.))
                        .children(self.key_inputs.as_ref().map(|inputs| Input::new(&inputs[slot])))
                };
                let music_info = self.section(
                    "Music info",
                    vec![
                        self.row(
                            "Look up tracks with",
                            "The online database Look up searches first. MusicBrainz covers more releases; the iTunes Store gives cleaner albums and a genre; Discogs is strong on vinyl and club styles. You can switch in the Look up window.",
                            provider,
                            cx,
                        ),
                        self.row(
                            "AcoustID key",
                            "Identifies a track by its sound when its tags are wrong or missing. Get a free application key at acoustid.org/new-application and paste it here.",
                            key_box(0),
                            cx,
                        ),
                        self.row(
                            "Discogs token",
                            "Lets Look up search Discogs. Make a personal access token at discogs.com/settings/developers and paste it here. It is stored in djcraft's settings file.",
                            key_box(1),
                            cx,
                        ),
                    ],
                    cx,
                );
                vec![self.section(
                    "Folders",
                    vec![
                        self.row(
                            "Scan folders at start-up",
                            "Look for new, changed and removed files each time djcraft opens.",
                            scan,
                            cx,
                        ),
                        self.row(
                            "Watch folders for changes",
                            "Add, update and remove tracks as files change while djcraft is open.",
                            watch,
                            cx,
                        ),
                        self.row(
                            "Library folder",
                            self.library_notice.clone().unwrap_or_else(||format!("{} — artwork, stems, cues and a library snapshot. Changes apply after restart; an existing library is opened, an empty folder receives a copy. Use one computer at a time.",crate::storage::configured_folder().display())),
                            Button::new("choose-library-folder").outline().label("Choose folder…")
                                .on_click(cx.listener(|this,_,_,cx|this.choose_library(cx))),
                            cx,
                        ),
                        self.row("Local settings", "Audio devices, credentials and the stem runtime stay on this computer.",
                            self.info(crate::storage::machine_dir().to_string_lossy().into_owned(),cx),cx),
                    ],
                    cx,
                ), analysis, tags, music_info]
            }
            Page::Assistant => vec![self.section(
                "Assistant",
                vec![self.row(
                    "Not available yet",
                    "The assistant has not been built. Its settings will appear here.",
                    div(),
                    cx,
                )],
                cx,
            )],
            Page::Plugins => vec![self.section(
                "Installed plugins",
                vec![self.row(
                    "No plugins installed",
                    "djcraft cannot run plugins yet. Installed plugins and their permissions will be managed here.",
                    div(),
                    cx,
                )],
                cx,
            )],
            Page::Marketplace => vec![self.section(
                "Stores",
                vec![self.row(
                    "No marketplace connected",
                    "Plugin stores you subscribe to will be listed here once plugins can be installed.",
                    div(),
                    cx,
                )],
                cx,
            )],
            Page::About => vec![self.section(
                "faderCraft",
                vec![
                    self.row(
                        "Version",
                        "The version of this copy.",
                        self.info(env!("CARGO_PKG_VERSION"), cx),
                        cx,
                    ),
                    self.row(
                        "Licence",
                        "djcraft is open-source software.",
                        self.info("MIT", cx),
                        cx,
                    ),
                ],
                cx,
            )],
        }
    }
}

impl EventEmitter<SettingsEvent> for SettingsView {}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.make_key_inputs(window, cx);
        let page = self.page;
        let sections = self.page(cx);
        let sidebar = self.sidebar(cx).into_any_element();
        let theme = cx.theme();
        let (background, border, muted) = (theme.background, theme.border, theme.muted_foreground);

        // The dimmed backdrop; clicking it closes the dialog.
        div()
            .id("settings-backdrop")
            .absolute()
            .inset_0()
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .p_8()
            .bg(gpui_kit::black().opacity(0.6))
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(|_, _: &CloseSettings, _, cx| cx.emit(SettingsEvent::Closed)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _, _, cx| cx.emit(SettingsEvent::Closed)),
            )
            .child(
                div()
                    .id("settings")
                    .flex()
                    .size_full()
                    .max_w(px(980.))
                    .max_h(px(660.))
                    .bg(background)
                    .border_1()
                    .border_color(border)
                    .rounded_xl()
                    .overflow_hidden()
                    .shadow_lg()
                    // Clicks inside the dialog are not clicks on the backdrop.
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(sidebar)
                    .child(
                        div()
                            .id("settings-page")
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .gap_6()
                            .p_8()
                            .overflow_y_scroll()
                            .child(
                                div()
                                    .flex()
                                    .items_start()
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .flex_1()
                                            .gap_1()
                                            .child(
                                                div()
                                                    .text_2xl()
                                                    .font_weight(FontWeight::BOLD)
                                                    .child(page.title()),
                                            )
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .text_color(muted)
                                                    .child(page.summary()),
                                            ),
                                    )
                                    .child(
                                        Button::new("close-settings")
                                            .ghost()
                                            .icon(IconName::X)
                                            .on_click(cx.listener(|_, _, _, cx| {
                                                cx.emit(SettingsEvent::Closed)
                                            })),
                                    ),
                            )
                            .children(sections),
                    ),
            )
    }
}

fn displayed_mapping(saved: &str, controller_present: bool) -> &str {
    if controller_present {
        saved
    } else {
        "@disabled"
    }
}

#[cfg(test)]
mod controller_display_tests {
    #[test]
    fn absent_controller_displays_disabled_without_discarding_mapping() {
        for saved in ["", "@bundled", "custom.json", "@disabled"] {
            assert_eq!(super::displayed_mapping(saved, false), "@disabled");
            assert_eq!(super::displayed_mapping(saved, true), saved);
        }
    }
}
