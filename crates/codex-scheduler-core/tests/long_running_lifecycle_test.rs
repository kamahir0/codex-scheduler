use chrono::{Duration, Utc};
use codex_scheduler_core::adapter::codex::CodexAdapter;
use codex_scheduler_core::models::{Job, JobStatus, ProviderType, RetryPolicy};
use codex_scheduler_core::os_scheduler::{SchedulerBackend, SchedulerError, SchedulerOwner};
use codex_scheduler_core::runner::{self, MAX_BOUNDED_LOG_BYTES};
use codex_scheduler_core::store::JobStore;
use codex_scheduler_core::SchedulerService;
use std::path::{Path, PathBuf};
use tempfile::tempdir;

struct MockScheduler;
impl SchedulerBackend for MockScheduler {
    fn is_scheduler_installed(&self) -> bool {
        true
    }
    fn is_scheduler_ready(&self) -> bool {
        true
    }
    fn supports_persistent_scheduler(&self) -> bool {
        true
    }
    fn ensure_scheduler_installed(&self, _: &Path) -> Result<(), SchedulerError> {
        Ok(())
    }
    fn register_job(&self, _: &Job, _: &Path) -> Result<(), SchedulerError> {
        Ok(())
    }
    fn unregister_job(&self, _: &str) -> Result<(), SchedulerError> {
        Ok(())
    }
    fn get_scheduler_owner(&self) -> SchedulerOwner {
        SchedulerOwner::Cli
    }
    fn get_scheduler_executable_path(&self) -> Option<PathBuf> {
        None
    }
    fn is_scheduler_path_matched(&self, _: &Path) -> bool {
        true
    }
    fn is_target_executable_exists(&self) -> bool {
        true
    }
    fn is_owner_target_valid(&self) -> bool {
        true
    }
    fn uninstall_scheduler(&self) -> Result<(), SchedulerError> {
        Ok(())
    }
    fn uninstall_scheduler_as_cli(&self) -> Result<(), SchedulerError> {
        Ok(())
    }
}

/// Helper to create a fake executable wrapper script on Unix / Windows.
fn create_fake_codex_script(dir: &Path, behavior: &str) -> PathBuf {
    #[cfg(windows)]
    {
        let path = dir.join(format!("fake_codex_{}.cmd", behavior));
        let content: String = match behavior {
            "success" => "@echo off\r\necho Fake task finished successfully\r\nexit /b 0\r\n".to_string(),
            "quota_error" => "@echo off\r\necho Error: Rate limit exceeded. Try again at 02:00.\r\nexit /b 1\r\n".to_string(),
            "non_quota_error" => "@echo off\r\necho Fatal: syntax error in script\r\nexit /b 1\r\n".to_string(),
            "large_output" => {
                let mut s = String::from("@echo off\r\n");
                for i in 0..600 {
                    s.push_str(&format!("echo Line {}: This is a streaming log entry with padding text 1234567890\r\n", i));
                }
                s.push_str("exit /b 0\r\n");
                s
            }
            "early_quota_with_large_output" => {
                let mut s = String::from("@echo off\r\n");
                s.push_str("echo Error: You have hit your usage limit for this period.\r\n");
                for i in 0..600 {
                    s.push_str(&format!("echo Subsequent padding line {}: normal execution trace text 1234567890\r\n", i));
                }
                s.push_str("exit /b 1\r\n");
                s
            }
            "slow" => "@echo off\r\nping -n 2 127.0.0.1 >nul\r\nexit /b 0\r\n".to_string(),
            _ => "@echo off\r\nexit /b 0\r\n".to_string(),
        };
        std::fs::write(&path, content).expect("write fake script");
        path
    }

    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join(format!("fake_codex_{}.sh", behavior));
        let content: String = match behavior {
            "success" => "#!/bin/sh\necho 'Fake task finished successfully'\nexit 0\n".to_string(),
            "quota_error" => "#!/bin/sh\necho 'Error: Rate limit exceeded. Try again at 02:00.' >&2\nexit 1\n".to_string(),
            "non_quota_error" => "#!/bin/sh\necho 'Fatal: syntax error in script' >&2\nexit 1\n".to_string(),
            "large_output" => {
                let mut s = String::from("#!/bin/sh\n");
                for i in 0..600 {
                    s.push_str(&format!("echo 'Line {}: This is a streaming log entry with padding text 1234567890'\n", i));
                }
                s.push_str("exit 0\n");
                s
            }
            "early_quota_with_large_output" => {
                let mut s = String::from("#!/bin/sh\n");
                s.push_str("echo 'Error: You have hit your usage limit for this period.' >&2\n");
                for i in 0..600 {
                    s.push_str(&format!("echo 'Subsequent padding line {}: normal execution trace text 1234567890'\n", i));
                }
                s.push_str("exit 1\n");
                s
            }
            "slow" => "#!/bin/sh\nsleep 1\nexit 0\n".to_string(),
            _ => "#!/bin/sh\nexit 0\n".to_string(),
        };
        std::fs::write(&path, content).expect("write fake script");
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).expect("set executable perms");
        path
    }
}

