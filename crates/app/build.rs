// GPUI's unoptimised view builders create sizeable stack frames. Windows'
// default 1 MiB executable stack is insufficient for the nested deck/loop UI.
// Reserve address space (not committed memory) for the main UI thread.
fn main() {
    let target = std::env::var("TARGET").unwrap_or_default();
    if target.contains("windows-msvc") {
        println!("cargo:rustc-link-arg-bin=djcraft=/STACK:8388608");
        // GPUI loads the application icon from Windows resource ID 1.
        println!("cargo:rerun-if-changed=assets/fadercraft.ico");
        println!("cargo:rerun-if-changed=assets/fadercraft.rc");
        let output =
            std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("fadercraft.res");
        let status = std::process::Command::new("rc.exe")
            .args(["/nologo", "/I", "assets", "/fo"])
            .arg(&output)
            .arg("assets/fadercraft.rc")
            .status()
            .expect("Windows SDK resource compiler rc.exe must be on PATH");
        assert!(status.success(), "failed to compile faderCraft resources");
        println!("cargo:rustc-link-arg-bin=djcraft={}", output.display());
    } else if target.contains("windows-gnu") {
        println!("cargo:rustc-link-arg-bin=djcraft=-Wl,--stack,8388608");
    }
}
