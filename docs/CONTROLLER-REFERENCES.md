# Controller references

## XDJ-RX3

Primary source: [AlphaTheta MIDI Message List E1](https://downloads.support.alphatheta.com/software_info/all-in-one-dj-systems/XDJ-RX3/XDJ-RX3_MIDI_Message_List_E1.pdf).
Reviewed the rendered deck and mixer tables (PDF pages 2-3).
Local reference copy: `../../references/XDJ-RX3_MIDI_Message_List_E1.pdf`.

Selected device-to-computer messages, in hexadecimal. Mixer status is `B4`.
Values span 0-127 unless noted.

| Control | Deck 1 CC | Deck 2 CC |
|---|---|---|
| Trim | 01 | 06 |
| High EQ | 02 | 07 |
| Mid EQ | 03 | 08 |
| Low EQ | 04 | 09 |
| Channel fader | 11 | 12 |
| Headphone cue | 46 | 47 |

Shared CCs: crossfader `0B`, master `18`, headphone level `1A`,
cue/master mix `1B`. Cue buttons send 0/127.

Deck status uses channel 1/2 (`90/91` notes, `B0/B1` CC).
Play/pause is note `00`, cue `01`, jog touch `20`.
Jog platter CC `22` and rim CC `21` encode relative movement around `40`.
Tempo combines CC `00` MSB and `20` LSB into a 14-bit value.
The document also specifies LED and channel-meter feedback.

## Implementation decisions

Use these published addresses to build an initial RX3 profile instead of
requiring MIDI Learn for every control. Keep MIDI Learn for customisation.
This is a reference specification, not an implemented or hardware-tested map.

Maintain tempo-pair state separately per port and deck, and reset it on
disconnect. Test partial pairs and message ordering against real captures.
Handle relative jog movement separately from absolute faders; calibrate
scratch speed and touch-release behaviour on the device.

Keep the RX3's hardware audio processing and software mixer ownership explicit
to avoid applying gain/EQ twice. MIDI addresses do not establish audio routing,
latency, driver availability or touchscreen integration.

## ZIP supplied by the user

URL: https://mixxx.discourse.group/uploads/short-url/uCrfDSyRkqyvmLqH4wqX8geDWP5.zip

The website blocked downloading. The user subsequently supplied
`C:/Users/Ian Llewellyn/Downloads/Mixxx-Mapping-Numark-MixTrack-Pro-II.zip`,
inspected read-only on 15 October 2026 without executing the JavaScript.
SHA-256: `DA75CE720BCEF31ED0D85D4506CFD1AC7B6D1BA610E271F8ED16F89F80B035B8`.

Contents: six-page PDF guide, `Numark MixTrack Pro II.midi.xml` (preset v1.0,
Mixxx 1.11.0+) and `Numark-Mixtrack-Pro-II-scripts.js`. The guide credits
Armen Rizal and MIDI specifications supplied by Ricecows. The script credits
multiple contributors. No explicit licence grant was found in the supplied
files; do not incorporate the script or distribute the preset as MIT code.

### Mixtrack Pro II candidate profile

These are community mapping observations, not manufacturer-verified or
hardware-tested compatibility. The archive identifies Pro II; it does not
establish that the user's device is Pro II rather than the original model.
Both decks use status `B0` for these CCs (all numbers hexadecimal):

| Control | Deck A | Deck B |
|---|---|---|
| High EQ | 10 | 11 |
| Mid EQ | 12 | 13 |
| Low EQ | 14 | 15 |
| Channel fader | 16 | 07 |
| Pitch fader | 0D | 0E |
| Jog movement | 19 | 18 |
| Beats encoder, repurposed as trim | 21 | 22 |

Shared CCs: master `17`, crossfader `0A`, headphone level `0B`, cue/master
mix `0C`. Unlike the RX3, deck identity comes from control numbers here.

Notes use status `90`: play A/B `3B/42`, transport cue `33/3C`, headphone
cue `51/52`, jog touch `4D/4E`, shift `61/62`, load `4B/34`.
The XML also maps status `80` for headphone cue. Preserve release semantics.

The script treats jog values above 63 as negative by subtracting 128;
this differs from the RX3's offset-around-64 encoding. Pitch is a single
7-bit value with centre 64, unlike the RX3's paired 14-bit messages.
Trim assignments use relative encoder handling, not absolute fader scaling.
The script sends note-status LED feedback with 100 for on and 0 for off;
verify the relevant LED addresses and values on hardware.

### Behaviour to validate independently

The script header reports missing cue-point LED addressing, broken sidebar
selection and play trouble after braking. It forces key lock on after
scratching rather than reliably restoring the previous state. Our acceptance
checks must cover these interactions without inheriting those behaviours.

Build an independent Rust profile from validated protocol facts. Confirm
device model, capture real messages and verify transport, EQ/fader endpoints,
relative encoder direction, jog release and LED feedback before enabling it
as a tested factory profile. The archive's custom FX and pad behaviours are
reference choices, not new user feature requirements.
