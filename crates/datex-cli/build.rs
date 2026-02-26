use std::{fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=Cargo.toml");

    // Find a Cargo.lock by walking up from the crate dir (works in workspace + target/package)
    let mut dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let lock_path = loop {
        let candidate = dir.join("Cargo.lock");
        if candidate.exists() {
            println!("cargo:rerun-if-changed={}", candidate.display());
            break candidate;
        }
        if !dir.pop() {
            panic!(
                "Could not find Cargo.lock by walking up from CARGO_MANIFEST_DIR"
            );
        }
    };

    let lock_str = fs::read_to_string(&lock_path).unwrap_or_else(|e| {
        panic!("failed to read {}: {e}", lock_path.display())
    });

    let lock_toml: toml::Value = lock_str
        .parse()
        .unwrap_or_else(|e| panic!("failed to parse Cargo.lock as TOML: {e}"));

    let pkgs = lock_toml
        .get("package")
        .and_then(|v| v.as_array())
        .expect("Cargo.lock missing [package] array");

    let datex_core_version = pkgs
        .iter()
        .find(|p| p.get("name").and_then(|n| n.as_str()) == Some("datex-core"))
        .and_then(|p| p.get("version").and_then(|v| v.as_str()))
        .unwrap_or_else(|| panic!("datex-core not found in Cargo.lock"))
        .to_string();

    println!(
        "cargo:rustc-env=DEP_DATEX_CORE_VERSION={}",
        datex_core_version
    );
}
