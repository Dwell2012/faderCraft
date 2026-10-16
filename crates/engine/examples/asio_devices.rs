use djcraft_engine::{Engine, output_devices};
fn main() {
    if let Some(name) = std::env::args().nth(1) {
        let frames = std::env::args()
            .nth(2)
            .map(|v| v.parse().expect("buffer frames"))
            .unwrap_or(0);
        let engine = Engine::with_output(true, Some(&name), frames).expect("ASIO open failed");
        println!("Opened: {}", engine.output_description());
        std::thread::sleep(std::time::Duration::from_millis(250));
    } else {
        println!("Available ASIO outputs: {:?}", output_devices(true));
    }
}
