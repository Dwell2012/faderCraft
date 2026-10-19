//! Native MIDI transport. Callbacks only enqueue; mapping/engine work stays on
//! the host control thread. Opening ports is explicit, never automatic for all devices.
use crate::Event;
use midir::{MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver},
};

pub fn input_ports() -> Result<Vec<String>, String> {
    let input = MidiInput::new("faderCraft discovery").map_err(|e| e.to_string())?;
    input
        .ports()
        .iter()
        .map(|p| input.port_name(p).map_err(|e| e.to_string()))
        .collect()
}

pub struct Input {
    rx: Receiver<Event>,
    overflow: Arc<AtomicBool>,
    _connection: MidiInputConnection<()>,
}

pub struct Output(MidiOutputConnection);
impl Output {
    pub fn open(name: &str) -> Result<Self, String> {
        let output = MidiOutput::new("faderCraft LEDs").map_err(|e| e.to_string())?;
        let ports: Vec<_> = output
            .ports()
            .into_iter()
            .filter(|p| output.port_name(p).is_ok_and(|n| n == name))
            .collect();
        if ports.len() != 1 {
            return Err(format!(
                "expected one MIDI output named {name:?}, found {}",
                ports.len()
            ));
        }
        output
            .connect(&ports[0], "faderCraft LEDs")
            .map(Self)
            .map_err(|e| e.to_string())
    }
    pub fn send(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.0.send(bytes).map_err(|e| e.to_string())
    }
}

impl Input {
    pub fn open(name: &str) -> Result<Self, String> {
        let input = MidiInput::new("faderCraft").map_err(|e| e.to_string())?;
        let ports: Vec<_> = input
            .ports()
            .into_iter()
            .filter(|p| input.port_name(p).is_ok_and(|n| n == name))
            .collect();
        if ports.len() != 1 {
            return Err(format!(
                "expected one MIDI input named {name:?}, found {}",
                ports.len()
            ));
        }
        let (tx, rx) = mpsc::sync_channel(1024);
        let overflow = Arc::new(AtomicBool::new(false));
        let callback_overflow = overflow.clone();
        let connection = input
            .connect(
                &ports[0],
                "faderCraft input",
                move |_, bytes, _| {
                    if let Some(event) = Event::parse(bytes) {
                        if tx.try_send(event).is_err() {
                            callback_overflow.store(true, Ordering::Release);
                        }
                    }
                },
                (),
            )
            .map_err(|e| e.to_string())?;
        Ok(Self {
            rx,
            overflow,
            _connection: connection,
        })
    }

    /// Overflow is an error because a dropped button release can leave a held
    /// action active. The host should release held bindings and reopen the input.
    pub fn drain(&self) -> Result<Vec<Event>, String> {
        if self.overflow.swap(false, Ordering::AcqRel) {
            return Err("MIDI input queue overflow; release held actions and reconnect".into());
        }
        Ok(self.rx.try_iter().take(1024).collect())
    }
}
