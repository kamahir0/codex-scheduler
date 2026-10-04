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

#[test]
fn test_cli_show_json_running_active_observability() {
    use codex_scheduler_core::models::{Job, JobStatus, ProviderType};
    use codex_scheduler_core::runner;
    use codex_scheduler_core::store::JobStore;

    let temp = tempfile::tempdir().expect("tempdir");
    let store_path = temp.path().join("jobs.json");
    let store = JobStore::new_with_path(&store_path);

    let mut job = Job::new(
        ProviderType::Codex,
        "sess-observability-1".to_string(),
        temp.path().to_path_buf(),
        Some("continue".to_string()),
        chrono::Utc::now(),
        None,
    )
    .expect("create job");
    job.set_status(JobStatus::Running);
    store.insert_job(job.clone()).expect("insert job");

    // Create active log file for attempt 1
    let log_path = runner::runner_log_path(&job.id, 1).expect("log path");
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent).expect("create parent");
    }
    std::fs::write(&log_path, "Processing step 1: active streaming output line\n").expect("write log");

    let bin_path = env!("CARGO_BIN_EXE_codex-scheduler");
    let output = Command::new(bin_path)
        .env("CODEX_SCHEDULER_STORE", &store_path)
        .args(["show", &job.id, "--json"])
        .output()
        .expect("failed to run show --json");

    assert!(output.status.success(), "show --json must exit successfully");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let val: serde_json::Value = serde_json::from_str(&stdout).expect("valid single json");

    assert_eq!(val["id"], job.id);
    assert_eq!(val["status"], "running");

    // Must contain active_execution object
    assert!(val.get("active_execution").is_some(), "Running job must have active_execution");
    let active = &val["active_execution"];
    assert_eq!(active["attempt_number"], 1);
    assert_eq!(active["is_runner_active"], false); // Lock is not held in this mock test
    assert!(active["log_path"].as_str().unwrap().contains(&job.id));
    assert!(active["latest_output"].as_str().unwrap().contains("Processing step 1"));

    // Cleanup log
    let _ = runner::cleanup_job_logs(&job.id);
}
