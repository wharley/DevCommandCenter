//! Computer use on macOS is Objective-C (`native/computer_use_macos.m`):
//! accessibility, ScreenCaptureKit captures and input.
use std::{env, path::PathBuf, process::Command};

fn main() {
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    let source = PathBuf::from("native/computer_use_macos.m");
    let object = out_dir.join("computer_use_macos.o");
    let archive = out_dir.join("libdcc_computer_use_macos.a");
    println!("cargo:rerun-if-changed={}", source.display());
    println!("cargo:rerun-if-changed=native/computer_use_macos.h");
    println!("cargo:rerun-if-env-changed=MACOSX_DEPLOYMENT_TARGET");
    let target = env::var("TARGET").unwrap_or_default();
    let architecture = if target.starts_with("aarch64-") {
        "arm64"
    } else {
        "x86_64"
    };
    // Do not let the installed SDK's version become the application's minimum
    // OS. Respect Cargo's deployment target, with conservative architecture
    // defaults when the environment did not set one.
    let deployment_target = env::var("MACOSX_DEPLOYMENT_TARGET").unwrap_or_else(|_| {
        if architecture == "arm64" {
            "11.0".to_string()
        } else {
            "10.13".to_string()
        }
    });
    let compiled = Command::new("xcrun")
        .args(["clang", "-fobjc-arc", "-c"])
        .arg(&source)
        .args(["-arch", architecture])
        .arg(format!("-mmacosx-version-min={deployment_target}"))
        .args(["-o"])
        .arg(&object)
        .status()
        .expect("xcrun clang must be available to build macOS computer use")
        .success();
    assert!(compiled, "failed to compile the macOS computer-use bridge");
    let archived = Command::new("ar")
        .args(["crus"])
        .arg(&archive)
        .arg(&object)
        .status()
        .expect("ar must be available to build macOS computer use")
        .success();
    assert!(archived, "failed to archive the macOS computer-use bridge");
    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=dcc_computer_use_macos");
    println!("cargo:rustc-link-lib=framework=AppKit");
    println!("cargo:rustc-link-lib=framework=ApplicationServices");
    println!("cargo:rustc-link-lib=framework=ImageIO");
    // ScreenCaptureKit is weakly loaded: the feature is guarded at runtime
    // on macOS 14+.
    println!("cargo:rustc-link-arg=-Wl,-weak_framework,ScreenCaptureKit");
}
