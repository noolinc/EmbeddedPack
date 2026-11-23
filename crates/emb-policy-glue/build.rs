//! Compiles the hardware profile into the policy. `EMB_PROFILE=<path>` builds
//! against another profile (the end-to-end tests use it with
//! `nool admin plugin build --out`); by default it is the repository's
//! `hardware-profile.toml`.
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let profile = match std::env::var("EMB_PROFILE") {
        Ok(p) if !p.is_empty() => PathBuf::from(p),
        _ => manifest.join("../../hardware-profile.toml"),
    };
    println!("cargo:rerun-if-env-changed=EMB_PROFILE");
    println!("cargo:rerun-if-changed={}", profile.display());
    let text = std::fs::read_to_string(&profile).unwrap_or_else(|e| panic!("cannot read profile {}: {e}", profile.display()));
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("hardware-profile.toml");
    std::fs::write(out, text).unwrap();
}
