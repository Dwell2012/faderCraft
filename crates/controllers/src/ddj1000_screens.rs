//! Drives the DDJ-1000 jog screens on Linux: SysEx login over MIDI, then HID
//! reports over `hidraw`. See [`crate::ddj1000_link`] for the protocol.
//!
//! A background thread owns the connection and retries every couple of seconds,
//! so the controller can be plugged in after the app starts. It needs read/write
//! access to the unit's `/dev/hidraw*` node (`tools/linux/70-ddj1000.rules`).
//! The kernel exposes no audio driver for the unit, so the "no audio driver"
//! message is cleared separately by `tools/ddj1000_announce.py`.
use crate::ddj1000_link::{self as link, ScreenDeck, WaveColumn};
use midir::{Ignore, MidiInput, MidiOutput};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

/// The stable (client-number-free) MIDI port name the unit appears as.
pub const PORT: &str = "DDJ-1000:DDJ-1000 MIDI 1";
const FRAME: Duration = Duration::from_millis(25);
const RETRY: Duration = Duration::from_secs(2);
/// Never extrapolate a position further than this past the last update.
const MAX_EXTRAPOLATION: Duration = Duration::from_millis(250);

type Wave = Option<Arc<Vec<WaveColumn>>>;

struct Shared {
    decks: [ScreenDeck; 4],
    stamp: Instant,
    waves: [Wave; 4],
    /// Bumped whenever a deck's waveform is set or cleared.
    wave_generation: [u64; 4],
}