pub fn get_test_cli_runner_exe() -> PathBuf {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("target")
        .join("debug")
        .join("codex-scheduler");
    #[cfg(windows)]
    let path = path.with_extension("exe");
    if !path.exists() {
        let status = std::process::Command::new("cargo")
            .args(["build", "-p", "codex-scheduler-cli"])
            .status()
            .expect("build codex-scheduler-cli");
        assert!(status.success());
    }
    path
}

fn get_test_dummy_runner_exe() -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(comspec) = std::env::var_os("COMSPEC") {
            let p = PathBuf::from(comspec);
            if p.is_file() {
                return p;
            }
        }
        let sys_root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
        PathBuf::from(sys_root).join("System32").join("cmd.exe")
    }
    #[cfg(not(windows))]
    {
        PathBuf::from("/bin/echo")
    }
}

#[tokio::test]
async fn test_tick_claims_job_into_running_and_returns_without_blocking() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("jobs.json");
    let store = JobStore::new_with_path(&store_path);

    // Cross-platform dummy runner executable path (exists so tick detaches runner and returns immediately)
    let service = SchedulerService::with_scheduler(
        store.clone(),
        Some(get_test_dummy_runner_exe()),
        Box::new(MockScheduler),
    );

    let now = Utc::now();
    let job = Job::new(
        ProviderType::Codex,
        "sess-long-1".to_string(),
        temp.path().to_path_buf(),
        Some("prompt".to_string()),
        now - Duration::minutes(2),
        None,
    )
    .unwrap();
    store.insert_job(job.clone()).unwrap();

    // 1. Tick should claim the job into Running and return immediately
    let claimed = service.execute_tick().await.unwrap();
    assert_eq!(claimed.len(), 1);
    assert_eq!(claimed[0].id, job.id);
    assert_eq!(claimed[0].status, JobStatus::Running);

    let in_store = store.get_job(&job.id).unwrap().unwrap();
    assert_eq!(in_store.status, JobStatus::Running);

    // 2. Second tick immediately should find 0 due jobs
    let second = service.execute_tick().await.unwrap();
    assert_eq!(second.len(), 0);
}

#[tokio::test]
async fn test_same_session_jobs_deferred_to_prevent_duplicate_writer() {
    let temp = tempdir().unwrap();
    let store = JobStore::new_with_path(temp.path().join("jobs.json"));

    let now = Utc::now();
    let session_id = "sess-shared-writer";

    // Two jobs scheduled for the same session ID
    let job1 = Job::new(
        ProviderType::Codex,
        session_id.to_string(),
        temp.path().to_path_buf(),
        Some("prompt 1".to_string()),
        now - Duration::minutes(5),
        None,
    )
    .unwrap();

    let job2 = Job::new(
        ProviderType::Codex,
        session_id.to_string(),
        temp.path().to_path_buf(),
        Some("prompt 2".to_string()),
        now - Duration::minutes(3),
        None,
    )
    .unwrap();

    store.insert_job(job1.clone()).unwrap();
    store.insert_job(job2.clone()).unwrap();

    // First claim: only job1 should be claimed, job2 must be deferred because same session is active
    let claimed = store.claim_due_jobs(now).unwrap();
    assert_eq!(claimed.len(), 1);
    assert_eq!(claimed[0].id, job1.id);

    // Store state: job1 is Running, job2 is still Scheduled
    assert_eq!(store.get_job(&job1.id).unwrap().unwrap().status, JobStatus::Running);
    assert_eq!(store.get_job(&job2.id).unwrap().unwrap().status, JobStatus::Scheduled);

    // Second claim while job1 is still Running: job2 is STILL deferred
    let second_claim = store.claim_due_jobs(now).unwrap();
    assert_eq!(second_claim.len(), 0);

    // Once job1 finishes to Succeeded:
    let mut finished_job1 = job1.clone();
    finished_job1.set_status(JobStatus::Succeeded);
    store.update_job(&finished_job1).unwrap();

    // Third claim: job2 can now be claimed safely
    let third_claim = store.claim_due_jobs(now).unwrap();
    assert_eq!(third_claim.len(), 1);
    assert_eq!(third_claim[0].id, job2.id);
    assert_eq!(third_claim[0].status, JobStatus::Running);
}

