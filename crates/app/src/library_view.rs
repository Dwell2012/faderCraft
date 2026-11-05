use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::f32::consts::TAU;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
use std::time::{Duration, Instant};

use djcraft_analysis::TempoRange;
use djcraft_library::lookup::Provider;
use djcraft_library::{
    AlignOptions, ApplySummary, Camelot, Cleanup, Crate, FolderWatcher, Group, GroupCount,
    ImportSummary, ImportedPlaylist, Library, PlannedChange, Playlist, PlaylistFolder, Source,
    SourceKind, TempoResult, Track, TrackEdit, TrackQuery, discover_playlists, read_artwork,
    read_playlists,
};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme, Disableable, Icon, Selectable, Sizable, WindowExt,
    button::{Button, ButtonRounded, ButtonVariants},
    checkbox::Checkbox,
    input::{Input, InputEvent, InputState},
    menu::{DropdownMenu, PopupMenu, PopupMenuItem},
    notification::Notification,
    table::{Column, ColumnSort, DataTable, TableDelegate, TableEvent, TableState},
    tooltip::Tooltip,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::lookup_view::{LookupEvent, LookupField, LookupView};
use crate::organise_view::{DuplicatesView, NameForm, OrganiseEvent, RelocateView, SmartForm};
use crate::tag_tools::{FormEvent, RenameForm, ReplaceForm, ReviewEvent, ReviewView};

/// How often the track list is refreshed while a scan is running.
const SCAN_REFRESH: Duration = Duration::from_millis(400);
/// How often it is refreshed while tracks are being analysed.
const ANALYSIS_REFRESH: Duration = Duration::from_secs(2);

enum PlaylistPickEvent {
    Import(Vec<ImportedPlaylist>),
}

struct PlaylistPicker {
    playlists: Vec<ImportedPlaylist>,
    selected: Vec<bool>,
    errors: Vec<String>,
}

impl EventEmitter<PlaylistPickEvent> for PlaylistPicker {}

impl Render for PlaylistPicker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self.selected.iter().filter(|selected| **selected).count();
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(div().text_sm().child(format!(
                "Found {} playlist{} in the added folder. Choose which to add; tracks already scanned will be reused.",
                self.playlists.len(), if self.playlists.len() == 1 { "" } else { "s" }
            )))
            .child(
                div().id("playlist-picker-list").overflow_y_scroll().max_h(px(360.)).flex().flex_col().gap_1()
                    .children(self.playlists.iter().enumerate().map(|(index, playlist)| {
                        Checkbox::new(("found-playlist", index))
                            .label(format!("{} ({} tracks)", playlist.name, playlist.files.len()))
                            .checked(self.selected[index])
                            .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                                this.selected[index] = *checked;
                                cx.notify();
                            }))
                    })),
            )
            .when(!self.errors.is_empty(), |panel| panel.child(
                div().text_xs().text_color(cx.theme().muted_foreground).child(format!(
                    "{} unreadable playlist file{} skipped.",
                    self.errors.len(), if self.errors.len() == 1 { " was" } else { "s were" }
                )),
            ))
            .child(
                div().flex().justify_end().gap_2()
                    .child(Button::new("skip-found-playlists").outline().label("Skip")
                        .on_click(|_, window, cx| window.close_dialog(cx)))
                    .child(Button::new("import-found-playlists").primary()
                        .label(format!("Add {selected} selected"))
                        .disabled(selected == 0)
                        .on_click(cx.listener(|this, _, _, cx| {
                            let chosen = this.playlists.iter().zip(&this.selected)
                                .filter_map(|(playlist, selected)| selected.then_some(playlist.clone()))
                                .collect();
                            cx.emit(PlaylistPickEvent::Import(chosen));
                        }))),
            )
    }
}
/// What a menu entry does to the library view.
type MenuAction = Box<dyn Fn(&mut LibraryView, &mut Window, &mut Context<LibraryView>)>;

/// How many suggestions for the next track are listed.
const SUGGESTIONS: usize = 50;
/// How often watched folders are asked whether they changed.
const WATCH_POLL: Duration = Duration::from_secs(1);
/// How long a folder must be left alone before it is scanned, so that a
/// copy in progress is scanned once, at its end.
const WATCH_QUIET: Duration = Duration::from_secs(2);

/// A section of the library, chosen from the rail on the left.
#[derive(Clone, Copy, PartialEq)]
enum Section {
    Library,
    Playlists,
    Flow,
    History,
}

impl Section {
    const ALL: [Section; 4] = [
        Section::Library,
        Section::Playlists,
        Section::Flow,
        Section::History,
    ];

    fn icon_and_label(self) -> (IconName, &'static str) {
        match self {
            Section::Library => (IconName::Library, "Library"),
            Section::Playlists => (IconName::ListMusic, "Playlists"),
            Section::Flow => (IconName::Workflow, "Flow"),
            Section::History => (IconName::Clock, "History"),
        }
    }
}

/// Where the tracks shown in the library come from.
#[derive(Clone, Copy, PartialEq)]
enum Origin {
    /// Every source together.
    All,
    Local,
    AppleMusic,
    Spotify,
    /// Not a source: the page for adding more of them.
    Plugins,
}

impl Origin {
    /// The sources shown as tabs.
    const ALL: [Origin; 4] = [
        Origin::All,
        Origin::Local,
        Origin::AppleMusic,
        Origin::Spotify,
    ];

    fn icon_and_label(self) -> (IconName, &'static str) {
        match self {
            Origin::All => (IconName::Layers, "All"),
            Origin::Local => (IconName::HardDrive, "Local"),
            Origin::AppleMusic => (IconName::Music, "Apple Music"),
            Origin::Spotify => (IconName::Disc3, "Spotify"),
            Origin::Plugins => (IconName::Plus, "Plugins"),
        }
    }
}

/// The details that can be edited in the info panel.
#[derive(Clone, Copy)]
enum EditField {
    Title,
    Artist,
    Album,
    Genre,
    Bpm,
    Key,
}

impl EditField {
    const ALL: [EditField; 6] = [
        EditField::Title,
        EditField::Artist,
        EditField::Album,
        EditField::Genre,
        EditField::Bpm,
        EditField::Key,
    ];

    fn label(self) -> &'static str {
        match self {
            EditField::Title => "Title",
            EditField::Artist => "Artist",
            EditField::Album => "Album",
            EditField::Genre => "Genre",
            EditField::Bpm => "BPM",
            EditField::Key => "Key",
        }
    }
}

/// A tempo without needless decimals: 128, 127.5.
/// How many decimals tempos are shown with, from the settings.
pub fn bpm_decimals(cx: &App) -> u8 {
    cx.try_global::<crate::settings::Settings>()
        .map_or(1, |settings| settings.bpm_decimals)
}

/// A tempo for an edit box: as precise as it is, without trailing zeros.
fn format_bpm(bpm: f64) -> String {
    let text = format!("{bpm:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

#[derive(Clone, Copy)]
enum Field {
    Title,
    Artist,
    Album,
    Genre,
    Bpm,
    Key,
    Energy,
    Year,
    Time,
    Source,
    Stems,
}

/// The four destinations in the compact Perform browser's icon rail.
#[derive(Clone, Copy, PartialEq)]
enum BrowserSection {
    Folders,
    Playlists,
    Crates,
    OnlineSources,
}

impl BrowserSection {
    const ALL: [Self; 4] = [
        Self::Folders,
        Self::Playlists,
        Self::Crates,
        Self::OnlineSources,
    ];

    fn icon_and_label(self) -> (IconName, &'static str) {
        match self {
            Self::Folders => (IconName::FolderOpen, "Folders"),
            Self::Playlists => (IconName::ListMusic, "Playlists"),
            Self::Crates => (IconName::Box, "Crates"),
            Self::OnlineSources => (IconName::Globe, "Online Sources"),
        }
    }
}

const COLUMNS: [(Field, &str, f32); 11] = [
    (Field::Title, "Title", 260.),
    (Field::Artist, "Artist", 170.),
    (Field::Album, "Album", 170.),
    (Field::Genre, "Genre", 110.),
    (Field::Bpm, "BPM", 70.),
    (Field::Key, "Key", 60.),
    (Field::Energy, "Energy", 80.),
    (Field::Year, "Year", 60.),
    (Field::Time, "Time", 70.),
    (Field::Source, "Source", 110.),
    (Field::Stems, "Stems", 80.),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StemState {
    Ready,
    Queued,
    Progress(u8),
}

fn controller_row(current: Option<usize>, len: usize, steps: i32) -> Option<usize> {
    if len == 0 || steps == 0 {
        return current.filter(|i| *i < len);
    }
    let start = current
        .filter(|i| *i < len)
        .map(|i| i as i64)
        .unwrap_or(if steps > 0 { -1 } else { len as i64 });
    Some((start + i64::from(steps)).clamp(0, len as i64 - 1) as usize)
}

/// What the library asks the rest of the app to do.
pub enum LibraryEvent {
    NewSet,
    SeparateStems {
        path: PathBuf,
    },
    Queue(Vec<djcraft_protocol::QueueTrack>),
    /// A track was chosen (double-clicked, or sent to a deck from its menu).
    /// On the decks that loads it onto `deck`, or onto whichever is free.
    Load {
        id: i64,
        path: PathBuf,
        deck: Option<usize>,
    },
    /// A track was picked out in one of the compact browsers.
    Selected {
        id: i64,
    },
    /// A scan or analysis moved on, so the database may hold different
    /// tracks, or new results for them.
    Changed,
    /// The user chose which columns to hide in track lists.
    Columns(Vec<String>),
}

/// A track being dragged out of a track list.
#[derive(Clone)]
pub struct DraggedTrack {
    pub id: i64,
    pub playlist_entry: Option<i64>,
    pub title: SharedString,
    pub path: PathBuf,
}

impl Render for DraggedTrack {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_1()
            .rounded_md()
            .bg(cx.theme().secondary)
            .border_1()
            .border_color(cx.theme().border)
            .text_sm()
            .child(self.title.clone())
    }
}

const WHEEL_SIZE: f32 = 204.;
const WHEEL_LABEL: f32 = 24.;
/// Inner and outer edge of each ring of the wheel.
const WHEEL_MINOR: (f32, f32) = (44., 71.);
const WHEEL_MAJOR: (f32, f32) = (73., 100.);

/// Where a key sits round the wheel: twelve o'clock is 12, and each number
/// is a twelfth of a turn on.
fn wheel_angle(key: Camelot) -> f32 {
    (key.number % 12) as f32 * (TAU / 12.) - TAU / 4.
}

/// The top-left corner of a key's label, within the wheel.
fn wheel_label_corner(key: Camelot) -> (Pixels, Pixels) {
    let (inner, outer) = if key.major { WHEEL_MAJOR } else { WHEEL_MINOR };
    let radius = (inner + outer) / 2.;
    let angle = wheel_angle(key);
    (
        px(WHEEL_SIZE / 2. + radius * angle.cos() - WHEEL_LABEL / 2.),
        px(WHEEL_SIZE / 2. + radius * angle.sin() - WHEEL_LABEL / 2.),
    )
}

/// The wheel's 24 coloured segments. With a key lit, it and the keys that
/// mix with it stand out and the rest are dimmed.
fn wheel_segments(lit: Option<[Camelot; 4]>) -> impl IntoElement {
    let strength = move |key: Camelot| match lit {
        None => 0.85,
        Some(lit) if lit[0] == key => 1.,
        Some(lit) if lit.contains(&key) => 0.8,
        Some(_) => 0.18,
    };
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let centre = bounds.origin + point(px(WHEEL_SIZE / 2.), px(WHEEL_SIZE / 2.));
            let at = |radius: f32, angle: f32| {
                centre + point(px(radius * angle.cos()), px(radius * angle.sin()))
            };
            for key in Camelot::all() {
                let (inner, outer) = if key.major { WHEEL_MAJOR } else { WHEEL_MINOR };
                // A sliver is left between neighbours.
                let (from, to) = (
                    wheel_angle(key) - TAU / 24. + 0.02,
                    wheel_angle(key) + TAU / 24. - 0.02,
                );
                let steps = 8;
                let sweep = |step: usize| from + (to - from) * step as f32 / steps as f32;
                let mut path = PathBuilder::fill();
                path.move_to(at(outer, from));
                for step in 1..=steps {
                    path.line_to(at(outer, sweep(step)));
                }
                for step in (0..=steps).rev() {
                    path.line_to(at(inner, sweep(step)));
                }
                path.close();
                if let Ok(path) = path.build() {
                    window.paint_path(path, key_colour(key, strength(key)));
                }
            }
        },
    )
    .absolute()
    .size_full()
}

/// A Camelot wheel that only shows a key: it and the keys that mix with it
/// are lit, and its name is in the middle.
pub fn wheel_display(key: Option<Camelot>, muted: Hsla) -> Div {
    let lit = key.map(|key| key.compatible());
    let labels = Camelot::all().map(|label| {
        let (left, top) = wheel_label_corner(label);
        let dimmed = lit.is_some_and(|lit| !lit.contains(&label));
        div()
            .absolute()
            .left(left)
            .top(top)
            .size(px(WHEEL_LABEL))
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(10.))
            .font_weight(FontWeight::BOLD)
            .text_color(gpui_kit::white().opacity(if dimmed { 0.45 } else { 1. }))
            .child(label.to_string())
    });
    let hub = div()
        .absolute()
        .left(px(WHEEL_SIZE / 2. - 40.))
        .top(px(WHEEL_SIZE / 2. - 20.))
        .w(px(80.))
        .h(px(40.))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .map(|this| match key {
            Some(key) => this
                .child(div().font_weight(FontWeight::BOLD).child(key.to_string()))
                .child(div().text_size(px(10.)).text_color(muted).child(key.name())),
            None => this.child(div().text_xs().text_color(muted).child("No key")),
        });
    div()
        .relative()
        .flex_none()
        .size(px(WHEEL_SIZE))
        .child(wheel_segments(lit))
        .children(labels)
        .child(hub)
}

/// The colour that stands for a key: hue by its number round the wheel,
/// darker for minor keys.
pub fn key_colour(key: Camelot, strength: f32) -> Hsla {
    let lightness = if key.major { 0.52 } else { 0.38 };
    hsla((key.number - 1) as f32 / 12., 0.62, lightness, strength)
}

struct TrackTable {
    played: HashSet<PathBuf>,
    view: WeakEntity<LibraryView>,
    tracks: Vec<Track>,
    sort: Option<(usize, ColumnSort)>,
    playlists: Vec<Playlist>,
    crates: Vec<Crate>,
    /// The playlist being shown, if any.
    playlist: Option<i64>,
    crate_id: Option<i64>,
    /// Whether it is a smart playlist, whose order cannot be changed.
    smart: bool,
    /// Rows come in an order of their own (best suggestions first), kept
    /// unless the user sorts.
    keep_order: bool,
    /// The columns shown, as places in [`COLUMNS`].
    visible: Vec<usize>,
    stem_states: HashMap<PathBuf, StemState>,
}

impl TrackTable {
    fn new(view: WeakEntity<LibraryView>) -> Self {
        Self {
            view,
            tracks: Vec::new(),
            played: HashSet::new(),
            sort: None,
            playlists: Vec::new(),
            crates: Vec::new(),
            playlist: None,
            crate_id: None,
            smart: false,
            keep_order: false,
            visible: (0..COLUMNS.len()).collect(),
            stem_states: HashMap::new(),
        }
    }

    /// The field shown in a column of the table as it is now.
    fn field(&self, col_ix: usize) -> Field {
        COLUMNS[self.visible[col_ix]].0
    }

    /// Replaces the rows, keeping the user's chosen sort order.
    fn set_tracks(
        &mut self,
        tracks: Vec<Track>,
        playlists: Vec<Playlist>,
        crates: Vec<Crate>,
        playlist: Option<i64>,
        crate_id: Option<i64>,
    ) {
        self.tracks = tracks;
        self.playlists = playlists;
        self.crates = crates;
        self.playlist = playlist;
        self.crate_id = crate_id;
        self.apply_sort();
    }

    fn sorted_by_user(&self) -> bool {
        matches!(
            self.sort,
            Some((_, ColumnSort::Ascending | ColumnSort::Descending))
        )
    }

    fn apply_sort(&mut self) {
        // An unsorted playlist stays in playlist order.
        if (self.playlist.is_some() || self.keep_order) && !self.sorted_by_user() {
            return;
        }
        let text = |a: &str, b: &str| a.to_lowercase().cmp(&b.to_lowercase());
        // Tracks with no value sort after those with one.
        let optional = |a: Option<f64>, b: Option<f64>| match (a, b) {
            (Some(a), Some(b)) => a.total_cmp(&b),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        };
        let (field, descending) = match self.sort {
            Some((column, ColumnSort::Ascending)) => (self.field(column), false),
            Some((column, ColumnSort::Descending)) => (self.field(column), true),
            _ => (Field::Artist, false),
        };
        self.tracks.sort_by(|a, b| {
            let order = match field {
                Field::Title => text(&a.title, &b.title),
                Field::Artist => text(&a.artist, &b.artist),
                Field::Album => text(&a.album, &b.album),
                Field::Genre => text(&a.genre, &b.genre),
                Field::Bpm => optional(a.bpm, b.bpm),
                Field::Energy => optional(a.energy.map(f64::from), b.energy.map(f64::from)),
                Field::Year => optional(a.year.map(f64::from), b.year.map(f64::from)),
                // Everything is local for now, so fall back to the folder.
                Field::Source => text(&a.source_name, &b.source_name),
                // Wheel order, with keys that could not be read last.
                Field::Key => match (a.camelot, b.camelot) {
                    (Some(a), Some(b)) => a.cmp(&b),
                    (Some(_), None) => Ordering::Less,
                    (None, Some(_)) => Ordering::Greater,
                    (None, None) => text(
                        a.key.as_deref().unwrap_or(""),
                        b.key.as_deref().unwrap_or(""),
                    ),
                },
                Field::Time => a.duration_seconds.total_cmp(&b.duration_seconds),
                Field::Stems => {
                    let rank = |path: &PathBuf| match self.stem_states.get(path) {
                        Some(StemState::Progress(_)) => 0,
                        Some(StemState::Queued) => 1,
                        Some(StemState::Ready) => 2,
                        None => 3,
                    };
                    rank(&a.path).cmp(&rank(&b.path))
                }
            };
            let order = if descending { order.reverse() } else { order };
            order.then_with(|| text(&a.title, &b.title))
        });
    }
}

