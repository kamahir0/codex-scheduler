use chrono::Utc;
use codex_scheduler_core::SchedulerService;
use codex_scheduler_core::models::{JobStatus, ProviderType, RetryPolicy};
use codex_scheduler_core::store::JobStore;
use std::path::PathBuf;

struct ReadyMockScheduler;
impl codex_scheduler_core::os_scheduler::SchedulerBackend for ReadyMockScheduler {
    fn ensure_scheduler_installed(
        &self,
        _path: &std::path::Path,
    ) -> Result<(), codex_scheduler_core::os_scheduler::SchedulerError> {
        Ok(())
    }
    fn is_scheduler_installed(&self) -> bool {
        true
    }
    fn is_scheduler_ready(&self) -> bool {
        true
    }
    fn is_scheduler_path_matched(&self, _path: &std::path::Path) -> bool {
        true
    }
    fn uninstall_scheduler(
        &self,
    ) -> Result<(), codex_scheduler_core::os_scheduler::SchedulerError> {
        Ok(())
    }
    fn register_job(
        &self,
        _job: &codex_scheduler_core::models::Job,
        _path: &std::path::Path,
    ) -> Result<(), codex_scheduler_core::os_scheduler::SchedulerError> {
        Ok(())
    }
    fn unregister_job(
        &self,
        _job_id: &str,
    ) -> Result<(), codex_scheduler_core::os_scheduler::SchedulerError> {
        Ok(())
    }
}

#[tokio::test]
async fn test_full_scheduler_workflow() {
    let temp_dir = tempfile::tempdir().unwrap();
    let store_path = temp_dir.path().join("test_jobs.json");
    let store = JobStore::new_with_path(store_path);

    let service = SchedulerService::with_scheduler(
        store.clone(),
        Some(PathBuf::from("codex-scheduler-cli")),
        Box::new(ReadyMockScheduler),
    );

    // 1. Schedule a job
    let scheduled_time = Utc::now() + chrono::Duration::hours(2);
    let job = service
        .schedule_job(
            ProviderType::Codex,
            "session-integration-001".to_string(),
            temp_dir.path().to_path_buf(),
            Some("continue".to_string()),
            scheduled_time,
            Some(RetryPolicy {
                enabled: true,
                interval_seconds: 60,
                max_attempts: 3,
                retry_on_quota_only: true,
            }),
        )
        .expect("Should schedule job");

    assert_eq!(job.status, JobStatus::Scheduled);
    assert_eq!(job.session_id, "session-integration-001");
    assert_eq!(job.prompt, "continue");

    // 2. Query job from store
    let retrieved = store
        .get_job(&job.id)
        .unwrap()
        .expect("Job should be in store");
    assert_eq!(retrieved.id, job.id);

    // 3. Cancel job
    let cancelled = service.cancel_job(&job.id).expect("Should cancel job");
    assert_eq!(cancelled.status, JobStatus::Cancelled);

    let from_store = store.get_job(&job.id).unwrap().unwrap();
    assert_eq!(from_store.status, JobStatus::Cancelled);

    // 4. Delete job
    let deleted = service.delete_job(&job.id).expect("Should delete job");
    assert!(deleted);
    assert!(store.get_job(&job.id).unwrap().is_none());
}

#[tokio::test]
async fn test_worker_provisioning_and_scheduler_integration() {
    use codex_scheduler_core::worker::canonical_worker_path;

    let canonical = canonical_worker_path();
    assert!(canonical.to_string_lossy().contains("codex-scheduler"));

    let store_dir = tempfile::tempdir().unwrap();
    let store = JobStore::new_with_path(store_dir.path().join("jobs.json"));

    // Verify service uses worker path
    let dummy_worker = store_dir.path().join("mock-worker");
    std::fs::write(&dummy_worker, b"mock").unwrap();

    let service = SchedulerService::new(store.clone(), dummy_worker.clone());
    assert_eq!(service.cli_path(), dummy_worker.as_path());
}

