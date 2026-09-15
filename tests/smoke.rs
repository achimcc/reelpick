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
