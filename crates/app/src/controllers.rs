//! MIDI mapping host. One unambiguous matching device connects automatically.
use djcraft_controllers::{
    Bridge, Profile,
    native::{Input, Output, input_ports},
};
use djcraft_protocol::{Action, SessionState};

/// A controller can change a lamp locally on either edge of a button. Forget
/// only those lamps so the next feedback pass repairs them once per batch.
/// No MIDI output is sent while draining input packets.
fn invalidate_feedback(
    profile: &Profile,
    port: &str,
    source: djcraft_controllers::Source,
    previous: &mut Vec<[u8; 3]>,
) {
    use djcraft_controllers::{Binding, Source};
    let mut forget = |source| {
        if let Source::Note { channel, number } = source {
            previous.retain(|message| message[..2] != [0x90 | channel, number]);
        }
    };
    forget(source);
    for mapping in &profile.mappings {
        if mapping.port != port {
            continue;
        }
        match &mapping.binding {
            Binding::Jog { config } if config.scratch_toggle == Some(source) => {
                if let Some(led) = config.scratch_led {
                    forget(led);
                }
            }
            Binding::HostButton { led: Some(led), .. } if mapping.source == source => forget(*led),
            _ => {}
        }
    }
}

fn feedback_messages<'a>(
    desired: &'a [[u8; 3]],
    previous: &'a [[u8; 3]],
    force: bool,
) -> impl Iterator<Item = &'a [u8; 3]> {
    desired
        .iter()
        .filter(move |message| force || !previous.contains(message))
}

#[cfg(test)]
mod feedback_batch_tests {
    use super::*;
    use djcraft_controllers::{Event, Source};

    #[test]
    fn rapid_hotcue_edges_repair_one_lamp_once_and_leave_no_feedback_backlog() {
        let profile = Profile::from_json(include_str!(
            "../../../profiles/numark-mixtrack-pro-ii.json"
        ))
        .unwrap();
        let mut bridge = Bridge::new(profile.clone()).unwrap();
        let port = "MixTrack Pro II";
        let mut state = SessionState::default();
        state.decks = vec![djcraft_protocol::DeckState::default(); djcraft_protocol::DECKS];
        state.decks[0].hot_cues[0] = Some(djcraft_protocol::HotCue {
            seconds: 1.,
            loop_end: None,
        });
        let mut desired = bridge.feedback_monitor(port, state.monitor);
        desired.extend(bridge.state_feedback(port, &state, 0, &[None; 4]));
        assert!(
            desired.len() > 16,
            "a full refresh is substantially more than the pressed lamp"
        );
        let mut previous = desired.clone();
        let source = Source::Note {
            channel: 0,
            number: 0x6d,
        };
        for _ in 0..32 {
            for value in [127, 0] {
                let actions = bridge.process(port, Event { source, value }, &state);
                assert_eq!(actions.len(), usize::from(value != 0));
                invalidate_feedback(&profile, port, source, &mut previous);
            }
        }
        let sent: Vec<_> = feedback_messages(&desired, &previous, false)
            .copied()
            .collect();
        assert_eq!(sent, vec![[0x90, 0x6d, 100]]);
        previous = desired.clone();
        assert_eq!(feedback_messages(&desired, &previous, false).count(), 0);
        assert!(bridge.tick(&state).is_empty());
        // Full initialization/reconnection remains available.
        assert_eq!(
            feedback_messages(&desired, &previous, true).count(),
            desired.len()
        );
        // A CC cannot disturb the note feedback cache.
        invalidate_feedback(
            &profile,
            port,
            Source::Cc {
                channel: 0,
                number: 0x6d,
            },
            &mut previous,
        );
        assert_eq!(previous, desired);
    }

