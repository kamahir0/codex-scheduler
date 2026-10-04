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
