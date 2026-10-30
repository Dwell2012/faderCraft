//! Read-only waveform attack diagnostic: bass_attacks <audio> [from] [to].
use djcraft_engine::Track;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let path = args.get(1).ok_or("audio file required")?;
    let from: f64 = args.get(2).map(|v| v.parse()).transpose()?.unwrap_or(0.);
    let to: f64 = args
        .get(3)
        .map(|v| v.parse())
        .transpose()?
        .unwrap_or(f64::MAX);
    let track = Track::decode(std::path::Path::new(path), 48000)?;
    let levels: Vec<_> = (0..track.peak_count()).map(|i| track.bands(i)[0]).collect();
    let dt = track.frames_per_peak() as f64 / track.sample_rate as f64;
    for attack in djcraft_analysis::bass_attacks(&levels, dt) {
        let seconds = attack.index as f64 * dt;
        if seconds >= from && seconds <= to {
            println!("{seconds:.3},{:.3}", attack.strength);
        }
    }
    Ok(())
}
