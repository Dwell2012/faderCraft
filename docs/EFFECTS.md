# Deck effects

Reference: https://virtualdj.com/manuals/virtualdj/interface/decks/decksadvanced/effects.html

Six persistent serial effect slots per deck, after EQ/stems and before the volume
fader/crossfader. Slot 1 is reserved for the always-on Filter, with a linked FILTER
knob in each mixer channel. Centre is neutral; double-click the mixer knob to
return to centre. The FX button and controller toggle also reset it to centre,
shown as OFF in the UI and controller LED; turning the sweep engages it again.
Resonance is preserved when resetting. Slots 2–6 remain assignable. Click their effect name to toggle; use the adjacent arrow to
select an effect. FX x3 shows three slots with mix dials, FX x1 shows the selected
slot with two dials, FX x3 Full shows three with two dials each, and FX x6 shows all
six in two columns. Switch views using the FX label or its view dropdown. Hidden
slots keep running. The active count includes all six slots.

The ellipsis opens the slot's details beside the music browser. Closing details
never disables the effect. Details expose mix, the main parameter, feedback where
applicable, beat sync and tails. Drag dials vertically or scroll, with Shift-wheel
for fine adjustment; double-click resets to midpoint. In synced mode, the timing
dial selects beat divisions. Settings/views currently last for this app session.

## Effects bank

| Effect | Main parameter / behaviour |
| --- | --- |
| Filter | Low-pass left, neutral centre, high-pass right |
| Echo | Repeating stereo delay, adjustable feedback |
| Delay | Single delayed copy, no feedback |
| Ping-pong Echo | Mono input send; repeats alternate left/right |
| Low-cut Echo | Repeats high-passed at 300 Hz |
| Echo Out | Dry signal/send fades into echoes; 100% mix gives a full exit |
| Reverb | Stereo four-line feedback-network reverb; size and decay feedback |
| Flanger | Short modulated delay; rate and feedback |
| Chorus | Stereo-offset longer modulated delays; rate |
| Phaser | Six all-pass stages; rate and feedback |
| Cut | Soft-edged rhythmic gate |
| Tremolo | Sinusoidal amplitude modulation |
| Auto-pan | Opposed left/right sinusoidal modulation |
| Distortion | Soft saturation with drive compensation |
| Bitcrusher | Bit-depth reduction plus sample-and-hold |
| Noise | Filtered noise follows incoming audio level |
| Pitch Shift | Dual-grain -12 to +12 semitones, neutral centre |
| Spiral | Delay with +7-semitone pitch shift in the feedback path |
| Beat Repeat | Captures preceding audio; fills a slice first if needed; toggle to recapture |
| Brake | Sound slows to silence over the selected time; 100% mix for a full stop |

Beat sync uses original analysed BPM multiplied by current deck tempo. Divisions
range from 1/16 to 2 beats. Without a grid the engine uses manual timing and the
UI indicates this. Modulation is rate-synced, not phase-locked to beat markers.
Timing changes are smoothed and can produce pitch sweeps on delay effects.
Beat Repeat resamples its captured slice when timing changes. Brake is an audio
effect: the transport keeps advancing underneath, as with Beat Repeat.
Pitch Shift/Spiral are granular creative effects and may colour transients;
they are independent of the deck's key-lock processor.

## Tails and signal routing

Delay/reverb tails continue after pause and, with Tails ON, after bypass. Bypassing
fades new input into the effect out over 10 ms and lets its stored signal decay.
Bypass tails have a 30-second maximum with a final one-second fade. Tails OFF clears
the tail once the source stops and fades it out on bypass. Replacing a track clears
all histories. Changing a slot's effect also clears that slot's history.
Pre-fader tails are muted by the channel fader, crossfader or master. The live-load
warning accounts for pending tails, including the gap before the first repeat;
it conservatively reserves a 30-second tail window after input.

All delay memory is allocated during engine construction. Protocol validation
rejects invalid slot, parameter, BPM, division and feedback values. Feedback is
limited to 90%. No dependencies or external plugins were added.

This implements our own DSP, not a sample-identical reproduction of VirtualDJ.
Master effects, post-fader routing, stem-specific routing, floating detail windows,
custom effect lists remain outside this bank. Pads 16 is now available from the FX view selector; see PADS.md.
Loop Record continues to export original source audio without effects.

The Filter's first knob is its sweep: 0–49% moves low-pass cutoff from 20 Hz to 20 kHz; 50% is neutral; 51–100% moves high-pass cutoff from 20 Hz to 20 kHz. Turning the sweep applies full filtering; the expanded Mix control can blend dry audio back in. Cutoff is limited below Nyquist at low sample rates.

Echo, Delay, Ping Pong, Low Cut Echo, Spiral and Reverb add their return to the unchanged dry track. Their amount controls the return level rather than fading the dry track away. Echo Out, Brake and rhythmic gating retain their intentional fades/cuts.


Filter also has a per-slot Resonance dial (inline in expanded views and in slot
details). It ranges from 0% (Butterworth Q = 0.707) to 100% (Q = 4);
New effects start at 50% resonance; double-click resets resonance to 50%. Higher resonance emphasizes frequencies
around the cutoff and increases peak level, so allow some gain headroom. Older
snapshots default to zero resonance. Changes use the existing 10 ms smoothing.

The two sweeps now have separate curves: normalized travel away from centre is
raised to 1.2 for LP and 1.4 for HP before logarithmic frequency mapping. LP moves
from 20 kHz down to 20 Hz; HP moves from 20 Hz up to 20 kHz. Halfway along each
active sweep gives roughly 990 Hz LP and 274 Hz HP. This gives a gentle departure
from centre and more HP travel for bass removal. The centre transition retains
its dry blend, and the displayed nominal cutoff uses the same curve as the DSP.
These are original tuning choices, not a reproduction of Beatport's code.