#[tokio::test]
async fn test_tick_due_and_future_jobs_filter() {
    let temp_dir = tempfile::tempdir().unwrap();
    let store_path = temp_dir.path().join("test_tick_jobs.json");
    let store = JobStore::new_with_path(store_path);
    let service = SchedulerService::with_scheduler(
        store.clone(),
        Some(PathBuf::from("codex-scheduler-cli")),
        Box::new(ReadyMockScheduler),
    );

    // 過去時刻のジョブ（期限到来）
    let past_time = Utc::now() - chrono::Duration::minutes(5);
    let due_job = service
        .schedule_job(
            ProviderType::Codex,
            "session-due".to_string(),
            temp_dir.path().to_path_buf(),
            Some("continue".to_string()),
            past_time,
            None,
        )
        .unwrap();

    // 未来時刻のジョブ（まだ待機）
    let future_time = Utc::now() + chrono::Duration::hours(2);
    let future_job = service
        .schedule_job(
            ProviderType::Codex,
            "session-future".to_string(),
            temp_dir.path().to_path_buf(),
            Some("continue".to_string()),
            future_time,
            None,
        )
        .unwrap();

    // tick 相当の判定ロジックを検証
    let all_jobs = store.load_all().unwrap();
    let now = Utc::now();

    let due_jobs: Vec<_> = all_jobs
        .iter()
        .filter(|j| j.status == JobStatus::Scheduled && j.scheduled_at <= now)
        .collect();

    assert_eq!(due_jobs.len(), 1);
    assert_eq!(due_jobs[0].id, due_job.id);

    let future_jobs: Vec<_> = all_jobs
        .iter()
        .filter(|j| j.status == JobStatus::Scheduled && j.scheduled_at > now)
        .collect();

    assert_eq!(future_jobs.len(), 1);
    assert_eq!(future_jobs[0].id, future_job.id);
}

#[tokio::test]
async fn test_execute_tick_shared_semantics_updates_history() {
    let temp_dir = tempfile::tempdir().unwrap();
    let store_path = temp_dir.path().join("test_shared_tick.json");
    let store = JobStore::new_with_path(store_path);
    let service = SchedulerService::with_scheduler(
        store.clone(),
        Some(PathBuf::from("/bin/echo")),
        Box::new(ReadyMockScheduler),
    );

    // Past due job (ready for execution)
    let past_time = Utc::now() - chrono::Duration::minutes(1);
    let job = service
        .schedule_job(
            ProviderType::Codex,
            "session-due-tick".to_string(),
            temp_dir.path().to_path_buf(),
            Some("test prompt".to_string()),
            past_time,
            None,
        )
        .unwrap();

    // Future job (not due)
    let future_time = Utc::now() + chrono::Duration::hours(1);
    let _future = service
        .schedule_job(
            ProviderType::Codex,
            "session-future-tick".to_string(),
            temp_dir.path().to_path_buf(),
            Some("future prompt".to_string()),
            future_time,
            None,
        )
        .unwrap();

    // Run execute_tick directly without Tauri/GUI initialization
    let finished_jobs = service.execute_tick().await.unwrap();
    assert_eq!(finished_jobs.len(), 1);
    assert_eq!(finished_jobs[0].id, job.id);

    // Job in store should now have execution attempt history
    let updated_job = store.get_job(&job.id).unwrap().unwrap();
    assert_eq!(updated_job.execution_history.len(), 1);

    // Next tick execution should find 0 due jobs
    let second_tick = service.execute_tick().await.unwrap();
    assert_eq!(second_tick.len(), 0);
}