impl TableDelegate for TrackTable {
    fn columns_count(&self, _: &App) -> usize {
        self.visible.len()
    }

    fn rows_count(&self, _: &App) -> usize {
        self.tracks.len()
    }

    fn column(&self, col_ix: usize, _: &App) -> Column {
        let (field, name, width) = COLUMNS[self.visible[col_ix]];
        let column = Column::new(name, name).width(px(width)).sortable();
        match field {
            Field::Bpm | Field::Energy | Field::Year | Field::Time => column.text_right(),
            _ => column,
        }
    }

    fn perform_sort(
        &mut self,
        col_ix: usize,
        sort: ColumnSort,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) {
        self.sort = Some((col_ix, sort));
        if self.playlist.is_some() && !self.sorted_by_user() {
            // Back to playlist order, which only the database knows.
            let view = self.view.clone();
            cx.defer(move |cx| {
                view.update(cx, |view, cx| view.reload(cx)).ok();
            });
        } else {
            self.apply_sort();
        }
        cx.notify();
    }

    fn context_menu(
        &mut self,
        row_ix: usize,
        menu: PopupMenu,
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) -> PopupMenu {
        let Some(track) = self.tracks.get(row_ix) else {
            return menu;
        };
        let track_id = track.id;
        let entry_id = track.playlist_entry;
        let item = |label: SharedString,
                    action: fn(&mut LibraryView, i64, i64, &mut Context<LibraryView>),
                    argument: i64| {
            let view = self.view.clone();
            PopupMenuItem::new(label).on_click(move |_, _, cx| {
                view.update(cx, |view, cx| action(view, track_id, argument, cx))
                    .ok();
            })
        };
        let mut menu = menu;
        let entry_item = |label: SharedString,
                          action: fn(&mut LibraryView, i64, i64, &mut Context<LibraryView>),
                          argument: i64| {
            let view = self.view.clone();
            PopupMenuItem::new(label).on_click(move |_, _, cx| {
                if let Some(entry) = entry_id {
                    view.update(cx, |view, cx| action(view, entry, argument, cx))
                        .ok();
                }
            })
        };
        for deck in 0..djcraft_protocol::DECKS {
            menu = menu.item(item(
                format!("Load to Deck {}", crate::shell::deck_label(deck)).into(),
                LibraryView::load_to_deck,
                deck as i64,
            ));
        }
        menu = menu.item(item("Add to Automix".into(), LibraryView::queue_track, 0));
        if !track.missing {
            menu = menu.item(item(
                "Separate stems".into(),
                LibraryView::separate_stems,
                0,
            ));
        }
        if track.missing {
            menu = menu
                .separator()
                .item(item("Locate file…".into(), LibraryView::locate_file, 0));
        }
        if let Some(playlist) = self.playlist.filter(|_| !self.smart) {
            // Moving rows only makes sense in the playlist's own order.
            if !self.sorted_by_user() {
                let last = self.tracks.len() as i64 - 1;
                let at = row_ix as i64;
                menu = menu.separator();
                if at > 0 {
                    menu = menu
                        .item(entry_item("Move to top".into(), LibraryView::move_track, 0))
                        .item(entry_item(
                            "Move up".into(),
                            LibraryView::move_track,
                            at - 1,
                        ));
                }
                if at < last {
                    menu = menu
                        .item(entry_item(
                            "Move down".into(),
                            LibraryView::move_track,
                            at + 1,
                        ))
                        .item(entry_item(
                            "Move to bottom".into(),
                            LibraryView::move_track,
                            last,
                        ));
                }
            }
            menu = menu.separator().item(entry_item(
                "Remove from playlist".into(),
                LibraryView::remove_from_playlist,
                playlist,
            ));
        }
        if let Some(crate_id) = self.crate_id {
            menu = menu.separator().item(item(
                "Remove from crate".into(),
                LibraryView::remove_from_crate,
                crate_id,
            ));
        }
        if !self.crates.is_empty() {
            menu = menu.separator().label("Add to crate");
            for crate_item in &self.crates {
                menu = menu.item(item(
                    crate_item.name.clone().into(),
                    LibraryView::add_to_crate,
                    crate_item.id,
                ));
            }
        }
        // A smart playlist fills itself, so tracks cannot be added by hand.
        let others: Vec<_> = self
            .playlists
            .iter()
            .filter(|playlist| Some(playlist.id) != self.playlist && playlist.rules.is_none())
            .collect();
        if !others.is_empty() {
            menu = menu.separator().label("Add to playlist");
            for playlist in others {
                menu = menu.item(item(
                    playlist.name.clone().into(),
                    LibraryView::add_to_playlist,
                    playlist.id,
                ));
            }
        }
        menu.separator().item(item(
            "Remove from library (keep file)".into(),
            LibraryView::remove_from_library,
            0,
        ))
    }

    fn render_tr(
        &mut self,
        row_ix: usize,
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) -> Stateful<Div> {
        let mut row = div().id(("row", row_ix));
        if let Some(target) = self
            .tracks
            .get(row_ix)
            .filter(|_| self.playlist.is_some() && !self.smart && !self.sorted_by_user())
        {
            let target = target.playlist_entry;
            let playlist = self.playlist;
            let view = self.view.clone();
            row = row
                .drag_over::<DraggedTrack>(|style, _, _, _| style.bg(rgb(0x183849)))
                .on_drop(move |dragged: &DraggedTrack, _, cx| {
                    view.update(cx, |view, cx| {
                        if view.playlist == playlist && dragged.playlist_entry != target {
                            if let (Some(from), Some(to)) = (dragged.playlist_entry, target) {
                                view.drop_playlist_track(from, to, cx);
                            }
                        }
                    })
                    .ok();
                });
        }
        // Rows can be dragged out, onto a Flow lane for one.
        match self.tracks.get(row_ix) {
            Some(track) => row.on_drag(
                DraggedTrack {
                    id: track.id,
                    playlist_entry: track.playlist_entry,
                    title: track.title.clone().into(),
                    path: track.path.clone(),
                },
                |dragged, _, _, cx| cx.new(|_| dragged.clone()),
            ),
            None => row,
        }
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let track = &self.tracks[row_ix];
        let decimals = bpm_decimals(cx);
        let text: SharedString = match self.field(col_ix) {
            Field::Title => track.title.clone().into(),
            Field::Artist => track.artist.clone().into(),
            Field::Album => track.album.clone().into(),
            Field::Genre => track.genre.clone().into(),
            // Where the track comes from; folders and loose files are local.
            Field::Source => match track.source_kind {
                SourceKind::Folder | SourceKind::Files => "Local".into(),
            },
            Field::Energy => track
                .energy
                .map(|energy| energy.to_string())
                .unwrap_or_default()
                .into(),
            Field::Bpm => track
                .bpm
                .map(|bpm| crate::settings::format_bpm(bpm, decimals))
                .unwrap_or_default()
                .into(),
            // Camelot notation when the tag can be understood, else as tagged.
            Field::Key => match track.camelot {
                Some(key) => key.to_string().into(),
                None => track.key.clone().unwrap_or_default().into(),
            },
            Field::Year => track
                .year
                .map(|year| year.to_string())
                .unwrap_or_default()
                .into(),
            Field::Time => {
                let seconds = track.duration_seconds.round() as u64;
                format!("{}:{:02}", seconds / 60, seconds % 60).into()
            }
            Field::Stems => match self.stem_states.get(&track.path) {
                Some(StemState::Ready) => "Ready".into(),
                Some(StemState::Queued) => "Queued".into(),
                Some(StemState::Progress(percent)) => format!("{percent}%").into(),
                None => "—".into(),
            },
        };
        // A track whose file is missing is shown faded, with a mark.
        let missing = track.missing;
        let text: SharedString = match self.field(col_ix) {
            Field::Title if missing => format!("⚠ {text}").into(),
            _ => text,
        };
        if matches!(self.field(col_ix), Field::Title) {
            return div()
                .flex()
                .items_center()
                .gap_1()
                .when(missing, |this| this.opacity(0.5))
                .when(self.played.contains(&track.path), |this| {
                    this.child(
                        div()
                            .id(("played", row_ix))
                            .flex()
                            .flex_shrink_0()
                            .child(Icon::new(IconName::Check).small())
                            .tooltip(|window, cx| {
                                Tooltip::new("Played in current set").build(window, cx)
                            }),
                    )
                })
                .child(div().min_w_0().truncate().child(text));
        }
        if matches!(self.field(col_ix), Field::Key) {
            let mode = track.modal_key.as_ref().map_or_else(
                || "Mode unresolved".to_owned(),
                |mode| format!("Estimated mode: {}", mode.label(0)),
            );
            return div().child(
                div()
                    .id(("library-key-info", row_ix))
                    .truncate()
                    .when(missing, |this| this.opacity(0.5))
                    .child(text)
                    .tooltip(move |window, cx| Tooltip::new(mode.clone()).build(window, cx)),
            );
        }
        div()
            .truncate()
            .when(missing, |this| this.opacity(0.5))
            .child(text)
    }
}

#[derive(Default)]
struct ScanCounters {
    done: AtomicUsize,
    total: AtomicUsize,
}

/// The Library tab: sources on the left, every track from them on the right.
pub struct LibraryView {
    history_set: Option<i64>,
    db_path: PathBuf,
    /// `None` if the database could not be opened; `error` says why.
    library: Option<Library>,
    error: Option<SharedString>,
    /// The outcome of the last import.
    notice: Option<SharedString>,
    /// The small browser under the decks, not the full Library tab.
    compact: bool,
    browser_section: BrowserSection,
    controller_folders: bool,
    /// Whether a compact browser offers the Camelot wheel, from a button
    /// beside its track count.
    wheel_in_browser: bool,
    /// Whether that wheel is showing.
    wheel_open: bool,
    /// When the wheel was last closed by a click outside it.
    wheel_dismissed: Option<Instant>,
    section: Section,
    origin: Origin,
    sources: Vec<Source>,
    /// The source being shown, or `None` for all tracks.
    selected: Option<i64>,
    selected_subfolder: Option<PathBuf>,
    source_subfolders: Vec<(i64, PathBuf)>,
    source_folder_parents: HashSet<(i64, PathBuf)>,
    source_folders_dirty: bool,
    expanded_source_folders: HashSet<(i64, PathBuf)>,
    playlists: Vec<Playlist>,
    /// The playlist open in the Playlists section.
    playlist: Option<i64>,
    crates: Vec<Crate>,
    crate_id: Option<i64>,
    collapsed_crates: HashSet<i64>,
    new_playlist: Entity<InputState>,
    search: Entity<InputState>,
    table: Entity<TableState<TrackTable>>,
    /// The track shown in the info panel.
    inspected: Option<Track>,
    /// One text box per [`EditField`], in that order.
    fields: [Entity<InputState>; 6],
    artwork: Option<Arc<Image>>,
    provider: Provider,
    /// Whether the info panel's details can be changed. Off until asked.
    editing: bool,
    running_scans: usize,
    scan_counters: Arc<ScanCounters>,
    /// Watches the folders for changes; `None` when that is switched off.
    watcher: Option<FolderWatcher>,
    /// The tempo range tracks are analysed with.
    tempo_range: TempoRange,
    /// Set when the library must be analysed again once the pass under way
    /// has stopped.
    reanalyse: bool,
    /// The automatic group shown, in place of a source.
    group: Option<Group>,
    groups: Vec<GroupCount>,
    folders: Vec<PlaylistFolder>,
    /// Playlist folders shown closed.
    collapsed: HashSet<i64>,
    collapsed_playlists: HashSet<i64>,
    /// In the browser under the decks: the track on the decks that
    /// suggestions are for, and whether they are being shown.
    reference: Option<(i64, SharedString)>,
    suggesting: bool,
    /// The duplicates dialog, while it is open, to refresh after a merge.
    duplicates: Option<Entity<DuplicatesView>>,
    /// The key picked on the Camelot wheel.
    wheel_key: Option<Camelot>,
    key_counts: HashMap<Camelot, usize>,
    /// The Flow editor; the compact browsers have none.
    flow: Option<Entity<crate::flow_view::FlowView>>,
    /// Whether tracks are being analysed in the background.
    analysing: bool,
    analysis_error: Option<String>,
    analysis: Arc<crate::analysis::Progress>,
}