    #[test]
    fn dirty_lamps_are_channel_scoped_and_refresh_real_changes_too() {
        let profile = Profile::from_json(include_str!(
            "../../../profiles/numark-mixtrack-pro-ii.json"
        ))
        .unwrap();
        let mut previous = vec![[0x90, 0x6d, 100], [0x90, 0x6e, 100], [0x91, 0x6d, 100]];
        // Different channels are independent, and unrelated LEDs remain cached.
        invalidate_feedback(
            &profile,
            "MixTrack Pro II",
            djcraft_controllers::Source::Note {
                channel: 0,
                number: 0x6d,
            },
            &mut previous,
        );
        let desired = [[0x90, 0x6d, 100], [0x90, 0x6e, 0], [0x91, 0x6d, 100]];
        assert_eq!(
            feedback_messages(&desired, &previous, false)
                .copied()
                .collect::<Vec<_>>(),
            desired[..2]
        );
    }
}

/// Keep the latest measured velocity/position per deck in a drained MIDI batch.
/// Transport and other actions are barriers, so releases and seeks keep order.
#[derive(Default)]
pub(super) struct JogBatch {
    pending: [Option<Action>; djcraft_protocol::DECKS],
}
impl JogBatch {
    pub fn push(&mut self, action: Action) -> Vec<Action> {
        if let Action::Deck {
            deck,
            action: djcraft_protocol::DeckAction::JogScratch { .. },
        } = &action
            && let Some(pending) = self.pending.get_mut(usize::from(*deck))
        {
            *pending = Some(action);
            Vec::new()
        } else {
            let mut actions = self.flush();
            actions.push(action);
            actions
        }
    }
    pub fn flush(&mut self) -> Vec<Action> {
        self.pending.iter_mut().filter_map(Option::take).collect()
    }
}

#[cfg(test)]
mod jog_batch_tests {
    use super::*;
    use djcraft_protocol::DeckAction;

    fn motion(deck: u8, seconds: f64, speed: f64) -> Action {
        Action::Deck {
            deck,
            action: DeckAction::JogScratch {
                seconds,
                speed,
                interval_seconds: 0.01,
            },
        }
    }
    #[test]
    fn delayed_batches_keep_latest_velocity_and_transport_order() {
        let mut batch = JogBatch::default();
        for count in 0..100 {
            assert!(
                batch
                    .push(motion(0, 8. + count as f64 * 0.003, 0.3))
                    .is_empty()
            );
        }
        assert!(batch.push(motion(1, 4., -0.5)).is_empty());
        assert!(batch.push(motion(0, 8.25, -0.1)).is_empty());
        let release = Action::Deck {
            deck: 0,
            action: DeckAction::Scratch { speed: None },
        };
        assert_eq!(
            batch.push(release.clone()),
            vec![motion(0, 8.25, -0.1), motion(1, 4., -0.5), release]
        );
        assert!(batch.flush().is_empty());
        let seek = Action::Deck {
            deck: 0,
            action: DeckAction::Seek { seconds: 8.25 },
        };
        assert_eq!(batch.push(seek.clone()), vec![seek]);
    }
}

#[derive(Default)]
struct ConnectionConsent {
    present: std::collections::BTreeSet<String>,
    decisions: std::collections::BTreeMap<String, bool>,
}
impl ConnectionConsent {
    fn accept_single_matching(
        &mut self,
        available: &[String],
        mapped: &std::collections::BTreeSet<String>,
    ) {
        let mut matching = available.iter().filter(|name| mapped.contains(*name));
        if let Some(name) = matching.next() {
            if matching.next().is_none() && self.present.contains(name) {
                self.decisions.entry(name.clone()).or_insert(true);
            }
        }
    }
    fn update(&mut self, available: &[String]) {
        self.present = available
            .iter()
            .filter(|name| available.iter().filter(|other| *other == *name).count() == 1)
            .cloned()
            .collect();
        self.decisions.retain(|name, _| self.present.contains(name));
    }
    fn allowed(&self, name: &str) -> bool {
        self.present.contains(name) && self.decisions.get(name) == Some(&true)
    }
}

pub struct Controllers {
    consent: ConnectionConsent,
    bridge: Bridge,
    auto_profiles: Vec<Profile>,
    auto_active: usize,
    inputs: Vec<(String, Input)>,
    releases: Vec<Action>,
    next_scan: std::time::Instant,
    failed: std::collections::BTreeSet<String>,
    trace_notes: usize,
    monitor: djcraft_protocol::MonitorState,
    browser_folders: bool,
    state_leds: std::collections::BTreeMap<String, Vec<[u8; 3]>>,
    started: std::time::Instant,
    outputs: std::collections::BTreeMap<String, (Output, Vec<[u8; 3]>)>,
}

