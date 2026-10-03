pub mod adapter;
pub mod models;
pub mod os_scheduler;
pub mod retry;
pub mod store;
pub mod worker;

use adapter::codex::CodexAdapter;
use adapter::ExecutionResult;
use chrono::Utc;
use models::{ExecutionAttempt, Job, JobStatus, ProviderType};
use os_scheduler::get_platform_scheduler;
use retry::{NextAction, RetryEngine};
use std::path::{Path, PathBuf};
use store::{JobStore, StoreError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("Store error: {0}")]
    Store(#[from] StoreError),
    #[error("Validation error: {0}")]
    Validation(#[from] models::ValidationError),
    #[error("OS Scheduler error: {0}")]
    Scheduler(#[from] os_scheduler::SchedulerError),
    #[error("Adapter error: {0}")]
    Adapter(#[from] adapter::AdapterError),
    #[error("Job not found: {0}")]
    JobNotFound(String),
}

pub struct SchedulerService {
    store: JobStore,
    cli_path: PathBuf,
    scheduler: std::sync::Arc<dyn os_scheduler::SchedulerBackend>,
}

impl SchedulerService {
    pub fn new(store: JobStore, cli_path: PathBuf) -> Self {
        Self {
            store,
            cli_path,
            scheduler: std::sync::Arc::from(get_platform_scheduler()),
        }
    }

    pub fn with_scheduler(
        store: JobStore,
        cli_path: PathBuf,
        scheduler: Box<dyn os_scheduler::SchedulerBackend>,
    ) -> Self {
        Self {
            store,
            cli_path,
            scheduler: std::sync::Arc::from(scheduler),
        }
    }

    pub fn default_service() -> Result<Self, CoreError> {
        let store = JobStore::default_store()?;
        let worker_path = worker::ensure_worker_installed()
            .unwrap_or_else(|_| worker::canonical_worker_path());
        Ok(Self::new(store, worker_path))
    }

    pub fn store(&self) -> &JobStore {
        &self.store
    }

    pub fn cli_path(&self) -> &Path {
        &self.cli_path
    }

    pub fn ensure_scheduler(&self) -> Result<(), CoreError> {
        self.scheduler.ensure_scheduler_installed(&self.cli_path)?;
        Ok(())
    }

    pub fn is_scheduler_installed(&self) -> bool {
        self.scheduler.is_scheduler_installed()
    }

    pub fn schedule_job(
        &self,
        provider: ProviderType,
        session_id: String,
        cwd: PathBuf,
        prompt: Option<String>,
        scheduled_at: chrono::DateTime<Utc>,
        retry_policy: Option<models::RetryPolicy>,
    ) -> Result<Job, CoreError> {
        let job = Job::new(provider, session_id, cwd, prompt, scheduled_at, retry_policy)?;

        // Ensure persistent OS scheduler service is installed (no-op if already present)
        let _ = self.ensure_scheduler();

        // Save to store
        self.store.insert_job(job.clone())?;
        Ok(job)
    }

    pub fn cancel_job(&self, job_id: &str) -> Result<Job, CoreError> {
        let mut job = self
            .store
            .get_job(job_id)?
            .ok_or_else(|| CoreError::JobNotFound(job_id.to_string()))?;

        job.set_status(JobStatus::Cancelled);
        self.store.update_job(&job)?;

        // 旧バージョンのレガシー個別plistが残っていた場合の後始末
        let _ = self.scheduler.unregister_job(job_id);

        Ok(job)
    }

    pub fn delete_job(&self, job_id: &str) -> Result<bool, CoreError> {
        let _ = self.scheduler.unregister_job(job_id);
        Ok(self.store.delete_job(job_id)?)
    }

    pub async fn execute_job(&self, job_id: &str) -> Result<Job, CoreError> {
        let mut job = self
            .store
            .get_job(job_id)?
            .ok_or_else(|| CoreError::JobNotFound(job_id.to_string()))?;

        job.set_status(JobStatus::Running);
        self.store.update_job(&job)?;

        let adapter = CodexAdapter::new();
        let attempt_number = (job.execution_history.len() + 1) as u32;
        let started_at = Utc::now();

        let result = adapter
            .execute_resume(&job.session_id, &job.cwd, &job.prompt)
            .await;

        let finished_at = Utc::now();

        let exec_result = match result {
            Ok(res) => res,
            Err(e) => ExecutionResult {
                exit_code: Some(1),
                stdout: String::new(),
                stderr: e.to_string(),
                is_quota_error: false,
                success: false,
                error_message: Some(e.to_string()),
            },
        };

        let attempt = ExecutionAttempt {
            attempt_number,
            started_at,
            finished_at,
            exit_code: exec_result.exit_code,
            stdout: exec_result.stdout.clone(),
            stderr: exec_result.stderr.clone(),
            is_quota_error: exec_result.is_quota_error,
            error_message: exec_result.error_message.clone(),
        };

        job.add_attempt(attempt);

        // Apply retry engine
        let action = RetryEngine::apply_evaluation(&mut job, &exec_result);

        match action {
            NextAction::CompleteSuccess => {}
            NextAction::CompleteFailure(_) => {}
            NextAction::RetryAfter(next_time) => {
                job.scheduled_at = next_time;
                // 次回tickで拾われるため、OSスケジューラの再登録・通知は発生させない
            }
        }

        self.store.update_job(&job)?;
        Ok(job)
    }
}
