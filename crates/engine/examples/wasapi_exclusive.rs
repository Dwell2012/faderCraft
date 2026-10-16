//! Opens silent exclusive output and checks event delivery and clean shutdown.
use djcraft_engine::Engine;
fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let frames = args.first().and_then(|v| v.parse().ok()).unwrap_or(0);
    let name = args.get(1).map(String::as_str);
    let engine = Engine::with_wasapi_exclusive(name, frames).expect("exclusive output failed");
    println!("{}", engine.output_description());
    std::thread::sleep(std::time::Duration::from_secs(2));
    assert!(
        !engine.output_description().contains("stopped"),
        "{}",
        engine.output_description()
    );
    drop(engine);
    println!("Stream ran and released the device");
}
