//! Pioneer DDJ-1000 jog-screen link: message encoding only, no I/O.
//!
//! The unit shows its rekordbox logo until a host logs in with a SysEx
//! challenge-response, then draws whatever 64-byte HID reports it is sent. The
//! layouts were decoded from rekordbox USB captures by
//! <https://github.com/RudolfsUiska/ddj1000-linux-mixxx> (`docs/PROTOCOL.md`)
//! on top of the public CDJ analysis at
//! <https://swiftb0y.github.io/CDJHidProtocol/hid-analysis/handshake.html>.
//! This module is an independent implementation of those documented facts; the
//! captured byte strings in the tests are the only data taken from there.
//!
//! Status: `documented`. Nothing here is hardware-verified in faderCraft yet.

/// Every handshake and housekeeping SysEx starts with this prefix.
pub const PREFIX: [u8; 9] = [0xf0, 0x00, 0x40, 0x05, 0x00, 0x00, 0x02, 0x00, 0x00];
const VENDOR: &[u8] = b"PioneerDJ";
const PRODUCT: &[u8] = b"rekordbox";
const VENDOR_SECRET: u32 = 0x6801_31fb;
/// The DDJ-1000's device id as rekordbox presents it.
const DEVICE_ID: [u8; 10] = [0x87, 0xa0, 0x8e, 0xea, 0xc0, 0x90, 0x34, 0x76, 0x0b, 0x90];

/// Sent every 200 ms while logged in; the unit answers with stage `0x11`.
pub const KEEPALIVE: [u8; 12] = [
    0xf0, 0x00, 0x40, 0x05, 0x00, 0x00, 0x02, 0x00, 0x00, 0x50, 0x01, 0xf7,
];
pub const KEEPALIVE_INTERVAL_MS: u64 = 200;

pub const STAGE_HELLO_ACK: u8 = 0x11;
pub const STAGE_UNIT_CHALLENGE: u8 = 0x13;
pub const STAGE_ACCEPTED: u8 = 0x15;

/// Messages rekordbox sends right after logging in. Their meaning is not
/// decoded; they are replayed unchanged.
pub fn setup_messages() -> Vec<Vec<u8>> {
    let mut third = vec![
        0x00, 0x0a, 0x00, 0x28, 0x00, 0x26, 0x00, 0x08, 0x15, 0x62, 0x14, 0x28, 0x16,
    ];
    third.extend([0u8; 36]);
    [
        vec![0x00, 0x0b, 0x31, 0x00, 0x00, 0x00, 0x00, 0x00],
        vec![0x00, 0x0c, 0x00, 0x00, 0x02, 0x0e, 0x0e, 0x00, 0x00, 0x00],
        third,
    ]
    .into_iter()
    .map(|body| {
        let mut message = PREFIX.to_vec();
        message.extend(body);
        message.push(0xf7);
        message
    })
    .collect()
}

pub fn fnv1a(data: &[u8]) -> u32 {
    data.iter().fold(0x811c_9dc5u32, |hash, byte| {
        (hash ^ u32::from(*byte)).wrapping_mul(0x0100_0193)
    })
}

/// Split each byte into two nibbles so binary values stay 7-bit clean.
fn spread(data: &[u8]) -> Vec<u8> {
    data.iter().flat_map(|b| [b >> 4, b & 0x0f]).collect()
}

fn unspread(data: &[u8]) -> Vec<u8> {
    data.chunks_exact(2).map(|p| (p[0] << 4) | p[1]).collect()
}

fn tlv(tag: u8, value: &[u8]) -> Vec<u8> {
    let mut out = vec![tag, value.len() as u8 + 2];
    out.extend(value);
    out
}

fn message(stage: u8, args: &[Vec<u8>]) -> Vec<u8> {
    let body: Vec<u8> = args.concat();
    let mut out = PREFIX.to_vec();
    out.extend(tlv(stage, &body));
    out.push(0xf7);
    out
}

