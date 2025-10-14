use std::process::Command;

fn main() {
    // Run `cargo metadata` to get dependency information
    let output = Command::new("cargo")
        .args(["metadata", "--format-version", "1"])
        .output()
        .expect("failed to run cargo metadata");

    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    // Find the `serde` package version
    let serde_version = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|pkg| pkg["name"] == "datex-core")
        .and_then(|pkg| pkg["version"].as_str())
        .unwrap()
        .to_string();

    // Set environment variable for compile-time use
    println!("cargo:rustc-env=DEP_DATEX_CORE_VERSION={}", serde_version);
}