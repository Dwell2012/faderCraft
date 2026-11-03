use std::path::Path;
use std::sync::Arc;

use crate::Track;
use crate::track::DecodeError;

/// Audio and timing for one Rhythm Shift pack. Return points are measured in
/// beats from the start of each roll, independently of the loop length.
pub struct Pack {
    pub bpm: f64,
    pub loop_beats: f64,
    pub loop_audio: Arc<Track>,
    pub rolls: [Arc<Track>; 4],
    pub return_beats: [f64; 4],
}

impl Pack {
    pub fn load(
        folder: &Path,
        sample_rate: u32,
        bpm: f64,
        loop_beats: f64,
        return_beats: [f64; 4],
    ) -> Result<Self, DecodeError> {
        let loop_audio = Track::decode(&folder.join("loop.wav"), sample_rate)?;
        let mut rolls = Vec::with_capacity(4);
        for index in 1..=4 {
            rolls.push(Track::decode(
                &folder.join(format!("roll-{index}.wav")),
                sample_rate,
            )?);
        }
        Ok(Self {
            bpm,
            loop_beats,
            loop_audio,
            rolls: rolls
                .try_into()
                .unwrap_or_else(|_| unreachable!("four rolls")),
            return_beats,
        })
    }

    pub fn valid(&self, sample_rate: u32) -> bool {
        self.bpm.is_finite()
            && self.bpm > 20.
            && self.bpm < 300.
            && self.loop_beats.is_finite()
            && self.loop_beats > 0.
            && self.loop_beats * 60. / self.bpm * sample_rate as f64
                <= self.loop_audio.frames() as f64
            && self
                .rolls
                .iter()
                .zip(self.return_beats)
                .all(|(roll, point)| {
                    point.is_finite()
                        && point > 0.
                        && point * 60. / self.bpm * (sample_rate as f64) < roll.frames() as f64
                })
    }
}

#[derive(Default)]
pub struct Voice {
    pub enabled: bool,
    pub pending: bool,
    pub pending_roll: Option<usize>,
    pub roll: Option<usize>,
    pub phase: f64,
    pub roll_phase: f64,
    pub tail: Option<(usize, f64)>,
}

impl Voice {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn sample(&mut self, pack: &Pack, step: f64, sample_rate: u32) -> [f32; 2] {
        if !self.enabled {
            return [0.; 2];
        }
        if let Some(index) = self.roll {
            let return_frame = pack.return_beats[index] * 60. / pack.bpm * sample_rate as f64;
            if self.roll_phase >= return_frame {
                self.roll = None;
                self.tail = Some((index, self.roll_phase));
                self.phase = 0.;
            }
        }
        let mut output = if let Some(index) = self.roll {
            let audio = &pack.rolls[index];
            let sample = stereo(audio, self.roll_phase);
            self.roll_phase += step;
            sample
        } else {
            let sample = stereo(&pack.loop_audio, self.phase);
            let loop_frames = pack.loop_beats * 60. / pack.bpm * sample_rate as f64;
            self.phase = (self.phase + step).rem_euclid(loop_frames);
            sample
        };
        if let Some((index, position)) = self.tail {
            let audio = &pack.rolls[index];
            let tail = stereo(audio, position);
            output[0] += tail[0];
            output[1] += tail[1];
            let next = position + step;
            self.tail = (next < audio.frames() as f64).then_some((index, next));
        }
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roll_returns_to_loop_with_crash_on_the_same_frame() {
        let rate = 100;
        let loop_audio = Arc::new(Track::from_samples("loop", rate, vec![0.25; 800]));
        let mut roll_samples = vec![0.5; 1600];
        roll_samples[800] = 1.;
        roll_samples[801] = 1.;
        roll_samples[802..].fill(0.);
        let roll = Arc::new(Track::from_samples("roll", rate, roll_samples));
        let pack = Pack {
            bpm: 60.,
            loop_beats: 4.,
            loop_audio,
            rolls: std::array::from_fn(|_| roll.clone()),
            return_beats: [4.; 4],
        };
        assert!(pack.valid(rate));
        let mut voice = Voice {
            enabled: true,
            roll: Some(0),
            ..Voice::default()
        };
        for _ in 0..400 {
            assert_eq!(voice.sample(&pack, 1., rate), [0.5; 2]);
        }
        assert_eq!(voice.sample(&pack, 1., rate), [1.25; 2]);
        assert_eq!(voice.sample(&pack, 1., rate), [0.25; 2]);
    }

    #[test]
    fn return_point_is_independent_of_loop_length() {
        let rate = 100;
        let loop_audio = Arc::new(Track::from_samples("loop", rate, vec![0.; 1600]));
        let roll = Arc::new(Track::from_samples("roll", rate, vec![0.; 2400]));
        let pack = Pack {
            bpm: 60.,
            loop_beats: 8.,
            loop_audio,
            rolls: std::array::from_fn(|_| roll.clone()),
            return_beats: [8.; 4],
        };
        assert!(pack.valid(rate));
    }
}

fn stereo(track: &Track, position: f64) -> [f32; 2] {
    let index = position.floor() as usize;
    if index >= track.decoded_frames() {
        return [0.; 2];
    }
    let next = (index + 1).min(track.decoded_frames() - 1);
    let fraction = (position - index as f64) as f32;
    [0, 1].map(|channel| {
        let a = track.sample(index * 2 + channel);
        let b = track.sample(next * 2 + channel);
        a + (b - a) * fraction
    })
}