/// Stage `0x12`: the host introduces itself with a random 8-byte seed.
pub fn hello(seed_a: &[u8; 8]) -> Vec<u8> {
    message(
        0x12,
        &[tlv(1, VENDOR), tlv(2, PRODUCT), tlv(3, &spread(seed_a))],
    )
}

fn secret(seed_e: &[u8]) -> [u8; 4] {
    let mut padded = [0u8; 4];
    let take = seed_e.len().min(4);
    padded[4 - take..].copy_from_slice(&seed_e[..take]);
    (u32::from_be_bytes(padded) ^ VENDOR_SECRET).to_be_bytes()
}

/// A parsed handshake SysEx from the unit: its stage and tagged arguments.
pub fn parse(sysex: &[u8]) -> Option<(u8, Vec<(u8, Vec<u8>)>)> {
    if sysex.len() < PREFIX.len() + 3 || sysex[..PREFIX.len()] != PREFIX {
        return None;
    }
    let body = &sysex[PREFIX.len()..sysex.len() - 1];
    let (stage, length) = (body[0], usize::from(body[1]));
    let mut args = Vec::new();
    let mut at = 2;
    while at + 2 <= length.min(body.len()) {
        let (tag, len) = (body[at], usize::from(body[at + 1]));
        if len < 2 || at + len > body.len() {
            return None;
        }
        args.push((tag, body[at + 2..at + len].to_vec()));
        at += len;
    }
    Some((stage, args))
}

fn arg(args: &[(u8, Vec<u8>)], tag: u8) -> Option<&[u8]> {
    args.iter()
        .find(|(t, _)| *t == tag)
        .map(|(_, v)| v.as_slice())
}

/// Verify the unit's stage `0x13` reply; on success return its seed.
pub fn check_unit_reply(seed_a: &[u8; 8], args: &[(u8, Vec<u8>)]) -> Option<Vec<u8>> {
    let seed_e = unspread(arg(args, 3)?);
    let hash = unspread(arg(args, 4)?);
    let mut input = seed_a.to_vec();
    input.extend(secret(&seed_e));
    (hash == fnv1a(&input).to_be_bytes()).then_some(seed_e)
}

/// Stage `0x14`: prove the host knows the secret, using the unit's seed.
pub fn answer(seed_e: &[u8]) -> Vec<u8> {
    let mut input = seed_e.to_vec();
    input.extend(secret(seed_e));
    let hash = fnv1a(&input).to_be_bytes();
    message(
        0x14,
        &[
            tlv(1, VENDOR),
            tlv(2, PRODUCT),
            tlv(4, &spread(&hash)),
            tlv(5, &spread(&DEVICE_ID)),
        ],
    )
}

// ---- HID reports -----------------------------------------------------------

pub const REPORT_LEN: usize = 64;
const CHUNK_DATA: usize = 58;

/// A deck as the jog screen shows it. `deck` is 1-4.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScreenDeck {
    pub loaded: bool,
    pub playing: bool,
    /// The tempo-master deck, drawn highlighted.
    pub leader: bool,
    pub position_ms: u32,
    pub duration_ms: u32,
    /// Effective BPM, or 0 when unknown.
    pub bpm: f64,
    /// Tempo offset in percent, e.g. 6.5 for +6.5 %.
    pub tempo_percent: f64,
}

/// `[minutes][seconds][milliseconds LE16]`, as the unit displays time.
pub fn time_field(ms: u32) -> [u8; 4] {
    let ms = ms as u64;
    let milliseconds = ((ms % 1000) as u16).to_le_bytes();
    [
        (ms / 60_000).min(255) as u8,
        ((ms / 1000) % 60) as u8,
        milliseconds[0],
        milliseconds[1],
    ]
}

