use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig, SupportedStreamConfig};

use crate::EngineError;
use crate::mixer::Mixer;
use crate::track::CHANNELS;

/// Frames rendered per pass when the device format needs converting.
const CONVERT_FRAMES: usize = 1024;

/// Zero-based left channels of non-overlapping stereo pairs on one device.
#[derive(Clone, Copy, Debug, Default)]
pub struct OutputRouting {
    pub master: u16,
    pub cue: Option<u16>,
}
impl OutputRouting {
    fn required_channels(self) -> Result<u16, EngineError> {
        let invalid = || {
            EngineError::Device("Choose distinct stereo pairs for master and headphones.".into())
        };
        if self.master % 2 != 0 || self.cue.is_some_and(|c| c % 2 != 0 || c == self.master) {
            return Err(invalid());
        }
        self.master
            .max(self.cue.unwrap_or(0))
            .checked_add(2)
            .ok_or_else(invalid)
    }
}

#[cfg(test)]
mod routing_tests {
    use super::OutputRouting;
    #[test]
    fn pairs_are_distinct_aligned_and_do_not_overflow() {
        assert_eq!(OutputRouting::default().required_channels().unwrap(), 2);
        assert_eq!(
            OutputRouting {
                master: 0,
                cue: Some(2)
            }
            .required_channels()
            .unwrap(),
            4
        );
        assert_eq!(
            OutputRouting {
                master: 4,
                cue: Some(0)
            }
            .required_channels()
            .unwrap(),
            6
        );
        for routing in [
            OutputRouting {
                master: 0,
                cue: Some(0),
            },
            OutputRouting {
                master: 1,
                cue: None,
            },
            OutputRouting {
                master: 0,
                cue: Some(3),
            },
            OutputRouting {
                master: u16::MAX - 1,
                cue: None,
            },
        ] {
            assert!(routing.required_channels().is_err());
        }
    }
}

fn host(asio: bool) -> Result<cpal::Host, EngineError> {
    if asio {
        #[cfg(target_os = "windows")]
        {
            return cpal::host_from_id(cpal::HostId::Asio)
                .map_err(|e| EngineError::Device(e.to_string()));
        }
        #[cfg(not(target_os = "windows"))]
        {
            return Err(EngineError::Device(
                "ASIO is available on Windows only".into(),
            ));
        }
    }
    Ok(cpal::default_host())
}
/// Enumerates installed output drivers/devices for the selected backend.
pub fn output_devices(asio: bool) -> Result<Vec<String>, EngineError> {
    let host = host(asio)?;
    let devices = host
        .output_devices()
        .map_err(|e| EngineError::Device(e.to_string()))?;
    Ok(devices
        .filter_map(|d| d.description().ok().map(|d| d.name().to_owned()))
        .collect())
}
pub(crate) fn device(
    asio: bool,
    name: Option<&str>,
    routing: OutputRouting,
) -> Result<(cpal::Device, SupportedStreamConfig), EngineError> {
    let host = host(asio)?;
    let device = if let Some(name) = name.filter(|s| !s.is_empty()) {
        host.output_devices()
            .map_err(|e| EngineError::Device(e.to_string()))?
            .find(|d| d.description().is_ok_and(|d| d.name() == name))
            .ok_or_else(|| EngineError::Device(format!("Output device '{name}' is unavailable")))?
    } else {
        host.default_output_device()
            .ok_or(EngineError::NoOutputDevice)?
    };
    let mut config = device
        .default_output_config()
        .map_err(|e| EngineError::Device(e.to_string()))?;
    let required = routing.required_channels()?;
    // Some ASIO drivers report an unknown (zero) rate until a stream is opened.
    // Never construct DSP at zero Hz; negotiate a rate the device advertises.
    if config.sample_rate() == 0 {
        config = known_rate_config(
            device
                .supported_output_configs()
                .map_err(|e| EngineError::Device(e.to_string()))?,
            config.channels().max(required),
        )?;
    }
    if config.channels() < required
        && !(config.channels() == 1 && routing.master == 0 && routing.cue.is_none())
    {
        let rate = config.sample_rate();
        config = device.supported_output_configs().map_err(|e| EngineError::Device(e.to_string()))?
            .filter(|c| c.channels() >= required && c.min_sample_rate() <= rate && c.max_sample_rate() >= rate)
            .min_by_key(|c| c.channels())
            .map(|c| c.with_sample_rate(rate))
            .ok_or_else(|| EngineError::Device(format!("Selected routing needs {required} output channels at {rate} Hz. Choose a multichannel device/ASIO driver or disable headphone routing.")))?;
    }
    Ok((device, config))
}