#[tokio::test]
async fn test_cli_service_guards_against_launchagent_registration() {
    use codex_scheduler_core::models::Job;
    use codex_scheduler_core::os_scheduler::{SchedulerBackend, SchedulerError};
    use std::path::Path;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct MockScheduler {
        installed: Arc<AtomicBool>,
        ready: Arc<AtomicBool>,
    }
    impl MockScheduler {
        fn new(installed: bool, ready: bool) -> Self {
            Self {
                installed: Arc::new(AtomicBool::new(installed)),
                ready: Arc::new(AtomicBool::new(ready)),
            }
        }
    }
    impl SchedulerBackend for MockScheduler {
        fn ensure_scheduler_installed(&self, _path: &Path) -> Result<(), SchedulerError> {
            self.installed.store(true, Ordering::SeqCst);
            self.ready.store(true, Ordering::SeqCst);
            Ok(())
        }
        fn is_scheduler_installed(&self) -> bool {
            self.installed.load(Ordering::SeqCst)
        }
        fn is_scheduler_ready(&self) -> bool {
            self.ready.load(Ordering::SeqCst)
        }
        fn is_scheduler_path_matched(&self, _path: &Path) -> bool {
            self.installed.load(Ordering::SeqCst)
        }
        fn uninstall_scheduler(&self) -> Result<(), SchedulerError> {
            Ok(())
        }
        fn register_job(&self, _job: &Job, _path: &Path) -> Result<(), SchedulerError> {
            Ok(())
        }
        fn unregister_job(&self, _job_id: &str) -> Result<(), SchedulerError> {
            Ok(())
        }
    }

    let temp_dir = tempfile::tempdir().unwrap();
    let store = JobStore::new_with_path(temp_dir.path().join("cli_guard_jobs.json"));

    // Case 1: CLI service without executable path cannot ensure scheduler
    let cli_service = SchedulerService::with_scheduler(
        store.clone(),
        None,
        Box::new(MockScheduler::new(false, false)),
    );
    assert!(cli_service.desktop_exe_path().is_none());

    #[cfg(target_os = "macos")]
    {
        let ensure_err = cli_service.ensure_scheduler().unwrap_err();
        match ensure_err {
            codex_scheduler_core::CoreError::Scheduler(SchedulerError::ExecutableNotFound(_)) => {}
            _ => panic!(
                "Expected ExecutableNotFound when exe_path is None, got {:?}",
                ensure_err
            ),
        }

        // Case 2: If scheduler is NOT ready and exe_path is None, schedule_job fails because it cannot auto-ensure
        let schedule_err = cli_service
            .schedule_job(
                ProviderType::Codex,
                "session-cli".to_string(),
                temp_dir.path().to_path_buf(),
                Some("cli prompt".to_string()),
                Utc::now() + chrono::Duration::hours(1),
                None,
            )
            .unwrap_err();

        match schedule_err {
            codex_scheduler_core::CoreError::Scheduler(SchedulerError::ExecutableNotFound(_)) => {}
            _ => panic!("Expected ExecutableNotFound, got {:?}", schedule_err),
        }

        // Case 3: If scheduler IS installed AND READY (e.g. by Desktop app), scheduling from CLI succeeds without touching scheduler
        let installed_cli_service = SchedulerService::with_scheduler(
            store.clone(),
            None,
            Box::new(MockScheduler::new(true, true)),
        );
        let scheduled_job = installed_cli_service
            .schedule_job(
                ProviderType::Codex,
                "session-cli-ok".to_string(),
                temp_dir.path().to_path_buf(),
                Some("cli prompt ok".to_string()),
                Utc::now() + chrono::Duration::hours(1),
                None,
            )
            .expect("Should schedule when already installed and ready");

        assert_eq!(scheduled_job.session_id, "session-cli-ok");

        // Case 4: CLI with valid executable path can auto-ensure scheduler when not yet installed
        let dummy_cli = temp_dir.path().join("codex-scheduler");
        std::fs::write(&dummy_cli, b"#!/bin/sh\nexit 0").unwrap();
        let auto_ensure_service = SchedulerService::with_scheduler(
            store.clone(),
            Some(dummy_cli.clone()),
            Box::new(MockScheduler::new(false, false)),
        );
        let auto_job = auto_ensure_service
            .schedule_job(
                ProviderType::Codex,
                "session-auto".to_string(),
                temp_dir.path().to_path_buf(),
                Some("auto prompt".to_string()),
                Utc::now() + chrono::Duration::hours(1),
                None,
            )
            .expect("Should auto-ensure when CLI exe_path is present");
        assert_eq!(auto_job.session_id, "session-auto");

        // Case 5: When ensure/repair fails to make scheduler ready, schedule_job fails and NO job is added to the store (atomicity)
        struct FailingEnsureScheduler;
        impl SchedulerBackend for FailingEnsureScheduler {
            fn ensure_scheduler_installed(&self, _path: &Path) -> Result<(), SchedulerError> {
                Ok(()) // returns Ok without making ready true (simulating unready state)
            }
            fn is_scheduler_installed(&self) -> bool {
                true
            }
            fn is_scheduler_ready(&self) -> bool {
                false
            }
            fn is_scheduler_path_matched(&self, _path: &Path) -> bool {
                true
            }
            fn uninstall_scheduler(&self) -> Result<(), SchedulerError> {
                Ok(())
            }
            fn register_job(&self, _job: &Job, _path: &Path) -> Result<(), SchedulerError> {
                Ok(())
            }
            fn unregister_job(&self, _job_id: &str) -> Result<(), SchedulerError> {
                Ok(())
            }
        }
        let failing_service = SchedulerService::with_scheduler(
            store.clone(),
            Some(dummy_cli),
            Box::new(FailingEnsureScheduler),
        );
        let jobs_before = store.load_all().unwrap().len();
        let fail_res = failing_service.schedule_job(
            ProviderType::Codex,
            "session-atomic-fail".to_string(),
            temp_dir.path().to_path_buf(),
            Some("fail prompt".to_string()),
            Utc::now() + chrono::Duration::hours(1),
            None,
        );
        assert!(
            fail_res.is_err(),
            "schedule_job must fail when scheduler is not ready after ensure"
        );
        let jobs_after = store.load_all().unwrap().len();
        assert_eq!(
            jobs_before, jobs_after,
            "JobStore must not retain new job when scheduler is unready"
        );
    }
}