fn route_fx1_to_colour(profile: &mut Profile) {
    for mapping in &mut profile.mappings {
        use djcraft_controllers::{Binding, LedFeedback, Parameter};
        match &mut mapping.binding {
            Binding::Parameter { parameter, .. } => {
                if let Parameter::EffectParameter { deck, slot: 0 } = *parameter {
                    *parameter = Parameter::ColourFx { deck };
                }
            }
            Binding::ToggleEffect { deck, slot: 0 } => {
                mapping.binding = Binding::ToggleColourFx { deck: *deck };
            }
            Binding::Led { feedback } => {
                if let LedFeedback::Effect { deck, slot: 0 } = *feedback {
                    *feedback = LedFeedback::ColourFx { deck };
                }
            }
            _ => {}
        }
    }
}

fn profile_for_settings(
    selection: &str,
    settings: &crate::settings::Settings,
    scratch_modes: &[Option<bool>; djcraft_protocol::DECKS],
) -> Result<Profile, String> {
    let mut profile = read_mapping(selection)?;
    if is_mixtrack_profile(&profile) && settings.controller_options_for(&profile).fx1_colour {
        route_fx1_to_colour(&mut profile);
    }
    for mapping in &mut profile.mappings {
        if let djcraft_controllers::Binding::Jog { config } = &mut mapping.binding
            && (config.mode_controlled || config.scratch_toggle.is_some())
            && let Some(enabled) = scratch_modes
                .get(usize::from(config.deck))
                .copied()
                .flatten()
        {
            config.scratch_enabled = enabled;
        }
        if mapping.id.starts_with("headphone-cue-")
            && let djcraft_controllers::Binding::HostButton { press, .. } = &mut mapping.binding
            && let djcraft_controllers::HostCommand::SelectDeck { deck } = *press
        {
            *press = djcraft_controllers::HostCommand::ToggleHeadphoneCue { deck };
        }
    }
    Ok(profile)
}

fn automatic_profile_index(profiles: &[Profile], current: usize, available: &[String]) -> usize {
    let matches = |profile: &Profile| {
        profile.mappings.iter().any(|mapping| {
            available
                .iter()
                .filter(|port| **port == mapping.port)
                .count()
                == 1
        })
    };
    if matches(&profiles[current]) {
        return current;
    }
    let mut candidates = profiles
        .iter()
        .enumerate()
        .filter(|(_, profile)| matches(profile));
    match (candidates.next(), candidates.next()) {
        (Some((index, _)), None) => index,
        _ => current,
    }
}

impl Controllers {
    pub fn set_vinyl_mode(&mut self, deck: u8, enabled: bool) -> Vec<Action> {
        self.bridge.set_vinyl_mode(deck, enabled)
    }
    pub fn set_tempo_bpms(&mut self, bpms: &[Option<f64>]) {
        self.bridge.set_tempo_bpms(bpms);
    }
    pub fn load(
        settings: &crate::settings::Settings,
        scratch_modes: &[Option<bool>; djcraft_protocol::DECKS],
    ) -> Result<Option<Self>, String> {
        if settings.controller_mapping == "@disabled" {
            return Ok(None);
        }
        let profile = profile_for_settings(&settings.controller_mapping, settings, scratch_modes)?;
        let auto_profiles = if settings.controller_mapping.is_empty()
            && std::env::var_os("DJCRAFT_CONTROLLER_MAP").is_none()
        {
            vec![
                profile.clone(),
                profile_for_settings("@grv6", settings, scratch_modes)?,
            ]
        } else {
            Vec::new()
        };
        let bridge = Bridge::new(profile)?;
        let mut controllers = Self {
            consent: Default::default(),
            bridge,
            auto_profiles,
            auto_active: 0,
            inputs: Vec::new(),
            releases: Vec::new(),
            next_scan: std::time::Instant::now(),
            failed: Default::default(),
            trace_notes: std::env::var("DJCRAFT_MIDI_TRACE_NOTES")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
            outputs: Default::default(),
            monitor: Default::default(),
            browser_folders: false,
            state_leds: Default::default(),
            started: std::time::Instant::now(),
        };
        controllers.rescan();
        Ok(Some(controllers))
    }