impl LibraryView {
    pub fn set_stem_states(&mut self, states: HashMap<PathBuf, StemState>, cx: &mut Context<Self>) {
        self.table.update(cx, |table, cx| {
            if table.delegate().stem_states != states {
                table.delegate_mut().stem_states = states;
                table.refresh(cx);
            }
        });
    }
    /// `compact` makes the track browser shown under the decks: the same
    /// library, without the rail, source tabs or editing controls.
    ///
    /// With `scan`, every folder is checked for changes straight away.
    pub fn new(compact: bool, scan: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let db_path = crate::storage::database_path();
        let (library, error) = match Library::open(&db_path) {
            Ok(library) => (Some(library), None),
            Err(error) => (None, Some(error.to_string().into())),
        };
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));
        cx.subscribe(&search, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.reload(cx);
            }
        })
        .detach();
        let new_playlist = cx.new(|cx| InputState::new(window, cx).placeholder("New playlist"));
        cx.subscribe_in(
            &new_playlist,
            window,
            |this, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    // Enter in the New playlist dialog is the same as Create.
                    if this.create_named_playlist(cx) {
                        window.close_dialog(cx);
                    }
                }
            },
        )
        .detach();
        let view = cx.weak_entity();
        let table = cx.new(|cx| TableState::new(TrackTable::new(view), window, cx));
        cx.subscribe_in(
            &table,
            window,
            |this, table, event: &TableEvent, window, cx| {
                let track = |row: &usize| table.read(cx).delegate().tracks.get(*row).cloned();
                match event {
                    TableEvent::DoubleClickedRow(row) => {
                        if let Some(track) = track(row) {
                            cx.emit(LibraryEvent::Load {
                                id: track.id,
                                path: track.path,
                                deck: None,
                            });
                        }
                    }
                    TableEvent::SelectRow(row) => {
                        if let Some(track) = track(row) {
                            if this.compact {
                                cx.emit(LibraryEvent::Selected { id: track.id });
                            } else {
                                this.inspect(track, window, cx);
                            }
                        }
                    }
                    _ => {}
                }
            },
        )
        .detach();
        let fields = EditField::ALL
            .map(|field| cx.new(|cx| InputState::new(window, cx).placeholder(field.label())));

        let mut view = Self {
            db_path,
            library,
            error,
            notice: None,
            compact,
            browser_section: BrowserSection::Folders,
            wheel_in_browser: false,
            controller_folders: false,
            wheel_open: false,
            wheel_dismissed: None,
            history_set: None,
            section: Section::Library,
            origin: Origin::All,
            sources: Vec::new(),
            selected: None,
            selected_subfolder: None,
            source_subfolders: Vec::new(),
            source_folder_parents: HashSet::new(),
            source_folders_dirty: true,
            expanded_source_folders: HashSet::new(),
            playlists: Vec::new(),
            playlist: None,
            crates: Vec::new(),
            crate_id: None,
            collapsed_crates: HashSet::new(),
            new_playlist,
            search,
            table,
            inspected: None,
            fields,
            artwork: None,
            provider: Provider::default(),
            editing: false,
            running_scans: 0,
            scan_counters: Arc::default(),
            watcher: None,
            tempo_range: TempoRange::default(),
            reanalyse: false,
            group: None,
            groups: Vec::new(),
            folders: Vec::new(),
            collapsed: HashSet::new(),
            collapsed_playlists: HashSet::new(),
            reference: None,
            suggesting: false,
            duplicates: None,
            flow: (!compact).then(|| cx.new(|cx| crate::flow_view::FlowView::new(window, cx))),
            wheel_key: None,
            key_counts: HashMap::new(),
            analysing: false,
            analysis_error: None,
            analysis: Arc::default(),
        };
        if !compact && let Some(settings) = cx.try_global::<crate::settings::Settings>().cloned() {
            view.tempo_range = settings.tempo_range();
            // The database holds this too, for every connection to see.
            if let Some(library) = &view.library
                && let Err(error) = library.set_keep_tagged(settings.keep_tagged())
            {
                view.error = Some(error.to_string().into());
            }
        }
        view.show_section(Section::Library, cx);
        // Pick up anything that changed on disk since the last run.
        // The Library tab does this; the browser shares its database.
        let sources = if scan && !compact {
            view.sources
                .iter()
                .filter(|source| source.kind == SourceKind::Folder)
                .cloned()
                .collect()
        } else {
            Vec::new()
        };
        if !sources.is_empty() {
            view.scan_added_folders(sources, window, cx);
        }
        view.analyse(cx);
        if !compact {
            view.poll_watcher(cx);
        }
        view
    }

    /// Follows a change to the settings that affect the library: how
    /// tracks are listed, and the tempo range they are analysed with.
    pub fn apply_settings(&mut self, settings: &crate::settings::Settings, cx: &mut Context<Self>) {
        self.set_watching(settings.watch_folders);
        if let Some(library) = &self.library
            && library.keep_tagged().ok() != Some(settings.keep_tagged())
            && let Err(error) = library.set_keep_tagged(settings.keep_tagged())
        {
            self.error = Some(error.to_string().into());
        }
        let range = settings.tempo_range();
        if range != self.tempo_range {
            self.tempo_range = range;
            self.reanalyse(cx);
        }
        self.reload(cx);
        cx.emit(LibraryEvent::Changed);
    }

    /// Analyses every track again. A pass under way is stopped first, since
    /// it was started with the old settings.
    fn reanalyse(&mut self, cx: &mut Context<Self>) {
        if self.analysing {
            self.reanalyse = true;
            self.analysis.cancel.store(true, AtomicOrdering::Relaxed);
            return;
        }
        if let Some(library) = &self.library
            && let Err(error) = library.reset_analysis()
        {
            self.error = Some(error.to_string().into());
        }
        self.analyse(cx);
    }

    /// The beat grid of the library track at `path`, if it has one.
    pub fn beat_grid_at(&self, path: &Path) -> Option<TempoResult> {
        let library = self.library.as_ref()?;
        let id = library.track_at(path).ok()??;
        library.beat_grid(id).ok()?
    }

    /// Move a loaded track's persistent grid anchor by whole beats.
    pub fn nudge_beat_grid_at(&mut self, path: &Path, beats: i32, cx: &mut Context<Self>) {
        let Some(library) = self.library.as_ref() else {
            return;
        };
        let Ok(Some(id)) = library.track_at(path) else {
            return;
        };
        let Ok(Some(grid)) = library.beat_grid(id) else {
            return;
        };
        let next = grid.first_beat_seconds + beats as f64 * grid.beat_seconds();
        if next < 0. {
            return;
        }
        self.change_library(
            |library| library.set_beat_grid_anchor(id, next, grid.downbeat),
            cx,
        );
    }

    /// Uses the same accepted/tagged/analysed key shown in the library.
    pub fn key_at(&self, path: &Path) -> Option<Camelot> {
        let library = self.library.as_ref()?;
        let id = library.track_at(path).ok()??;
        library
            .tracks(&TrackQuery {
                id: Some(id),
                ..Default::default()
            })
            .ok()?
            .pop()?
            .camelot
    }

    pub fn modal_key_at(&self, path: &Path) -> Option<djcraft_library::ModalKeyResult> {
        let library = self.library.as_ref()?;
        let id = library.track_at(path).ok()??;
        library
            .tracks(&TrackQuery {
                id: Some(id),
                ..Default::default()
            })
            .ok()?
            .pop()?
            .modal_key
    }

    /// Starts or stops watching the library's folders for changes.
    pub fn set_watching(&mut self, on: bool) {
        if on == self.watcher.is_some() {
            return;
        }
        self.watcher = on.then(FolderWatcher::new);
        for source in self.sources.clone() {
            self.watch(&source);
        }
    }

    fn watch(&mut self, source: &Source) {
        let Some(watcher) = &mut self.watcher else {
            return;
        };
        if source.kind == SourceKind::Folder {
            // A folder that cannot be watched (an unplugged drive, say) is
            // still scanned at start-up.
            let _ = watcher.watch(source.id, Path::new(&source.location));
        }
    }

    /// Scans watched folders once changes to them have settled, for as
    /// long as the view lives.
    fn poll_watcher(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(WATCH_POLL).await;
                let alive = this.update(cx, |this, cx| {
                    // Changes during a scan wait for it to finish.
                    let changed = match &this.watcher {
                        Some(watcher) if this.running_scans == 0 => {
                            watcher.take_changed(WATCH_QUIET)
                        }
                        _ => Vec::new(),
                    };
                    for source_id in changed {
                        this.scan(source_id, cx);
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    /// Analyses tracks that have not been analysed yet, in the background,
    /// showing results as they come in. Does nothing if already under way.
    fn analyse(&mut self, cx: &mut Context<Self>) {
        if self.compact || self.analysing || self.library.is_none() {
            return;
        }
        self.analysing = true;
        self.analysis_error = None;
        self.analysis.cancel.store(false, AtomicOrdering::Relaxed);
        let db_path = self.db_path.clone();
        let progress = self.analysis.clone();
        let range = self.tempo_range;
        let work = cx.background_spawn(async move {
            let waveforms = crate::settings::waveform_dir();
            crate::analysis::analyse_pending(&db_path, &waveforms, range, &progress)
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            this.update(cx, |this, cx| {
                this.analysing = false;
                cx.emit(LibraryEvent::Changed);
                if std::mem::take(&mut this.reanalyse) {
                    this.analysis.cancel.store(false, AtomicOrdering::Relaxed);
                    this.reanalyse(cx);
                    this.reload(cx);
                    return;
                }
                let analysed_some = this.analysis.total.load(AtomicOrdering::Relaxed) > 0;
                match result {
                    // Tracks may have been added while this pass was busy;
                    // go round again until a pass finds nothing to do.
                    Ok(()) if analysed_some => this.analyse(cx),
                    Ok(()) => {}
                    Err(error) => {
                        this.analysis_error = Some(error.to_string());
                        this.error = Some(error.to_string().into());
                    }
                }
                this.reload(cx);
            })
            .ok();
        })
        .detach();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(ANALYSIS_REFRESH).await;
                let analysing = this.update(cx, |this, cx| {
                    this.reload(cx);
                    cx.emit(LibraryEvent::Changed);
                    this.analysing
                });
                if !analysing.unwrap_or(false) {
                    break;
                }
            }
        })
        .detach();
    }

    pub fn record_play(&mut self, path: &Path, title: &str, cx: &mut Context<Self>) -> bool {
        let Some(library) = &self.library else {
            return false;
        };
        match library.record_play(path, title) {
            Ok(()) => {
                self.reload(cx);
                true
            }
            Err(error) => {
                self.error = Some(error.to_string().into());
                cx.notify();
                false
            }
        }
    }

    fn history_panel(&self, cx: &mut Context<Self>) -> Div {
        let result = self.library.as_ref().map(|library| {
            let current = library.current_set()?;
            let selected = self.history_set.unwrap_or(current).min(current);
            Ok::<_, djcraft_library::Error>((current, selected, library.play_history(selected)?))
        });
        let Some(Ok((current, selected, entries))) = result else {
            return self.placeholder("Play history is unavailable.", cx);
        };
        div().flex().flex_col().flex_1().min_w_0().min_h_0().gap_3()
            .child(div().flex().items_center().gap_3()
                .child(format!("Set {selected}{} - {} tracks", if selected == current { " (current)" } else { "" }, entries.len()))
                .child(Button::new("previous-set").label("Previous").disabled(selected <= 1)
                    .on_click(cx.listener(move |this, _, _, cx| { this.history_set = Some(selected - 1); cx.notify(); })))
                .child(Button::new("next-set").label("Next").disabled(selected >= current)
                    .on_click(cx.listener(move |this, _, _, cx| { this.history_set = Some(selected + 1); cx.notify(); })))
                .child(Button::new("new-set").label("New set").tooltip("Clear played flags and start a new set; earlier history is kept")
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(library) = &this.library {
                            match library.new_set() {
                                Ok(_) => { this.history_set = None; this.reload(cx); cx.emit(LibraryEvent::NewSet); }
                                Err(error) => { this.error = Some(error.to_string().into()); cx.notify(); }
                            }
                        }
                    }))))
                .child("A checkmark marks tracks heard in the current set. Tracks qualify after 10 seconds of audible master playback; headphone previews do not count.")
            .child(div().id("history-entries").flex().flex_col().flex_1().min_h_0().overflow_y_scroll()
                .children(entries.into_iter().enumerate().map(|(index, entry)| {
                    div().p_2().child(format!("{}. {}", index + 1, entry.title))
                        .child(div().text_xs().child(entry.path.to_string_lossy().into_owned()))
                })))
    }

    /// Re-reads the sources and the visible tracks from the database.
    fn reload(&mut self, cx: &mut Context<Self>) {
        crate::track_identity::clear_cache();
        let Some(library) = &self.library else {
            return;
        };
        let in_playlists = self.section == Section::Playlists;
        let in_crates = self.compact && self.browser_section == BrowserSection::Crates;
        let query = TrackQuery {
            id: None,
            source: self.selected.filter(|_| !in_playlists && !in_crates),
            playlist: self.playlist.filter(|_| in_playlists),
            crate_id: self.crate_id.filter(|_| in_crates),
            search: self.search.read(cx).value().to_string(),
            // On the wheel, a chosen key lists everything that mixes with it.
            keys: self
                .wheel_key
                // In a browser the wheel filters whatever is listed.
                .filter(|_| self.section == Section::Library || self.compact)
                .map(|key| key.compatible().to_vec())
                .unwrap_or_default(),
            group: self
                .group
                .clone()
                .filter(|_| !in_playlists && !self.compact),
        };
        let suggest_for = self
            .reference
            .as_ref()
            .map(|(id, _)| *id)
            .filter(|_| self.compact && self.suggesting);
        let loaded = (|| {
            let mut tracks: Vec<Track> = if let Some(reference) = suggest_for {
                library
                    .suggest_next(reference, SUGGESTIONS)?
                    .into_iter()
                    .map(|suggestion| suggestion.track)
                    .collect()
            } else if (in_playlists && query.playlist.is_none())
                || (in_crates && query.crate_id.is_none())
            {
                Vec::new()
            } else {
                library.tracks(&query)?
            };
            if !in_playlists
                && !in_crates
                && let (Some(source), Some(folder)) = (self.selected, &self.selected_subfolder)
            {
                if let Some(root) = library.source(source)? {
                    let path = Path::new(&root.location).join(folder);
                    tracks.retain(|track| track.path.starts_with(&path));
                }
            }
            self.key_counts = library.key_counts()?;
            if !self.compact {
                self.groups = library.groups()?;
                self.folders = library.folders()?;
            }
            let played = library
                .play_history(library.current_set()?)?
                .into_iter()
                .map(|entry| entry.path)
                .collect();
            Ok::<_, djcraft_library::Error>((
                library.sources()?,
                self.source_folders_dirty
                    .then(|| library.source_subfolders())
                    .transpose()?,
                library.playlists()?,
                library.crates()?,
                tracks,
                played,
            ))
        })();
        match loaded {
            Ok((sources, subfolders, playlists, crates, tracks, played)) => {
                self.sources = sources;
                if let Some(subfolders) = subfolders {
                    self.source_folder_parents = subfolders
                        .iter()
                        .filter_map(|(source, path)| {
                            path.parent()
                                .filter(|parent| !parent.as_os_str().is_empty())
                                .map(|parent| (*source, parent.to_path_buf()))
                        })
                        .collect();
                    self.source_subfolders = subfolders;
                    self.source_folders_dirty = false;
                }
                self.playlists = playlists.clone();
                self.crates = crates.clone();
                let smart = query
                    .playlist
                    .and_then(|id| playlists.iter().find(|p| p.id == id))
                    .is_some_and(|playlist| playlist.rules.is_some());
                self.table.update(cx, |table, cx| {
                    let table = table.delegate_mut();
                    table.played = played;
                    table.smart = smart;
                    // Suggestions come best first; a sort would hide that.
                    table.keep_order = suggest_for.is_some();
                    table.set_tracks(tracks, playlists, crates, query.playlist, query.crate_id);
                    cx.notify();
                });
            }
            Err(error) => self.error = Some(error.to_string().into()),
        }
        cx.notify();
    }

    fn show_section(&mut self, section: Section, cx: &mut Context<Self>) {
        self.section = section;
        if self.compact {
            self.browser_section = if section == Section::Playlists {
                BrowserSection::Playlists
            } else {
                BrowserSection::Folders
            };
        }
        match (section, &self.flow) {
            (Section::Flow, Some(flow)) => flow.update(cx, |flow, cx| flow.refresh(cx)),
            // Leaving Flow stops its player.
            (_, Some(flow)) => flow.update(cx, |flow, _| flow.pause()),
            _ => {}
        }
        if section == Section::Playlists && self.playlist.is_none() {
            self.playlist = self.playlists.first().map(|playlist| playlist.id);
        }
        self.reload(cx);
    }

    fn show_browser_section(&mut self, section: BrowserSection, cx: &mut Context<Self>) {
        self.browser_section = section;
        match section {
            BrowserSection::Folders => self.show_section(Section::Library, cx),
            BrowserSection::Playlists => self.show_section(Section::Playlists, cx),
            BrowserSection::Crates => {
                self.section = Section::Library;
                if self.crate_id.is_none() {
                    self.crate_id = self.crates.first().map(|c| c.id);
                }
                self.reload(cx);
            }
            BrowserSection::OnlineSources => cx.notify(),
        }
    }

    fn browser_rail(&self, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .w(px(40.))
            .gap(px(4.))
            .p(px(4.))
            .border_1()
            .border_color(cx.theme().border)
            .rounded_lg()
            .children(
                BrowserSection::ALL
                    .into_iter()
                    .enumerate()
                    .map(|(index, section)| {
                        let (icon, label) = section.icon_and_label();
                        Button::new(("browser-section", index))
                            .ghost()
                            .w(px(30.))
                            .h(px(30.))
                            .icon(icon)
                            .tooltip(label)
                            .selected(self.browser_section == section)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.show_browser_section(section, cx)
                            }))
                    }),
            )
    }

    fn create_playlist(&mut self, name: &str, cx: &mut Context<Self>) {
        let Some(library) = &self.library else {
            return;
        };
        match library.create_playlist(name) {
            Ok(playlist) => self.playlist = Some(playlist.id),
            Err(error) => self.error = Some(error.to_string().into()),
        }
        self.reload(cx);
    }

    fn delete_playlist(&mut self, id: i64, cx: &mut Context<Self>) {
        let Some(library) = &self.library else {
            return;
        };
        if let Err(error) = library.delete_playlist(id) {
            self.error = Some(error.to_string().into());
        }
        if self.playlist == Some(id) {
            self.playlist = None;
        }
        self.reload(cx);
        if self.playlist.is_none() {
            self.show_section(self.section, cx);
        }
    }

    fn add_to_playlist(&mut self, track: i64, playlist: i64, cx: &mut Context<Self>) {
        let Some(library) = &mut self.library else {
            return;
        };
        if let Err(error) = library.add_to_playlist(playlist, &[track]) {
            self.error = Some(error.to_string().into());
        }
        self.reload(cx);
    }

    fn add_to_crate(&mut self, track: i64, crate_id: i64, cx: &mut Context<Self>) {
        let Some(library) = &mut self.library else {
            return;
        };
        if let Err(error) = library.add_to_crate(crate_id, &[track]) {
            self.error = Some(error.to_string().into());
        }
        self.reload(cx);
    }

    fn remove_from_crate(&mut self, track: i64, crate_id: i64, cx: &mut Context<Self>) {
        let Some(library) = &self.library else {
            return;
        };
        if let Err(error) = library.remove_from_crate(crate_id, track) {
            self.error = Some(error.to_string().into());
        }
        self.reload(cx);
    }

    fn remove_from_library(&mut self, track: i64, _: i64, cx: &mut Context<Self>) {
        let Some(library) = &mut self.library else {
            return;
        };
        match library.remove_track(track) {
            Ok(()) => {
                self.notice = Some(
                    "Removed from library. Audio file kept; future folder scans will skip it."
                        .into(),
                );
                self.reload(cx);
                cx.emit(LibraryEvent::Changed);
            }
            Err(error) => {
                self.error = Some(error.to_string().into());
                cx.notify();
            }
        }
    }

    fn remove_from_playlist(&mut self, track: i64, playlist: i64, cx: &mut Context<Self>) {
        let Some(library) = &self.library else {
            return;
        };
        if let Err(error) = library.remove_from_playlist(playlist, track) {
            self.error = Some(error.to_string().into());
        }
        self.reload(cx);
    }

    fn queue_track(&mut self, id: i64, _: i64, cx: &mut Context<Self>) {
        if let Some(track) = self
            .table
            .read(cx)
            .delegate()
            .tracks
            .iter()
            .find(|t| t.id == id)
        {
            cx.emit(LibraryEvent::Queue(vec![djcraft_protocol::QueueTrack {
                grid: None,
                path: track.path.to_string_lossy().into_owned(),
                title: track.title.clone(),
            }]));
        }
    }
    pub fn save_automix_playlist(
        &mut self,
        tracks: &[djcraft_protocol::QueueTrack],
        cx: &mut Context<Self>,
    ) -> Result<String, String> {
        let library = self.library.as_mut().ok_or("Library unavailable")?;
        let mut ids = Vec::new();
        for track in tracks {
            let id = library
                .track_at(Path::new(&track.path))
                .map_err(|e| e.to_string())?
                .ok_or("A queued track is no longer in the library")?;
            ids.push(id);
        }
        if ids.is_empty() {
            return Err("Queue is empty".into());
        }
        let name = format!(
            "Automix {}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_secs()
        );
        let playlist = library.create_playlist(&name).map_err(|e| e.to_string())?;
        library
            .add_to_playlist(playlist.id, &ids)
            .map_err(|e| e.to_string())?;
        self.reload(cx);
        cx.emit(LibraryEvent::Changed);
        Ok(name)
    }
    fn queue_view(&mut self, cx: &mut Context<Self>) {
        let tracks = self
            .table
            .read(cx)
            .delegate()
            .tracks
            .iter()
            .map(|t| djcraft_protocol::QueueTrack {
                grid: None,
                path: t.path.to_string_lossy().into_owned(),
                title: t.title.clone(),
            })
            .collect();
        cx.emit(LibraryEvent::Queue(tracks));
    }
    pub fn stem_notice(&mut self, message: String, cx: &mut Context<Self>) {
        self.notice = Some(message.into());
        cx.notify();
    }
    fn separate_stems(&mut self, track: i64, _: i64, cx: &mut Context<Self>) {
        if let Some(track) = self
            .table
            .read(cx)
            .delegate()
            .tracks
            .iter()
            .find(|t| t.id == track)
        {
            cx.emit(LibraryEvent::SeparateStems {
                path: track.path.clone(),
            });
        }
    }
    fn load_to_deck(&mut self, track: i64, deck: i64, cx: &mut Context<Self>) {
        let tracks = &self.table.read(cx).delegate().tracks;
        if let Some(track) = tracks.iter().find(|t| t.id == track) {
            cx.emit(LibraryEvent::Load {
                id: track.id,
                path: track.path.clone(),
                deck: Some(deck as usize),
            });
        }
    }

    fn field(&self, field: EditField) -> &Entity<InputState> {
        &self.fields[field as usize]
    }

    fn field_text(&self, field: EditField, cx: &App) -> String {
        self.field(field).read(cx).value().trim().to_string()
    }

    fn set_field(&self, field: EditField, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let text = text.to_string();
        self.field(field)
            .update(cx, |input, cx| input.set_value(text, window, cx));
    }

    /// Opens a track in the info panel.
    fn inspect(&mut self, track: Track, window: &mut Window, cx: &mut Context<Self>) {
        let bpm = track.bpm.map(format_bpm).unwrap_or_default();
        for (field, text) in [
            (EditField::Title, track.title.as_str()),
            (EditField::Artist, track.artist.as_str()),
            (EditField::Album, track.album.as_str()),
            (EditField::Genre, track.genre.as_str()),
            (EditField::Bpm, bpm.as_str()),
            (EditField::Key, track.key.as_deref().unwrap_or("")),
        ] {
            self.set_field(field, text, window, cx);
        }
        self.artwork = None;
        self.notice = None;
        self.editing = false;

        let (id, path) = (track.id, track.path.clone());
        let cover = self
            .library
            .as_ref()
            .and_then(|library| library.saved_lookup(id).ok().flatten())
            .and_then(|saved| saved.details.artwork_file)
            .and_then(|relative| {
                djcraft_library::artwork::cached_path(&crate::settings::data_dir(), &relative)
            });
        self.inspected = Some(track);
        cx.spawn(async move |this, cx| {
            let artwork = cx
                .background_spawn(async move {
                    cover
                        .and_then(|path| std::fs::read(path).ok())
                        .map(|data| djcraft_library::Artwork {
                            data,
                            mime_type: None,
                        })
                        .or_else(|| read_artwork(&path))
                })
                .await
                .and_then(|artwork| {
                    let format = match artwork.data.as_slice() {
                        [0xff, 0xd8, ..] => ImageFormat::Jpeg,
                        [0x89, b'P', b'N', b'G', ..] => ImageFormat::Png,
                        _ => return None,
                    };
                    Some(Arc::new(Image::from_bytes(format, artwork.data)))
                });
            this.update(cx, |this, cx| {
                if this.inspected.as_ref().map(|track| track.id) == Some(id) {
                    this.artwork = artwork;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Shows what Save would write to the file, for the user to confirm.
    fn save_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(track), Some(library)) = (&self.inspected, &self.library) else {
            return;
        };
        let id = track.id;
        let bpm_text = self.field_text(EditField::Bpm, cx);
        let bpm = match bpm_text.parse::<f64>() {
            Ok(bpm) if bpm > 0.0 => Some(bpm),
            _ if bpm_text.is_empty() => None,
            _ => {
                self.error = Some("BPM must be a number, such as 128 or 127.5.".into());
                cx.notify();
                return;
            }
        };
        let edit = TrackEdit {
            date: None,
            title: self.field_text(EditField::Title, cx),
            artist: self.field_text(EditField::Artist, cx),
            album: self.field_text(EditField::Album, cx),
            genre: self.field_text(EditField::Genre, cx),
            bpm,
            key: Some(self.field_text(EditField::Key, cx)).filter(|key| !key.is_empty()),
            comment: None,
            grouping: None,
        };
        match library.plan_edit(id, &edit) {
            Ok(change) if change.is_empty() => {
                self.editing = false;
                self.notice = Some("Nothing to save: the file already says this.".into());
            }
            Ok(change) => self.review("Edit track details".into(), vec![change], window, cx),
            Err(error) => self.error = Some(error.to_string().into()),
        }
        cx.notify();
    }

    /// The tracks a tag tool works on: those in the list as it is shown.
    fn listed_ids(&self, cx: &App) -> Vec<i64> {
        self.table
            .read(cx)
            .delegate()
            .tracks
            .iter()
            .map(|track| track.id)
            .collect()
    }

    /// Plans a tag tool's changes for the listed tracks and shows them.
    fn run_tool(
        &mut self,
        label: &str,
        plan: impl FnOnce(&Library, &[i64]) -> djcraft_library::Result<Vec<PlannedChange>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ids = self.listed_ids(cx);
        let Some(library) = &self.library else {
            return;
        };
        match plan(library, &ids) {
            Ok(changes) if changes.is_empty() => {
                self.notice = Some(format!("{label}: nothing to change.").into());
            }
            Ok(changes) => self.review(label.to_string(), changes, window, cx),
            Err(error) => self.error = Some(error.to_string().into()),
        }
        cx.notify();
    }

    /// Opens the review of planned changes; confirming it writes them.
    fn review(
        &mut self,
        label: String,
        changes: Vec<PlannedChange>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let count = changes.len();
        let review = cx.new(|_| ReviewView::new(label.clone(), changes));
        cx.subscribe_in(
            &review,
            window,
            |this, _, event: &ReviewEvent, window, cx| {
                if let ReviewEvent::Apply(label, changes) = event {
                    this.write_changes(label.clone(), changes.clone(), window, cx);
                }
                window.close_dialog(cx);
            },
        )
        .detach();
        let title = format!(
            "{label}: review {count} file{}",
            if count == 1 { "" } else { "s" }
        );
        window.open_dialog(cx, move |dialog, _, _| {
            dialog
                .title(title.clone())
                .w(px(760.))
                .child(review.clone())
        });
    }

    /// Writes reviewed changes on a background thread, then shows the
    /// result.
    fn write_changes(
        &mut self,
        label: String,
        changes: Vec<PlannedChange>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let db_path = self.db_path.clone();
        let work =
            cx.background_spawn(
                async move { Library::open(&db_path)?.apply_changes(&label, &changes) },
            );
        self.notice = Some("Writing tags…".into());
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = work.await;
            this.update_in(cx, |this, window, cx| {
                this.written(result, "Wrote", window, cx)
            })
            .ok();
        })
        .detach();
    }

    /// Undoes the most recent batch of tag changes.
    fn undo_last(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(batch) = self
            .library
            .as_ref()
            .and_then(|library| library.last_batch().ok().flatten())
        else {
            self.notice = Some("There is nothing to undo.".into());
            cx.notify();
            return;
        };
        let db_path = self.db_path.clone();
        let work =
            cx.background_spawn(async move { Library::open(&db_path)?.undo_batch(batch.id) });
        cx.spawn_in(window, async move |this, cx| {
            let result = work.await;
            this.update_in(cx, |this, window, cx| {
                this.written(result, "Undid", window, cx)
            })
            .ok();
        })
        .detach();
    }

    /// Reports how writing or undoing went, and shows the new tags.
    fn written(
        &mut self,
        result: djcraft_library::Result<ApplySummary>,
        verb: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let notice = match result {
            Ok(summary) => {
                let files = |count: usize| match count {
                    1 => "1 file".to_string(),
                    count => format!("{count} files"),
                };
                let mut notice = format!("{verb} {}.", files(summary.written));
                if let Some((path, why)) = summary.skipped.first() {
                    let name = path
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    notice.push_str(&format!(
                        " Skipped {}: {name}, because {why}.",
                        files(summary.skipped.len())
                    ));
                }
                self.error = None;
                Some(notice)
            }
            Err(error) => {
                self.error = Some(error.to_string().into());
                None
            }
        };
        self.editing = false;
        self.reload(cx);
        // The info panel shows the track as its file now is.
        if let Some(id) = self.inspected.as_ref().map(|track| track.id) {
            let fresh = self.library.as_ref().and_then(|library| {
                let query = TrackQuery {
                    id: Some(id),
                    ..Default::default()
                };
                library.tracks(&query).ok()?.pop()
            });
            match fresh {
                Some(track) => self.inspect(track, window, cx),
                None => self.inspected = None,
            }
        }
        // Set last, as opening the track again clears it.
        self.notice = notice.map(Into::into);
        cx.emit(LibraryEvent::Changed);
        cx.notify();
    }

    /// Opens a form that sets up a tag tool, and then its review.
    fn open_form<F: EventEmitter<FormEvent> + Render>(
        &mut self,
        title: &'static str,
        form: Entity<F>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.subscribe_in(&form, window, |this, _, event: &FormEvent, window, cx| {
            window.close_dialog(cx);
            match event {
                FormEvent::Replace(cleanup) => {
                    let cleanup = cleanup.clone();
                    this.run_tool(
                        "Find and replace",
                        |library, ids| library.plan_cleanup(ids, &cleanup),
                        window,
                        cx,
                    );
                }
                FormEvent::Rename(template) => {
                    let template = template.clone();
                    this.run_tool(
                        "Rename files",
                        |library, ids| library.plan_rename(ids, &template),
                        window,
                        cx,
                    );
                }
                FormEvent::Cancel => {}
            }
        })
        .detach();
        window.open_dialog(cx, move |dialog, _, _| {
            dialog.title(title).w(px(520.)).child(form.clone())
        });
    }

    /// The Tag tools menu: changes to the tags of every track in the list.
    fn tag_tools_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.weak_entity();
        let undo = self
            .library
            .as_ref()
            .and_then(|library| library.last_batch().ok().flatten())
            .map(|batch| format!("Undo \"{}\" ({} files)", batch.label, batch.files));
        Button::new("tag-tools")
            .outline()
            .icon(IconName::Tags)
            .label("Tools")
            .dropdown_caret(true)
            .dropdown_menu(move |menu, _, _| {
                let item = |label: String, run: fn(&mut LibraryView, &mut Window, &mut Context<LibraryView>)| {
                    let view = view.clone();
                    PopupMenuItem::new(label).on_click(move |_, window, cx| {
                        view.update(cx, |view, cx| run(view, window, cx)).ok();
                    })
                };
                let mut menu = menu
                    .label("Tags of every track in the list")
                    .item(item("Write BPM, key and energy…".into(), |this, window, cx| {
                        let settings = cx
                            .try_global::<crate::settings::Settings>()
                            .cloned()
                            .unwrap_or_default();
                        let options = AlignOptions {
                            bpm: true,
                            key: settings.key_placement,
                            energy: settings.energy_placement,
                        };
                        this.run_tool(
                            "Write BPM, key and energy",
                            |library, ids| library.plan_align(ids, &options),
                            window,
                            cx,
                        );
                    }))
                    .item(item("Find and replace…".into(), |this, window, cx| {
                        let form = cx.new(|cx| ReplaceForm::new(window, cx));
                        this.open_form("Find and replace", form, window, cx);
                    }))
                    .item(item("Remove extra spaces…".into(), |this, window, cx| {
                        this.run_tool(
                            "Remove extra spaces",
                            |library, ids| library.plan_cleanup(ids, &Cleanup::ExtraSpaces),
                            window,
                            cx,
                        );
                    }))
                    .item(item("Fix capitalisation…".into(), |this, window, cx| {
                        this.run_tool(
                            "Fix capitalisation",
                            |library, ids| library.plan_cleanup(ids, &Cleanup::Capitalise),
                            window,
                            cx,
                        );
                    }))
                    .item(item("Rename files from tags…".into(), |this, window, cx| {
                        let form = cx.new(|cx| RenameForm::new(window, cx));
                        this.open_form("Rename files from tags", form, window, cx);
                    }));
                if let Some(undo) = undo.clone() {
                    menu = menu
                        .separator()
                        .item(item(undo, |this, window, cx| this.undo_last(window, cx)));
                }
                menu.separator()
                    .label("Library")
                    .item(item("Find duplicates…".into(), |this, window, cx| {
                        this.find_duplicates(window, cx)
                    }))
                    .item(item("Find missing files in a folder…".into(), |this, window, cx| {
                        this.find_moved(window, cx)
                    }))
            })
    }

    /// Chooses where "Look up" searches.
    pub fn set_provider(&mut self, provider: Provider, cx: &mut Context<Self>) {
        self.provider = provider;
        cx.notify();
    }

    /// Opens the Look up popup for the track in the info panel, searching
    /// with what the text boxes hold now.
    fn open_lookup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let current = [
            self.field_text(EditField::Title, cx),
            self.field_text(EditField::Artist, cx),
            self.field_text(EditField::Album, cx),
            self.field_text(EditField::Genre, cx),
        ];
        let provider = self.provider;
        let Some((id, path)) = self
            .inspected
            .as_ref()
            .map(|track| (track.id, track.path.clone()))
        else {
            return;
        };
        let saved = match self
            .library
            .as_ref()
            .map(|library| library.saved_lookup(id))
            .transpose()
        {
            Ok(saved) => saved.flatten(),
            Err(error) => {
                self.error = Some(error.to_string().into());
                cx.notify();
                return;
            }
        };
        let lookup = cx.new(|cx| LookupView::new(current, provider, path, saved, window, cx));
        cx.subscribe_in(
            &lookup,
            window,
            move |this, _, event: &LookupEvent, window, cx| {
                if let LookupEvent::Apply(values) = event {
                    for (field, value) in values {
                        let field = match field {
                            LookupField::Title => EditField::Title,
                            LookupField::Artist => EditField::Artist,
                            LookupField::Album => EditField::Album,
                            LookupField::Genre => EditField::Genre,
                        };
                        this.set_field(field, value, window, cx);
                    }
                    this.notice = (!values.is_empty())
                        .then(|| "Details copied. Press Save to write them to the file.".into());
                    this.editing = true;
                    cx.notify();
                }
                if let LookupEvent::SaveLibrary(found, details, cover_warning) = event {
                    let Some(library) = this.library.as_mut() else { return; };
                    if let Err(error) = library.save_lookup(id, found, details) {
                        this.error = Some(error.to_string().into()); cx.notify(); return;
                    }
                    let fresh = library.tracks(&TrackQuery { id: Some(id), ..Default::default() }).ok().and_then(|mut tracks| tracks.pop());
                    this.reload(cx);
                    if let Some(track) = fresh { this.inspect(track, window, cx); }
                    let mut notice="Metadata saved to library. Available for local search; music files unchanged.".to_string();
                    if let Some(warning)=cover_warning {notice.push_str(&format!(" Cover art: {warning}"));}
                    this.notice = Some(notice.into());
                    cx.emit(LibraryEvent::Changed);
                    cx.notify();
                }
                window.close_dialog(cx);
            },
        )
        .detach();
        window.open_dialog(cx, move |dialog, _, _| {
            dialog
                .title("Look up track details")
                .w(px(760.))
                .child(lookup.clone())
        });
    }

    /// Lists the other tracks on the inspected track's album.
    fn show_album(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let album = self.field_text(EditField::Album, cx);
        if !album.is_empty() {
            self.search
                .update(cx, |search, cx| search.set_value(album, window, cx));
        }
    }

    fn nudge_first_beat(
        &mut self,
        id: i64,
        fallback: TempoResult,
        seconds: f64,
        cx: &mut Context<Self>,
    ) {
        self.change_library(
            |library| {
                let grid = library.beat_grid(id)?.unwrap_or(fallback);
                library.set_beat_grid_anchor(
                    id,
                    (grid.first_beat_seconds + seconds).max(0.),
                    grid.downbeat,
                )
            },
            cx,
        );
    }

    /// The panel on the right: the selected track's details, editable.
    fn info_panel(&self, track: &Track, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (border, muted, tile) = (theme.border, theme.muted_foreground, theme.secondary);
        let seconds = track.duration_seconds.round() as u64;
        let editing = self.editing;
        let decimals = bpm_decimals(cx);
        let bpm = track
            .bpm
            .map(|bpm| crate::settings::format_bpm(bpm, decimals))
            .unwrap_or_default();
        let grid = self
            .library
            .as_ref()
            .and_then(|library| library.beat_grid(track.id).ok().flatten())
            .or_else(|| {
                track.bpm.map(|bpm| TempoResult {
                    bpm,
                    first_beat_seconds: 0.,
                    downbeat: 0,
                    confidence: 1.,
                })
            });
        let manual_grid = self
            .library
            .as_ref()
            .and_then(|library| library.beat_grid_anchor_is_manual(track.id).ok())
            .unwrap_or(false);
        let track_id = track.id;
        let labelled = |field: EditField| {
            let label = div().text_xs().text_color(muted).child(field.label());
            let column = div().flex().flex_col().gap_1().child(label);
            if editing {
                return column.child(Input::new(self.field(field)));
            }
            let value = match field {
                EditField::Title => track.title.as_str(),
                EditField::Artist => track.artist.as_str(),
                EditField::Album => track.album.as_str(),
                EditField::Genre => track.genre.as_str(),
                EditField::Bpm => bpm.as_str(),
                EditField::Key => track.key.as_deref().unwrap_or(""),
            };
            column.child(if value.is_empty() {
                div().text_color(muted).child("—")
            } else {
                div().child(value.to_string())
            })
        };
        div()
            .id("track-info")
            .flex()
            .flex_col()
            .w(px(300.))
            .gap_3()
            .p_3()
            .border_1()
            .border_color(border)
            .rounded_lg()
            .overflow_y_scroll()
            .child(
                div()
                    .flex()
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .font_weight(FontWeight::BOLD)
                            .child("Track info"),
                    )
                    .child(
                        Button::new("close-info")
                            .ghost()
                            .xsmall()
                            .icon(IconName::X)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.inspected = None;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .w_full()
                    .h(px(180.))
                    .rounded_lg()
                    .bg(tile)
                    .overflow_hidden()
                    .text_sm()
                    .text_color(muted)
                    .map(|this| match &self.artwork {
                        Some(artwork) => this.child(
                            img(artwork.clone())
                                .size_full()
                                .object_fit(ObjectFit::Contain),
                        ),
                        None => this.child("No cover art"),
                    }),
            )
            .child(labelled(EditField::Title))
            .child(labelled(EditField::Artist))
            .child(
                div()
                    .flex()
                    .items_end()
                    .gap_2()
                    .child(div().flex_1().child(labelled(EditField::Album)))
                    .child(
                        Button::new("show-album")
                            .outline()
                            .icon(IconName::Disc3)
                            .tooltip("Show this album's tracks")
                            .on_click(
                                cx.listener(|this, _, window, cx| this.show_album(window, cx)),
                            ),
                    ),
            )
            .child(labelled(EditField::Genre))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(div().flex_1().child(labelled(EditField::Bpm)))
                    .child(div().flex_1().child(labelled(EditField::Key))),
            )
            .when_some(grid, |this, grid| {
                this.child(div().flex().flex_col().gap_1().p_2().rounded_md().bg(tile)
                    .child(div().text_xs().text_color(muted).child(if manual_grid { "Beatgrid · manual" } else { "Beatgrid · analysed" }))
                    .child(format!("First beat {:.3}s", grid.first_beat_seconds))
                    .child(div().flex().gap_1()
                        .child(Button::new("first-beat-back").xsmall().outline().label("−10 ms")
                            .on_click(cx.listener(move |this, _, _, cx| this.nudge_first_beat(track_id, grid, -0.010, cx))))
                        .child(Button::new("first-beat-forward").xsmall().outline().label("+10 ms")
                            .on_click(cx.listener(move |this, _, _, cx| this.nudge_first_beat(track_id, grid, 0.010, cx)))))
                    .child(div().flex().gap_1()
                        .child(Button::new("first-beat-beat-back").xsmall().outline().label("−Beat")
                            .on_click(cx.listener(move |this, _, _, cx| this.nudge_first_beat(track_id, grid, -grid.beat_seconds(), cx))))
                        .child(Button::new("first-beat-beat-forward").xsmall().outline().label("+Beat")
                            .on_click(cx.listener(move |this, _, _, cx| this.nudge_first_beat(track_id, grid, grid.beat_seconds(), cx)))))
                    .child(div().flex().gap_1()
                        .child(Button::new("first-beat-exact").xsmall().outline().label("Set time…")
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.ask_name("First beat (seconds)", &format!("{:.3}", grid.first_beat_seconds), "Set",
                                    move |this, value, cx| match value.parse::<f64>() {
                                        Ok(seconds) => this.change_library(|library| library.set_beat_grid_anchor(track_id, seconds, grid.downbeat), cx),
                                        Err(_) => { this.error = Some("Enter a time in seconds, such as 0.250".into()); cx.notify(); }
                                    }, window, cx);
                            })))
                        .child(Button::new("beatgrid-bar-phase").xsmall().outline()
                            .label(format!("Bar beat {}", grid.downbeat + 1))
                            .dropdown_menu({
                                let view = cx.weak_entity();
                                move |mut menu, _, _| {
                                    for downbeat in 0..4u8 {
                                        let view = view.clone();
                                        menu = menu.item(PopupMenuItem::new(format!("Beat {} starts bar", downbeat + 1))
                                            .checked(grid.downbeat == downbeat).on_click(move |_, _, cx| {
                                                view.update(cx, |this, cx| this.change_library(
                                                    |library| library.set_beat_grid_anchor(track_id, grid.first_beat_seconds, downbeat), cx)).ok();
                                            }));
                                    }
                                    menu
                                }
                            })))
                    .child(Button::new("beatgrid-reset").xsmall().outline().label("Reset to analysed")
                        .disabled(!manual_grid).on_click(cx.listener(move |this, _, _, cx| {
                            this.change_library(|library| library.clear_beat_grid_anchor(track_id), cx);
                        }))))
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .text_xs()
                    .text_color(muted)
                    .child(format!("Length {}:{:02}", seconds / 60, seconds % 60))
                    .child(div().child(track.path.to_string_lossy().into_owned())),
            )
            .when(!editing, |this| {
                this.child(
                    div().flex().gap_2().child(
                        Button::new("edit-info")
                            .outline()
                            .icon(IconName::Pencil)
                            .label("Edit")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.editing = true;
                                cx.notify();
                            })),
                    ).child(Button::new("lookup-library-metadata").outline().icon(IconName::Search)
                        .label("Metadata…")
                        .tooltip("Look up metadata once, or inspect saved library metadata offline")
                        .on_click(cx.listener(|this, _, window, cx| this.open_lookup(window, cx)))),
                )
            })
            .when(editing, |this| {
                this.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .child(
                            Button::new("cancel-edit")
                                .outline()
                                .label("Cancel")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    // Opening the track again discards the edits.
                                    if let Some(track) = this.inspected.clone() {
                                        this.inspect(track, window, cx);
                                    }
                                })),
                        )
                        .child(
                            Button::new("save-info")
                                .primary()
                                .label("Save")
                                .tooltip("Write these details to the file's tags")
                                .on_click(
                                    cx.listener(|this, _, window, cx| this.save_edit(window, cx)),
                                ),
                        )
                        .child(
                            Button::new("look-up")
                                .outline()
                                .icon(IconName::Search)
                                .label("Look up…")
                                .tooltip("Search an online database for this track's details")
                                .on_click(
                                    cx.listener(|this, _, window, cx| this.open_lookup(window, cx)),
                                ),
                        ),
                )
            })
    }

    /// Gives a compact browser a button that pops up the Camelot wheel.
    pub fn show_wheel(&mut self, cx: &mut Context<Self>) {
        self.wheel_in_browser = true;
        cx.notify();
    }

    /// Stops anything this view is playing, for when it goes out of view.
    pub fn pause_players(&mut self, cx: &mut Context<Self>) {
        if let Some(flow) = &self.flow {
            flow.update(cx, |flow, _| flow.pause());
        }
    }

    /// Re-reads the database, after another view may have changed it.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.reload(cx);
    }

    pub fn analysis_progress(&self) -> Arc<crate::analysis::Progress> {
        self.analysis.clone()
    }

    pub fn analysis_status(&self) -> String {
        if let Some(error) = &self.analysis_error {
            format!("Analysis stopped: {error}")
        } else if self.analysing && self.analysis.total.load(AtomicOrdering::Relaxed) == 0 {
            "Checking for tracks to analyse…".into()
        } else {
            self.analysis.status()
        }
    }

    /// The full Library owns analysis, including changes scanned by the browser.
    pub fn refresh_and_analyse(&mut self, cx: &mut Context<Self>) {
        self.reload(cx);
        self.analyse(cx);
    }

    /// Explicitly discover additions/changes on disk, rather than only reloading SQLite.
    fn refresh_folders(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.running_scans > 0 {
            return;
        }
        self.reload(cx);
        let folders: Vec<_> = self
            .sources
            .iter()
            .filter(|source| source.kind == SourceKind::Folder)
            .cloned()
            .collect();
        self.error = None;
        self.scan_counters.done.store(0, AtomicOrdering::Relaxed);
        self.scan_counters.total.store(0, AtomicOrdering::Relaxed);
        if folders.is_empty() {
            self.notice = Some("Add a music folder to refresh its contents.".into());
        } else {
            self.scan_added_folders(folders, window, cx);
        }
        cx.notify();
    }

    fn select(&mut self, source: Option<i64>, cx: &mut Context<Self>) {
        self.section = Section::Library;
        if self.compact {
            self.browser_section = BrowserSection::Folders;
        }
        self.selected = source;
        self.selected_subfolder = None;
        self.group = None;
        self.reload(cx);
    }

    fn select_subfolder(&mut self, source: i64, path: PathBuf, cx: &mut Context<Self>) {
        self.section = Section::Library;
        self.browser_section = BrowserSection::Folders;
        self.selected = Some(source);
        self.selected_subfolder = Some(path);
        self.group = None;
        self.reload(cx);
    }

    pub fn controller_folders(&self) -> bool {
        self.controller_folders
    }

    pub fn controller_back(&mut self, cx: &mut Context<Self>) {
        self.controller_folders = !self.controller_folders;
        self.notice = Some(
            if self.controller_folders {
                "Controller: sources / playlists"
            } else {
                "Controller: tracks"
            }
            .into(),
        );
        cx.notify();
    }

    pub fn controller_browse(&mut self, steps: i32, cx: &mut Context<Self>) {
        if !matches!(self.section, Section::Library | Section::Playlists) {
            self.show_section(Section::Library, cx);
        }
        if self.controller_folders {
            let mut choices = vec![(false, None)];
            choices.extend(self.sources.iter().map(|s| (false, Some(s.id))));
            choices.extend(self.playlists.iter().map(|p| (true, Some(p.id))));
            let current = choices.iter().position(|(playlist, id)| {
                if *playlist {
                    self.section == Section::Playlists && self.playlist == *id
                } else {
                    self.section != Section::Playlists && self.selected == *id
                }
            });
            if let Some(index) = controller_row(current, choices.len(), steps) {
                let (playlist, id) = choices[index];
                self.table.update(cx, |table, cx| table.clear_selection(cx));
                if playlist {
                    self.playlist = id;
                    self.show_section(Section::Playlists, cx);
                } else {
                    self.select(id, cx);
                }
            }
        } else {
            let table = self.table.read(cx);
            let index = controller_row(table.selected_row(), table.delegate().tracks.len(), steps);
            if let Some(index) = index {
                self.table
                    .update(cx, |table, cx| table.set_selected_row(index, cx));
            }
        }
        cx.notify();
    }

    pub fn controller_load(&mut self, deck: usize, cx: &mut Context<Self>) {
        let table = self.table.read(cx);
        if let Some(track) = table
            .selected_row()
            .and_then(|row| table.delegate().tracks.get(row))
        {
            cx.emit(LibraryEvent::Load {
                id: track.id,
                path: track.path.clone(),
                deck: Some(deck),
            });
        } else {
            self.notice = Some("Select a track with Browse before loading.".into());
            cx.notify();
        }
    }

    fn select_group(&mut self, group: Group, cx: &mut Context<Self>) {
        self.section = Section::Library;
        self.selected = None;
        self.group = Some(group);
        self.reload(cx);
    }

    /// Sets the track on the decks that suggestions are for, for the
    /// browser under the decks.
    pub fn set_reference(
        &mut self,
        reference: Option<(i64, SharedString)>,
        cx: &mut Context<Self>,
    ) {
        if self.reference != reference {
            self.reference = reference;
            if self.suggesting {
                self.reload(cx);
            }
            cx.notify();
        }
    }

    /// The library track at a path, with its title, if there is one.
    pub fn deck_artwork_path(&self, path: &Path) -> Option<PathBuf> {
        let library = self.library.as_ref()?;
        let id = library.track_at(path).ok()??;
        let relative = library.saved_lookup(id).ok()??.details.artwork_file?;
        djcraft_library::artwork::cached_path(&crate::settings::data_dir(), &relative)
    }

    pub fn deck_metadata(&self, path: &Path) -> Option<(String, String)> {
        let library = self.library.as_ref()?;
        let id = library.track_at(path).ok()??;
        let track = library
            .tracks(&TrackQuery {
                id: Some(id),
                ..Default::default()
            })
            .ok()?
            .pop()?;
        Some((track.title, track.artist))
    }

    pub fn track_at(&self, path: &Path) -> Option<(i64, SharedString)> {
        let library = self.library.as_ref()?;
        let id = library.track_at(path).ok()??;
        let query = TrackQuery {
            id: Some(id),
            ..Default::default()
        };
        let track = library.tracks(&query).ok()?.pop()?;
        Some((id, track.title.into()))
    }

    /// Shows only the columns not named in `hidden`.
    pub fn set_hidden_columns(&mut self, hidden: &[String], cx: &mut Context<Self>) {
        let visible: Vec<usize> = COLUMNS
            .iter()
            .enumerate()
            .filter(|(_, (_, name, _))| !hidden.iter().any(|h| h == name))
            .map(|(index, _)| index)
            .collect();
        self.table.update(cx, |table, cx| {
            if table.delegate().visible != visible {
                let delegate = table.delegate_mut();
                delegate.visible = visible;
                delegate.sort = None;
                table.refresh(cx);
            }
        });
    }

    /// A menu of columns to show or hide.
    fn columns_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let visible = self.table.read(cx).delegate().visible.clone();
        let view = cx.weak_entity();
        Button::new("columns")
            .ghost()
            .small()
            .icon(IconName::Columns3)
            .tooltip("Columns")
            .dropdown_menu(move |mut menu, _, _| {
                // The title is always shown.
                for (index, (_, name, _)) in COLUMNS.iter().enumerate().skip(1) {
                    let shown = visible.contains(&index);
                    let mut hidden: Vec<String> = COLUMNS
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| !visible.contains(i))
                        .map(|(_, (_, name, _))| name.to_string())
                        .collect();
                    if shown {
                        hidden.push(name.to_string());
                    } else {
                        hidden.retain(|h| h != name);
                    }
                    let view = view.clone();
                    menu = menu.item(PopupMenuItem::new(*name).checked(shown).on_click(
                        move |_, _, cx| {
                            view.update(cx, |_, cx| cx.emit(LibraryEvent::Columns(hidden.clone())))
                                .ok();
                        },
                    ));
                }
                menu
            })
    }

    /// Opens a small dialog that asks for a name, then calls `done` with it.
    fn ask_name(
        &mut self,
        title: &'static str,
        initial: &str,
        action: &'static str,
        done: impl Fn(&mut Self, String, &mut Context<Self>) + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let form = cx.new(|cx| NameForm::new(initial, action, window, cx));
        cx.subscribe_in(
            &form,
            window,
            move |this, _, event: &OrganiseEvent, window, cx| {
                if let OrganiseEvent::Named(name) = event {
                    done(this, name.clone(), cx);
                }
                window.close_dialog(cx);
            },
        )
        .detach();
        window.open_dialog(cx, move |dialog, _, _| {
            dialog.title(title).w(px(420.)).child(form.clone())
        });
    }

    /// Runs a change to the library, showing any error, then reloads.
    fn change_library(
        &mut self,
        change: impl FnOnce(&mut Library) -> djcraft_library::Result<()>,
        cx: &mut Context<Self>,
    ) {
        let Some(library) = &mut self.library else {
            return;
        };
        if let Err(error) = change(library) {
            self.error = Some(error.to_string().into());
        }
        self.reload(cx);
        cx.emit(LibraryEvent::Changed);
    }

    /// Opens the smart playlist editor, for a new one or to change one.
    fn edit_smart(&mut self, playlist: Option<i64>, window: &mut Window, cx: &mut Context<Self>) {
        let existing = playlist
            .and_then(|id| self.playlists.iter().find(|p| p.id == id))
            .cloned();
        let name = existing
            .as_ref()
            .map(|p| p.name.clone())
            .unwrap_or_default();
        let rules = existing.as_ref().and_then(|p| p.rules.clone());
        let form = cx.new(|cx| SmartForm::new(&name, rules.as_ref(), window, cx));
        cx.subscribe_in(
            &form,
            window,
            move |this, _, event: &OrganiseEvent, window, cx| {
                if let OrganiseEvent::Smart(name, rules) = event {
                    let (name, rules) = (name.clone(), rules.clone());
                    let mut created = None;
                    this.change_library(
                        |library| {
                            match playlist {
                                Some(id) => {
                                    library.rename_playlist(id, &name)?;
                                    library.set_playlist_rules(id, &rules)?;
                                }
                                None => {
                                    created = Some(library.create_smart_playlist(&name, &rules)?.id)
                                }
                            }
                            Ok(())
                        },
                        cx,
                    );
                    if let Some(id) = created.or(playlist) {
                        this.open_playlist(id, cx);
                    }
                }
                window.close_dialog(cx);
            },
        )
        .detach();
        let title = if playlist.is_some() {
            "Edit smart playlist"
        } else {
            "New smart playlist"
        };
        window.open_dialog(cx, move |dialog, _, _| {
            dialog.title(title).w(px(680.)).child(form.clone())
        });
    }

    /// Lists duplicate tracks for the user to choose which copy to keep.
    fn find_duplicates(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(library) = &self.library else {
            return;
        };
        let groups = match library.duplicate_groups() {
            Ok(groups) => groups,
            Err(error) => {
                self.error = Some(error.to_string().into());
                cx.notify();
                return;
            }
        };
        let view = cx.new(|_| DuplicatesView::new(groups));
        self.duplicates = Some(view.clone());
        cx.subscribe_in(
            &view,
            window,
            |this, view, event: &OrganiseEvent, window, cx| match event {
                OrganiseEvent::Merge(keep, others) => {
                    let (keep, others) = (*keep, others.clone());
                    this.change_library(|library| library.merge_duplicates(keep, &others), cx);
                    let groups = this
                        .library
                        .as_ref()
                        .and_then(|library| library.duplicate_groups().ok())
                        .unwrap_or_default();
                    view.update(cx, |view, cx| view.set_groups(groups, cx));
                }
                _ => {
                    this.duplicates = None;
                    window.close_dialog(cx);
                }
            },
        )
        .detach();
        window.open_dialog(cx, move |dialog, _, _| {
            dialog.title("Duplicates").w(px(820.)).child(view.clone())
        });
    }

    /// Asks for a folder, looks there for the files of missing tracks, and
    /// offers to relink those it finds.
    fn find_moved(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let folder = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Search here".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(folder))) = folder.await else {
                return;
            };
            let Some(folder) = folder.into_iter().next() else {
                return;
            };
            this.update_in(cx, |this, window, cx| {
                let Some(library) = &this.library else {
                    return;
                };
                match library.find_moved(&folder) {
                    Ok(found) if found.is_empty() => {
                        this.notice = Some("None of the missing files are in that folder.".into());
                        cx.notify();
                    }
                    Ok(found) => this.confirm_relocate(found, window, cx),
                    Err(error) => {
                        this.error = Some(error.to_string().into());
                        cx.notify();
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    fn confirm_relocate(
        &mut self,
        found: Vec<(Track, PathBuf)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.new(|_| RelocateView::new(found));
        cx.subscribe_in(
            &view,
            window,
            |this, _, event: &OrganiseEvent, window, cx| {
                if let OrganiseEvent::Relocate(moves) = event {
                    let moves = moves.clone();
                    let count = moves.len();
                    this.change_library(
                        |library| {
                            for (id, path) in &moves {
                                library.relocate(*id, path)?;
                            }
                            Ok(())
                        },
                        cx,
                    );
                    this.notice = Some(format!("Relinked {count} tracks.").into());
                }
                window.close_dialog(cx);
            },
        )
        .detach();
        window.open_dialog(cx, move |dialog, _, _| {
            dialog
                .title("Missing files found")
                .w(px(760.))
                .child(view.clone())
        });
    }

    /// Asks for the file of one missing track and relinks it.
    fn locate_file(&mut self, track: i64, _: i64, cx: &mut Context<Self>) {
        let file = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Use this file".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(file))) = file.await else {
                return;
            };
            let Some(file) = file.into_iter().next() else {
                return;
            };
            this.update(cx, |this, cx| {
                this.change_library(|library| library.relocate(track, &file), cx);
            })
            .ok();
        })
        .detach();
    }

    /// Takes every missing track out of the library, after asking.
    fn remove_missing(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let count = self
            .groups
            .iter()
            .find(|group| group.group == Group::Missing)
            .map_or(0, |group| group.count);
        let view = cx.weak_entity();
        window.open_dialog(cx, move |dialog, _, _| {
            let view = view.clone();
            dialog
                .title("Remove missing tracks?")
                .w(px(460.))
                .child(div().text_sm().child(format!(
                    "{count} tracks whose files are missing will leave the library, with their places in playlists and their cue points. No file is touched."
                )))
                .footer(
                    div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            Button::new("keep-missing")
                                .outline()
                                .label("Cancel")
                                .on_click(|_, window, cx| window.close_dialog(cx)),
                        )
                        .child(
                            Button::new("remove-missing")
                                .danger()
                                .label(format!("Remove {count} tracks"))
                                .on_click(move |_, window, cx| {
                                    view.update(cx, |view, cx| {
                                        view.change_library(|library| library.remove_missing().map(|_| ()), cx);
                                        view.group = None;
                                        view.reload(cx);
                                    })
                                    .ok();
                                    window.close_dialog(cx);
                                }),
                        ),
                )
        });
    }

    /// Moves a track within the open playlist, to `to`.
    fn move_track(&mut self, track: i64, to: i64, cx: &mut Context<Self>) {
        let Some(playlist) = self.playlist else {
            return;
        };
        self.change_library(
            |library| library.move_in_playlist(playlist, track, to.max(0) as usize),
            cx,
        );
    }

    /// Resolve the destination against the complete playlist, even when search
    /// hides some rows. Moving downward lands after the target; upward before it.
    fn drop_playlist_track(&mut self, track: i64, target: i64, cx: &mut Context<Self>) {
        let Some(playlist) = self.playlist else {
            return;
        };
        self.change_library(
            |library| {
                let tracks = library.tracks(&TrackQuery {
                    playlist: Some(playlist),
                    ..Default::default()
                })?;
                if tracks.iter().any(|row| row.playlist_entry == Some(track))
                    && let Some(to) = tracks
                        .iter()
                        .position(|row| row.playlist_entry == Some(target))
                {
                    library.move_in_playlist(playlist, track, to)?;
                }
                Ok(())
            },
            cx,
        );
    }

    fn open_playlist(&mut self, playlist: i64, cx: &mut Context<Self>) {
        self.section = Section::Playlists;
        if self.compact {
            self.browser_section = BrowserSection::Playlists;
        }
        self.playlist = Some(playlist);
        self.reload(cx);
    }

    /// Asks for playlist files or a folder containing playlists and imports them.
    fn import_playlists(&mut self, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: true,
            multiple: true,
            prompt: Some("Import".into()),
        });
        let db_path = self.db_path.clone();
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            // Reading tags for new tracks can take a while.
            let import = cx.background_spawn(async move {
                let mut playlists = Vec::new();
                for path in &paths {
                    playlists.extend(read_playlists(path)?);
                }
                Library::open(&db_path)?.import_playlists(&playlists)
            });
            let result = import.await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(summary) => {
                        this.error = None;
                        this.notice =
                            Some(import_notice(summary, "not found on this computer").into());
                        this.section = Section::Playlists;
                    }
                    Err(error) => this.error = Some(error.to_string().into()),
                }
                this.reload(cx);
                this.analyse(cx);
                cx.emit(LibraryEvent::Changed);
            })
            .ok();
        })
        .detach();
    }

    fn add_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: true,
            prompt: Some("Add".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            this.update_in(cx, |this, window, cx| {
                let mut added = Vec::new();
                for path in paths {
                    let Some(library) = &this.library else {
                        return;
                    };
                    match library.add_folder(&path) {
                        Ok(source) => {
                            this.watch(&source);
                            added.push(source);
                        }
                        Err(error) => this.error = Some(error.to_string().into()),
                    }
                }
                this.source_folders_dirty = true;
                this.reload(cx);
                if !added.is_empty() {
                    this.scan_added_folders(added, window, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// Scan audio first, then offer any playlists found underneath the roots.
    fn scan_added_folders(
        &mut self,
        sources: Vec<Source>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let db_path = self.db_path.clone();
        let counters = self.scan_counters.clone();
        let scan = cx.background_spawn(async move {
            let mut library = Library::open(&db_path)?;
            let mut playlists = Vec::new();
            let mut playlist_errors = Vec::new();
            for source in sources {
                library.scan(source.id, |progress| {
                    counters.done.store(progress.done, AtomicOrdering::Relaxed);
                    counters
                        .total
                        .store(progress.total, AtomicOrdering::Relaxed);
                })?;
                let (found, errors) = discover_playlists(Path::new(&source.location));
                playlists.extend(found);
                playlist_errors.extend(errors);
            }
            let existing: HashSet<_> = library
                .playlists()?
                .into_iter()
                .map(|playlist| playlist.name)
                .collect();
            playlists.retain(|playlist| !existing.contains(&playlist.name));
            Ok::<_, djcraft_library::Error>((playlists, playlist_errors))
        });
        self.running_scans += 1;
        cx.spawn_in(window, async move |this, cx| {
            let result = scan.await;
            this.update_in(cx, |this, window, cx| {
                this.running_scans -= 1;
                this.source_folders_dirty = true;
                this.reload(cx);
                cx.emit(LibraryEvent::Changed);
                if this.running_scans == 0 {
                    this.analyse(cx);
                }
                match result {
                    Ok((playlists, errors)) if !playlists.is_empty() => {
                        this.offer_playlists(playlists, errors, window, cx);
                    }
                    Ok((_, errors)) if !errors.is_empty() => {
                        this.error = Some(
                            format!("{} playlist file(s) could not be read.", errors.len()).into(),
                        );
                    }
                    Ok(_) => {}
                    Err(error) => this.error = Some(error.to_string().into()),
                }
            })
            .ok();
        })
        .detach();
        if self.running_scans == 1 {
            cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor().timer(SCAN_REFRESH).await;
                    let scanning = this.update(cx, |this, cx| {
                        this.reload(cx);
                        this.running_scans > 0
                    });
                    if !scanning.unwrap_or(false) {
                        break;
                    }
                }
            })
            .detach();
        }
    }

    fn offer_playlists(
        &mut self,
        playlists: Vec<ImportedPlaylist>,
        errors: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let count = playlists.len();
        let picker = cx.new(|_| PlaylistPicker {
            selected: vec![true; count],
            playlists,
            errors,
        });
        cx.subscribe_in(
            &picker,
            window,
            |this, _, event: &PlaylistPickEvent, window, cx| {
                let PlaylistPickEvent::Import(selected) = event;
                this.import_selected_playlists(selected.clone(), cx);
                window.close_dialog(cx);
            },
        )
        .detach();
        window.open_dialog(cx, move |dialog, _, _| {
            dialog
                .title("Playlists found in music folder")
                .w(px(560.))
                .child(picker.clone())
        });
    }

    fn import_selected_playlists(
        &mut self,
        playlists: Vec<ImportedPlaylist>,
        cx: &mut Context<Self>,
    ) {
        let db_path = self.db_path.clone();
        let import = cx.background_spawn(async move {
            Library::open(&db_path)?.link_scanned_playlists(&playlists)
        });
        cx.spawn(async move |this, cx| {
            let result = import.await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(summary) => {
                        this.notice =
                            Some(import_notice(summary, "not in the scanned library").into());
                        this.error = None;
                    }
                    Err(error) => this.error = Some(error.to_string().into()),
                }
                this.reload(cx);
                cx.emit(LibraryEvent::Changed);
            })
            .ok();
        })
        .detach();
    }

    fn remove_source(&mut self, id: i64, cx: &mut Context<Self>) {
        let Some(library) = &self.library else {
            return;
        };
        if let Err(error) = library.remove_source(id) {
            self.error = Some(error.to_string().into());
        }
        if let Some(watcher) = &mut self.watcher {
            watcher.unwatch(id);
        }
        if self.selected == Some(id) {
            self.selected = None;
            self.selected_subfolder = None;
        }
        self.source_folders_dirty = true;
        self.reload(cx);
    }

    /// Scans a source on a background thread, refreshing the list as tracks
    /// come in.
    fn scan(&mut self, source_id: i64, cx: &mut Context<Self>) {
        let db_path = self.db_path.clone();
        let counters = self.scan_counters.clone();
        let scan = cx.background_spawn(async move {
            // The scan gets its own connection; the UI keeps reading on its.
            let mut library = Library::open(&db_path)?;
            library.scan(source_id, |progress| {
                counters.done.store(progress.done, AtomicOrdering::Relaxed);
                counters
                    .total
                    .store(progress.total, AtomicOrdering::Relaxed);
            })
        });
        self.running_scans += 1;
        cx.spawn(async move |this, cx| {
            let result = scan.await;
            this.update(cx, |this, cx| {
                this.running_scans -= 1;
                if let Err(error) = result {
                    this.error = Some(error.to_string().into());
                }
                this.source_folders_dirty = true;
                this.reload(cx);
                cx.emit(LibraryEvent::Changed);
                if let (Section::Flow, Some(flow)) = (this.section, &this.flow) {
                    flow.update(cx, |flow, cx| flow.refresh(cx));
                }
                // Whatever the scan found needs analysing.
                if this.running_scans == 0 {
                    this.analyse(cx);
                }
            })
            .ok();
        })
        .detach();

        if self.running_scans == 1 {
            cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor().timer(SCAN_REFRESH).await;
                    let scanning = this.update(cx, |this, cx| {
                        this.reload(cx);
                        this.running_scans > 0
                    });
                    if !scanning.unwrap_or(false) {
                        break;
                    }
                }
            })
            .detach();
        }
    }

    /// The vertical rail that switches between the library's sections.
    fn rail(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (active, muted, tile) = (theme.foreground, theme.muted_foreground, theme.secondary);
        let item = |section: Section| {
            let selected = self.section == section;
            let (icon, label) = section.icon_and_label();
            div()
                .id(label)
                .relative()
                .flex()
                .flex_col()
                .items_center()
                .gap_1()
                .w_full()
                .py_2()
                .text_xs()
                .text_color(if selected { active } else { muted })
                .when(selected, |this| {
                    this.font_weight(FontWeight::BOLD).child(
                        div()
                            .absolute()
                            .left_0()
                            .top(px(12.))
                            .w(px(3.))
                            .h(px(32.))
                            .rounded_full()
                            .bg(active),
                    )
                })
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(px(40.))
                        .rounded_lg()
                        .when(selected, |this| this.bg(tile))
                        .child(Icon::new(icon).large()),
                )
                .child(label)
                .hover(|style| style.text_color(active))
                .on_click(cx.listener(move |this, _, _, cx| this.show_section(section, cx)))
        };
        div()
            .flex()
            .flex_col()
            .w(px(84.))
            .gap_1()
            .py_2()
            .border_1()
            .border_color(theme.border)
            .rounded_lg()
            .children(Section::ALL.map(item))
    }

    /// The tabs that choose where tracks come from.
    fn origin_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div().flex().child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .p_1()
                .rounded_full()
                .border_1()
                .border_color(cx.theme().border)
                .children(Origin::ALL.map(|origin| {
                    let (icon, label) = origin.icon_and_label();
                    Button::new(label)
                        .ghost()
                        .small()
                        .rounded(ButtonRounded::Size(px(999.)))
                        .icon(icon)
                        .label(label)
                        .selected(self.origin == origin)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.origin = origin;
                            cx.notify();
                        }))
                }))
                .child(
                    Button::new("add-source")
                        .ghost()
                        .small()
                        .rounded(ButtonRounded::Size(px(999.)))
                        .icon(IconName::Plus)
                        .tooltip("Add a source from a plugin")
                        .selected(self.origin == Origin::Plugins)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.origin = Origin::Plugins;
                            cx.notify();
                        })),
                ),
        )
    }

    /// Where source plugins are managed. Nothing can be installed yet: this
    /// is the place the plugin runtime and store will fill in.
    fn plugins(&self, cx: &Context<Self>) -> Div {
        let theme = cx.theme();
        let (border, muted) = (theme.border, theme.muted_foreground);
        let section = |title: &'static str, body: Div| {
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(div().font_weight(FontWeight::BOLD).child(title))
                .child(body)
        };
        let card = |icon: IconName, name: &'static str, status: &'static str| {
            div()
                .flex()
                .items_center()
                .gap_3()
                .p_3()
                .border_1()
                .border_color(border)
                .rounded_lg()
                .child(Icon::new(icon).large())
                .child(div().flex_1().child(name))
                .child(div().text_sm().text_color(muted).child(status))
        };
        div()
            .flex()
            .flex_col()
            .flex_1()
            .max_w(px(720.))
            .gap_6()
            .child(section(
                "Sources",
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .children(Origin::ALL.into_iter().skip(1).map(|origin| {
                        let (icon, name) = origin.icon_and_label();
                        let status = match origin {
                            Origin::Local => "Built in",
                            _ => "Not connected yet",
                        };
                        card(icon, name, status)
                    })),
            ))
            .child(section(
                "Plugin store",
                div()
                    .p_3()
                    .border_1()
                    .border_color(border)
                    .rounded_lg()
                    .text_sm()
                    .text_color(muted)
                    .child("Plugins cannot be installed yet. Sources from the store will be listed here."),
            ))
    }

    fn folders(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (hover, muted) = (theme.secondary, theme.muted_foreground);
        let row = |id: ElementId, icon: IconName, label: SharedString, source: Option<i64>| {
            div()
                .id(id)
                .flex()
                .items_center()
                .gap_2()
                .h(px(32.))
                .px_2()
                .rounded_md()
                .text_sm()
                .when(
                    self.selected == source
                        && self.selected_subfolder.is_none()
                        && self.group.is_none(),
                    |this| this.bg(hover),
                )
                .hover(|style| style.bg(hover))
                .child(Icon::new(icon).small().text_color(muted))
                .child(div().flex_1().min_w_0().truncate().child(label))
                .on_click(cx.listener(move |this, _, _, cx| this.select(source, cx)))
        };
        div()
            .id("folders")
            // A browser's panel is short; let it scroll once the wheel is in.
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .w(px(220.))
            .gap_1()
            .p_2()
            .border_1()
            .border_color(cx.theme().border)
            .rounded_lg()
            .child(row(
                "all-tracks".into(),
                IconName::Library,
                "All tracks".into(),
                None,
            ))
            .children(self.sources.iter().map(|source| {
                let id = source.id;
                let folders: Vec<_> = self
                    .source_subfolders
                    .iter()
                    .filter(|(source_id, _)| *source_id == id)
                    .filter(|(_, path)| {
                        let mut parent = path.parent();
                        while let Some(ancestor) =
                            parent.filter(|path| !path.as_os_str().is_empty())
                        {
                            if !self
                                .expanded_source_folders
                                .contains(&(id, ancestor.to_path_buf()))
                            {
                                return false;
                            }
                            parent = ancestor.parent();
                        }
                        true
                    })
                    .map(|(_, path)| {
                        let path = path.clone();
                        let depth = path.components().count().min(8);
                        let selected = self.selected == Some(id)
                            && self.selected_subfolder.as_ref() == Some(&path);
                        let has_children = self.source_folder_parents.contains(&(id, path.clone()));
                        let expanded = self.expanded_source_folders.contains(&(id, path.clone()));
                        let toggle_path = path.clone();
                        let select_path = path.clone();
                        div()
                            .flex()
                            .items_center()
                            .h(px(30.))
                            .pl(px((depth as f32) * 12.))
                            .when(selected, |row| row.bg(hover))
                            .hover(|style| style.bg(hover))
                            .child(
                                Button::new(format!(
                                    "source-folder-toggle-{id}-{}",
                                    path.display()
                                ))
                                .ghost()
                                .xsmall()
                                .icon(if has_children {
                                    if expanded {
                                        IconName::ChevronDown
                                    } else {
                                        IconName::ChevronRight
                                    }
                                } else {
                                    IconName::Folder
                                })
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        let key = (id, toggle_path.clone());
                                        if has_children {
                                            if !this.expanded_source_folders.insert(key.clone()) {
                                                this.expanded_source_folders.remove(&key);
                                            }
                                            cx.notify();
                                        } else {
                                            this.select_subfolder(id, toggle_path.clone(), cx);
                                        }
                                    },
                                )),
                            )
                            .child(
                                div()
                                    .id(format!("source-folder-row-{id}-{}", path.display()))
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_sm()
                                    .child(
                                        path.file_name()
                                            .unwrap_or_default()
                                            .to_string_lossy()
                                            .to_string(),
                                    )
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.select_subfolder(id, select_path.clone(), cx)
                                    })),
                            )
                    })
                    .collect();
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .child(div().flex_1().min_w_0().child(row(
                                ("source", id as usize).into(),
                                match source.kind {
                                    SourceKind::Folder => IconName::FolderOpen,
                                    SourceKind::Files => IconName::Music,
                                },
                                source.name.clone().into(),
                                Some(id),
                            )))
                            .when(!self.compact, |this| {
                                this.child(
                                    Button::new(("remove-source", id as usize))
                                        .ghost()
                                        .xsmall()
                                        .icon(IconName::Minus)
                                        .tooltip("Remove from library (files are kept)")
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.remove_source(id, cx)
                                        })),
                                )
                            }),
                    )
                    .children(folders)
            }))
            .when(!self.compact && !self.groups.is_empty(), |this| {
                this.child(
                    div()
                        .px_2()
                        .pt_2()
                        .text_xs()
                        .text_color(muted)
                        .child("Groups"),
                )
                .children(self.groups.iter().enumerate().map(|(index, group)| {
                    let chosen = group.group.clone();
                    let selected = self.group.as_ref() == Some(&group.group);
                    let icon = match group.group {
                        Group::Genre(_) => IconName::Tag,
                        Group::Energy(_) => IconName::Zap,
                        Group::Decade(_) => IconName::Calendar,
                        Group::Missing => IconName::FileSearch,
                    };
                    div()
                        .id(("group", index))
                        .flex()
                        .items_center()
                        .gap_2()
                        .h(px(28.))
                        .px_2()
                        .rounded_md()
                        .text_sm()
                        .when(selected, |this| this.bg(hover))
                        .hover(|style| style.bg(hover))
                        .child(Icon::new(icon).small().text_color(muted))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .child(group.group.label()),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .child(group.count.to_string()),
                        )
                        .on_click(
                            cx.listener(move |this, _, _, cx| {
                                this.select_group(chosen.clone(), cx)
                            }),
                        )
                }))
            })
            .when(!self.compact, |this| {
                this.child(div().flex_1())
                    .child(self.wheel(cx))
                    .child(
                        Button::new("import-to-library")
                            .outline()
                            .icon(IconName::Download)
                            .label("Import playlists")
                            .tooltip("From a rekordbox XML export, a playlist folder or M3U files")
                            .on_click(cx.listener(|this, _, _, cx| this.import_playlists(cx))),
                    )
                    .child(
                        Button::new("add-folder")
                            .outline()
                            .icon(IconName::Plus)
                            .label("Add folder")
                            .on_click(
                                cx.listener(|this, _, window, cx| this.add_folder(window, cx)),
                            ),
                    )
            })
    }

    fn playlist_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut rows = Vec::new();
        self.playlist_rows(None, 0, &mut rows, cx);
        div()
            .flex()
            .flex_col()
            .w(px(240.))
            .gap_1()
            .p_2()
            .border_1()
            .border_color(cx.theme().border)
            .rounded_lg()
            .child(
                div()
                    .id("playlist-tree")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .children(rows),
            )
            .child(
                Button::new("new-playlist")
                    .outline()
                    .icon(IconName::Plus)
                    .label("New playlist")
                    .on_click(
                        cx.listener(|this, _, window, cx| this.new_playlist_dialog(window, cx)),
                    ),
            )
            .child(
                Button::new("new-smart-playlist")
                    .outline()
                    .icon(IconName::WandSparkles)
                    .label("New smart playlist")
                    .tooltip("A playlist that fills itself from rules, such as genre and tempo")
                    .on_click(cx.listener(|this, _, window, cx| this.edit_smart(None, window, cx))),
            )
            .child(
                Button::new("new-folder")
                    .outline()
                    .icon(IconName::FolderPlus)
                    .label("New folder")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.ask_name(
                            "New folder",
                            "",
                            "Create",
                            |this, name, cx| {
                                this.change_library(
                                    |library| library.create_folder(&name, None).map(|_| ()),
                                    cx,
                                )
                            },
                            window,
                            cx,
                        )
                    })),
            )
            .child(
                Button::new("import-playlists")
                    .outline()
                    .icon(IconName::Download)
                    .label("Import playlists")
                    .tooltip("From a rekordbox XML export, a playlist folder or M3U files")
                    .on_click(cx.listener(|this, _, _, cx| this.import_playlists(cx))),
            )
            .child(self.export_button(cx))
    }

    fn crate_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut rows = Vec::new();
        self.crate_rows(None, 0, &mut rows, cx);
        div()
            .flex()
            .flex_col()
            .w(px(240.))
            .gap(px(4.))
            .p_2()
            .border_1()
            .border_color(cx.theme().border)
            .rounded_lg()
            .child(
                div()
                    .id("crate-tree")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .children(rows),
            )
            .child(
                Button::new("new-crate")
                    .outline()
                    .icon(IconName::Plus)
                    .label("New crate")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.ask_name(
                            "New crate",
                            "",
                            "Create",
                            |this, name, cx| {
                                let result = this
                                    .library
                                    .as_ref()
                                    .map(|library| library.create_crate(&name, None));
                                match result {
                                    Some(Ok(crate_item)) => this.crate_id = Some(crate_item.id),
                                    Some(Err(error)) => this.error = Some(error.to_string().into()),
                                    None => return,
                                }
                                this.reload(cx);
                            },
                            window,
                            cx,
                        );
                    })),
            )
    }

    fn crate_rows(
        &self,
        parent: Option<i64>,
        depth: usize,
        rows: &mut Vec<AnyElement>,
        cx: &mut Context<Self>,
    ) {
        for crate_item in self.crates.iter().filter(|item| item.parent == parent) {
            let id = crate_item.id;
            let has_children = self.crates.iter().any(|child| child.parent == Some(id));
            let open = !self.collapsed_crates.contains(&id);
            let theme = cx.theme();
            rows.push(
                div()
                    .id(("crate", id as usize))
                    .flex()
                    .items_center()
                    .gap_2()
                    .h(px(30.))
                    .pl(px(8. + depth as f32 * 14.))
                    .pr_1()
                    .rounded_md()
                    .text_sm()
                    .when(self.crate_id == Some(id), |row| row.bg(theme.secondary))
                    .hover(|style| style.bg(theme.secondary))
                    .drag_over::<DraggedTrack>(|style, _, _, _| style.bg(rgb(0x183849)))
                    .on_drop(cx.listener(move |this, dragged: &DraggedTrack, _, cx| {
                        this.add_to_crate(dragged.id, id, cx);
                        cx.stop_propagation();
                    }))
                    .when(has_children, |row| {
                        row.child(
                            Button::new(("expand-crate", id as usize))
                                .ghost()
                                .xsmall()
                                .icon(if open {
                                    IconName::ChevronDown
                                } else {
                                    IconName::ChevronRight
                                })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if !this.collapsed_crates.remove(&id) {
                                        this.collapsed_crates.insert(id);
                                    }
                                    cx.stop_propagation();
                                    cx.notify();
                                })),
                        )
                    })
                    .child(
                        Icon::new(IconName::Box)
                            .small()
                            .text_color(theme.muted_foreground),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(crate_item.name.clone()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(crate_item.track_count.to_string()),
                    )
                    .child(self.crate_menu(crate_item, cx))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.crate_id = Some(id);
                        this.reload(cx);
                    }))
                    .into_any_element(),
            );
            if has_children && open {
                self.crate_rows(Some(id), depth + 1, rows, cx);
            }
        }
    }

    fn crate_menu(&self, crate_item: &Crate, cx: &mut Context<Self>) -> impl IntoElement {
        let id = crate_item.id;
        let name = crate_item.name.clone();
        let current = crate_item.parent;
        let mut parents = vec![(None, "Top level".to_string())];
        for possible in &self.crates {
            let mut at = Some(possible.id);
            let mut inside = false;
            while let Some(candidate) = at {
                if candidate == id {
                    inside = true;
                    break;
                }
                at = self
                    .crates
                    .iter()
                    .find(|item| item.id == candidate)
                    .and_then(|item| item.parent);
            }
            if !inside {
                parents.push((Some(possible.id), possible.name.clone()));
            }
        }
        let view = cx.weak_entity();
        Button::new(("crate-menu", id as usize))
            .ghost()
            .xsmall()
            .icon(IconName::Ellipsis)
            .dropdown_menu(move |mut menu, _, _| {
                let act = |label: &str, run: MenuAction| {
                    let view = view.clone();
                    PopupMenuItem::new(label.to_string()).on_click(move |_, window, cx| {
                        view.update(cx, |view, cx| run(view, window, cx)).ok();
                    })
                };
                menu = menu.item(act(
                    "New sub-crate…",
                    Box::new(move |this, window, cx| {
                        this.ask_name(
                            "New sub-crate",
                            "",
                            "Create",
                            move |this, name, cx| {
                                let result = this
                                    .library
                                    .as_ref()
                                    .map(|library| library.create_crate(&name, Some(id)));
                                match result {
                                    Some(Ok(crate_item)) => {
                                        this.crate_id = Some(crate_item.id);
                                        this.collapsed_crates.remove(&id);
                                    }
                                    Some(Err(error)) => this.error = Some(error.to_string().into()),
                                    None => return,
                                }
                                this.reload(cx);
                            },
                            window,
                            cx,
                        );
                    }),
                ));
                let name = name.clone();
                menu = menu
                    .item(act(
                        "Rename…",
                        Box::new(move |this, window, cx| {
                            this.ask_name(
                                "Rename crate",
                                &name,
                                "Rename",
                                move |this, name, cx| {
                                    this.change_library(
                                        |library| library.rename_crate(id, &name),
                                        cx,
                                    );
                                },
                                window,
                                cx,
                            );
                        }),
                    ))
                    .separator()
                    .label("Move to");
                for (parent, label) in parents.clone() {
                    menu = menu.item(
                        act(
                            &label,
                            Box::new(move |this, _, cx| {
                                this.change_library(|library| library.move_crate(id, parent), cx);
                            }),
                        )
                        .checked(parent == current),
                    );
                }
                menu.separator().item(act(
                    "Delete crate",
                    Box::new(move |this, _, cx| {
                        this.change_library(|library| library.delete_crate(id), cx);
                        if this.crate_id == Some(id) {
                            this.crate_id = None;
                            this.reload(cx);
                        }
                    }),
                ))
            })
    }

    /// The folders and playlists in `folder` (the top level for `None`),
    /// indented by `depth`, with open folders' contents below them.
    fn playlist_rows(
        &self,
        folder: Option<i64>,
        depth: usize,
        rows: &mut Vec<AnyElement>,
        cx: &mut Context<Self>,
    ) {
        let theme = cx.theme();
        let (hover, muted) = (theme.secondary, theme.muted_foreground);
        let indent = px(8. + depth as f32 * 14.);
        for sub in self.folders.iter().filter(|f| f.parent == folder) {
            let id = sub.id;
            let open = !self.collapsed.contains(&id);
            rows.push(
                div()
                    .id(("folder", id as usize))
                    .flex()
                    .items_center()
                    .gap_2()
                    .h(px(30.))
                    .pl(indent)
                    .pr_1()
                    .rounded_md()
                    .text_sm()
                    .hover(|style| style.bg(hover))
                    .child(
                        Icon::new(if open {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronRight
                        })
                        .xsmall()
                        .text_color(muted),
                    )
                    .child(Icon::new(IconName::Folder).small().text_color(muted))
                    .child(div().flex_1().min_w_0().truncate().child(sub.name.clone()))
                    .child(self.folder_menu(sub, cx))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !this.collapsed.remove(&id) {
                            this.collapsed.insert(id);
                        }
                        cx.notify();
                    }))
                    .into_any_element(),
            );
            if open {
                self.playlist_rows(Some(id), depth + 1, rows, cx);
            }
        }
        self.playlist_children(folder, None, depth, rows, cx);
    }

    fn playlist_children(
        &self,
        folder: Option<i64>,
        parent: Option<i64>,
        depth: usize,
        rows: &mut Vec<AnyElement>,
        cx: &mut Context<Self>,
    ) {
        let theme = cx.theme();
        let (hover, muted) = (theme.secondary, theme.muted_foreground);
        let indent = px(8. + depth as f32 * 14.);
        for playlist in self
            .playlists
            .iter()
            .filter(|p| parent.is_none() && p.folder == folder)
        {
            let has_children = false;
            let open = false;
            let id = playlist.id;
            let theme = cx.theme();
            rows.push(
                div()
                    .id(("playlist", id as usize))
                    .drag_over::<DraggedTrack>(|s, _, _, _| s.bg(rgb(0x183849)))
                    .on_drop(cx.listener(move |this, track: &DraggedTrack, _, cx| {
                        this.add_to_playlist(track.id, id, cx);
                        cx.stop_propagation();
                    }))
                    .flex()
                    .items_center()
                    .gap_2()
                    .h(px(30.))
                    .pl(indent)
                    .pr_1()
                    .rounded_md()
                    .text_sm()
                    .when(
                        self.section == Section::Playlists && self.playlist == Some(id),
                        |this| this.bg(theme.secondary),
                    )
                    .hover(|style| style.bg(hover))
                    .when(has_children, |row| {
                        row.child(
                            Button::new(("expand-playlist", id as usize))
                                .ghost()
                                .xsmall()
                                .icon(if open {
                                    IconName::ChevronDown
                                } else {
                                    IconName::ChevronRight
                                })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if !this.collapsed_playlists.remove(&id) {
                                        this.collapsed_playlists.insert(id);
                                    }
                                    cx.stop_propagation();
                                    cx.notify();
                                })),
                        )
                    })
                    .child(
                        Icon::new(if playlist.rules.is_some() {
                            IconName::WandSparkles
                        } else {
                            IconName::ListMusic
                        })
                        .small()
                        .text_color(muted),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(playlist.name.clone()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child(playlist.track_count.to_string()),
                    )
                    .child(self.playlist_menu(playlist, cx))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_playlist(id, cx);
                    }))
                    .into_any_element(),
            );
            if has_children && open {
                self.playlist_children(folder, Some(id), depth + 1, rows, cx);
            }
        }
    }

    /// Menu entries that move something into each folder (or the top level),
    /// leaving out `except` and its subfolders.
    fn folder_choices(&self, except: Option<i64>) -> Vec<(Option<i64>, String)> {
        let inside = |mut at: Option<i64>| {
            while let Some(id) = at {
                if Some(id) == except {
                    return true;
                }
                at = self
                    .folders
                    .iter()
                    .find(|f| f.id == id)
                    .and_then(|f| f.parent);
            }
            false
        };
        std::iter::once((None, "Top level".to_string()))
            .chain(
                self.folders
                    .iter()
                    .filter(|f| !inside(Some(f.id)))
                    .map(|f| (Some(f.id), f.name.clone())),
            )
            .collect()
    }

    fn playlist_menu(&self, playlist: &Playlist, cx: &mut Context<Self>) -> impl IntoElement {
        let (id, name, smart) = (playlist.id, playlist.name.clone(), playlist.rules.is_some());
        let view = cx.weak_entity();
        let folders = self.folder_choices(None);
        let current = playlist.folder;
        let nested = playlist.parent.is_some();
        Button::new(("playlist-menu", id as usize))
            .ghost()
            .xsmall()
            .icon(IconName::Ellipsis)
            .dropdown_menu(move |mut menu, _, _| {
                let act = |label: &str, run: MenuAction| {
                    let view = view.clone();
                    PopupMenuItem::new(label.to_string()).on_click(move |_, window, cx| {
                        view.update(cx, |view, cx| run(view, window, cx)).ok();
                    })
                };
                let name = name.clone();
                menu = menu.item(act(
                    "Rename…",
                    Box::new(move |this, window, cx| {
                        this.ask_name(
                            "Rename playlist",
                            &name,
                            "Rename",
                            move |this, name, cx| {
                                this.change_library(
                                    |library| library.rename_playlist(id, &name),
                                    cx,
                                )
                            },
                            window,
                            cx,
                        )
                    }),
                ));
                if smart {
                    menu = menu.item(act(
                        "Edit rules…",
                        Box::new(move |this, window, cx| this.edit_smart(Some(id), window, cx)),
                    ));
                }
                menu = menu.separator().label("Move to");
                for (folder, label) in folders.clone() {
                    menu = menu.item(
                        act(
                            &label,
                            Box::new(move |this, _, cx| {
                                this.change_library(|library| library.move_playlist(id, folder), cx)
                            }),
                        )
                        .checked(folder == current && !nested),
                    );
                }
                menu.separator().item(act(
                    "Delete playlist (keeps sublists)",
                    Box::new(move |this, _, cx| this.delete_playlist(id, cx)),
                ))
            })
    }

    fn folder_menu(&self, folder: &PlaylistFolder, cx: &mut Context<Self>) -> impl IntoElement {
        let (id, name, current) = (folder.id, folder.name.clone(), folder.parent);
        let view = cx.weak_entity();
        let folders = self.folder_choices(Some(id));
        Button::new(("folder-menu", id as usize))
            .ghost()
            .xsmall()
            .icon(IconName::Ellipsis)
            .dropdown_menu(move |mut menu, _, _| {
                let act = |label: &str, run: MenuAction| {
                    let view = view.clone();
                    PopupMenuItem::new(label.to_string()).on_click(move |_, window, cx| {
                        view.update(cx, |view, cx| run(view, window, cx)).ok();
                    })
                };
                let name = name.clone();
                menu = menu
                    .item(act(
                        "New folder inside…",
                        Box::new(move |this, window, cx| {
                            this.ask_name(
                                "New folder",
                                "",
                                "Create",
                                move |this, name, cx| {
                                    this.collapsed.remove(&id);
                                    this.change_library(
                                        |library| {
                                            library.create_folder(&name, Some(id)).map(|_| ())
                                        },
                                        cx,
                                    )
                                },
                                window,
                                cx,
                            )
                        }),
                    ))
                    .item(act(
                        "Rename…",
                        Box::new(move |this, window, cx| {
                            this.ask_name(
                                "Rename folder",
                                &name,
                                "Rename",
                                move |this, name, cx| {
                                    this.change_library(
                                        |library| library.rename_folder(id, &name),
                                        cx,
                                    )
                                },
                                window,
                                cx,
                            )
                        }),
                    ))
                    .separator()
                    .label("Move to");
                for (parent, label) in folders.clone() {
                    menu = menu.item(
                        act(
                            &label,
                            Box::new(move |this, _, cx| {
                                this.change_library(|library| library.move_folder(id, parent), cx)
                            }),
                        )
                        .checked(parent == current),
                    );
                }
                menu.separator().item(act(
                    "Delete folder (keeps its playlists)",
                    Box::new(move |this, _, cx| {
                        this.change_library(|library| library.delete_folder(id), cx)
                    }),
                ))
            })
    }

    /// Asks for a name and creates a playlist with it.
    fn new_playlist_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let input = self.new_playlist.clone();
        input.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
        let view = cx.weak_entity();
        window.open_dialog(cx, move |dialog, _, _| {
            let view = view.clone();
            dialog
                .title("New playlist")
                .w(px(420.))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(div().text_sm().child("Name"))
                        .child(Input::new(&input)),
                )
                .footer(
                    div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            Button::new("cancel-new-playlist")
                                .outline()
                                .label("Cancel")
                                .on_click(|_, window, cx| window.close_dialog(cx)),
                        )
                        .child(
                            Button::new("create-new-playlist")
                                .primary()
                                .label("Create")
                                .on_click(move |_, window, cx| {
                                    let created = view
                                        .update(cx, |view, cx| view.create_named_playlist(cx))
                                        .unwrap_or(false);
                                    // An empty name keeps the dialog open.
                                    if created {
                                        window.close_dialog(cx);
                                    }
                                }),
                        ),
                )
        });
    }

    /// Creates a playlist named by the dialog's text box. Returns false, and
    /// does nothing, if no name has been typed.
    fn create_named_playlist(&mut self, cx: &mut Context<Self>) -> bool {
        let name = self.new_playlist.read(cx).value().trim().to_string();
        if name.is_empty() {
            return false;
        }
        self.create_playlist(&name, cx);
        true
    }

    /// A menu of file formats to save the open playlist in.
    fn export_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.weak_entity();
        Button::new("export-playlist")
            .outline()
            .icon(IconName::Upload)
            .label("Export playlist")
            .dropdown_caret(true)
            .disabled(self.playlist.is_none())
            .dropdown_menu(move |menu, _, _| {
                let item = |label: &'static str, extension: &'static str| {
                    let view = view.clone();
                    PopupMenuItem::new(label).on_click(move |_, _, cx| {
                        view.update(cx, |view, cx| view.export_playlist(extension, cx))
                            .ok();
                    })
                };
                menu.item(item("M3U playlist (most DJ software)", "m3u8"))
                    .item(item("rekordbox XML", "xml"))
            })
    }

    /// Asks where to save, and writes the open playlist there.
    fn export_playlist(&mut self, extension: &'static str, cx: &mut Context<Self>) {
        let Some(playlist) = self.playlist else {
            return;
        };
        let name = self
            .playlists
            .iter()
            .find(|candidate| candidate.id == playlist)
            .map(|found| found.name.clone())
            .unwrap_or_else(|| "Playlist".into());
        // Slashes would be read as folders in a file name.
        let suggested = format!("{}.{extension}", name.replace(['/', ':'], "-"));
        let folder = dirs::document_dir()
            .or_else(dirs::home_dir)
            .unwrap_or_default();
        let chosen = cx.prompt_for_new_path(&folder, Some(&suggested));
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(mut file))) = chosen.await else {
                return;
            };
            // The format follows the extension, so make sure it is there.
            if file.extension().and_then(|e| e.to_str()) != Some(extension) {
                file.set_extension(extension);
            }
            this.update(cx, |this, cx| {
                let Some(library) = &this.library else {
                    return;
                };
                match library.export_playlist(playlist, &file) {
                    Ok(count) => {
                        let tracks = if count == 1 { "track" } else { "tracks" };
                        this.notice = Some(
                            format!("Exported {count} {tracks} to {}.", file.display()).into(),
                        );
                    }
                    Err(error) => this.error = Some(error.to_string().into()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The Camelot wheel: major keys on the outer ring, minor on the inner.
    /// Picking a key lights up the keys that mix with it and narrows the
    /// track list to them.
    fn wheel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        const SIZE: f32 = WHEEL_SIZE;
        const LABEL: f32 = WHEEL_LABEL;

        let muted = cx.theme().muted_foreground;
        let picked = self.wheel_key;
        let lit = picked.map(|key| key.compatible());
        let segments = wheel_segments(lit);

        let labels = Camelot::all().map(|key| {
            let (left, top) = wheel_label_corner(key);
            let dimmed = lit.is_some_and(|lit| !lit.contains(&key));
            let count = self.key_counts.get(&key).copied().unwrap_or(0);
            div()
                .id(("wheel-key", key.number as usize * 2 + key.major as usize))
                .absolute()
                .left(left)
                .top(top)
                .size(px(LABEL))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(10.))
                .font_weight(FontWeight::BOLD)
                .text_color(gpui_kit::white().opacity(if dimmed { 0.45 } else { 1. }))
                .child(key.to_string())
                .tooltip(move |window, cx| {
                    let tracks = if count == 1 { "track" } else { "tracks" };
                    Tooltip::new(format!("{} · {count} {tracks}", key.name())).build(window, cx)
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.wheel_key = (this.wheel_key != Some(key)).then_some(key);
                    this.reload(cx);
                }))
        });

        let hub = div()
            .absolute()
            .left(px(SIZE / 2. - 40.))
            .top(px(SIZE / 2. - 20.))
            .w(px(80.))
            .h(px(40.))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .map(|this| match picked {
                Some(key) => this
                    .child(div().font_weight(FontWeight::BOLD).child(key.to_string()))
                    .child(div().text_size(px(10.)).text_color(muted).child(key.name())),
                None => this.child(div().text_xs().text_color(muted).child("Any key")),
            });

        div().flex().justify_center().py_2().child(
            div()
                .relative()
                .flex_none()
                .size(px(SIZE))
                .child(segments)
                .children(labels)
                .child(hub),
        )
    }

    /// Local music: folders on the left, their tracks on the right.
    fn local(&self, cx: &mut Context<Self>) -> Div {
        let empty = (self.sources.is_empty() && self.library.is_some())
            .then_some("Add a folder to start your library.");
        div()
            .flex()
            .flex_1()
            .min_h_0()
            .gap_3()
            .child(self.folders(cx))
            .child(self.track_pane(empty, cx))
            .when_some(self.inspected.clone(), |this, track| {
                this.child(self.info_panel(&track, cx))
            })
    }

    /// Playlists on the left, the open playlist's tracks on the right.
    fn playlists(&self, cx: &mut Context<Self>) -> Div {
        let empty = if self.playlists.is_empty() {
            Some("Type a name in the box on the left and press Enter to create a playlist.")
        } else if self.table.read(cx).delegate().tracks.is_empty()
            && self.search.read(cx).value().is_empty()
        {
            Some("Right-click a track in the library to add it to this playlist.")
        } else {
            None
        };
        div()
            .flex()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .gap_3()
            .child(self.playlist_list(cx))
            .child(self.track_pane(empty, cx))
            .when_some(self.inspected.clone(), |this, track| {
                this.child(self.info_panel(&track, cx))
            })
    }

    /// A button showing the key picked on the Camelot wheel, which opens
    /// the wheel in a panel floating below it.
    fn wheel_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (border, background) = (theme.border, theme.background);
        let label: SharedString = match self.wheel_key {
            Some(key) => key.to_string().into(),
            None => "Key".into(),
        };
        div()
            .relative()
            .child(
                Button::new("open-wheel")
                    .outline()
                    .small()
                    .icon(IconName::CircleDashed)
                    .label(label)
                    .tooltip("Show tracks that mix with a key")
                    .selected(self.wheel_open || self.wheel_key.is_some())
                    .on_click(cx.listener(|this, _, _, cx| {
                        // A click on this button while the wheel is open has
                        // already closed it, as a click outside the wheel;
                        // it must not open it again.
                        let just_dismissed = this
                            .wheel_dismissed
                            .take()
                            .is_some_and(|at| at.elapsed() < Duration::from_millis(400));
                        this.wheel_open = !just_dismissed;
                        cx.notify();
                    })),
            )
            .when(self.wheel_open, |this| {
                // Painted last, so it floats over the track list.
                this.child(deferred(
                    div()
                        .id("wheel-popup")
                        .absolute()
                        .top(px(36.))
                        .right_0()
                        .occlude()
                        .p_2()
                        .rounded_lg()
                        .border_1()
                        .border_color(border)
                        .bg(background)
                        .shadow_lg()
                        .child(self.wheel(cx))
                        .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                            this.wheel_open = false;
                            this.wheel_dismissed = Some(Instant::now());
                            cx.notify();
                        })),
                ))
            })
    }

    /// The search box, status line and track table, or `empty` in place of
    /// the table when there is nothing to list.
    fn track_pane(&self, empty: Option<&'static str>, cx: &mut Context<Self>) -> Div {
        let count = self.table.read(cx).delegate().tracks.len();
        let total = self.scan_counters.total.load(AtomicOrdering::Relaxed);
        let status = if self.running_scans > 0 && total == 0 {
            // Still listing the folder; nothing to count yet.
            "Scanning…".into()
        } else if self.running_scans > 0 {
            let done = self.scan_counters.done.load(AtomicOrdering::Relaxed);
            format!("Scanning {done} of {total}…")
        } else if self.analysing {
            let done = self.analysis.done.load(AtomicOrdering::Relaxed);
            let total = self.analysis.total.load(AtomicOrdering::Relaxed);
            format!("{count} tracks · analysing {done} of {total}…")
        } else if count == 1 {
            "1 track".into()
        } else {
            format!("{count} tracks")
        };

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .w(px(320.))
                            .child(Input::new(&self.search).cleanable(true)),
                    )
                    .child(Button::new("queue-view").small().outline().label("Queue all").tooltip("Append the displayed tracks to Automix in this order").on_click(cx.listener(|this,_,_,cx|this.queue_view(cx))))
                    .child(Button::new("refresh-library").small().outline()
                        .label(if self.running_scans > 0 { "Refreshing…" } else { "Refresh" })
                        .tooltip("Rescan all added music folders for new and changed tracks.")
                        .disabled(self.running_scans > 0 || self.library.is_none())
                        .on_click(cx.listener(|this, _, window, cx| this.refresh_folders(window, cx))))
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(status),
                    )
                    .when(self.group == Some(Group::Missing) && !self.compact, |this| {
                        this.child(
                            Button::new("find-moved")
                                .outline()
                                .small()
                                .icon(IconName::FileSearch)
                                .label("Find in a folder…")
                                .tooltip("Look for the missing files in a folder and relink them")
                                .on_click(cx.listener(|this, _, window, cx| this.find_moved(window, cx))),
                        )
                        .child(
                            Button::new("remove-missing-tracks")
                                .outline()
                                .small()
                                .icon(IconName::Trash)
                                .label("Remove all")
                                .on_click(
                                    cx.listener(|this, _, window, cx| this.remove_missing(window, cx)),
                                ),
                        )
                    })
                    .when_some(
                        self.reference.clone().filter(|_| self.compact),
                        |this, (_, title)| {
                            this.child(
                                Button::new("suggest-next")
                                    .outline()
                                    .small()
                                    .icon(IconName::Sparkles)
                                    .label(if self.suggesting {
                                        format!("Mixes with {title}")
                                    } else {
                                        "Suggest next".to_string()
                                    })
                                    .tooltip("Tracks whose key and tempo mix with the track on the decks")
                                    .selected(self.suggesting)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.suggesting = !this.suggesting;
                                        this.reload(cx);
                                    })),
                            )
                        },
                    )
                    .child(self.columns_button(cx))
                    .when(!self.compact, |this| this.child(self.tag_tools_button(cx)))
                    .when(self.wheel_in_browser, |this| {
                        this.child(self.wheel_button(cx))
                    }),
            )
            .child(match empty {
                Some(text) => self.placeholder(text, cx),
                None => div()
                    .flex_1()
                    .min_h_0()
                    .child(DataTable::new(&self.table).stripe(true)),
            })
    }

    fn placeholder(&self, text: &'static str, cx: &Context<Self>) -> Div {
        div()
            .flex()
            .flex_1()
            .items_center()
            .justify_center()
            .text_color(cx.theme().muted_foreground)
            .child(text)
    }
}

