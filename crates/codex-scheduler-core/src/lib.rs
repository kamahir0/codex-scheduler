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
    desktop_exe_path: Option<PathBuf>,
    scheduler: std::sync::Arc<dyn os_scheduler::SchedulerBackend>,
}

impl SchedulerService {
    /// Creates a scheduler service for the Desktop GUI application with its executable path.
    pub fn new(store: JobStore, desktop_exe_path: PathBuf) -> Self {
        Self {
            store,
            desktop_exe_path: Some(desktop_exe_path),
            scheduler: std::sync::Arc::from(get_platform_scheduler()),
        }
    }

    /// Creates a scheduler service for CLI tools where the running CLI binary must NOT be registered as the LaunchAgent.
    pub fn new_for_cli(store: JobStore) -> Self {
        Self {
            store,
            desktop_exe_path: None,
            scheduler: std::sync::Arc::from(get_platform_scheduler()),
        }
    }

    pub fn with_scheduler(
        store: JobStore,
        desktop_exe_path: Option<PathBuf>,
        scheduler: Box<dyn os_scheduler::SchedulerBackend>,
    ) -> Self {
        Self {
            store,
            desktop_exe_path,
            scheduler: std::sync::Arc::from(scheduler),
        }
    }

    pub fn default_service() -> Result<Self, CoreError> {
        let store = JobStore::default_store()?;
        Ok(Self::new_for_cli(store))
    }

    pub fn store(&self) -> &JobStore {
        &self.store
    }

    pub fn desktop_exe_path(&self) -> Option<&Path> {
        self.desktop_exe_path.as_deref()
    }

    pub fn exe_path(&self) -> &Path {
        self.desktop_exe_path
            .as_deref()
            .unwrap_or_else(|| Path::new(""))
    }

    pub fn cli_path(&self) -> &Path {
        self.exe_path()
    }

    pub fn ensure_scheduler(&self) -> Result<(), CoreError> {
        match &self.desktop_exe_path {
            Some(path) => {
                self.scheduler.ensure_scheduler_installed(path)?;
                Ok(())
            }
            None => {
                #[cfg(target_os = "macos")]
                {
                    Err(CoreError::Scheduler(os_scheduler::SchedulerError::DesktopAppRequired))
                }
                #[cfg(not(target_os = "macos"))]
                {
                    Ok(())
                }
            }
        }
    }

    pub fn is_scheduler_installed(&self) -> bool {
        self.scheduler.is_scheduler_installed()
    }

    pub fn is_scheduler_ready(&self) -> bool {
        self.scheduler.is_scheduler_ready()
    }

    pub fn is_scheduler_path_matched(&self) -> bool {
        match &self.desktop_exe_path {
            Some(path) => self.scheduler.is_scheduler_path_matched(path),
            None => false,
        }
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

        // If invoked from Desktop GUI (desktop_exe_path is Some), ensure scheduler is registered with app path.
        // If invoked from CLI (desktop_exe_path is None):
        // - on macOS: verify scheduler is READY (properly configured for headless execution and loaded);
        //   do NOT accept legacy/unmigrated plist!
        // - on non-macOS: proceed.
        if let Some(path) = &self.desktop_exe_path {
            self.scheduler.ensure_scheduler_installed(path)?;
        } else {
            #[cfg(target_os = "macos")]
            {
                if !self.is_scheduler_ready() {
                    return Err(CoreError::Scheduler(os_scheduler::SchedulerError::DesktopAppRequired));
                }
            }
        }

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

    /// Periodic tick execution: atomically claims due jobs and executes them.
    /// Shared between headless GUI `--scheduler-tick` and CLI `tick`.
    pub async fn execute_tick(&self) -> Result<Vec<Job>, CoreError> {
        let now = Utc::now();
        let due_jobs = self.store.claim_due_jobs(now)?;
        let mut results = Vec::new();

        for job in due_jobs {
            match self.execute_claimed_job(job).await {
                Ok(finished) => results.push(finished),
                Err(e) => eprintln!("[Tick] Error executing job: {}", e),
            }
        }

        Ok(results)
    }

    /// Single job manual or specific execution.
    /// Atomically claims the job to prevent duplicate concurrent execution.
    pub async fn execute_job(&self, job_id: &str) -> Result<Job, CoreError> {
        let job = self.store.claim_job_for_execution(job_id)?;
        self.execute_claimed_job(job).await
    }

    /// Shared core job execution logic after the job has been claimed into `Running` status.
    async fn execute_claimed_job(&self, mut job: Job) -> Result<Job, CoreError> {
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
