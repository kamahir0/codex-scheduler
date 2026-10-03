use chrono::Utc;
use codex_scheduler_core::models::{JobStatus, ProviderType, RetryPolicy};
use codex_scheduler_core::store::JobStore;
use codex_scheduler_core::SchedulerService;
use std::path::PathBuf;

#[tokio::test]
async fn test_full_scheduler_workflow() {
    let temp_dir = tempfile::tempdir().unwrap();
    let store_path = temp_dir.path().join("test_jobs.json");
    let store = JobStore::new_with_path(store_path);

    let service = SchedulerService::with_scheduler(
        store.clone(),
        Some(PathBuf::from("codex-scheduler-cli")),
        Box::new(codex_scheduler_core::os_scheduler::FallbackScheduler),
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
    let retrieved = store.get_job(&job.id).unwrap().expect("Job should be in store");
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
        Box::new(codex_scheduler_core::os_scheduler::FallbackScheduler),
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
        Box::new(codex_scheduler_core::os_scheduler::FallbackScheduler),
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
    use codex_scheduler_core::os_scheduler::{SchedulerBackend, SchedulerError};
    use codex_scheduler_core::models::Job;
    use std::path::Path;

    struct MockScheduler {
        installed: bool,
        ready: bool,
    }
    impl SchedulerBackend for MockScheduler {
        fn ensure_scheduler_installed(&self, _path: &Path) -> Result<(), SchedulerError> {
            Ok(())
        }
        fn is_scheduler_installed(&self) -> bool {
            self.installed
        }
        fn is_scheduler_ready(&self) -> bool {
            self.ready
        }
        fn is_scheduler_path_matched(&self, _path: &Path) -> bool {
            self.installed
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
        Box::new(MockScheduler { installed: false, ready: false }),
    );
    assert!(cli_service.desktop_exe_path().is_none());

    #[cfg(target_os = "macos")]
    {
        let ensure_err = cli_service.ensure_scheduler().unwrap_err();
        match ensure_err {
            codex_scheduler_core::CoreError::Scheduler(SchedulerError::ExecutableNotFound(_)) => {}
            _ => panic!("Expected ExecutableNotFound when exe_path is None, got {:?}", ensure_err),
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
            Box::new(MockScheduler { installed: true, ready: true }),
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
            Some(dummy_cli),
            Box::new(MockScheduler { installed: false, ready: false }),
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
    assert_eq!(early_claims.len(), 0, "Future retrying job must not be claimed prematurely");

    // 4. At / after due time: next tick MUST claim the Retrying job and transition to Running
    let after_due = next_retry_at + chrono::Duration::seconds(5);
    let due_claims = store.claim_due_jobs(after_due).unwrap();
    assert_eq!(due_claims.len(), 1, "Due retrying job must be claimed for re-execution");
    assert_eq!(due_claims[0].id, job.id);
    assert_eq!(due_claims[0].status, JobStatus::Running);

    let in_store = store.get_job(&job.id).unwrap().unwrap();
    assert_eq!(in_store.status, JobStatus::Running);

    // 5. Subsequent immediate tick: must return 0 jobs (preventing duplicate execution)
    let duplicate_claims = store.claim_due_jobs(after_due).unwrap();
    assert_eq!(duplicate_claims.len(), 0, "Claimed running job must not be claimed again");
}