#[tokio::test]
async fn test_different_session_jobs_claimed_concurrently() {
    let temp = tempdir().unwrap();
    let store = JobStore::new_with_path(temp.path().join("jobs.json"));

    let now = Utc::now();
    let job_a = Job::new(
        ProviderType::Codex,
        "sess-A".to_string(),
        temp.path().to_path_buf(),
        None,
        now - Duration::minutes(5),
        None,
    )
    .unwrap();

    let job_b = Job::new(
        ProviderType::Codex,
        "sess-B".to_string(),
        temp.path().to_path_buf(),
        None,
        now - Duration::minutes(5),
        None,
    )
    .unwrap();

    store.insert_job(job_a.clone()).unwrap();
    store.insert_job(job_b.clone()).unwrap();

    let claimed = store.claim_due_jobs(now).unwrap();
    assert_eq!(claimed.len(), 2, "Jobs with different session IDs should run concurrently");
}

#[tokio::test]
async fn test_orphan_recovery_on_crashed_runner() {
    let temp = tempdir().unwrap();
    let store = JobStore::new_with_path(temp.path().join("jobs.json"));
    let service = SchedulerService::with_scheduler(store.clone(), None, Box::new(MockScheduler));

    let now = Utc::now();
    let mut orphan_job = Job::new(
        ProviderType::Codex,
        "sess-orphan".to_string(),
        temp.path().to_path_buf(),
        None,
        now - Duration::minutes(10),
        None,
    )
    .unwrap();
    // Simulate job was claimed to Running by a previous tick, but runner process died without updating store
    orphan_job.set_status(JobStatus::Running);
    orphan_job.updated_at = now - Duration::minutes(5);
    store.insert_job(orphan_job.clone()).unwrap();

    // Verify lock is NOT held (is_runner_active == false)
    assert!(!runner::is_runner_active(&orphan_job.id));

    // Running reconcile_running_jobs directly
    let recovered = service.reconcile_running_jobs().unwrap();
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].id, orphan_job.id);
    // Non-quota crash defaults to Failed
    assert_eq!(recovered[0].status, JobStatus::Failed);

    let in_store = store.get_job(&orphan_job.id).unwrap().unwrap();
    assert_eq!(in_store.status, JobStatus::Failed);
    assert_eq!(in_store.execution_history.len(), 1);
    assert!(in_store.execution_history[0].error_message.as_ref().unwrap().contains("Runner process terminated unexpectedly"));
}

#[tokio::test]
async fn test_streaming_log_and_bounded_tail_preservation() {
    let temp = tempdir().unwrap();
    let fake_script = create_fake_codex_script(temp.path(), "large_output");

    let adapter = CodexAdapter::with_executable(fake_script);
    let log_file = temp.path().join("streaming.log");

    let result = adapter
        .execute_resume_streaming("sess-large", temp.path(), "continue", Some(&log_file))
        .await
        .expect("execute streaming");

    assert!(result.success);
    assert_eq!(result.exit_code, Some(0));

    // Disk log file should have captured full output (many KB)
    assert!(log_file.exists());
    let full_content = std::fs::read_to_string(&log_file).unwrap();
    assert!(full_content.len() > MAX_BOUNDED_LOG_BYTES);

    // In-memory bounded stdout should be trimmed
    assert!(result.stdout.len() <= MAX_BOUNDED_LOG_BYTES + 50);
    assert!(result.stdout.contains("[...truncated...]"));
}