    pub fn release_all(&mut self) -> Vec<Action> {
        let mut actions = std::mem::take(&mut self.releases);
        for (name, _) in &self.inputs {
            actions.extend(self.bridge.disconnect(name));
        }
        actions
    }

    pub fn scratch_modes(&self) -> [Option<bool>; djcraft_protocol::DECKS] {
        self.bridge.scratch_modes()
    }
    pub fn fx1_routes_colour(&self, deck: usize) -> bool {
        self.bridge
            .profile()
            .mappings
            .iter()
            .any(|mapping| match mapping.binding {
                djcraft_controllers::Binding::ToggleColourFx { deck: mapped } => {
                    usize::from(mapped) == deck
                }
                djcraft_controllers::Binding::Parameter {
                    parameter: djcraft_controllers::Parameter::ColourFx { deck: mapped },
                    ..
                } => usize::from(mapped) == deck,
                _ => false,
            })
    }
    pub fn use_connected(&mut self) {
        while let Some((name, _)) = self.pending_connection() {
            self.answer_connection(name, true);
        }
    }

    pub fn has_matching_device(&self) -> bool {
        self.bridge
            .profile()
            .mappings
            .iter()
            .any(|m| self.consent.present.contains(&m.port))
    }

    pub fn pending_connection(&self) -> Option<(String, String)> {
        self.bridge
            .profile()
            .mappings
            .iter()
            .map(|m| &m.port)
            .find(|name| {
                self.consent.present.contains(*name) && !self.consent.decisions.contains_key(*name)
            })
            .map(|name| (name.clone(), self.bridge.profile().name.clone()))
    }

    pub fn connection_status(&self) -> String {
        let mapping = &self.bridge.profile().name;
        if !self.inputs.is_empty() {
            let names = self
                .inputs
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            format!("Mapping loaded: {mapping} · Connected: {names}")
        } else if self.has_matching_device() {
            format!("Mapping loaded: {mapping} · Controller awaiting connection")
        } else {
            format!("Mapping loaded: {mapping} · No matching controller connected")
        }
    }

    pub fn answer_connection(&mut self, name: String, use_controller: bool) {
        if self.consent.present.contains(&name) {
            self.consent.decisions.insert(name, use_controller);
            self.next_scan = std::time::Instant::now();
        }
    }

