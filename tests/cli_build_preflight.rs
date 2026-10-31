use std::process::Command;

#[test]
fn build_all_reports_owning_repository_without_building_components() {
    let temporary = tempfile::tempdir().unwrap();
    let home = temporary.path().join("isos-home");
    let missing_ui = temporary.path().join("missing-ui");
    let output = Command::new(env!("CARGO_BIN_EXE_isos"))
        .arg("build")
        .env("ISOS_HOME", &home)
        .env("ISOSUI_REPO_PATH", &missing_ui)
        .env_remove("ISOS_CONFIG")
        .output()
        .expect("run IsOS CLI");
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("Build all in its owning repository"),
        "{stderr}"
    );
    assert!(!home.join("data/builds").exists());
}