#[tokio::test]
async fn test_runner_quota_error_and_retry_backoff() {
    let temp = tempdir().unwrap();
    let fake_script = create_fake_codex_script(temp.path(), "quota_error");
    let store = JobStore::new_with_path(temp.path().join("jobs.json"));

    let now = Utc::now();
    let policy = RetryPolicy {
        enabled: true,
        interval_seconds: 120,
        max_attempts: 3,
        retry_on_quota_only: true,
    };

    let job = Job::new(
        ProviderType::Codex,
        "sess-quota".to_string(),
        temp.path().to_path_buf(),
        None,
        now,
        Some(policy),
    )
    .unwrap();
    store.insert_job(job.clone()).unwrap();

    // Use adapter override via execution
    let adapter = CodexAdapter::with_executable(fake_script);
    let exec_res = adapter
        .execute_resume("sess-quota", temp.path(), "continue")
        .await
        .unwrap();

    assert!(exec_res.is_quota_error);
    assert!(!exec_res.success);

    // Apply evaluation to job
    let mut claimed = store.claim_job_for_execution(&job.id).unwrap();
    let attempt = codex_scheduler_core::models::ExecutionAttempt {
        attempt_number: 1,
        started_at: now,
        finished_at: Utc::now(),
        exit_code: exec_res.exit_code,
        stdout: exec_res.stdout.clone(),
        stderr: exec_res.stderr.clone(),
        is_quota_error: exec_res.is_quota_error,
        error_message: exec_res.error_message.clone(),
    };
    claimed.add_attempt(attempt);

    let action = codex_scheduler_core::retry::RetryEngine::apply_evaluation(&mut claimed, &exec_res);
    if let codex_scheduler_core::retry::NextAction::RetryAfter(next) = action {
        claimed.scheduled_at = next;
    }
    assert_eq!(claimed.status, JobStatus::Retrying);
    assert!(claimed.scheduled_at >= now + Duration::seconds(119));
}

#[tokio::test]
async fn test_runner_lock_mutual_exclusion_and_cleanup() {
    let job_id = "test-mutex-job-unique";

    // Acquire lock 1
    let lock1 = runner::RunnerLock::acquire(job_id).expect("acquire lock 1");
    assert!(runner::is_runner_active(job_id));

    // Acquire lock 2 should fail
    let lock2_res = runner::RunnerLock::acquire(job_id);
    assert!(lock2_res.is_err(), "Second lock acquire must fail with LockHeld");

    // Drop lock 1
    drop(lock1);

    // Runner should now report inactive and lock file is removed
    assert!(!runner::is_runner_active(job_id));

    // Now acquire lock 3 succeeds
    let lock3 = runner::RunnerLock::acquire(job_id).expect("acquire lock 3 after release");
    assert!(runner::is_runner_active(job_id));
    drop(lock3);
    assert!(!runner::is_runner_active(job_id));
}

#[tokio::test]
async fn test_early_quota_detected_even_if_trimmed_from_bounded_tail() {
    let temp = tempdir().unwrap();
    let fake_script = create_fake_codex_script(temp.path(), "early_quota_with_large_output");

    let adapter = CodexAdapter::with_executable(fake_script);
    let log_file = temp.path().join("streaming_quota.log");

    let result = adapter
        .execute_resume_streaming("sess-early-quota", temp.path(), "continue", Some(&log_file))
        .await
        .expect("execute streaming with early quota");

    // Output status was exit 1 and quota was detected early
    assert!(!result.success);
    assert_eq!(result.exit_code, Some(1));
    assert!(result.is_quota_error, "Incremental streaming must detect quota error even if pushed out of bounded tail");

    // Bounded tail must NOT contain the early signature (proving it was trimmed)
    assert!(
        !result.stdout.contains("usage limit"),
        "The early quota line should have been trimmed from the 10KB bounded tail"
    );
    assert!(result.stdout.contains("[...truncated...]"));

    // Full disk log must contain both early quota and large output
    let full_log = std::fs::read_to_string(&log_file).unwrap();
    assert!(full_log.contains("You have hit your usage limit"));
    assert!(full_log.contains("Subsequent padding line 599"));
}

