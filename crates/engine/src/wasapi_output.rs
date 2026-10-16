//! Direct event-driven WASAPI exclusive output. All COM objects stay on their worker.
use crate::{Engine, EngineError, Mixer};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::thread::{self, JoinHandle};
use wasapi::{AudioClient, DeviceEnumerator, Direction, SampleType, StreamMode, WaveFormat};

pub(crate) struct Stream {
    stop: Arc<AtomicBool>,
    failed: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}
impl Stream {
    pub fn failed(&self) -> bool {
        self.failed.load(Ordering::Relaxed)
    }
}
impl Drop for Stream {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
struct Com;
impl Drop for Com {
    fn drop(&mut self) {
        wasapi::deinitialize();
    }
}

fn failure(error: impl std::fmt::Display) -> EngineError {
    EngineError::Device(format!(
        "WASAPI exclusive: {error}. Close other apps using this output, or choose WASAPI shared in Settings and restart."
    ))
}

pub(crate) fn open(name: Option<&str>, frames: u32) -> Result<Engine, EngineError> {
    let name = name.unwrap_or_default().to_owned();
    let stop = Arc::new(AtomicBool::new(false));
    let failed = Arc::new(AtomicBool::new(false));
    let worker_stop = stop.clone();
    let worker_failed = failed.clone();
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let (mixer_tx, mixer_rx) = mpsc::sync_channel::<Mixer>(1);
    let (started_tx, started_rx) = mpsc::sync_channel(1);
    let worker = thread::Builder::new().name("wasapi-exclusive".into()).spawn(move || {
        let prepared = (|| -> Result<_, String> {
            wasapi::initialize_mta().ok().map_err(|e| e.to_string())?;
            let com = Com;
            let enumerator = DeviceEnumerator::new().map_err(|e| e.to_string())?;
            let device = if name.is_empty() {
                enumerator.get_default_device(&Direction::Render)
            } else {
                enumerator.get_device_collection(&Direction::Render)
                    .and_then(|devices| devices.get_device_with_name(&name))
            }.map_err(|e| e.to_string())?;
            let mut client = device.get_iaudioclient().map_err(|e| e.to_string())?;
            let format = choose_format(&client)?;
            let rate = format.get_samplespersec();
            let (default_period, _) = client.get_device_period().map_err(|e| e.to_string())?;
            let requested = if frames == 0 { default_period } else { wasapi::calculate_period_100ns(frames as i64, rate as i64) };
            let period = client.calculate_aligned_period_near(requested, None, &format).map_err(|e| e.to_string())?;
            if let Err(error) = client.initialize_client(&format, &Direction::Render, &StreamMode::EventsExclusive { period_hns: period }) {
                let unaligned = matches!(&error, wasapi::WasapiError::Windows(e) if e.code() == windows::Win32::Media::Audio::AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED);
                if !unaligned { return Err(error.to_string()); }
                let aligned = client.get_buffer_size().map_err(|e| e.to_string())?;
                drop(client);
                client = device.get_iaudioclient().map_err(|e| e.to_string())?;
                client.initialize_client(&format, &Direction::Render, &StreamMode::EventsExclusive {
                    period_hns: wasapi::calculate_period_100ns(aligned as i64, rate as i64)
                }).map_err(|e| e.to_string())?;
            }
            let frames = client.get_buffer_size().map_err(|e| e.to_string())?;
            if frames == 0 || frames > rate { return Err("Invalid device buffer size".into()); }
            let event = client.set_get_eventhandle().map_err(|e| e.to_string())?;
            let render = client.get_audiorenderclient().map_err(|e| e.to_string())?;
            let description = format!("WASAPI exclusive · {} · {rate} Hz · {frames} frames ({:.2} ms buffer)",
                device.get_friendlyname().map_err(|e| e.to_string())?, frames as f64 * 1000. / rate as f64);
            Ok((com, client, format, event, render, frames, description))
        })();
        let (com, client, format, event, render, frames, description) = match prepared {
            Ok(value) => value,
            Err(error) => { let _ = ready_tx.send(Err(error)); return; }
        };
        // COM must outlive every interface, including on early return.
        let _com = com;
        let client = client;
        let event = event;
        let render = render;
        if ready_tx.send(Ok((format.get_samplespersec(), description))).is_err() { return; }
        let Ok(mut mixer) = mixer_rx.recv() else { return; };
        let mut stereo = vec![0.0; frames as usize * 2];
        let mut bytes = vec![0; frames as usize * format.get_blockalign() as usize];
        let float = format.get_subformat().is_ok_and(|t| t == SampleType::Float);
        let bits = format.get_bitspersample();
        let valid = format.get_validbitspersample();
        let mut write = || {
            mixer.render(&mut stereo);
            encode(&stereo, &mut bytes, bits, valid, float);
            render.write_to_device(frames as usize, &bytes, None)
        };
        // MMCSS priority applies only to this audio worker and is reverted on exit.
        let priority = Priority::new();
        let start = write().and_then(|_| client.start_stream()).map_err(|e| e.to_string());
        let running = start.is_ok();
        if started_tx.send(start).is_ok() && running {
            while !worker_stop.load(Ordering::Relaxed) {
                if event.wait_for_event(1000).is_err() {
                    if !worker_stop.load(Ordering::Relaxed) { worker_failed.store(true, Ordering::Relaxed); }
                    break;
                }
                if worker_stop.load(Ordering::Relaxed) { break; }
                if write().is_err() { worker_failed.store(true, Ordering::Relaxed); break; }
            }
        }
        let _ = client.stop_stream();
        drop(priority);
    }).map_err(failure)?;
    let stream = Stream {
        stop,
        failed,
        worker: Some(worker),
    };
    let (rate, description) = ready_rx.recv().map_err(failure)?.map_err(failure)?;
    let (mut engine, mixer) = Engine::headless(rate);
    mixer_tx.send(mixer).map_err(failure)?;
    started_rx.recv().map_err(failure)?.map_err(failure)?;
    engine.output_description = description;
    engine.exclusive_stream = Some(stream);
    Ok(engine)
}

fn choose_format(client: &AudioClient) -> Result<WaveFormat, String> {
    let default_rate = client
        .get_mixformat()
        .map_err(|e| e.to_string())?
        .get_samplespersec();
    for rate in [default_rate, 48000, 44100, 96000] {
        for (bits, valid, sample) in [
            (32, 32, SampleType::Float),
            (32, 24, SampleType::Int),
            (24, 24, SampleType::Int),
            (16, 16, SampleType::Int),
            (32, 32, SampleType::Int),
        ] {
            let format = WaveFormat::new(bits, valid, &sample, rate as usize, 2, None);
            if let Ok(format) = client.is_supported_exclusive_with_quirks(&format) {
                return Ok(format);
            }
        }
    }
    Err("No supported stereo format at the default rate, 44.1, 48 or 96 kHz".into())
}

// PCM valid bits are left-aligned inside their container (including 24-in-32).
fn encode(input: &[f32], output: &mut [u8], bits: u16, valid: u16, float: bool) {
    let width = bits as usize / 8;
    for (&sample, out) in input.iter().zip(output.chunks_exact_mut(width)) {
        let sample = if sample.is_finite() {
            sample.clamp(-1., 1.)
        } else {
            0.
        };
        if float {
            out.copy_from_slice(&sample.to_le_bytes());
        } else {
            let pcm = (sample as f64 * 2147483648.).clamp(i32::MIN as f64, i32::MAX as f64) as i32;
            let pcm = pcm & (!0i32 << (32 - valid));
            out.copy_from_slice(&pcm.to_le_bytes()[4 - width..]);
        }
    }
}
struct Priority(Option<windows::Win32::Foundation::HANDLE>);
impl Priority {
    fn new() -> Self {
        use windows::{Win32::System::Threading::AvSetMmThreadCharacteristicsW, core::w};
        let mut index = 0;
        Self(unsafe { AvSetMmThreadCharacteristicsW(w!("Pro Audio"), &mut index) }.ok())
    }
}
impl Drop for Priority {
    fn drop(&mut self) {
        if let Some(handle) = self.0 {
            let _ = unsafe {
                windows::Win32::System::Threading::AvRevertMmThreadCharacteristics(handle)
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::encode;
    #[test]
    fn pcm_conversion_preserves_polarity_silence_and_clips_safely() {
        let input = [-2., -0.5, 0., 0.5, 2., f32::NAN];
        let mut bytes = [0; 12];
        encode(&input, &mut bytes, 16, 16, false);
        let actual: Vec<_> = bytes
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect();
        assert_eq!(actual, [-32768, -16384, 0, 16384, 32767, 0]);
        let mut bytes = [0; 6];
        encode(&[-1., 1.], &mut bytes, 24, 24, false);
        assert_eq!(bytes, [0, 0, 128, 255, 255, 127]);
        let mut padded = [0; 8];
        encode(&[0.12345, -0.12345], &mut padded, 32, 24, false);
        assert_eq!(padded[0], 0);
        assert_eq!(padded[4], 0);
        let mut bytes = [0; 8];
        encode(&[0.5, -0.5], &mut bytes, 32, 32, true);
        assert_eq!(&bytes[..4], &0.5f32.to_le_bytes());
        assert_eq!(&bytes[4..], &(-0.5f32).to_le_bytes());
    }
}