/// The 0x21 status report, sent continuously for each deck.
pub fn status_report(deck: u8, state: &ScreenDeck) -> [u8; REPORT_LEN] {
    let mut f = [0u8; REPORT_LEN];
    f[0] = deck << 4;
    f[1] = 0x21;
    f[4] = 0x21;
    f[2] = if state.playing || !state.loaded {
        0x18
    } else {
        0x1a
    };
    f[3] = if state.leader { 0x0a } else { 0x08 };
    f[5] = if state.leader { 0x81 } else { 0x01 };
    f[27] = 0x80;
    f[60] = 0x80;
    f[61] = 0x0d;
    if !state.loaded {
        f[9] = 0x10;
        return f;
    }
    f[9] = 0xb4;
    f[58] = 0x01;
    f[11..15].copy_from_slice(&time_field(state.position_ms));
    f[15..19].copy_from_slice(&time_field(state.duration_ms));
    // BPM is the integer part, then tenths in the high nibble. A non-zero low
    // nibble makes the screen show 999.99.
    let tenths = (state.bpm * 10.).round().clamp(0., 2559.) as u32;
    f[21] = (tenths / 10) as u8;
    f[22] = ((tenths % 10) as u8) << 4;
    f[38] = f[21];
    f[39] = f[22];
    let tempo = (state.tempo_percent * 100.).round().clamp(-32768., 32767.) as i16;
    f[23..25].copy_from_slice(&tempo.to_le_bytes());
    f
}

/// Split a payload into numbered 64-byte reports for one command.
pub fn chunked(deck: u8, command: u8, payload: &[u8]) -> Vec<[u8; REPORT_LEN]> {
    let total = payload.len().div_ceil(CHUNK_DATA).max(1);
    (0..total)
        .map(|index| {
            let mut report = [0u8; REPORT_LEN];
            report[0] = deck << 4;
            report[1] = command;
            report[2..4].copy_from_slice(&((index + 1) as u16).to_le_bytes());
            report[4..6].copy_from_slice(&(total as u16).to_le_bytes());
            let start = index * CHUNK_DATA;
            let end = payload.len().min(start + CHUNK_DATA);
            if start < end {
                report[6..6 + end - start].copy_from_slice(&payload[start..end]);
            }
            report
        })
        .collect()
}

/// Reports sent once when a track is loaded onto a deck: length, a constant
/// block, an empty beat grid, a blank waveform and no artwork (the record icon).
pub fn track_loaded_reports(deck: u8, duration_ms: u32) -> Vec<[u8; REPORT_LEN]> {
    let length = time_field(duration_ms);
    let mut p30 = length.to_vec();
    p30.resize(116, 0);
    let mut p2d = length.to_vec();
    p2d.extend([0x0a, 0x00]);
    for _ in 0..10 {
        p2d.extend([0x9e, 0x2f, 0x27, 0x01, 0x00]);
    }
    let mut reports = chunked(deck, 0x30, &p30);
    reports.extend(chunked(deck, 0x2d, &p2d));
    reports.extend(chunked(deck, 0x2f, &[0, 0]));
    reports.extend(chunked(deck, 0x2c, &[0u8; 73 * CHUNK_DATA]));
    reports.extend(chunked(deck, 0x2b, &[0u8; CHUNK_DATA]));
    reports
}

/// The overview waveform has this many columns, spread over the whole track.
pub const WAVE_COLUMNS: usize = 600;
/// Column heights run 0-40 pixels.
pub const WAVE_MAX_HEIGHT: u8 = 40;

/// One waveform column: the peak in an outer colour, the average level in a
/// brighter inner colour. Colours are RGB565.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WaveColumn {
    pub outer: u16,
    pub inner: u16,
    pub outer_height: u8,
    pub inner_height: u8,
}

pub fn rgb565(r: u8, g: u8, b: u8) -> u16 {
    (u16::from(r >> 3) << 11) | (u16::from(g >> 2) << 5) | u16::from(b >> 3)
}