fn known_rate_config(
    configs: impl IntoIterator<Item = cpal::SupportedStreamConfigRange>,
    channels: u16,
) -> Result<SupportedStreamConfig, EngineError> {
    configs.into_iter()
        .filter(|c| c.channels() == channels && c.min_sample_rate() > 0 && c.max_sample_rate() >= c.min_sample_rate())
        .map(|c| { let rate = 48_000u32.clamp(c.min_sample_rate(), c.max_sample_rate()); c.with_sample_rate(rate) })
        .min_by_key(|c| c.sample_rate().abs_diff(48_000))
        .ok_or_else(|| EngineError::Device("Audio driver reported an unknown sample rate and no supported rate for the selected channels.".into()))
}

#[cfg(test)]
mod rate_tests {
    use super::*;
    fn range(channels: u16, rate: u32) -> cpal::SupportedStreamConfigRange {
        cpal::SupportedStreamConfigRange::new(
            channels,
            rate,
            rate,
            cpal::SupportedBufferSize::Unknown,
            SampleFormat::F32,
        )
    }
    #[test]
    fn unknown_rate_uses_supported_rate_and_preserves_channels() {
        let selected = known_rate_config(
            [
                range(2, 48000),
                range(8, 0),
                range(8, 44100),
                range(8, 48000),
            ],
            8,
        )
        .unwrap();
        assert_eq!((selected.channels(), selected.sample_rate()), (8, 48000));
        assert_eq!(
            known_rate_config([range(8, 44100)], 8)
                .unwrap()
                .sample_rate(),
            44100
        );
        assert!(known_rate_config([range(8, 0), range(2, 48000)], 8).is_err());
    }
}

pub(crate) fn start(
    device: &cpal::Device,
    config: SupportedStreamConfig,
    mixer: Mixer,
    buffer_frames: u32,
    routing: OutputRouting,
) -> Result<cpal::Stream, EngineError> {
    let format = config.sample_format();
    if buffer_frames > 0 {
        if let cpal::SupportedBufferSize::Range { min, max } = config.buffer_size() {
            if buffer_frames < *min || buffer_frames > *max {
                return Err(EngineError::Device(format!(
                    "Buffer {buffer_frames} is unsupported; driver range is {min}–{max} frames. Choose Driver default."
                )));
            }
        }
    }
    let mut config: StreamConfig = config.into();
    if buffer_frames > 0 {
        config.buffer_size = cpal::BufferSize::Fixed(buffer_frames);
    }
    let stream = match format {
        SampleFormat::F32 => build::<f32>(device, config, mixer, routing),
        SampleFormat::I16 => build::<i16>(device, config, mixer, routing),
        SampleFormat::U16 => build::<u16>(device, config, mixer, routing),
        SampleFormat::F64 => build::<f64>(device, config, mixer, routing),
        SampleFormat::I24 => build::<cpal::I24>(device, config, mixer, routing),
        SampleFormat::I32 => build::<i32>(device, config, mixer, routing),
        other => Err(EngineError::Device(format!(
            "unsupported sample format {other}"
        ))),
    }?;
    stream
        .play()
        .map_err(|e| EngineError::Device(e.to_string()))?;
    Ok(stream)
}

fn build<T>(
    device: &cpal::Device,
    config: StreamConfig,
    mut mixer: Mixer,
    routing: OutputRouting,
) -> Result<cpal::Stream, EngineError>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = config.channels as usize;
    let mut stereo = vec![0f32; CONVERT_FRAMES * CHANNELS];
    let mut cue = vec![0f32; CONVERT_FRAMES * CHANNELS];
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                for chunk in data.chunks_mut(CONVERT_FRAMES * channels) {
                    let frames = chunk.len() / channels;
                    let stereo = &mut stereo[..frames * CHANNELS];
                    let cue = &mut cue[..frames * CHANNELS];
                    mixer.render_with_monitor(stereo, Some(cue));
                    for ((out, input), cue) in chunk
                        .chunks_mut(channels)
                        .zip(stereo.chunks(CHANNELS))
                        .zip(cue.chunks(CHANNELS))
                    {
                        out.fill(T::EQUILIBRIUM);
                        if channels == 1 {
                            out[0] = T::from_sample((input[0] + input[1]) * 0.5);
                        } else {
                            let master = routing.master as usize;
                            out[master] = T::from_sample(input[0]);
                            out[master + 1] = T::from_sample(input[1]);
                            if let Some(pair) = routing.cue {
                                out[pair as usize] = T::from_sample(cue[0]);
                                out[pair as usize + 1] = T::from_sample(cue[1]);
                            }
                        }
                    }
                }
            },
            |error| eprintln!("audio output error: {error}"),
            None,
        )
        .map_err(|e| EngineError::Device(e.to_string()))
}