    fn rescan(&mut self) {
        let now = std::time::Instant::now();
        if now < self.next_scan {
            return;
        }
        self.next_scan = now + std::time::Duration::from_secs(1);
        let Ok(available) = input_ports() else {
            return;
        };
        if !self.auto_profiles.is_empty() {
            let selected =
                automatic_profile_index(&self.auto_profiles, self.auto_active, &available);
            if selected != self.auto_active {
                for (output, leds) in self.outputs.values_mut() {
                    for message in leds {
                        let mut off = *message;
                        off[2] = 0;
                        let _ = output.send(&off);
                    }
                }
                self.outputs.clear();
                self.state_leds.clear();
                let releases = self.release_all();
                self.releases.extend(releases);
                self.inputs.clear();
                match Bridge::new(self.auto_profiles[selected].clone()) {
                    Ok(bridge) => {
                        self.bridge = bridge;
                        self.auto_active = selected;
                        self.failed.clear();
                        eprintln!(
                            "Controller mapping selected: {}",
                            self.bridge.profile().name
                        );
                    }
                    Err(error) => eprintln!("Controller mapping: {error}"),
                }
            }
        }
        self.consent.update(&available);
        // Names must be unambiguous; never guess between identical controllers.
        self.inputs.retain(|(name, _)| {
            if available.iter().filter(|n| *n == name).count() == 1 {
                return true;
            }
            self.releases.extend(self.bridge.disconnect(name));
            false
        });
        let desired: std::collections::BTreeSet<_> = self
            .bridge
            .profile()
            .mappings
            .iter()
            .map(|m| m.port.clone())
            .collect();
        self.consent.accept_single_matching(&available, &desired);
        for name in desired {
            if available.iter().filter(|n| **n == name).count() != 1 {
                if self.failed.insert(name.clone()) {
                    eprintln!("Controller waiting for one input named {name:?}");
                }
                continue;
            }
            if !self.consent.allowed(&name) {
                continue;
            }
            if self.inputs.iter().any(|(n, _)| *n == name) {
                continue;
            }
            match Input::open(&name) {
                Ok(input) => {
                    eprintln!(
                        "Controller connected: {name} ({} mappings)",
                        self.bridge
                            .profile()
                            .mappings
                            .iter()
                            .filter(|m| m.port == name)
                            .count()
                    );
                    self.failed.remove(&name);
                    self.inputs.push((name, input));
                }
                Err(error) => {
                    if self.failed.insert(name) {
                        eprintln!("Controller connection: {error}");
                    }
                }
            }
        }
        let removed: Vec<_> = self
            .outputs
            .keys()
            .filter(|name| !self.inputs.iter().any(|(n, _)| n == *name))
            .cloned()
            .collect();
        for name in removed {
            if let Some((mut output, leds)) = self.outputs.remove(&name) {
                for mut led in leds {
                    led[2] = 0;
                    let _ = output.send(&led);
                }
            }
        }
        let connected: Vec<_> = self.inputs.iter().map(|(n, _)| n.clone()).collect();
        for name in connected {
            if self.outputs.contains_key(&name)
                || (self.bridge.feedback_monitor(&name, self.monitor).is_empty()
                    && !self.bridge.profile().mappings.iter().any(|m| {
                        m.port == name
                            && matches!(
                                m.binding,
                                djcraft_controllers::Binding::Led { .. }
                                    | djcraft_controllers::Binding::BrowserLed { .. }
                                    | djcraft_controllers::Binding::BeatFx { .. }
                            )
                    }))
            {
                continue;
            }
            let error_key = format!("LED output {name}");
            match Output::open(&name) {
                Ok(mut output) => {
                    if name == "MixTrack Pro II" {
                        match output.send(djcraft_controllers::native::MIXTRACK_PRO_II_STARTUP) {
                            Ok(()) => {
                                self.bridge
                                    .prepare_startup_snapshot(&name, std::time::Instant::now());
                                eprintln!("Controller startup snapshot requested: {name}");
                            }
                            Err(error) => eprintln!("Controller startup snapshot: {error}"),
                        }
                    }
                    self.outputs.insert(name.clone(), (output, Vec::new()));
                    self.failed.remove(&error_key);
                }
                Err(error) => {
                    if self.failed.insert(error_key) {
                        eprintln!("Controller LEDs: {error}");
                    }
                }
            }
        }
    }

    fn sync_feedback(&mut self, port: &str) {
        let mut desired = self.bridge.feedback_monitor(port, self.monitor);
        desired.extend(self.state_leds.get(port).into_iter().flatten().copied());
        for m in &self.bridge.profile().mappings {
            if m.port == port
                && let djcraft_controllers::Binding::BrowserLed { folders } = m.binding
                && let djcraft_controllers::Source::Note { channel, number } = m.source
            {
                desired.push([
                    0x90 | channel,
                    number,
                    if folders == self.browser_folders {
                        100
                    } else {
                        0
                    },
                ]);
            }
        }
        let Some((output, previous)) = self.outputs.get_mut(port) else {
            return;
        };
        if *previous == desired {
            return;
        }
        let messages: Vec<_> = feedback_messages(&desired, previous, false)
            .copied()
            .collect();
        for message in messages {
            if let Err(error) = output.send(&message) {
                eprintln!("Controller LED send {port} {message:02X?}: {error}");
                self.outputs.remove(port);
                return;
            }
        }
        if previous.is_empty() {
            eprintln!(
                "Controller LED feedback sent: {port} ({} lamps)",
                desired.len()
            );
        }
        *previous = desired;
    }

