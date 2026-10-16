//! Track analysis: signal processing on decoded audio, with no file or
//! database access. Callers decode a track, pass the samples in, and store
//! what comes back.

mod fingerprint;
mod key;
mod structure;
mod tempo;

pub use fingerprint::{Fingerprint, fingerprint};
pub use key::{Key, detect_key};
pub use structure::{Cue, MAX_CUES, SectionLabel, Structure, detect_structure, label_sections};
pub use tempo::{BEATS_PER_BAR, Tempo, TempoRange, detect_tempo};

mod vocals;
pub use vocals::detect_vocal_regions;

mod loudness;
pub use loudness::measure_loudness;
