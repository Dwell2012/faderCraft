//! Optional JSON mapping host. No MIDI is opened without a user-owned profile.
use djcraft_controllers::{
    Bridge, Profile,
    native::{Input, Output, input_ports},
};
use djcraft_protocol::{Action, SessionState};

pub struct Controllers {
    bridge: Bridge,
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

impl Controllers {
    pub fn set_tempo_bpms(&mut self, bpms: &[Option<f64>]) {
        self.bridge.set_tempo_bpms(bpms);
    }
    pub fn load() -> Result<Option<Self>, String> {
        let Some(path) = std::env::var_os("DJCRAFT_CONTROLLER_MAP") else {
            return Ok(None);
        };
        let json = std::fs::read_to_string(&path)
            .map_err(|e| format!("controller profile {path:?}: {e}"))?;
        let mut profile = Profile::from_json(&json)?;
        // Upgrade only the named legacy headphone buttons; custom activation mappings stay intact.
        for mapping in &mut profile.mappings {
            if mapping.id.starts_with("headphone-cue-")
                && let djcraft_controllers::Binding::HostButton { press, .. } = &mut mapping.binding
                && let djcraft_controllers::HostCommand::SelectDeck { deck } = *press
            {
                *press = djcraft_controllers::HostCommand::ToggleHeadphoneCue { deck };
            }
        }
        let bridge = Bridge::new(profile)?;
        let mut controllers = Self {
            bridge,
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

    fn rescan(&mut self) {
        let now = std::time::Instant::now();
        if now < self.next_scan {
            return;
        }
        self.next_scan = now + std::time::Duration::from_secs(1);
        let Ok(available) = input_ports() else {
            return;
        };
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
        for name in desired {
            if available.iter().filter(|n| **n == name).count() != 1 {
                if self.failed.insert(name.clone()) {
                    eprintln!("Controller waiting for one input named {name:?}");
                }
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
                            )
                    }))
            {
                continue;
            }
            let error_key = format!("LED output {name}");
            match Output::open(&name) {
                Ok(output) => {
                    self.outputs.insert(name.clone(), (output, Vec::new()));
                    self.failed.remove(&error_key);
                    self.sync_feedback(&name, true);
                }
                Err(error) => {
                    if self.failed.insert(error_key) {
                        eprintln!("Controller LEDs: {error}");
                    }
                }
            }
        }
    }

    fn sync_feedback(&mut self, port: &str, force: bool) {
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
        if !force && *previous == desired {
            return;
        }
        for message in &desired {
            if !force && previous.contains(message) {
                continue;
            }
            if let Err(error) = output.send(message) {
                eprintln!("Controller LED send: {error}");
                self.outputs.remove(port);
                return;
            }
        }
        *previous = desired;
    }

    pub fn drain(&mut self) -> Vec<(String, Vec<djcraft_controllers::Event>)> {
        self.rescan();
        let mut events = Vec::new();
        self.inputs.retain(|(name, input)| match input.drain() {
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
        event: djcraft_controllers::Event,
        state: &SessionState,
    ) -> Vec<Action> {
        if self.trace_notes > 0 && matches!(event.source, djcraft_controllers::Source::Note { .. })
        {
            eprintln!("MIDI note {port:?}: {event:?}");
            self.trace_notes -= 1;
        }
        let force = self.bridge.profile().mappings.iter().any(|m| m.port == port && matches!(&m.binding, djcraft_controllers::Binding::Jog { config } if config.scratch_toggle == Some(event.source)));
        self.monitor = state.monitor;
        let actions = self.bridge.process(port, event, state);
        let force = force
            || matches!(event.source, djcraft_controllers::Source::Note { .. })
                && self.bridge.profile().mappings.iter().any(|m| {
                    m.port == port
                        && m.source == event.source
                        && matches!(
                            m.binding,
                            djcraft_controllers::Binding::Led { .. }
                                | djcraft_controllers::Binding::BrowserLed { .. }
                        )
                })
            || self.bridge.profile().mappings.iter().any(|m| {
                m.port == port
                    && m.source == event.source
                    && matches!(
                        m.binding,
                        djcraft_controllers::Binding::HostButton { led: Some(_), .. }
                    )
            });
        self.sync_feedback(port, force);
        actions
    }

    pub fn tick(&mut self, state: &SessionState, loop_in: &[Option<f64>]) -> Vec<Action> {
        let actions = self.bridge.tick(state);
        self.monitor = state.monitor;
        let ports: Vec<_> = self.outputs.keys().cloned().collect();
        for port in ports {
            self.state_leds.insert(
                port.clone(),
                self.bridge.state_feedback(
                    &port,
                    state,
                    self.started.elapsed().as_millis() as u64,
                    loop_in,
                ),
            );
            self.sync_feedback(&port, false);
        }
        actions
    }

    pub fn take_commands(&mut self) -> Vec<djcraft_controllers::HostCommand> {
        self.bridge.take_commands()
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