    pub fn drain(&mut self) -> Vec<(String, Vec<djcraft_controllers::native::TimedEvent>)> {
        self.rescan();
        let mut events = Vec::new();
        self.inputs
            .retain(|(name, input)| match input.drain_timed() {
                Ok(batch) => {
                    events.push((name.clone(), batch));
                    true
                }
                Err(error) => {
                    eprintln!("{name}: {error}");
                    self.releases.extend(self.bridge.disconnect(name));
                    false
                }
            });
        events
    }

    pub fn take_releases(&mut self) -> Vec<Action> {
        std::mem::take(&mut self.releases)
    }

    pub fn process(
        &mut self,
        port: &str,
        timed: djcraft_controllers::native::TimedEvent,
        state: &SessionState,
    ) -> Vec<Action> {
        let event = timed.event;
        if self.trace_notes > 0 && matches!(event.source, djcraft_controllers::Source::Note { .. })
        {
            eprintln!("MIDI note {port:?}: {event:?}");
            self.trace_notes -= 1;
        }
        self.monitor = state.monitor;
        let actions = self.bridge.process_at(port, event, state, timed.at);
        if let Some((_, previous)) = self.outputs.get_mut(port) {
            invalidate_feedback(self.bridge.profile(), port, event.source, previous);
        }
        // tick() sends feedback after the entire input batch and its actions.
        actions
    }

    pub fn tick(
        &mut self,
        state: &SessionState,
        loop_in: &[Option<f64>],
        sync_enabled: &[bool],
    ) -> Vec<Action> {
        let actions = self.bridge.tick(state);
        self.monitor = state.monitor;
        let ports: Vec<_> = self.outputs.keys().cloned().collect();
        for port in ports {
            self.state_leds.insert(
                port.clone(),
                self.bridge.state_feedback_with_sync(
                    &port,
                    state,
                    self.started.elapsed().as_millis() as u64,
                    loop_in,
                    sync_enabled,
                ),
            );
            self.sync_feedback(&port);
        }
        actions
    }

    pub fn take_commands(&mut self) -> Vec<djcraft_controllers::HostCommand> {
        self.bridge.take_commands()
    }

    pub fn take_pad_modes(
        &mut self,
    ) -> [Option<djcraft_controllers::PadMode>; djcraft_protocol::DECKS] {
        self.bridge.take_pad_modes()
    }

    pub fn set_browser_folders(&mut self, folders: bool) {
        self.browser_folders = folders;
    }

    pub fn cancel_deck(&mut self, deck: u8) -> Vec<Action> {
        self.bridge.cancel_deck(deck)
    }
}

impl Drop for Controllers {
    fn drop(&mut self) {
        for (output, leds) in self.outputs.values_mut() {
            for message in leds {
                let mut off = *message;
                off[2] = 0;
                let _ = output.send(&off);
            }
        }
    }
}

