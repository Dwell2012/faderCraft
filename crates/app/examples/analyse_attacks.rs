//! Populate persistent attack sidecars without changing library keys or grids.
#[path = "../src/attack_cache.rs"]
mod attack_cache;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::var_os("DJCRAFT_DATA_DIR").ok_or("Set DJCRAFT_DATA_DIR")?;
    let cache = std::path::Path::new(&root).join("waveforms");
    for file in std::env::args().skip(1) {
        let track =
            djcraft_engine::Track::decode_cached(std::path::Path::new(&file), 22050, Some(&cache))?;
        let attacks = attack_cache::load_or_analyse(&track);
        println!("{} cached attack events: {file}", attacks.len());
    }
    Ok(())
}