#[tokio::test]
async fn test_runner_crash_with_alive_child_maintains_running_and_blocks_duplicate_writer() {
    let temp = tempdir().unwrap();
    let store = JobStore::new_with_path(temp.path().join("jobs.json"));
    let service = SchedulerService::with_scheduler(store.clone(), None, Box::new(MockScheduler));

    let now = Utc::now();
    let session_id = "sess-orphan-child-alive";
    let mut job = Job::new(
        ProviderType::Codex,
        session_id.to_string(),
        temp.path().to_path_buf(),
        None,
        now - Duration::minutes(10),
        None,
    )
    .unwrap();
    job.set_status(JobStatus::Running);
    store.insert_job(job.clone()).unwrap();

    // Verify runner lock is NOT held (runner crashed)
    assert!(!runner::is_runner_active(&job.id));

    // Case 1: Codex child process is still alive on OS (we use current test process PID and start_time)
    let current_pid = std::process::id();
    let current_start = runner::get_process_start_time(current_pid)
        .expect("get current process start time")
        .expect("current process start time exists");
    let alive_info = runner::RunnerInfo {
        job_id: job.id.clone(),
        session_id: session_id.to_string(),
        runner_pid: 99999, // Dead runner
        runner_started_at: now - Duration::minutes(10),
        codex_pid: Some(current_pid),
        codex_start_time: Some(current_start),
    };
    runner::write_runner_info(&alive_info).unwrap();

    // Reconcile should NOT mark Failed or Retrying; child is still running!
    let recovered = service.reconcile_running_jobs().unwrap();
    assert_eq!(recovered.len(), 0, "Job must NOT be recovered while Codex child process is still alive");

    // Store state: job remains Running
    let in_store = store.get_job(&job.id).unwrap().unwrap();
    assert_eq!(in_store.status, JobStatus::Running);

    // Second job scheduled for same session MUST be deferred (duplicate writer blocked!)
    let second_job = Job::new(
        ProviderType::Codex,
        session_id.to_string(),
        temp.path().to_path_buf(),
        None,
        now - Duration::minutes(5),
        None,
    )
    .unwrap();
    store.insert_job(second_job.clone()).unwrap();

    let claimed = store.claim_due_jobs(now).unwrap();
    assert_eq!(claimed.len(), 0, "Second job must NOT be claimed while first job's child is alive");

    // Case 2: Codex child process terminates (simulate by setting non-existent PID)
    let dead_info = runner::RunnerInfo {
        job_id: job.id.clone(),
        session_id: session_id.to_string(),
        runner_pid: 99999,
        runner_started_at: now - Duration::minutes(10),
        codex_pid: Some(999_999_999), // Non-existent PID
        codex_start_time: Some("invalid_start_time".to_string()),
    };
    runner::write_runner_info(&dead_info).unwrap();

    let recovered_now = service.reconcile_running_jobs().unwrap();
    assert_eq!(recovered_now.len(), 1, "Job must now be recovered once Codex child has terminated");
    assert_eq!(recovered_now[0].status, JobStatus::Failed);

    // Now second job can be safely claimed
    let claimed_after = store.claim_due_jobs(now).unwrap();
    assert_eq!(claimed_after.len(), 1);
    assert_eq!(claimed_after[0].id, second_job.id);
}

#[tokio::test]
async fn test_job_deletion_cleans_up_logs_and_runner_info() {
    let temp = tempdir().unwrap();
    let store = JobStore::new_with_path(temp.path().join("jobs.json"));
    let service = SchedulerService::with_scheduler(store.clone(), None, Box::new(MockScheduler));

    let now = Utc::now();
    let mut job = Job::new(
        ProviderType::Codex,
        "sess-delete-test".to_string(),
        temp.path().to_path_buf(),
        None,
        now,
        None,
    )
    .unwrap();
    job.set_status(JobStatus::Running);
    store.insert_job(job.clone()).unwrap();

    // Create a dummy log file
    let log_path = runner::runner_log_path(&job.id, 1).unwrap();
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&log_path, "dummy log content").unwrap();
    assert!(log_path.exists());

    // 1. While job is Running, deletion must be rejected
    let del_res = service.delete_job(&job.id);
    assert!(del_res.is_err(), "Deleting running job must fail");
    assert!(log_path.exists(), "Logs must not be deleted when deletion fails");

    // 2. Mark job as Succeeded (terminal state), now deletion succeeds and cleans up logs
    job.set_status(JobStatus::Succeeded);
    store.update_job(&job).unwrap();

    let deleted = service.delete_job(&job.id).unwrap();
    assert!(deleted);

    // Log directory must now be cleaned up
    assert!(!log_path.exists());
}

