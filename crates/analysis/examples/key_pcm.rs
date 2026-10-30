//! Export decoded mono audio for independent, offline key diagnostics.
//! Usage: key_pcm <audio file> <new .f32 output> [sample rate]
//! Output is little-endian float32; source audio and library are read-only.
use djcraft_engine::Track;
use std::{io::Write, path::Path};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() < 3 {
        return Err("usage: key_pcm <audio file> <new .f32 output> [sample rate]".into());
    }
    let rate: u32 = args.get(3).map(|v| v.parse()).transpose()?.unwrap_or(22050);
    if !(8000..=96000).contains(&rate) {
        return Err("sample rate must be 8000–96000 Hz".into());
    }
    let track = Track::decode(Path::new(&args[1]), rate)?;
    let mono = track.mono();
    println!(
        "Key evidence: {:?}",
        djcraft_analysis::detect_key(&mono, rate)
    );
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[2])?;
    let mut output = std::io::BufWriter::new(file);
    for sample in &mono {
        output.write_all(&sample.to_le_bytes())?;
    }
    output.flush()?;
    println!(
        "{} mono samples at {rate} Hz ({:.3} seconds)",
        mono.len(),
        mono.len() as f64 / rate as f64
    );
    Ok(())
}
