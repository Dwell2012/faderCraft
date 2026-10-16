// GPUI's unoptimised view builders create sizeable stack frames. Windows'
// default 1 MiB executable stack is insufficient for the nested deck/loop UI.
// Reserve address space (not committed memory) for the main UI thread.
fn main() {
    let target = std::env::var("TARGET").unwrap_or_default();
    if target.contains("windows-msvc") {
        println!("cargo:rustc-link-arg-bin=djcraft=/STACK:8388608");
    } else if target.contains("windows-gnu") {
        println!("cargo:rustc-link-arg-bin=djcraft=-Wl,--stack,8388608");
    }
}