pub struct ScreenLink {
    shared: Arc<Mutex<Shared>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl ScreenLink {
    pub fn start() -> Self {
        let shared = Arc::new(Mutex::new(Shared {
            decks: Default::default(),
            stamp: Instant::now(),
            waves: Default::default(),
            wave_generation: [0; 4],
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let (shared, stop) = (shared.clone(), stop.clone());
            std::thread::Builder::new()
                .name("ddj1000-screens".into())
                .spawn(move || run(&shared, &stop))
                .ok()
        };
        Self {
            shared,
            stop,
            thread,
        }
    }

    /// Publish the latest state of decks 1-4. Cheap enough to call every tick.
    pub fn update(&self, decks: [ScreenDeck; 4]) {
        if let Ok(mut shared) = self.shared.lock() {
            shared.decks = decks;
            shared.stamp = Instant::now();
        }
    }
}

impl ScreenLink {
    /// Set (or clear) the overview waveform for deck `deck` (0-3). It is sent
    /// once the unit has its track-load data, and again after a reconnect.
    pub fn set_waveform(&self, deck: usize, columns: Option<Vec<WaveColumn>>) {
        if let Ok(mut shared) = self.shared.lock()
            && deck < 4
        {
            shared.waves[deck] = columns.map(Arc::new);
            shared.wave_generation[deck] += 1;
        }
    }
}

impl Drop for ScreenLink {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run(shared: &Mutex<Shared>, stop: &AtomicBool) {
    let mut reported = false;
    while !stop.load(Ordering::Acquire) {
        match session(shared, stop, &mut reported) {
            Ok(()) => {}
            Err(error) => {
                if !reported {
                    eprintln!("DDJ-1000 screens: {error}");
                    reported = true;
                }
            }
        }
        sleep_unless_stopped(RETRY, stop);
    }
}

fn sleep_unless_stopped(total: Duration, stop: &AtomicBool) {
    let end = Instant::now() + total;
    while Instant::now() < end && !stop.load(Ordering::Acquire) {
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn find_hidraw() -> Option<PathBuf> {
    for entry in std::fs::read_dir("/sys/class/hidraw").ok()?.flatten() {
        let uevent = std::fs::read_to_string(entry.path().join("device/uevent")).ok()?;
        if uevent.contains("DDJ-1000") {
            return Some(PathBuf::from("/dev").join(entry.file_name()));
        }
    }
    None
}

fn random_seed() -> std::io::Result<[u8; 8]> {
    let mut seed = [0u8; 8];
    File::open("/dev/urandom")?.read_exact(&mut seed)?;
    Ok(seed)
}

fn wait_for(rx: &Receiver<Vec<u8>>, stage: u8, timeout: Duration) -> Option<Vec<u8>> {
    let end = Instant::now() + timeout;
    loop {
        let left = end.checked_duration_since(Instant::now())?;
        let message = rx.recv_timeout(left).ok()?;
        if message.get(link::PREFIX.len()) == Some(&stage) {
            return Some(message);
        }
    }
}

fn session(shared: &Mutex<Shared>, stop: &AtomicBool, reported: &mut bool) -> Result<(), String> {
    let Some(hid_path) = find_hidraw() else {
        return Ok(()); // not plugged in; stay quiet and keep polling
    };
    let mut hid = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&hid_path)
        .map_err(|e| {
            format!(
                "cannot open {} ({e}); install tools/linux/70-ddj1000.rules",
                hid_path.display()
            )
        })?;

    let output = MidiOutput::new("faderCraft screens").map_err(|e| e.to_string())?;
    let mut input = MidiInput::new("faderCraft screens").map_err(|e| e.to_string())?;
    input.ignore(Ignore::TimeAndActiveSense); // keep SysEx
    let out_port = output
        .ports()
        .into_iter()
        .find(|p| {
            output
                .port_name(p)
                .is_ok_and(|n| crate::native::stable_port_name(&n) == PORT)
        })
        .ok_or("DDJ-1000 MIDI output not found")?;
    let in_port = input
        .ports()
        .into_iter()
        .find(|p| {
            input
                .port_name(p)
                .is_ok_and(|n| crate::native::stable_port_name(&n) == PORT)
        })
        .ok_or("DDJ-1000 MIDI input not found")?;
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let _input = input
        .connect(
            &in_port,
            "screens in",
            move |_, bytes, _| {
                if bytes.starts_with(&link::PREFIX) {
                    let _ = tx.send(bytes.to_vec());
                }
            },
            (),
        )
        .map_err(|e| e.to_string())?;
    let mut output = output
        .connect(&out_port, "screens out")
        .map_err(|e| e.to_string())?;
    let mut send = |bytes: &[u8]| output.send(bytes).map_err(|e| e.to_string());

    // Login: keepalive, hello, verify the unit, answer, wait for acceptance.
    send(&link::KEEPALIVE)?;
    wait_for(&rx, link::STAGE_HELLO_ACK, Duration::from_secs(1))
        .ok_or("no reply to keepalive; is the unit already logged in elsewhere?")?;
    let seed_a = random_seed().map_err(|e| e.to_string())?;
    send(&link::hello(&seed_a))?;
    let reply = wait_for(&rx, link::STAGE_UNIT_CHALLENGE, Duration::from_secs(1))
        .ok_or("no challenge from the unit")?;
    let (_, args) = link::parse(&reply).ok_or("unreadable challenge")?;
    let seed_e = link::check_unit_reply(&seed_a, &args).ok_or("unit failed verification")?;
    send(&link::answer(&seed_e))?;
    wait_for(&rx, link::STAGE_ACCEPTED, Duration::from_secs(1)).ok_or("login not accepted")?;
    for message in link::setup_messages() {
        send(&message)?;
    }
    eprintln!("DDJ-1000 screens: connected ({})", hid_path.display());
    *reported = false;

    let mut shown: [Option<(bool, u32)>; 4] = [None; 4];
    let mut sent_wave: [Option<u64>; 4] = [None; 4];
    let mut last_keepalive = Instant::now();
    while !stop.load(Ordering::Acquire) {
        let frame_start = Instant::now();
        if last_keepalive.elapsed() >= Duration::from_millis(link::KEEPALIVE_INTERVAL_MS) {
            send(&link::KEEPALIVE)?;
            last_keepalive = Instant::now();
        }
        while rx.try_recv().is_ok() {} // drain keepalive acknowledgements
        let (decks, age, waves, wave_generation) = {
            let shared = shared.lock().map_err(|_| "state lock poisoned")?;
            (
                shared.decks,
                shared.stamp.elapsed(),
                shared.waves.clone(),
                shared.wave_generation,
            )
        };
        for (index, deck) in decks.iter().enumerate() {
            let number = index as u8 + 1;
            let mut deck = *deck;
            if deck.playing && deck.loaded {
                let speed = 1. + deck.tempo_percent / 100.;
                let ahead = age.min(MAX_EXTRAPOLATION).as_secs_f64() * 1000. * speed;
                deck.position_ms =
                    (f64::from(deck.position_ms) + ahead).min(f64::from(deck.duration_ms)) as u32;
            }
            let track = (deck.loaded, if deck.loaded { deck.duration_ms } else { 0 });
            if shown[index] != Some(track) {
                let reports = if deck.loaded {
                    link::track_loaded_reports(number, deck.duration_ms)
                } else {
                    link::track_unloaded_reports(number)
                };
                for report in reports {
                    write_report(&mut hid, &report)?;
                }
                shown[index] = Some(track);
                sent_wave[index] = None; // the load blanked the waveform
            }
            if deck.loaded
                && sent_wave[index] != Some(wave_generation[index])
                && let Some(columns) = &waves[index]
            {
                for report in link::waveform_reports(number, columns) {
                    write_report(&mut hid, &report)?;
                }
                sent_wave[index] = Some(wave_generation[index]);
            }
            write_report(&mut hid, &link::status_report(number, &deck))?;
        }
        if let Some(left) = FRAME.checked_sub(frame_start.elapsed()) {
            std::thread::sleep(left);
        }
    }
    Ok(())
}

/// hidraw takes a leading report id; the unit uses none, so it is zero.
fn write_report(hid: &mut File, report: &[u8; link::REPORT_LEN]) -> Result<(), String> {
    let mut buffer = [0u8; link::REPORT_LEN + 1];
    buffer[1..].copy_from_slice(report);
    hid.write_all(&buffer)
        .map_err(|e| format!("HID write: {e}"))
}
