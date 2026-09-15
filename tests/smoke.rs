use std::process::Command;

#[test]
fn version_flag_prints_name_and_version() {
    let out = Command::new(env!("CARGO_BIN_EXE_reelpick"))
        .arg("--version")
        .output()
        .expect("binary runs");
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert_eq!(
        text.trim(),
        format!("reelpick {}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn unknown_command_fails_with_usage() {
    let out = Command::new(env!("CARGO_BIN_EXE_reelpick"))
        .arg("dance")
        .output()
        .expect("binary runs");
    assert!(!out.status.success());
    let text = String::from_utf8(out.stderr).unwrap();
    assert!(text.contains("usage: reelpick"), "stderr was: {text}");
}

#[test]
fn serve_refuses_to_start_without_a_configuration() {
    let out = Command::new(env!("CARGO_BIN_EXE_reelpick"))
        .arg("serve")
        .env("REELPICK_CONFIG", "/nonexistent/reelpick.toml")
        .output()
        .expect("binary runs");
    assert!(!out.status.success());
    let text = String::from_utf8(out.stderr).unwrap();
    assert!(
        text.contains("/nonexistent/reelpick.toml"),
        "stderr was: {text}"
    );
}

#[test]
fn pick_refuses_to_run_without_a_jellyfin_key() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("reelpick.toml");
    std::fs::write(&cfg, format!(
        "data_dir = {:?}\n[jellyfin]\nurl = \"http://127.0.0.1:9\"\npublic_url = \"http://x\"\nlibrary = \"Filme\"\n[ollama]\nurl = \"http://127.0.0.1:9\"\nmodel = \"m\"\n",
        dir.path().to_str().unwrap()
    )).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_reelpick"))
        .arg("pick")
        .env("REELPICK_CONFIG", &cfg)
        .output()
        .expect("binary runs");
    assert!(!out.status.success());
    let text = String::from_utf8(out.stderr).unwrap();
    assert!(text.contains("api_key_file"), "stderr was: {text}");
}
