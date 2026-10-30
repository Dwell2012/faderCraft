mod analysis;
mod attack_cache;
mod controllers;
mod flow_view;
mod library_view;
mod lookup_view;
mod loop_store;
mod organise_view;
mod pad_store;
mod preparation;
mod settings;
mod settings_view;
mod shell;
mod stems;
mod storage;
mod tag_tools;
mod track_identity;
mod waveform;
mod wheel;

use gpui_kit::assets::AllAssets;
use gpui_kit::*;

actions!(
    djcraft,
    [Quit, ShowLibrary, ShowLive, ShowRadio, OpenSettings]
);

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--asio-probe") {
        let device = std::env::args().nth(2).unwrap_or_default();
        let buffer = std::env::args()
            .nth(3)
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let master = std::env::args()
            .nth(4)
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let cue = std::env::args().nth(5).and_then(|v| v.parse().ok());
        match djcraft_engine::Engine::with_routing(
            true,
            Some(&device),
            buffer,
            djcraft_engine::OutputRouting { master, cue },
        ) {
            Ok(_engine) => {
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
            Err(error) => {
                eprintln!("ASIO startup: {error}");
                std::process::exit(1);
            }
        }
        return;
    }

    if let Err(error) = storage::initialize() {
        eprintln!("Library storage: {error}");
        storage::show_problem(error.to_string());
        return;
    }
    gpui_kit::application().with_assets(AllAssets).run(|cx| {
        gpui_kit::init(cx);
        let settings = settings::Settings::load(&storage::machine_dir());
        settings::move_old_waveforms();
        // Views read display settings, such as how tempos are shown, from here.
        cx.set_global(settings.clone());
        settings.apply_theme(cx);
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.bind_keys([
            KeyBinding::new("cmd-q", Quit, None),
            KeyBinding::new("cmd-1", ShowLibrary, None),
            KeyBinding::new("cmd-2", ShowLive, None),
            KeyBinding::new("cmd-3", ShowRadio, None),
            KeyBinding::new("cmd-,", OpenSettings, None),
            KeyBinding::new("ctrl-,", OpenSettings, None),
            KeyBinding::new(
                "escape",
                settings_view::CloseSettings,
                Some(settings_view::KEY_CONTEXT),
            ),
        ]);

        let bounds = Bounds::centered(None, size(px(1920.), px(1080.)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(960.), px(600.))),
            titlebar: Some(TitlebarOptions {
                title: Some("faderCraft".into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(16.), px(18.))),
            }),
            ..Default::default()
        };
        // Tracks named on the command line are loaded onto the decks in order.
        let tracks = std::env::args().skip(1).collect();
        gpui_kit::open_window(options, cx, |window, cx| {
            let shell = cx.new(|cx| shell::Shell::new(tracks, settings, window, cx));
            let show = |tab: shell::Tab| {
                let shell = shell.clone();
                move |cx: &mut App| shell.update(cx, |shell, cx| shell.show(tab, cx))
            };
            let (library, live, radio) = (
                show(shell::Tab::Library),
                show(shell::Tab::Live),
                show(shell::Tab::Radio),
            );
            cx.on_action(move |_: &ShowLibrary, cx| library(cx));
            cx.on_action(move |_: &ShowLive, cx| live(cx));
            cx.on_action(move |_: &ShowRadio, cx| radio(cx));
            let settings_shell = shell.clone();
            cx.on_action(move |_: &OpenSettings, cx| {
                settings_shell.update(cx, |shell, cx| shell.open_settings(cx))
            });
            shell
        })
        .expect("failed to open window");
        cx.activate(true);
    });
    stems::shutdown();
    if let Err(error) = storage::finish() {
        eprintln!("Library snapshot was not published: {error}");
    }
}
