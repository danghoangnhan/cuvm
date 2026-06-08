// The Dependency Rule (spec §3.2): cuvm-app depends ONLY on cuvm-core.
// We assert it at the dependency-graph level via `cargo metadata`.
use std::process::Command;

#[test]
fn cuvm_app_depends_only_on_cuvm_core() {
    let out = Command::new(env!("CARGO"))
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .output()
        .expect("run cargo metadata");
    assert!(out.status.success(), "cargo metadata failed");
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).expect("parse metadata json");

    let pkgs = json["packages"].as_array().unwrap();
    let app = pkgs
        .iter()
        .find(|p| p["name"] == "cuvm-app")
        .expect("cuvm-app package present");

    let internal: Vec<String> = app["dependencies"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["name"].as_str().unwrap().to_string())
        .filter(|n| n.starts_with("cuvm-"))
        .collect();

    assert_eq!(
        internal,
        vec!["cuvm-core".to_string()],
        "cuvm-app must depend on cuvm-core ONLY (got {internal:?})"
    );
}
