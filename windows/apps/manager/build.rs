//! Embeds the LingoSnap icon (windows/assets/app.rc, resource ID 1) with the
//! Windows resource compiler. `rc.exe` is on PATH inside the VsDevCmd shell
//! that vscargo.ps1 sets up; the build fails with a clear message without it.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let assets = manifest.join("../../assets");
    let rc_file = assets.join("app.rc");
    let icon_file = assets.join("lingosnap.ico");
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("out dir")).join("app.res");

    let status = Command::new("rc.exe")
        .args(["/nologo", "/fo"])
        .arg(&out)
        .arg(&rc_file)
        .status()
        .unwrap_or_else(|error| {
            panic!(
                "rc.exe (Windows resource compiler) could not be started: {error}. \
                 Build from a VsDevCmd.bat x64 shell, for example with windows/tmp/vscargo.ps1."
            )
        });
    assert!(status.success(), "rc.exe failed for {}", rc_file.display());

    println!("cargo:rustc-link-arg-bins={}", out.display());
    println!("cargo:rerun-if-changed={}", rc_file.display());
    println!("cargo:rerun-if-changed={}", icon_file.display());
    println!("cargo:rerun-if-changed=build.rs");
}