fn import_notice(summary: ImportSummary, missing_reason: &str) -> String {
    let plural = |count: usize, word: &str| {
        if count == 1 {
            format!("1 {word}")
        } else {
            format!("{count} {word}s")
        }
    };
    let mut notice = format!(
        "Imported {} ({} added).",
        plural(summary.playlists, "playlist"),
        plural(summary.tracks, "track"),
    );
    if summary.missing > 0 {
        notice.push_str(&format!(
            " {} {missing_reason}.",
            plural(summary.missing, "file")
        ));
    }
    notice
}

impl EventEmitter<LibraryEvent> for LibraryView {}

impl Render for LibraryView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Problems and results are shown as toasts, once each.
        let (error, notice) = (self.error.take(), self.notice.take());
        if error.is_some() || notice.is_some() {
            window.defer(cx, move |window, cx| {
                if let Some(error) = error {
                    window.push_notification(Notification::error(error), cx);
                }
                if let Some(notice) = notice {
                    window.push_notification(Notification::info(notice), cx);
                }
            });
        }
        if self.compact {
            return div()
                .flex()
                .size_full()
                .gap(px(4.))
                .child(self.browser_rail(cx))
                .child(match self.browser_section {
                    BrowserSection::Folders => div()
                        .flex()
                        .flex_1()
                        .min_w_0()
                        .min_h_0()
                        .gap(px(4.))
                        .child(self.folders(cx))
                        .child(self.track_pane(None, cx)),
                    BrowserSection::Playlists => div()
                        .flex()
                        .flex_1()
                        .min_w_0()
                        .min_h_0()
                        .gap(px(4.))
                        .child(self.playlist_list(cx))
                        .child(self.track_pane(None, cx)),
                    BrowserSection::Crates => div()
                        .flex()
                        .flex_1()
                        .min_w_0()
                        .min_h_0()
                        .gap(px(4.))
                        .child(self.crate_list(cx))
                        .child(self.track_pane(None, cx)),
                    BrowserSection::OnlineSources => {
                        self.placeholder("Online sources are not connected yet.", cx)
                    }
                });
        }
        let content = match self.section {
            Section::Library => div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .gap_3()
                .child(self.origin_tabs(cx))
                .child(match self.origin {
                    // Local folders are the only connected source so far, so
                    // "All" and "Local" list the same tracks.
                    Origin::All | Origin::Local => self.local(cx),
                    Origin::AppleMusic => self.placeholder("Apple Music is not connected yet.", cx),
                    Origin::Spotify => self.placeholder("Spotify is not connected yet.", cx),
                    Origin::Plugins => self.plugins(cx),
                }),
            Section::Playlists => self.playlists(cx),
            Section::Flow => match &self.flow {
                Some(flow) => div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .child(flow.clone()),
                None => self.placeholder("Flow is not available here.", cx),
            },
            Section::History => self.history_panel(cx),
        };
        div()
            .flex()
            .size_full()
            .gap_3()
            .child(self.rail(cx))
            .child(content)
    }
}
#[cfg(test)]
mod controller_browse_tests {
    use super::controller_row;
    #[test]
    fn navigation_handles_empty_filtered_lists_direction_and_bounds() {
        assert_eq!(controller_row(None, 0, 1), None);
        assert_eq!(controller_row(Some(5), 0, -1), None);
        assert_eq!(controller_row(None, 5, 1), Some(0));
        assert_eq!(controller_row(None, 5, -1), Some(4));
        assert_eq!(controller_row(Some(2), 5, 2), Some(4));
        assert_eq!(controller_row(Some(2), 5, -2), Some(0));
        assert_eq!(controller_row(Some(0), 5, i32::MIN), Some(0));
        assert_eq!(controller_row(Some(4), 5, i32::MAX), Some(4));
        assert_eq!(controller_row(Some(99), 2, 1), Some(0));
        assert_eq!(controller_row(Some(0), 2, 0), Some(0));
    }
}