#[tokio::test]
async fn test_retry_quota_backoff_and_subsequent_due_claim_lifecycle() {
    use codex_scheduler_core::adapter::ExecutionResult;
    use codex_scheduler_core::models::Job;
    use codex_scheduler_core::retry::{NextAction, RetryEngine};

    let temp_dir = tempfile::tempdir().unwrap();
    let store_path = temp_dir.path().join("test_retry_claim.json");
    let store = JobStore::new_with_path(store_path);

    let now = Utc::now();
    let job = Job::new(
        ProviderType::Codex,
        "session-retry-flow".to_string(),
        temp_dir.path().to_path_buf(),
        Some("resume prompt".to_string()),
        now - chrono::Duration::minutes(1),
        Some(RetryPolicy {
            enabled: true,
            interval_seconds: 60,
            max_attempts: 3,
            retry_on_quota_only: true,
        }),
    )
    .unwrap();

    // 1. Initial claim -> becomes Running
    store.insert_job(job.clone()).unwrap();
    let claimed = store.claim_due_jobs(now).unwrap();
    assert_eq!(claimed.len(), 1);
    assert_eq!(claimed[0].status, JobStatus::Running);

    // 2. Execution encounters quota error
    let quota_err = ExecutionResult {
        exit_code: Some(1),
        stdout: String::new(),
        stderr: "Rate limit reached / quota exceeded".to_string(),
        is_quota_error: true,
        success: false,
        error_message: Some("HTTP 429 Quota error".to_string()),
    };

    let mut running_job = store.get_job(&job.id).unwrap().unwrap();
    let action = RetryEngine::apply_evaluation(&mut running_job, &quota_err);

    let next_retry_at = match action {
        NextAction::RetryAfter(dt) => {
            running_job.scheduled_at = dt;
            dt
        }
        _ => panic!("Expected RetryAfter action, got: {:?}", action),
    };

    assert_eq!(running_job.status, JobStatus::Retrying);
    store.update_job(&running_job).unwrap();

    // 3. Before due time: tick should NOT claim the Retrying job
    let before_due = next_retry_at - chrono::Duration::seconds(10);
    let early_claims = store.claim_due_jobs(before_due).unwrap();
    assert_eq!(
        early_claims.len(),
        0,
        "Future retrying job must not be claimed prematurely"
    );

    // 4. At / after due time: next tick MUST claim the Retrying job and transition to Running
    let after_due = next_retry_at + chrono::Duration::seconds(5);
    let due_claims = store.claim_due_jobs(after_due).unwrap();
    assert_eq!(
        due_claims.len(),
        1,
        "Due retrying job must be claimed for re-execution"
    );
    assert_eq!(due_claims[0].id, job.id);
    assert_eq!(due_claims[0].status, JobStatus::Running);

    let in_store = store.get_job(&job.id).unwrap().unwrap();
    assert_eq!(in_store.status, JobStatus::Running);

    // 5. Subsequent immediate tick: must return 0 jobs (preventing duplicate execution)
    let duplicate_claims = store.claim_due_jobs(after_due).unwrap();
    assert_eq!(
        duplicate_claims.len(),
        0,
        "Claimed running job must not be claimed again"
    );
}

