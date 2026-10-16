use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig, SupportedStreamConfig};

use crate::EngineError;
use crate::mixer::Mixer;
use crate::track::CHANNELS;

/// Frames rendered per pass when the device format needs converting.
const CONVERT_FRAMES: usize = 1024;

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
    let config = device
        .default_output_config()
        .map_err(|e| EngineError::Device(e.to_string()))?;
    Ok((device, config))
}

pub(crate) fn start(
    device: &cpal::Device,
    config: SupportedStreamConfig,
    mixer: Mixer,
    buffer_frames: u32,
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
        SampleFormat::F32 => build::<f32>(device, config, mixer),
        SampleFormat::I16 => build::<i16>(device, config, mixer),
        SampleFormat::U16 => build::<u16>(device, config, mixer),
        SampleFormat::F64 => build::<f64>(device, config, mixer),
        SampleFormat::I24 => build::<cpal::I24>(device, config, mixer),
        SampleFormat::I32 => build::<i32>(device, config, mixer),
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
) -> Result<cpal::Stream, EngineError>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = config.channels as usize;
    let mut stereo = vec![0f32; CONVERT_FRAMES * CHANNELS];
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                for chunk in data.chunks_mut(CONVERT_FRAMES * channels) {
                    let frames = chunk.len() / channels;
                    let stereo = &mut stereo[..frames * CHANNELS];
                    mixer.render(stereo);
                    for (out, input) in chunk.chunks_mut(channels).zip(stereo.chunks(CHANNELS)) {
                        if channels == 1 {
                            out[0] = T::from_sample((input[0] + input[1]) * 0.5);
                        } else {
                            out[0] = T::from_sample(input[0]);
                            out[1] = T::from_sample(input[1]);
                            out[2..].fill(T::EQUILIBRIUM);
                        }
                    }
                }
            },
            |error| eprintln!("audio output error: {error}"),
            None,
        )
        .map_err(|e| EngineError::Device(e.to_string()))
}