#[cfg(test)]
mod consent_tests {
    use super::*;
    #[test]
    fn automatic_mapping_follows_a_single_known_controller() {
        let profiles = vec![
            read_mapping("@bundled").unwrap(),
            read_mapping("@grv6").unwrap(),
        ];
        assert_eq!(
            automatic_profile_index(&profiles, 0, &["DDJ-GRV6".into()]),
            1
        );
        assert_eq!(
            automatic_profile_index(&profiles, 1, &["MixTrack Pro II".into()]),
            0
        );
        assert_eq!(
            automatic_profile_index(&profiles, 0, &["unknown".into()]),
            0
        );
        assert_eq!(
            automatic_profile_index(&profiles, 0, &["DDJ-GRV6".into(), "DDJ-GRV6".into()]),
            0
        );
        assert_eq!(
            automatic_profile_index(&profiles, 0, &["MixTrack Pro II".into(), "DDJ-GRV6".into()]),
            0
        );
    }
    #[test]
    fn grv6_rhythm_shift_notes_are_valid_without_hardware() {
        let profile = read_mapping("@grv6").unwrap();
        for channel in 0..4 {
            for number in [44, 48, 49, 50, 51, 36] {
                assert!(profile.mappings.iter().any(|mapping| {
                    mapping.id.starts_with(&format!("deck_{}-rhythm-", ["a", "b", "c", "d"][channel]))
                        && matches!(mapping.source, djcraft_controllers::Source::Note { channel: c, number: n } if c == channel as u8 && n == number)
                }));
            }
        }
        Bridge::new(profile).unwrap();
    }
    #[test]
    fn fx1_colour_routing_changes_only_the_first_slot_and_follows_selected_combination() {
        use djcraft_controllers::{Binding, Event, LedFeedback, Parameter};
        use djcraft_protocol::{DeckAction, Role};
        let mut profile = Profile::from_json(include_str!(
            "../../../profiles/numark-mixtrack-pro-ii.json"
        ))
        .unwrap();
        route_fx1_to_colour(&mut profile);
        assert!(profile.mappings.iter().any(|mapping| matches!(
            mapping.binding,
            Binding::Parameter {
                parameter: Parameter::ColourFx { deck: 0 },
                ..
            }
        )));
        assert!(profile.mappings.iter().any(|mapping| matches!(
            mapping.binding,
            Binding::Parameter {
                parameter: Parameter::EffectParameter { deck: 0, slot: 1 },
                ..
            }
        )));
        assert!(profile.mappings.iter().any(|mapping| matches!(
            mapping.binding,
            Binding::Led {
                feedback: LedFeedback::ColourFx { deck: 0 }
            }
        )));
        let mut bridge = Bridge::new(profile).unwrap();
        let (mut engine, _) = djcraft_engine::Engine::headless(8000);
        engine
            .apply(
                Role::Host,
                Action::Deck {
                    deck: 0,
                    action: DeckAction::SetColourFx {
                        state: djcraft_protocol::ColourFxState {
                            position: 0.5,
                            selected: 97,
                            enabled: true,
                        },
                    },
                },
            )
            .unwrap();
        let before = engine.state();
        let actions = bridge.process(
            "MixTrack Pro II",
            Event::parse(&[0xb0, 0x1b, 1]).unwrap(),
            &before,
        );
        assert!(
            matches!(actions.as_slice(), [Action::Deck { deck: 0, action: DeckAction::SetColourFx { state } }] if state.selected == 97 && state.position > 0.5)
        );
        for action in actions {
            engine.apply(Role::Host, action).unwrap();
        }
        assert_eq!(engine.state().decks[0].effects, before.decks[0].effects);
        let actions = bridge.process(
            "MixTrack Pro II",
            Event::parse(&[0x90, 0x59, 127]).unwrap(),
            &engine.state(),
        );
        assert!(
            matches!(actions.as_slice(), [Action::Deck { deck: 0, action: DeckAction::SetColourFx { state } }] if state.position == 0.5 && !state.enabled && state.selected == 97)
        );
        for action in actions {
            engine.apply(Role::Host, action).unwrap();
        }
        let reset = engine.state();
        assert_eq!(reset.decks[0].effects, before.decks[0].effects);
        assert!(
            bridge
                .state_feedback("MixTrack Pro II", &reset, 0, &[])
                .contains(&[0x90, 0x59, 0])
        );
        assert!(
            bridge
                .process(
                    "MixTrack Pro II",
                    Event::parse(&[0x90, 0x59, 0]).unwrap(),
                    &reset
                )
                .is_empty()
        );
        let actions = bridge.process(
            "MixTrack Pro II",
            Event::parse(&[0xb0, 0x1b, 1]).unwrap(),
            &reset,
        );
        assert!(
            matches!(actions.as_slice(), [Action::Deck { action: DeckAction::SetColourFx { state }, .. }] if state.position > 0.5 && state.enabled && state.selected == 97)
        );
    }
    #[test]
    fn one_matching_controller_connects_automatically_including_reconnection() {
        let mapped = ["deck".to_string()].into_iter().collect();
        let available = vec!["deck".to_string(), "unrelated MIDI port".to_string()];
        let mut c = ConnectionConsent::default();
        c.update(&available);
        c.accept_single_matching(&available, &mapped);
        assert!(c.allowed("deck"));
        c.update(&[]);
        c.update(&available);
        c.accept_single_matching(&available, &mapped);
        assert!(c.allowed("deck"));
    }