/// The 0x2C waveform command: `0x80`, then 600 columns of
/// `[outer LE16][inner LE16][outer height][inner height][0]`. Short input is
/// padded with empty columns and long input truncated.
pub fn waveform_reports(deck: u8, columns: &[WaveColumn]) -> Vec<[u8; REPORT_LEN]> {
    let mut payload = vec![0x80];
    for index in 0..WAVE_COLUMNS {
        let column = columns.get(index).copied().unwrap_or_default();
        payload.extend(column.outer.to_le_bytes());
        payload.extend(column.inner.to_le_bytes());
        payload.push(column.outer_height.min(WAVE_MAX_HEIGHT));
        payload.push(column.inner_height.min(WAVE_MAX_HEIGHT));
        payload.push(0);
    }
    chunked(deck, 0x2c, &payload)
}

/// Reports sent when a deck is emptied.
pub fn track_unloaded_reports(deck: u8) -> Vec<[u8; REPORT_LEN]> {
    let mut reports = chunked(deck, 0x30, &[0u8; 116]);
    reports.extend(chunked(deck, 0x2f, &[0u8; CHUNK_DATA]));
    reports.extend(chunked(deck, 0x2b, &[0u8; CHUNK_DATA]));
    reports
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(text: &str) -> Vec<u8> {
        text.split_whitespace()
            .map(|b| u8::from_str_radix(b, 16).unwrap())
            .collect()
    }

    // Captured from rekordbox 7.2.19 talking to a DDJ-1000.
    const SEED_A: [u8; 8] = [0x02, 0x66, 0x4e, 0x6e, 0xa8, 0xe7, 0xa2, 0xcb];
    const HELLO: &str = "F0 00 40 05 00 00 02 00 00 12 2A 01 0B 50 69 6F 6E 65 65 72 44 4A 02 0B 72 65 6B 6F 72 64 62 6F 78 03 12 00 02 06 06 04 0E 06 0E 0A 08 0E 07 0A 02 0C 0B F7";
    const REPLY: &str = "F0 00 40 05 00 00 02 00 00 13 2A 01 0B 50 69 6F 6E 65 65 72 44 4A 02 09 44 44 4A 31 30 30 30 04 0A 01 04 07 0F 0F 0B 04 0B 03 0A 00 00 00 00 05 03 0D 0C F7";
    const ANSWER: &str = "F0 00 40 05 00 00 02 00 00 14 38 01 0B 50 69 6F 6E 65 65 72 44 4A 02 0B 72 65 6B 6F 72 64 62 6F 78 04 0A 05 04 00 08 05 05 0F 0E 05 16 08 07 0A 00 08 0E 0E 0A 0C 00 09 00 03 04 07 06 00 0B 09 00 F7";

    #[test]
    fn hello_matches_the_captured_message() {
        assert_eq!(hello(&SEED_A), hex(HELLO));
    }

    #[test]
    fn unit_reply_verifies_and_the_answer_matches_the_capture() {
        let (stage, args) = parse(&hex(REPLY)).unwrap();
        assert_eq!(stage, STAGE_UNIT_CHALLENGE);
        let seed_e = check_unit_reply(&SEED_A, &args).expect("reply must verify");
        assert_eq!(answer(&seed_e), hex(ANSWER));
    }

    #[test]
    fn a_forged_reply_is_rejected() {
        let mut reply = hex(REPLY);
        let hash_byte = reply.len() - 3;
        reply[hash_byte] ^= 1;
        let (_, args) = parse(&reply).unwrap();
        assert!(check_unit_reply(&SEED_A, &args).is_none());
    }

    #[test]
    fn parse_ignores_foreign_sysex_and_truncated_input() {
        assert!(parse(&[0xf0, 0x7e, 0x00, 0xf7]).is_none());
        assert!(parse(&PREFIX).is_none());
    }

    #[test]
    fn fnv1a_has_the_standard_test_vectors() {
        assert_eq!(fnv1a(b""), 0x811c_9dc5);
        assert_eq!(fnv1a(b"a"), 0xe40c_292c);
        assert_eq!(fnv1a(b"foobar"), 0xbf9c_f968);
    }

    #[test]
    fn status_report_encodes_time_bpm_and_tempo() {
        let deck = ScreenDeck {
            loaded: true,
            playing: true,
            leader: true,
            position_ms: 61_234,
            duration_ms: 185_000,
            bpm: 150.9,
            tempo_percent: 10.0,
        };
        let f = status_report(2, &deck);
        assert_eq!((f[0], f[1], f[2], f[4]), (0x20, 0x21, 0x18, 0x21));
        assert_eq!((f[3], f[5], f[9], f[58]), (0x0a, 0x81, 0xb4, 0x01));
        assert_eq!(&f[11..15], &[1, 1, 0xea, 0x00]);
        assert_eq!(&f[15..19], &[3, 5, 0, 0]);
        assert_eq!((f[21], f[22]), (150, 0x90));
        assert_eq!((f[38], f[39]), (150, 0x90));
        assert_eq!(&f[23..25], &[0xe8, 0x03]);
    }

    #[test]
    fn empty_and_paused_decks_use_their_own_flags() {
        let empty = status_report(1, &ScreenDeck::default());
        assert_eq!((empty[2], empty[9], empty[58]), (0x18, 0x10, 0));
        let paused = status_report(
            1,
            &ScreenDeck {
                loaded: true,
                ..Default::default()
            },
        );
        assert_eq!(paused[2], 0x1a);
    }

    #[test]
    fn long_payloads_are_numbered_and_zero_padded() {
        let reports = chunked(3, 0x2c, &[0xaa; 59]);
        assert_eq!(reports.len(), 2);
        assert_eq!(&reports[0][..6], &[0x30, 0x2c, 1, 0, 2, 0]);
        assert_eq!(&reports[1][..8], &[0x30, 0x2c, 2, 0, 2, 0, 0xaa, 0]);
        assert_eq!(chunked(1, 0x2b, &[]).len(), 1);
    }

    #[test]
    fn track_load_sends_the_documented_command_set() {
        let reports = track_loaded_reports(1, 200_000);
        let commands: Vec<u8> = reports.iter().map(|r| r[1]).collect();
        assert_eq!(commands.iter().filter(|c| **c == 0x30).count(), 2);
        assert_eq!(commands.iter().filter(|c| **c == 0x2c).count(), 73);
        assert_eq!(commands.iter().filter(|c| **c == 0x2d).count(), 1);
        assert_eq!(commands.iter().filter(|c| **c == 0x2b).count(), 1);
    }

    #[test]
    fn waveform_fills_73_chunks_with_the_documented_column_layout() {
        let columns = [WaveColumn {
            outer: rgb565(255, 0, 0),
            inner: rgb565(0, 255, 0),
            outer_height: 90, // clamped to 40
            inner_height: 12,
        }];
        let reports = waveform_reports(2, &columns);
        assert_eq!(reports.len(), 73);
        assert_eq!(&reports[0][..6], &[0x20, 0x2c, 1, 0, 73, 0]);
        // 0x80 marker, then the first column: red 0xF800, green 0x07E0 (LE).
        assert_eq!(
            &reports[0][6..14],
            &[0x80, 0x00, 0xf8, 0xe0, 0x07, 40, 12, 0]
        );
        // Everything after the first column is an empty column.
        assert!(reports[0][14..].iter().all(|b| *b == 0));
        assert!(reports[72][6..].iter().all(|b| *b == 0));
    }

    #[test]
    fn rgb565_packs_the_top_bits_of_each_channel() {
        assert_eq!(rgb565(0, 0, 0), 0);
        assert_eq!(rgb565(255, 255, 255), 0xffff);
        assert_eq!(rgb565(0, 0, 255), 0x001f);
    }

    #[test]
    fn setup_messages_are_well_formed_sysex() {
        for message in setup_messages() {
            assert_eq!(message[..9], PREFIX);
            assert_eq!(*message.last().unwrap(), 0xf7);
            assert!(message[1..message.len() - 1].iter().all(|b| *b < 0x80));
        }
    }
}
