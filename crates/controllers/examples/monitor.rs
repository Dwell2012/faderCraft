//! List ports, or monitor one exact named port without controlling the engine.
use djcraft_controllers::native::{Input, input_ports};

fn main() -> Result<(), String> {
    let Some(name) = std::env::args().nth(1) else {
        for name in input_ports()? {
            println!("{name}");
        }
        return Ok(());
    };
    let input = Input::open(&name)?;
    let seconds: Option<u64> = std::env::args().nth(2).and_then(|s| s.parse().ok());
    let started = std::time::Instant::now();
    println!("Monitoring {name}; Ctrl+C to stop");
    loop {
        for event in input.drain()? {
            println!("{event:?}");
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
        if seconds.is_some_and(|s| started.elapsed().as_secs() >= s) {
            return Ok(());
        }
    }
}
