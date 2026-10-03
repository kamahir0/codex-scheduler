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
        PathBuf::from("codex-scheduler-cli"),
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
        PathBuf::from("codex-scheduler-cli"),
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


