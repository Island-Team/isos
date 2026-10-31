use std::process::Command;

fn isos(home: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_isos"))
        .args(args)
        .env("ISOS_HOME", home)
        .env_remove("ISOS_CONFIG")
        .env_remove("ISOS_MODE")
        .output()
        .expect("run IsOS CLI")
}

#[test]
fn help_prints_quick_start_without_creating_home() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("isos-home");
    let output = isos(&home, &["--help"]);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("workbench"));
    assert!(stdout.contains("dev"));
    assert!(stdout.contains("product"));
    assert!(!home.exists());
}

#[test]
fn init_prints_next_steps_without_starting_manager() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("isos-home");
    let output = isos(&home, &["init", "--mode", "dev"]);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("editable graph:"));
    assert!(stdout.contains("Next steps:"));
    assert!(stdout.contains("isos version check"));
    assert!(stdout.contains("isos                               # open the workbench"));
    assert!(home.join("config/deployments/default.nodes.json").exists());
    assert!(!home.join("run/manager.sock").exists());
}

#[test]
fn product_init_mentions_configured_builds_without_starting_manager() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("isos-home");
    let output = isos(&home, &["init", "--mode", "product"]);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("ISOS_AETHER_BIN_DIR"));
    assert!(stdout.contains("ISOSUI_DIST_DIR"));
    assert!(stdout.contains("isos version check"));
    assert!(!home.join("run/manager.sock").exists());
}

#[test]
fn version_check_defaults_to_readable_report_and_supports_json() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("isos-home");
    let plain = isos(&home, &["version", "check"]);
    assert!(plain.status.success());
    let text = String::from_utf8(plain.stdout).unwrap();
    assert!(text.contains("Graph configuration:"));
    assert!(text.contains("Running:"));
    let machine = isos(&home, &["version", "check", "--json"]);
    assert!(machine.status.success());
    let report: serde_json::Value = serde_json::from_slice(&machine.stdout).unwrap();
    assert_eq!(report["cores"][0]["core"], "eos");
    assert!(report["cores"][0]["ready"].is_boolean());
}
