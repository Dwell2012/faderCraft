//! Test the manufacturer's known startup message without applying engine actions.
#[cfg(feature = "native")]
fn main() -> Result<(), String> {
    use djcraft_controllers::native::{Input, Output};
    use std::time::{Duration, Instant};
    let name = "MixTrack Pro II";
    let input = Input::open(name)?;
    let mut output = Output::open(name)?;
    for phase in 0..2 {
        if phase == 1 {
            output.send(djcraft_controllers::native::MIXTRACK_PRO_II_STARTUP)?;
        }
        let start = Instant::now();
        let mut count = 0;
        let mut controls = std::collections::BTreeMap::new();
        while start.elapsed() < Duration::from_secs(2) {
            for event in input.drain()? {
                *controls
                    .entry(format!("{:?}", event.source))
                    .or_insert(0usize) += 1;
                count += 1;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        println!("phase={phase} total={count}");
        for (source, packets) in controls {
            println!("  {source}: {packets} packets");
        }
    }
    Ok(())
}
#[cfg(not(feature = "native"))]
fn main() {
    eprintln!("Run with --features native");
}
