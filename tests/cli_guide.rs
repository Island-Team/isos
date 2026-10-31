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
fn no_command_prints_quick_start_without_creating_home() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("isos-home");
    let output = isos(&home, &[]);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("IsOS quick start"));
    assert!(stdout.contains("isos init --mode dev"));
    assert!(stdout.contains("isos serve"));
    assert!(stdout.contains("isos open"));
    assert!(stdout.contains("Validate and Apply"));
    assert!(stdout.contains("isos run"));
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
    assert!(stdout.contains("isos build eos"));
    assert!(stdout.contains("isos serve"));
    assert!(stdout.contains("isos open"));
    assert!(home.join("config/deployments/default.nodes.json").exists());
    assert!(!home.join("run/manager.sock").exists());
}

#[test]
fn product_init_mentions_isos_build_selection_without_starting_manager() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("isos-home");
    let output = isos(&home, &["init", "--mode", "product"]);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("isos build isos"));
    assert!(stdout.contains("isos version select isos"));
    assert!(!home.join("run/manager.sock").exists());
}
