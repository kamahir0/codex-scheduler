use std::process::Command;

#[test]
fn test_cli_help() {
    let bin_path = env!("CARGO_BIN_EXE_codex-scheduler");
    let output = Command::new(bin_path)
        .arg("--help")
        .output()
        .expect("failed to run codex-scheduler --help");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("codex-scheduler"));
    assert!(stdout.contains("schedule"));
    assert!(stdout.contains("list"));
    assert!(stdout.contains("show"));
    assert!(stdout.contains("cancel"));
    assert!(stdout.contains("delete"));
    assert!(stdout.contains("status"));
    assert!(stdout.contains("tick"));
    assert!(stdout.contains("install-scheduler"));
    assert!(stdout.contains("uninstall-scheduler"));
    assert!(stdout.contains("--scheduler-tick"));
}

#[test]
fn test_cli_status_json() {
    let bin_path = env!("CARGO_BIN_EXE_codex-scheduler");
    let output = Command::new(bin_path)
        .arg("status")
        .arg("--json")
        .output()
        .expect("failed to run codex-scheduler status --json");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let val: serde_json::Value = serde_json::from_str(&stdout).expect("valid json output");
    assert!(val.get("version").is_some());
    assert!(val.get("store_path").is_some());
    assert!(val.get("scheduler").is_some());
    assert!(val["scheduler"].get("owner").is_some());
    assert!(val["scheduler"].get("installed").is_some());
    assert!(val["scheduler"].get("ready").is_some());
    assert!(val["scheduler"].get("path_matched").is_some());
    assert!(val["scheduler"].get("target_exists").is_some());
    assert!(val["scheduler"].get("owner_target_valid").is_some());
}

#[test]
fn test_cli_list_json() {
    let bin_path = env!("CARGO_BIN_EXE_codex-scheduler");
    let output = Command::new(bin_path)
        .arg("list")
        .arg("--json")
        .output()
        .expect("failed to run codex-scheduler list --json");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let val: serde_json::Value = serde_json::from_str(&stdout).expect("valid json array");
    assert!(val.is_array());
}

#[test]
fn test_cli_not_found_errors_json() {
    let bin_path = env!("CARGO_BIN_EXE_codex-scheduler");

    // show non-existent
    let output = Command::new(bin_path)
        .args(["show", "00000000-0000-0000-0000-000000000000", "--json"])
        .output()
        .expect("failed to run show");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    let val: serde_json::Value = serde_json::from_str(&stderr).expect("valid error json");
    assert_eq!(val["error"]["code"], "job_not_found");

    // cancel non-existent
    let output = Command::new(bin_path)
        .args(["cancel", "00000000-0000-0000-0000-000000000000", "--json"])
        .output()
        .expect("failed to run cancel");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    let val: serde_json::Value = serde_json::from_str(&stderr).expect("valid error json");
    assert_eq!(val["error"]["code"], "job_not_found");

    // delete non-existent
    let output = Command::new(bin_path)
        .args(["delete", "00000000-0000-0000-0000-000000000000", "--json"])
        .output()
        .expect("failed to run delete");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    let val: serde_json::Value = serde_json::from_str(&stderr).expect("valid error json");
    assert_eq!(val["error"]["code"], "job_not_found");
}