#[cfg(windows)]
static ENV_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(windows)]
struct EnvVarGuard {
    key: &'static str,
    original: Option<std::ffi::OsString>,
}

#[cfg(windows)]
impl EnvVarGuard {
    fn set(key: &'static str, value: &std::ffi::OsStr) -> Self {
        let original = std::env::var_os(key);
        unsafe {
            std::env::set_var(key, value);
        }
        Self { key, original }
    }
}

#[cfg(windows)]
impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        unsafe {
            if let Some(ref orig) = self.original {
                std::env::set_var(self.key, orig);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }
}

#[cfg(windows)]
#[tokio::test]
async fn test_windows_codex_process_launch_and_tick() {
    use std::fs::{self, File};
    use std::io::Write;

    let _lock = ENV_MUTEX.lock().unwrap();

    let temp_root = tempfile::tempdir().expect("create temp root");
    let npm_bin = temp_root.path().join("npm");
    fs::create_dir_all(&npm_bin).expect("create npm bin dir");

    // 1. Create extensionless POSIX shim (would fail with os error 193 if executed)
    let posix_shim = npm_bin.join("codex");
    let mut f1 = File::create(&posix_shim).expect("create posix shim");
    writeln!(f1, "#!/bin/sh\necho 'ERROR: POSIX SHIM EXECUTED'\nexit 193")
        .expect("write posix shim");

    // 2. Compile helper executable with rustc to faithfully capture logical argv and CWD without cmd interpretation issues
    let helper_rs = temp_root.path().join("helper.rs");
    let helper_exe = npm_bin.join("helper.exe");
    fs::write(
        &helper_rs,
        r#"
use std::env;
use std::fs;

fn main() {
    let args: Vec<String> = env::args().collect();
    let cwd = env::current_dir().unwrap();
    let capture_file = env::var("CAPTURE_LOG").expect("CAPTURE_LOG env var");

    let mut content = format!("CWD={}\n", cwd.display());
    // args[0] is helper.exe, subsequent elements are the logical arguments
    for arg in &args[1..] {
        content.push_str(&format!("ARG={}\n", arg));
    }
    fs::write(&capture_file, content).expect("write capture log");
    println!("CWD={}", cwd.display());
}
"#,
    )
    .expect("write helper.rs");

    let rustc_status = std::process::Command::new("rustc")
        .arg(&helper_rs)
        .arg("-o")
        .arg(&helper_exe)
        .status()
        .expect("compile helper.exe with rustc");
    assert!(
        rustc_status.success(),
        "helper.exe compilation must succeed"
    );

    // 3. Create valid Windows .cmd launcher that delegates all arguments to helper.exe
    let cmd_launcher = npm_bin.join("codex.cmd");
    let mut f2 = File::create(&cmd_launcher).expect("create cmd launcher");
    writeln!(
        f2,
        "@echo off\n\"%~dp0helper.exe\" %*\nexit /b %ERRORLEVEL%"
    )
    .expect("write cmd launcher");

    // Setup capture file and RAII environment guards (mutex protected and restored on drop/panic)
    let capture_log = temp_root.path().join("captured.log");
    let _capture_guard = EnvVarGuard::set("CAPTURE_LOG", capture_log.as_os_str());

    let original_path = std::env::var_os("PATH").unwrap_or_default();
    let mut new_paths = vec![npm_bin.clone()];
    new_paths.extend(std::env::split_paths(&original_path));
    let joined_path = std::env::join_paths(new_paths).expect("join paths");
    let _path_guard = EnvVarGuard::set("PATH", &joined_path);

    // Verify adapter resolves codex.cmd and ignores extensionless codex
    let adapter = codex_scheduler_core::adapter::codex::CodexAdapter::new();
    let resolved = adapter.resolve_executable().expect("resolve executable");
    assert_eq!(
        resolved, cmd_launcher,
        "Adapter must resolve codex.cmd on Windows"
    );

    // 4. Test execute_resume with spaces in cwd and shell metacharacters / injection sentinel in prompt
    let project_dir = temp_root.path().join("My Test Project With Spaces");
    fs::create_dir_all(&project_dir).expect("create project dir");

    let marker_file = temp_root.path().join("INJECTED_SENTINEL.txt");
    let session_id = "sess-win-test-999";
    let prompt = format!(
        "fix & echo INJECTED > \"{}\" | ping ^ <file> > /dev/null %PATH% !VAR! \"double quoted\"",
        marker_file.display()
    );

    let result = adapter
        .execute_resume(session_id, &project_dir, &prompt)
        .await
        .expect("execute_resume must succeed");

    assert!(
        result.success,
        "Process execution must succeed: {:?}",
        result
    );
    assert_eq!(result.exit_code, Some(0));
    assert!(!result.is_quota_error);

    // Verify injection sentinel marker file was NOT created (direct proof of shell injection safety)
    assert!(
        !marker_file.exists(),
        "Shell injection occurred! Sentinel file {} was created",
        marker_file.display()
    );

    // Verify stdout contains CWD
    assert!(
        result
            .stdout
            .contains(&format!("CWD={}", project_dir.display())),
        "stdout should reflect project cwd: {}",
        result.stdout
    );

    // Verify captured exact CWD and exact logical argv
    let captured = fs::read_to_string(&capture_log).expect("read capture log");
    let mut captured_cwd = None;
    let mut captured_args = Vec::new();
    for line in captured.lines() {
        if let Some(rest) = line.strip_prefix("CWD=") {
            captured_cwd = Some(rest.to_string());
        } else if let Some(rest) = line.strip_prefix("ARG=") {
            captured_args.push(rest.to_string());
        }
    }

    assert_eq!(
        captured_cwd.as_deref(),
        Some(project_dir.display().to_string().as_str()),
        "CWD must match project_dir exactly"
    );

    let expected_args = vec![
        "exec".to_string(),
        "resume".to_string(),
        session_id.to_string(),
        prompt.clone(),
    ];
    assert_eq!(
        captured_args, expected_args,
        "Logical argv must match expected arguments exactly without alteration"
    );

    // 5. End-to-end SchedulerService tick execution
    let store_path = temp_root.path().join("jobs.json");
    let store = JobStore::new_with_path(store_path);
    let service = SchedulerService::with_scheduler(
        store.clone(),
        Some(PathBuf::from("codex-scheduler.exe")),
        Box::new(ReadyMockScheduler),
    );

    let due_time = Utc::now() - chrono::Duration::minutes(5);
    let job = service
        .schedule_job(
            ProviderType::Codex,
            session_id.to_string(),
            project_dir.clone(),
            Some(prompt.clone()),
            due_time,
            None,
        )
        .expect("schedule job");

    // Clear capture log before tick execution
    fs::remove_file(&capture_log).ok();

    let tick_results = service.execute_tick().await.expect("execute tick");
    assert_eq!(tick_results.len(), 1);
    assert_eq!(tick_results[0].status, JobStatus::Succeeded);
    assert_eq!(tick_results[0].id, job.id);

    let updated_job = store.get_job(&job.id).unwrap().unwrap();
    assert_eq!(updated_job.status, JobStatus::Succeeded);
    assert_eq!(updated_job.execution_history.len(), 1);
    assert_eq!(updated_job.execution_history[0].exit_code, Some(0));

    // Verify tick execution did not trigger injection and preserved exact argv
    assert!(
        !marker_file.exists(),
        "Shell injection occurred during execute_tick! Sentinel file was created"
    );
    let tick_captured = fs::read_to_string(&capture_log).expect("read capture log after tick");
    let mut tick_captured_args = Vec::new();
    for line in tick_captured.lines() {
        if let Some(rest) = line.strip_prefix("ARG=") {
            tick_captured_args.push(rest.to_string());
        }
    }
    assert_eq!(
        tick_captured_args, expected_args,
        "execute_tick logical argv must match expected arguments exactly"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn test_unix_codex_process_launch_and_tick() {
    use std::fs::{self, File, Permissions};
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    let temp_root = tempfile::tempdir().expect("create temp root");
    let bin_dir = temp_root.path().join("bin");
    fs::create_dir_all(&bin_dir).expect("create bin dir");

    // Create executable mock codex script
    let mock_codex = bin_dir.join("codex");
    let mut f = File::create(&mock_codex).expect("create mock codex");
    writeln!(
        f,
        "#!/bin/sh\necho \"CWD=$(pwd)\"\necho \"ARGV=$*\"\nexit 0"
    )
    .expect("write mock codex");
    fs::set_permissions(&mock_codex, Permissions::from_mode(0o755)).expect("set executable");

    // Prepend bin_dir to PATH
    let original_path = std::env::var_os("PATH").unwrap_or_default();
    let mut new_paths = vec![bin_dir.clone()];
    new_paths.extend(std::env::split_paths(&original_path));
    let joined_path = std::env::join_paths(new_paths).expect("join paths");
    unsafe {
        std::env::set_var("PATH", joined_path);
    }

    let adapter = codex_scheduler_core::adapter::codex::CodexAdapter::new();
    let resolved = adapter.resolve_executable().expect("resolve executable");
    assert_eq!(resolved, mock_codex);

    let project_dir = temp_root.path().join("Unix Project With Spaces");
    fs::create_dir_all(&project_dir).expect("create project dir");

    let canonical_project_dir = fs::canonicalize(&project_dir).expect("canonicalize project dir");

    let session_id = "sess-unix-test-001";
    let prompt = "continue with & special chars";

    let result = adapter
        .execute_resume(session_id, &project_dir, prompt)
        .await
        .expect("execute_resume");

    assert!(result.success);
    assert_eq!(result.exit_code, Some(0));
    assert!(
        result
            .stdout
            .contains(&format!("CWD={}", canonical_project_dir.display()))
    );
    assert!(
        result
            .stdout
            .contains("ARGV=exec resume sess-unix-test-001 continue with & special chars")
    );

    // End-to-end tick execution
    let store_path = temp_root.path().join("jobs.json");
    let store = JobStore::new_with_path(store_path);
    let service = SchedulerService::with_scheduler(
        store.clone(),
        Some(PathBuf::from("codex-scheduler")),
        Box::new(ReadyMockScheduler),
    );

    let due_time = Utc::now() - chrono::Duration::minutes(5);
    let job = service
        .schedule_job(
            ProviderType::Codex,
            session_id.to_string(),
            project_dir.clone(),
            Some(prompt.to_string()),
            due_time,
            None,
        )
        .expect("schedule job");

    let tick_results = service.execute_tick().await.expect("execute tick");
    assert_eq!(tick_results.len(), 1);
    assert_eq!(tick_results[0].status, JobStatus::Succeeded);

    let updated_job = store.get_job(&job.id).unwrap().unwrap();
    assert_eq!(updated_job.status, JobStatus::Succeeded);
    assert_eq!(updated_job.execution_history.len(), 1);
    assert_eq!(updated_job.execution_history[0].exit_code, Some(0));

    // Restore original PATH
    unsafe {
        std::env::set_var("PATH", original_path);
    }
}
