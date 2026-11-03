use djcraft_engine::RhythmShiftPack;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let folder = std::env::args()
        .nth(1)
        .ok_or("usage: check_rhythm_pack <folder> <bpm> <loop_beats> <return_beats>")?;
    let bpm: f64 = std::env::args().nth(2).ok_or("missing bpm")?.parse()?;
    let loop_beats: f64 = std::env::args()
        .nth(3)
        .ok_or("missing loop_beats")?
        .parse()?;
    let return_beats: f64 = std::env::args()
        .nth(4)
        .ok_or("missing return_beats")?
        .parse()?;
    for sample_rate in [44100, 48000] {
        let pack = RhythmShiftPack::load(
            Path::new(&folder),
            sample_rate,
            bpm,
            loop_beats,
            [return_beats; 4],
        )
        .map_err(|error| format!("{error:?}"))?;
        if !pack.valid(sample_rate) {
            return Err(format!("pack timing invalid at {sample_rate} Hz").into());
        }
        println!("{sample_rate} Hz: valid");
    }
    Ok(())
}