    #[test]
    fn multiple_duplicate_unknown_or_declined_devices_are_not_auto_accepted() {
        let mapped = ["deck".to_string(), "other deck".to_string()]
            .into_iter()
            .collect();
        for available in [
            vec!["deck".into(), "other deck".into()],
            vec!["deck".into(), "deck".into()],
            vec!["unknown".into()],
        ] {
            let mut c = ConnectionConsent::default();
            c.update(&available);
            c.accept_single_matching(&available, &mapped);
            assert!(!c.allowed("deck"));
            assert!(!c.allowed("other deck"));
        }
        let available = vec!["deck".to_string()];
        let mut c = ConnectionConsent::default();
        c.update(&available);
        c.decisions.insert("deck".into(), false);
        c.accept_single_matching(&available, &mapped);
        assert!(!c.allowed("deck"));
    }
    #[test]
    fn approval_is_required_and_resets_after_disconnect_or_ambiguity() {
        let mut c = ConnectionConsent::default();
        c.update(&["deck".into()]);
        assert!(!c.allowed("deck"));
        c.decisions.insert("deck".into(), false);
        c.update(&["deck".into()]);
        assert_eq!(c.decisions.get("deck"), Some(&false));
        c.decisions.insert("deck".into(), true);
        assert!(c.allowed("deck"));
        c.update(&[]);
        c.update(&["deck".into()]);
        assert!(!c.allowed("deck"));
        assert!(!c.decisions.contains_key("deck"));
        c.decisions.insert("deck".into(), true);
        c.update(&["deck".into(), "deck".into()]);
        assert!(!c.allowed("deck"));
        assert!(!c.decisions.contains_key("deck"));
    }
}

/// Validate a selected profile before changing the current controller connection.
pub fn read_mapping(selection: &str) -> Result<Profile, String> {
    let installed = crate::storage::machine_dir().join("controllers/numark-mixtrack-pro-ii.json");
    let path = match selection {
        "@bundled" => None,
        "@grv6" => {
            return Profile::from_json(include_str!("../../../profiles/alphatheta-ddj-grv6.json"));
        }
        "" => std::env::var_os("DJCRAFT_CONTROLLER_MAP")
            .map(std::path::PathBuf::from)
            .or_else(|| installed.exists().then_some(installed)),
        other => Some(std::path::PathBuf::from(other)),
    };
    let json = match path {
        Some(path) => std::fs::read_to_string(&path)
            .map_err(|e| format!("Mapping {}: {e}", path.display()))?,
        None => include_str!("../../../profiles/numark-mixtrack-pro-ii.json").to_owned(),
    };
    Profile::from_json(&json)
}

pub fn is_mixtrack_profile(profile: &Profile) -> bool {
    profile.name.starts_with("Numark MixTrack Pro II")
}

pub fn controller_settings_key(profile: &Profile) -> String {
    if is_mixtrack_profile(profile) {
        "numark-mixtrack-pro-ii".into()
    } else {
        profile.name.to_lowercase()
    }
}

pub fn mapping_choices() -> Vec<(String, String)> {
    let mut choices = vec![
        (String::new(), "Automatic (detected controller)".into()),
        ("@bundled".into(), "Numark MixTrack Pro II (bundled)".into()),
        ("@grv6".into(), "AlphaTheta DDJ-GRV6 (starter)".into()),
        ("@disabled".into(), "Disabled".into()),
    ];
    if let Ok(entries) = std::fs::read_dir(crate::storage::machine_dir().join("controllers")) {
        let mut paths: Vec<_> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                p.extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("json"))
            })
            .collect();
        paths.sort();
        for path in paths {
            let selection = path.to_string_lossy().into_owned();
            if let Ok(profile) = read_mapping(&selection) {
                choices.push((
                    selection,
                    format!(
                        "{} ({})",
                        profile.name,
                        path.file_name().unwrap_or_default().to_string_lossy()
                    ),
                ));
            }
        }
    }
    choices
}