#[tokio::test]
async fn test_detached_runner_process_boundary_with_real_runner_contract() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("jobs.json");
    let store = JobStore::new_with_path(&store_path);

    let cli_exe = get_test_cli_runner_exe();
    let service = SchedulerService::with_scheduler(
        store.clone(),
        Some(cli_exe),
        Box::new(MockScheduler),
    );

    // Setup fake codex in PATH that creates a started file, sleeps briefly, and exits 0
    let fake_bin_dir = temp.path().join("bin");
    std::fs::create_dir_all(&fake_bin_dir).unwrap();

    let started_marker = temp.path().join("started.marker");
    let stop_marker = temp.path().join("stop.marker");

    #[cfg(windows)]
    {
        let fake_codex = fake_bin_dir.join("codex.cmd");
        let script = format!(
            "@echo off\r\necho started > \"{}\"\r\n:loop\r\nif exist \"{}\" goto done\r\nping -n 2 127.0.0.1 >nul\r\ngoto loop\r\n:done\r\necho fake codex finished\r\nexit /b 0\r\n",
            started_marker.display(),
            stop_marker.display()
        );
        std::fs::write(&fake_codex, script).unwrap();
    }

    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        let fake_codex = fake_bin_dir.join("codex");
        let script = format!(
            "#!/bin/sh\necho started > \"{}\"\nwhile [ ! -f \"{}\" ]; do\n  sleep 0.2\ndone\necho 'fake codex finished'\nexit 0\n",
            started_marker.display(),
            stop_marker.display()
        );
        std::fs::write(&fake_codex, script).unwrap();
        let mut perms = std::fs::metadata(&fake_codex).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&fake_codex, perms).unwrap();
    }

    // Set PATH with fake_bin_dir prepended, and configure CODEX_SCHEDULER_STORE
    let orig_path = std::env::var_os("PATH").unwrap_or_default();
    let mut paths = vec![fake_bin_dir];
    paths.extend(std::env::split_paths(&orig_path));
    let new_path = std::env::join_paths(paths).unwrap();
    unsafe {
        std::env::set_var("PATH", &new_path);
        std::env::set_var("CODEX_SCHEDULER_STORE", &store_path);
    }

    let now = Utc::now();
    let job = Job::new(
        ProviderType::Codex,
        "sess-real-boundary-1".to_string(),
        temp.path().to_path_buf(),
        Some("continue".to_string()),
        now - Duration::minutes(2),
        None,
    )
    .unwrap();
    store.insert_job(job.clone()).unwrap();

    // 1. Tick claims the job into Running and returns immediately without awaiting child
    let tick_start = std::time::Instant::now();
    let claimed = service.execute_tick().await.unwrap();
    let tick_elapsed = tick_start.elapsed();

    assert_eq!(claimed.len(), 1);
    assert_eq!(claimed[0].id, job.id);
    assert_eq!(claimed[0].status, JobStatus::Running);
    // Tick must complete quickly (under 500ms) while child keeps running
    assert!(tick_elapsed < std::time::Duration::from_millis(1500), "Tick must return immediately");

    // Wait until detached runner starts child (started.marker created)
    let mut started = false;
    for _ in 0..50 {
        if started_marker.exists() {
            started = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(started, "Detached runner must execute fake codex child");

    // 2. While child is running:
    // - Runner lock is held
    assert!(runner::is_runner_active(&job.id));
    // - Job in store is Running
    assert_eq!(store.get_job(&job.id).unwrap().unwrap().status, JobStatus::Running);

    // 3. Second tick while child is running:
    // - Reconcile does NOT mark Failed
    // - Job remains Running
    // - 0 due jobs claimed
    let second_tick = service.execute_tick().await.unwrap();
    assert_eq!(second_tick.len(), 0);
    assert_eq!(store.get_job(&job.id).unwrap().unwrap().status, JobStatus::Running);

    // 4. Signal child to finish
    std::fs::write(&stop_marker, "stop").unwrap();

    // 5. Wait for runner process to complete and release lock
    for _ in 0..50 {
        if !runner::is_runner_active(&job.id) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(!runner::is_runner_active(&job.id), "Runner must finish after child finishes");

    // 6. Job must now be terminal status (Succeeded) with execution history
    let finished_job = store.get_job(&job.id).unwrap().unwrap();
    assert_eq!(finished_job.status, JobStatus::Succeeded);
    assert_eq!(finished_job.execution_history.len(), 1);
    assert_eq!(finished_job.execution_history[0].exit_code, Some(0));

    // Restore PATH and CODEX_SCHEDULER_STORE
    unsafe {
        std::env::set_var("PATH", orig_path);
        std::env::remove_var("CODEX_SCHEDULER_STORE");
    }
}

#[tokio::test]
async fn test_running_job_cancel_and_delete_rejected_to_protect_session_writer() {
    let temp = tempdir().unwrap();
    let store = JobStore::new_with_path(temp.path().join("jobs.json"));
    let service = SchedulerService::with_scheduler(store.clone(), None, Box::new(MockScheduler));

    let now = Utc::now();
    let session_id = "sess-cancel-delete-guard";
    let mut job_a = Job::new(
        ProviderType::Codex,
        session_id.to_string(),
        temp.path().to_path_buf(),
        None,
        now - Duration::minutes(5),
        None,
    )
    .unwrap();
    job_a.set_status(JobStatus::Running);
    store.insert_job(job_a.clone()).unwrap();

    // 1. Cancelling running job must be rejected
    let cancel_res = service.cancel_job(&job_a.id);
    assert!(cancel_res.is_err(), "Cancelling running job must fail");
    assert_eq!(store.get_job(&job_a.id).unwrap().unwrap().status, JobStatus::Running);

    // 2. Deleting running job must be rejected
    let delete_res = service.delete_job(&job_a.id);
    assert!(delete_res.is_err(), "Deleting running job must fail");
    assert!(store.get_job(&job_a.id).unwrap().is_some());

    // 3. Second job for same session must NOT be claimed while first job is Running
    let job_b = Job::new(
        ProviderType::Codex,
        session_id.to_string(),
        temp.path().to_path_buf(),
        None,
        now - Duration::minutes(2),
        None,
    )
    .unwrap();
    store.insert_job(job_b.clone()).unwrap();

    let claimed = store.claim_due_jobs(now).unwrap();
    assert_eq!(claimed.len(), 0, "Second job must NOT be claimed while first job is Running");

    // 4. Scheduled job cancel must succeed
    let job_c = Job::new(
        ProviderType::Codex,
        "sess-scheduled-cancel".to_string(),
        temp.path().to_path_buf(),
        None,
        now + Duration::minutes(10),
        None,
    )
    .unwrap();
    store.insert_job(job_c.clone()).unwrap();
    let cancel_c = service.cancel_job(&job_c.id).unwrap();
    assert_eq!(cancel_c.status, JobStatus::Cancelled);
}

#[tokio::test]
async fn test_manual_execution_enforces_same_session_single_writer() {
    let temp = tempdir().unwrap();
    let store = JobStore::new_with_path(temp.path().join("jobs.json"));
    let service = SchedulerService::with_scheduler(store.clone(), None, Box::new(MockScheduler));

    let now = Utc::now();
    let session_id = "sess-manual-exclusion";
    let mut job_a = Job::new(
        ProviderType::Codex,
        session_id.to_string(),
        temp.path().to_path_buf(),
        None,
        now - Duration::minutes(5),
        None,
    )
    .unwrap();
    job_a.set_status(JobStatus::Running);
    store.insert_job(job_a.clone()).unwrap();

    // 1. Manual execute_job for another job with the SAME session must fail with SessionBusy
    let job_b = Job::new(
        ProviderType::Codex,
        session_id.to_string(),
        temp.path().to_path_buf(),
        None,
        now,
        None,
    )
    .unwrap();
    store.insert_job(job_b.clone()).unwrap();

    let exec_b_res = service.execute_job(&job_b.id).await;
    assert!(exec_b_res.is_err(), "Manual execution of conflicting session must fail");

    // 2. Manual execute_job for a DIFFERENT session must succeed (in claim)
    let job_c = Job::new(
        ProviderType::Codex,
        "sess-different".to_string(),
        temp.path().to_path_buf(),
        None,
        now,
        None,
    )
    .unwrap();
    store.insert_job(job_c.clone()).unwrap();
    let claimed_c = store.claim_job_for_execution(&job_c.id).unwrap();
    assert_eq!(claimed_c.status, JobStatus::Running);

    // 3. Orphan runner with alive child process: run_job_runner on same job must be rejected
    let current_pid = std::process::id();
    let current_start = runner::get_process_start_time(current_pid).unwrap().unwrap();
    let alive_info = runner::RunnerInfo {
        job_id: job_a.id.clone(),
        session_id: session_id.to_string(),
        runner_pid: 99999, // Dead runner
        runner_started_at: now - Duration::minutes(10),
        codex_pid: Some(current_pid),
        codex_start_time: Some(current_start),
    };
    runner::write_runner_info(&alive_info).unwrap();

    let runner_res = service.run_job_runner(&job_a.id).await;
    assert!(runner_res.is_err(), "run_job_runner must reject starting second writer when child is still alive");
}

#[tokio::test]
async fn test_claim_to_lock_handoff_grace_period_prevents_premature_orphan_recovery() {
    let temp = tempdir().unwrap();
    let store = JobStore::new_with_path(temp.path().join("jobs.json"));
    let service = SchedulerService::with_scheduler(store.clone(), None, Box::new(MockScheduler));

    let now = Utc::now();
    let mut fresh_job = Job::new(
        ProviderType::Codex,
        "sess-handoff-race".to_string(),
        temp.path().to_path_buf(),
        None,
        now,
        None,
    )
    .unwrap();
    // Simulate job freshly claimed into Running 1 second ago (runner process still spawning)
    fresh_job.set_status(JobStatus::Running);
    fresh_job.updated_at = now - chrono::Duration::seconds(1);
    store.insert_job(fresh_job.clone()).unwrap();

    // Reconcile must NOT recover this job during the 15-second grace period
    let recovered = service.reconcile_running_jobs().unwrap();
    assert_eq!(recovered.len(), 0, "Freshly claimed job must not be recovered during handoff grace period");
    assert_eq!(store.get_job(&fresh_job.id).unwrap().unwrap().status, JobStatus::Running);

    // Once 20 seconds have passed without runner lock or info, it is a genuine orphan crash and must recover
    fresh_job.updated_at = now - chrono::Duration::seconds(20);
    store.update_job(&fresh_job).unwrap();

    let recovered_after = service.reconcile_running_jobs().unwrap();
    assert_eq!(recovered_after.len(), 1, "Orphan job exceeding grace period must be recovered");
    assert_eq!(recovered_after[0].status, JobStatus::Failed);
}

#[tokio::test]
async fn test_unknown_liveness_observation_fails_closed() {
    let temp = tempdir().unwrap();
    let store = JobStore::new_with_path(temp.path().join("jobs.json"));
    let service = SchedulerService::with_scheduler(store.clone(), None, Box::new(MockScheduler));

    let now = Utc::now();
    let session_id = "sess-unknown-liveness";
    let mut job = Job::new(
        ProviderType::Codex,
        session_id.to_string(),
        temp.path().to_path_buf(),
        None,
        now - Duration::minutes(10),
        None,
    )
    .unwrap();
    job.set_status(JobStatus::Running);
    job.updated_at = now - Duration::minutes(5);
    store.insert_job(job.clone()).unwrap();

    // Write runner info where child start time is None (observation incomplete / Unknown)
    let unknown_info = runner::RunnerInfo {
        job_id: job.id.clone(),
        session_id: session_id.to_string(),
        runner_pid: 99999,
        runner_started_at: now - Duration::minutes(5),
        codex_pid: Some(12345),
        codex_start_time: None, // Unknown start time
    };
    runner::write_runner_info(&unknown_info).unwrap();

    // Reconcile must fail-closed: do NOT recover, maintain Running
    let recovered = service.reconcile_running_jobs().unwrap();
    assert_eq!(recovered.len(), 0, "Unknown liveness must fail-closed and not recover job");
    assert_eq!(store.get_job(&job.id).unwrap().unwrap().status, JobStatus::Running);

    // Same session job must continue to be blocked
    let second_job = Job::new(
        ProviderType::Codex,
        session_id.to_string(),
        temp.path().to_path_buf(),
        None,
        now,
        None,
    )
    .unwrap();
    store.insert_job(second_job.clone()).unwrap();
    let claimed = store.claim_due_jobs(now).unwrap();
    assert_eq!(claimed.len(), 0, "Second job must remain blocked while liveness is Unknown");
}

#[tokio::test]
async fn test_strict_10mb_log_cap_with_large_output() {
    let temp = tempdir().unwrap();
    let log_file_path = temp.path().join("attempt-1.log");

    let file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(&log_file_path)
        .await
        .unwrap();

    let mut writer = runner::CappedLogWriter::new(file, runner::MAX_LOG_FILE_BYTES);

    // Simulate 15 MB of output in 64KB chunks
    let chunk = vec![b'A'; 64 * 1024];
    for _ in 0..240 {
        writer.write_chunk(&chunk).await.unwrap();
    }

    // Check disk file size
    let metadata = std::fs::metadata(&log_file_path).unwrap();
    let actual_size = metadata.len();

    assert!(
        actual_size <= runner::MAX_LOG_FILE_BYTES,
        "Actual file size ({} bytes) must NOT exceed MAX_LOG_FILE_BYTES ({} bytes)",
        actual_size,
        runner::MAX_LOG_FILE_BYTES
    );
    assert_eq!(
        actual_size,
        runner::MAX_LOG_FILE_BYTES,
        "File size should match exactly the 10MB cap when output exceeds 15MB"
    );
}

