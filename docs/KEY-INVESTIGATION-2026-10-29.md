# Eyeball: audio and MIDI key investigation, 29 October 2026

Subsequent implementation: [estimated mode support](MODES.md) was added after
this investigation at the user's request. The full Eyeball recording still
leaves the mode unresolved; its result was not overridden to fit the hypothesis.

## Finding

The supplied audio and independent MIDI transcription both contain substantial
G-natural material, with E, B, D and C-sharp in the melody. They do not support
treating a website's E-major label as established ground truth.

An E-centred, Dorian-compatible reading is a useful working hypothesis. B minor
shares the same scale notes as E Dorian, explaining why these labels warrant
comparison rather than a simple vendor majority vote. A pitch-class collection
does not itself establish a tonic or mode: E Dorian is not a proven final label.
Our existing E-minor result captures the E-centred minor quality but cannot
express modal distinctions because the detector only compares 24 major/minor
profiles. No production detector, analysis version, stored key or music tag was
changed in this investigation.

## Inputs and prior evidence

- Local audio: `Sunburst - Eyeball (Eyeball Paul's Theme) (Original Mix)(2000).flac`.
  Duration 523.136 seconds. SHA-256:
  `AD76D9CFE9030CB8C315085AE91243258FC06FB8890B74F2C389CF430A927182`.
- User-supplied transcription:
  `Sunburst-Eyeball-Eyeball-Paul-s-Theme-v2-RailwayWater-20190327190100-nonstop2k.com.mid`.
  SHA-256:
  `DB71AAECEC874D22E71007DDE70EF2832C7D8CB8641BFE4D0A2B2703D4804EC4`.
  The file was parsed as MIDI data, not executed or redistributed.
- Previous measurements and vendor-label provenance are in
  [the session handover](SESSION-2026-10-29.md). faderCraft reported E minor,
  VirtualDJ F-sharp minor, and the user supplied an E-major Beatport listing.
  The user subsequently reported Tunebat's B-minor label and an E-major label
  associated with the MIDI. Those external labels were not independently verified
  in this follow-up; the actual audio and MIDI were inspected directly.

## Audio observations

The engine decoded the file to mono at 22050 Hz. The diagnostic then examined
spectral peaks without calling our production key-profile matcher. The broad
scan used a 16384-sample Hann window with 4096-sample steps, logarithmic
three-bin frequency interpolation, a 2.5% relative amplitude threshold and a
30-cent note tolerance. It rejected likely upper integer harmonics of stronger
lower peaks before retaining up to three peaks in the 180–1000 Hz band.

This is an exploratory heuristic, not source separation. It can discard genuine
octaves/fifths, miss detuned synth voices, or retain percussion. Relative peak
sums and overlapping frames are not probabilities or independent observations.

Longer 32768-sample windows starting at the times below gave these direct peaks:

| Window start | Selected peaks | Interpretation |
| --- | --- | --- |
| 2:44 | 81.45 Hz, 326.05 Hz, 393.61 Hz | E bass/upper E with G-natural material |
| 2:55 | 194.59, 197.10, 392.98, 784.55 Hz | G-natural across several octaves, with synth detuning |
| 5:38 | 655.36 Hz and 551.35 Hz | E5 and C-sharp5 material |
| 5:46 | 194.71, 196.90, 392.75 Hz | Repeated G-natural evidence in another passage |

For comparison, equal-tempered G3/G4/G5 are approximately 196/392/784 Hz;
G-sharp3/G-sharp4/G-sharp5 are approximately 208/415/831 Hz. The observed G
families are not merely a fifth harmonic of an E fundamental. Conversely, an
E fundamental's fifth harmonic can resemble G-sharp, so isolated G-sharp peaks
must not automatically be interpreted as a major third.

The independent-peak scan of 5:30–5:50 accumulated G 57.7, G-sharp 8.3,
C-sharp 14.7 and C 1.6 relative-level units. These arbitrary sums describe this
diagnostic only. They support inspecting G-natural and C-sharp together rather
than claiming an exclusively natural-minor scale.

A separate one-second-spaced bass check found E as the strongest qualifying
70–150 Hz tonal peak in all 120 windows starting from 0:30 through 2:29.
Other sections have changing bass/pitch emphasis. A prolonged E pedal supports
an E-centred interpretation but is not sufficient to prove a whole-track tonic.

## What the MIDI actually contains

The file is Standard MIDI format 1, with 13 tracks and 96 ticks per quarter note.
Its tempo event is 434783 microseconds per quarter note (approximately 138 BPM).
There is **no key-signature meta event**. An E-major website label therefore
cannot be attributed to an embedded E-major signature in this file.

Excluding General MIDI percussion channel 10, the note-on/off pairs contain:

| Pitch class | Note count | Summed duration, MIDI ticks |
| --- | ---: | ---: |
| E | 870 | 26163 |
| G | 644 | 20928 |
| B | 772 | 22080 |
| D | 383 | 9936 |
| C-sharp | 204 | 4230 |
| F-sharp | 205 | 4233 |
| G-sharp | 2 | 6 |
| D-sharp | 2 | 6 |
| C | 1 | 3 |

The two tiny G-sharp events are in the Koto track, not the sustained lead phrase.
The Voice/Piano 2 lead phrase is G4–F-sharp4–E4–B3–D4–C-sharp4–B3–E4.
Its first occurrence in Voice starts at beat zero; Piano 2 repeats it from beat
96. The bass and first piano also repeatedly alternate E/G and G/B material.

Pitch bends and pitch-bend-range controllers were checked rather than assuming
every note number always sounds at its nominal pitch. Main pitched channels
remain centred through the cited lead phrases. Non-centred bends occur in a
late effect passage at beats 260–264, with a 12-semitone bend range. Counts above
are nominal note events, not a bend-corrected transcription of that late effect.

The MIDI's arrangement and tempo differ from the full audio, and a third-party
transcription can be wrong. Nevertheless, its principal unbent melody agrees
with the G-natural/C-sharp evidence in the local audio. It is not strong evidence
for E major, whose characteristic G-sharp/D-sharp are almost absent from the
transcribed main parts.

## Reproduction and validation

Use fresh output paths; the exporter does not overwrite files:

```powershell
. ./tools/dev-shell.ps1
cargo run -p djcraft-analysis --example key_pcm --locked -- "<audio.flac>" "<new-mono.f32>" 22050
python tools/key_evidence.py --self-test
python tools/key_evidence.py "<new-mono.f32>" "<new-output-directory>" 22050
```

The Python diagnostic requires NumPy. It writes a timestamped/register-preserving
`independent-peaks.csv` and prints 30-second summaries. The self-test distinguishes
synthetic E/G/B and E/G-sharp/B harmonic controls and handles silence. These
controls validate the diagnostic mechanism; they are not validation against a
representative corpus of real music. The longer-window spot checks and MIDI
parsing described above were separate read-only diagnostic runs.

## Decision

Keep the detector and saved result unchanged. Do not introduce a Sunburst
override, transpose the file to match a label, or reinterpret profile correlation
as calibrated confidence. The next algorithm decision needs a labelled corpus,
tonal-centre evaluation and explicit consideration of modal music; candidate
ambiguity in the UI would be more honest than a vendor-specific correction.
