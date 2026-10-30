# Estimated musical modes

Key analysis retains a separate estimate for Ionian, Dorian, Phrygian,
Lydian, Mixolydian, Aeolian or Locrian. Deck headers, library cells and the
four-deck layout retain compact key/Camelot labels. Hover over a key to see
its estimated mode or "Mode unresolved". The tooltip follows manual key shift
and unlocked tempo changes, including fractional detuning in deck tooltips.
No mode-status text takes permanent space alongside the controls.

## Detection and limits

The existing profile matcher first estimates a tonic. At that tonic, the modal
pass checks the measured third, compares profiles for the corresponding chord
quality, requires a score margin of at least 0.025, and checks characteristic
degrees against their alternatives. Characteristic energy must exceed 12% of
the strongest non-tonic pitch class and be 1.4 times its alternative. A loud
tonic pedal is excluded from that reference level. Sparse triads and flat
pitch-class distributions leave the mode unresolved.

Modal profiles are heuristic alterations of the existing major/minor profiles:
raised sixth for Dorian, lowered second for Phrygian, raised fourth for Lydian,
lowered seventh for Mixolydian, and lowered second/fifth for Locrian. They are
not independently measured modal listening profiles. Scores and margins are
not calibrated probabilities. The tonic still comes from the original matcher;
this first implementation does not independently resolve relative modal tonics,
modulations or arbitrary non-diatonic scales. Requiring distinguishing notes
is intended to avoid assigning a mode from a bare triad, not to guarantee that
drums, overtones and production effects cannot mislead the estimates.

Tests cover all seven modes in transposed pitch-class examples, rejection of
ambiguous material, and a decoded-signal-style synthetic Dorian melody over an
E drone. Existing major/minor, noise, silence, detuning and sample-rate tests
remain in place. A labelled real-music corpus is still needed before claiming
broad mode-detection accuracy. The full Eyeball recording remains unresolved
under these checks: this feature does not hardcode its investigation hypothesis.

## Library and compatibility

Schema migration 21 adds nullable `tracks.key_modal` JSON, containing tonic,
mode, score and margin. The legacy detected key and Camelot representation are
retained. There is no exact Camelot-wheel entry for every mode: sorting,
harmonic suggestions, existing exports and tag review continue to use the
tonic's major/minor compatibility estimate. The modal display must not be read
as a new exact Camelot assignment.

Manual keys and preferred tagged keys take precedence over modal analysis.
Analysis and modal metadata do not write music tags. A new key-analysis result
clears stale modal data before saving its replacement; an unresolved result
stores no modal label. Analysis version 12 schedules existing tracks for the
normal background refresh. User-authored cues and manual keys stay protected.
