pub mod adapter;
pub mod models;
pub mod os_scheduler;
pub mod retry;
pub mod runner;
pub mod store;
pub mod worker;

use adapter::ExecutionResult;
use adapter::codex::CodexAdapter;
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
    #[error("Runner error: {0}")]
    Runner(#[from] runner::RunnerError),
    #[error("Job not found: {0}")]
    JobNotFound(String),
    #[error("Cannot cancel running job: {0}")]
    CannotCancelRunningJob(String),
    #[error("Cannot delete job while execution is active: {0}")]
    CannotDeleteRunningJob(String),
    #[error("Another job for session '{0}' is already running")]
    SessionBusy(String),
    #[error("Invalid state transition: {0}")]
    InvalidStateTransition(String),
}

pub struct SchedulerService {
    store: JobStore,
    exe_path: Option<PathBuf>,
    is_desktop: bool,
    scheduler: std::sync::Arc<dyn os_scheduler::SchedulerBackend>,
}

impl SchedulerService {
    /// Creates a scheduler service for the Desktop GUI application with its executable path.
    pub fn new(store: JobStore, desktop_exe_path: PathBuf) -> Self {
        Self {
            store,
            exe_path: Some(desktop_exe_path),
            is_desktop: true,
            scheduler: std::sync::Arc::from(get_platform_scheduler()),
        }
    }

    /// Explicit constructor for Desktop GUI application.
    pub fn new_desktop(store: JobStore, desktop_exe_path: PathBuf) -> Self {
        Self::new(store, desktop_exe_path)
    }

    /// Creates a scheduler service for CLI tools.
    pub fn new_for_cli(store: JobStore) -> Self {
        let exe_path = std::env::current_exe().ok();
        Self {
            store,
            exe_path,
            is_desktop: false,
            scheduler: std::sync::Arc::from(get_platform_scheduler()),
        }
    }

    /// Creates a scheduler service for CLI tools with an explicit executable path.
    pub fn new_for_cli_with_path(store: JobStore, cli_exe_path: PathBuf) -> Self {
        Self {
            store,
            exe_path: Some(cli_exe_path),
            is_desktop: false,
            scheduler: std::sync::Arc::from(get_platform_scheduler()),
        }
    }

    pub fn with_scheduler(
        store: JobStore,
        exe_path: Option<PathBuf>,
        scheduler: Box<dyn os_scheduler::SchedulerBackend>,
    ) -> Self {
        let is_desktop = exe_path
            .as_deref()
            .map(|p| {
                #[cfg(target_os = "windows")]
                {
                    os_scheduler::windows::WindowsTaskScheduler::is_desktop_executable(p)
                }
                #[cfg(not(target_os = "windows"))]
                {
                    os_scheduler::macos::MacOsLaunchdScheduler::is_desktop_executable(p)
                }
            })
            .unwrap_or(false);
        Self {
            store,
            exe_path,
            is_desktop,
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
        if self.is_desktop {
            self.exe_path.as_deref()
        } else {
            None
        }
    }

    pub fn exe_path(&self) -> &Path {
        self.exe_path.as_deref().unwrap_or_else(|| Path::new(""))
    }

    pub fn cli_path(&self) -> &Path {
        self.exe_path()
    }

    pub fn get_scheduler_owner(&self) -> os_scheduler::SchedulerOwner {
        self.scheduler.get_scheduler_owner()
    }

    pub fn get_scheduler_executable_path(&self) -> Option<PathBuf> {
        self.scheduler.get_scheduler_executable_path()
    }

    pub fn scheduler(&self) -> &dyn os_scheduler::SchedulerBackend {
        &*self.scheduler
    }

    pub fn ensure_scheduler(&self) -> Result<(), CoreError> {
        if let Some(path) = &self.exe_path {
            self.scheduler.ensure_scheduler_installed(path)?;
            Ok(())
        } else if self.scheduler.supports_persistent_scheduler() {
            Err(CoreError::Scheduler(
                os_scheduler::SchedulerError::ExecutableNotFound(
                    "No executable path configured for scheduler installation".to_string(),
                ),
            ))
        } else {
            Ok(())
        }
    }

    pub fn is_scheduler_installed(&self) -> bool {
        self.scheduler.is_scheduler_installed()
    }

    pub fn is_scheduler_ready(&self) -> bool {
        self.scheduler.is_scheduler_ready()
    }

    pub fn is_scheduler_path_matched(&self) -> bool {
        match &self.exe_path {
            Some(path) => self.scheduler.is_scheduler_path_matched(path),
            None => false,
        }
    }

    pub fn is_target_executable_exists(&self) -> bool {
        self.scheduler.is_target_executable_exists()
    }

    pub fn is_owner_target_valid(&self) -> bool {
        self.scheduler.is_owner_target_valid()
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
        let job = Job::new(
            provider,
            session_id,
            cwd,
            prompt,
            scheduled_at,
            retry_policy,
        )?;

        // If invoked from Desktop GUI (is_desktop is true), ensure scheduler is registered with app path.
        // If invoked from CLI (is_desktop is false):
        // - On platforms with persistent scheduler (macOS LaunchAgent, Windows Task Scheduler):
        //   If scheduler is ready (Desktop-owned or CLI-owned), keep existing registered scheduler.
        //   If NOT ready, safely ensure / repair using current CLI executable.
        // - On unsupported platforms: proceed with JobStore insert.
        if self.is_desktop {
            if let Some(path) = &self.exe_path {
                self.scheduler.ensure_scheduler_installed(path)?;
            }
        } else if self.scheduler.supports_persistent_scheduler() {
            if !self.is_scheduler_ready() {
                if let Some(cli_path) = &self.exe_path {
                    self.scheduler.ensure_scheduler_installed(cli_path)?;
                } else {
                    return Err(CoreError::Scheduler(
                        os_scheduler::SchedulerError::ExecutableNotFound(
                            "CLI executable path could not be resolved".to_string(),
                        ),
                    ));
                }
            }

            // WHY: Atomic scheduling invariant (OS-SCHED-006, CLI-CMD-004, OS-SCHED-002).
            //      If the persistent OS scheduler cannot be verified as ready/loaded in macOS or Windows,
            //      we must NOT save the new job to JobStore, preventing orphan jobs that would never trigger.
            // WHAT BREAKS: Silent insertion when scheduler is unready causes users to believe a job is scheduled,
            //              but background tick will never execute it.
            // EVIDENCE: docs/spec-changes/0015-macos-scheduler-health-and-cli-status.md
            if !self.is_scheduler_ready() {
                return Err(CoreError::Scheduler(os_scheduler::SchedulerError::CommandFailed(
                    "Persistent OS scheduler service is not ready or active. Job scheduling aborted.".to_string(),
                )));
            }
        } else {
            if let Some(path) = &self.exe_path {
                let _ = self.scheduler.ensure_scheduler_installed(path);
            }
        }

        // Save to store only after scheduler readiness is verified
        self.store.insert_job(job.clone())?;
        Ok(job)
    }

    // RATIONALE: [SCHED-JOB-005] Guard against cancelling running jobs to prevent session writer collisions
    pub fn cancel_job(&self, job_id: &str) -> Result<Job, CoreError> {
        let mut job = self
            .store
            .get_job(job_id)?
            .ok_or_else(|| CoreError::JobNotFound(job_id.to_string()))?;

        if job.status == JobStatus::Running || runner::get_job_liveness(job_id) != runner::LivenessState::Dead {
            return Err(CoreError::CannotCancelRunningJob(job_id.to_string()));
        }

        if !matches!(job.status, JobStatus::Scheduled | JobStatus::Retrying) {
            return Err(CoreError::InvalidStateTransition(
                format!("Only scheduled or retrying jobs can be cancelled (current status: {:?})", job.status),
            ));
        }

        job.set_status(JobStatus::Cancelled);
        self.store.update_job(&job)?;

        // 旧バージョンのレガシー個別plistが残っていた場合の後始末
        let _ = self.scheduler.unregister_job(job_id);

        Ok(job)
    }

    // RATIONALE: [SCHED-JOB-005] Guard against deleting running jobs to prevent session writer collisions
    pub fn delete_job(&self, job_id: &str) -> Result<bool, CoreError> {
        let job = match self.store.get_job(job_id)? {
            Some(j) => j,
            None => return Ok(false),
        };

        if job.status == JobStatus::Running || runner::get_job_liveness(job_id) != runner::LivenessState::Dead {
            return Err(CoreError::CannotDeleteRunningJob(job_id.to_string()));
        }

        let _ = self.scheduler.unregister_job(job_id);
        let _ = runner::cleanup_job_logs(job_id);
        runner::cleanup_runner_files(job_id);
        Ok(self.store.delete_job(job_id)?)
    }

    // RATIONALE: [SCHED-JOB-007] Automatic orphan job recovery based on durable evidence
    // Scans jobs currently marked Running and checks durable lease, kernel lock, and child process liveness.
    // Fixed time-based timeout is completely abolished. Recovery requires durable proof of termination:
    // 1. System reboot detected (boot timestamp mismatch) -> all previous processes terminated -> Dead.
    // 2. Runner lock is released and recorded runner PID does not exist on OS -> runner terminated.
    // 3. Child process is definitively Dead -> recover job to Retrying / Failed.
    // If child is alive or liveness is Unknown, maintains Running to prevent concurrent duplicate writers.
    pub fn reconcile_running_jobs(&self) -> Result<Vec<Job>, CoreError> {
        Ok(self.store.with_lock(|| {
            let mut jobs = self.store.load_all()?;
            let mut recovered = Vec::new();
            let now = Utc::now();

            for job in jobs.iter_mut() {
                if job.status == JobStatus::Running && !runner::is_runner_active(&job.id) {
                    // Check durable handoff lease first to determine machine reboot evidence
                    let lease_read = runner::read_handoff_lease_checked(&job.id);
                    let mut reboot_detected = false;
                    let mut runner_confirmed_dead = false;

                    match lease_read {
                        runner::HandoffLeaseRead::Present(ref lease) => {
                            if runner::is_reboot_detected(lease) {
                                reboot_detected = true;
                                runner_confirmed_dead = true;
                            } else {
                                // If machine reboot cannot be proven, unconfirmed guards must fail closed
                                if job.unconfirmed_execution || lease.unconfirmed_child {
                                    continue;
                                }
                                match runner::check_lease_liveness(lease) {
                                    runner::LeaseLiveness::Active | runner::LeaseLiveness::Unknown => {
                                        // Process actively running or state Unknown: do NOT recover
                                        continue;
                                    }
                                    runner::LeaseLiveness::Dead => {
                                        runner_confirmed_dead = true;
                                    }
                                }
                            }
                        }
                        runner::HandoffLeaseRead::Unreadable(_) => {
                            // Corrupt lease: fail-closed, maintain Running.
                            continue;
                        }
                        runner::HandoffLeaseRead::Missing => {
                            // No lease found: cannot prove reboot, fail-closed if unconfirmed guard exists
                            if job.unconfirmed_execution {
                                continue;
                            }
                        }
                    }

                    // Check handoff grace period (15s): defer recovery while processes may be starting up
                    // (bypassed if machine reboot was proven, as pre-reboot processes cannot survive OS restart)
                    let elapsed = now.signed_duration_since(job.updated_at);
                    let in_grace_period = elapsed < chrono::Duration::seconds(15);

                    // Inspect recorded Codex child process (bypassed if machine reboot was proven)
                    if !reboot_detected {
                        let info_read = runner::read_runner_info_checked(&job.id);
                        match info_read {
                            runner::RunnerInfoRead::Present(ref info) => {
                                let liveness = runner::check_codex_process_liveness(info);
                                if liveness != runner::LivenessState::Dead {
                                    // Child is actively running or liveness observation failed (Unknown).
                                    // Fail-closed: maintain Running to prevent concurrent duplicate writers.
                                    continue;
                                }
                            }
                            runner::RunnerInfoRead::Unreadable(_) => {
                                // Metadata read failed or corrupt: Fail-closed, maintain Running.
                                continue;
                            }
                            runner::RunnerInfoRead::Missing => {
                                // In the absence of runner info, we are in the handoff/startup phase.
                                // Defer recovery during grace period while processes may still be initializing.
                                if in_grace_period {
                                    continue;
                                }
                                // If runner was not confirmed dead via durable evidence (lease),
                                // we cannot guess it is dead solely by elapsed time. Fail-closed.
                                if !runner_confirmed_dead {
                                    continue;
                                }
                            }
                        }
                    }

                    // Codex child process is definitively Dead and runner lock is released
                    if reboot_detected {
                        // Clear unconfirmed execution guard upon proven machine reboot
                        job.unconfirmed_execution = false;
                    }

                    runner::cleanup_runner_files(&job.id);

                    let attempt_number = (job.execution_history.len() + 1) as u32;
                    let attempt = ExecutionAttempt {
                        attempt_number,
                        started_at: job.updated_at,
                        finished_at: now,
                        exit_code: None,
                        stdout: String::new(),
                        stderr: "Runner process terminated unexpectedly (process crash or system restart)".to_string(),
                        is_quota_error: false,
                        error_message: Some("Runner process terminated unexpectedly (process crash or system restart)".to_string()),
                    };
                    job.add_attempt(attempt);

                    let fake_res = ExecutionResult {
                        exit_code: None,
                        stdout: String::new(),
                        stderr: "Runner process terminated unexpectedly".to_string(),
                        is_quota_error: false,
                        success: false,
                        error_message: Some("Runner process terminated unexpectedly".to_string()),
                    };
                    let action = RetryEngine::apply_evaluation(job, &fake_res);
                    match action {
                        NextAction::CompleteSuccess => {}
                        NextAction::CompleteFailure(_) => {}
                        NextAction::RetryAfter(next_time) => {
                            job.scheduled_at = next_time;
                        }
                    }
                    recovered.push(job.clone());
                }
            }

            if !recovered.is_empty() {
                self.store.save_all(&jobs)?;
            }

            Ok(recovered)
        })?)
    }

    // RATIONALE: [OS-SCHED-003] Decouple scheduler tick from long-running execution
    // Claims due jobs atomically, spawns detached runner processes, and completes in milliseconds
    // so persistent OS scheduler ticks (launchd / Task Scheduler) are never occupied or starved.
    pub async fn execute_tick(&self) -> Result<Vec<Job>, CoreError> {
        // Step 1: Reconcile any orphan Running jobs from previous ticks/crashes
        let _ = self.reconcile_running_jobs();

        // Step 2: Claim due jobs (Scheduled / Retrying) atomically
        let now = Utc::now();
        let due_jobs = self.store.claim_due_jobs(now)?;

        let mut spawned = Vec::new();
        if let Some(ref exe) = self.exe_path {
            if exe.is_file() {
                for job in due_jobs {
                    match runner::spawn_detached_runner(exe, &job.id, self.is_desktop) {
                        Ok(runner_pid) => {
                            let runner_start = runner::get_process_start_time(runner_pid).ok().flatten();
                            let lease = runner::read_handoff_lease(&job.id)
                                .unwrap_or_else(|| runner::HandoffLease::new_initial(&job.id, std::process::id(), now));
                            let updated_lease = lease.with_runner(runner_pid, runner_start.clone());
                            if let Err(e) = runner::write_handoff_lease(&updated_lease) {
                                eprintln!("[Tick] Failed to persist updated runner lease for job {}: {}", job.id, e);
                                // RATIONALE: [SCHED-JOB-007] Confirm termination of runner & child on updated lease failure
                                // Do not mark Failed or clean up files unless both runner and any spawned child are confirmed Dead.
                                if runner::terminate_and_confirm_runner_and_child(&job.id, runner_pid, runner_start.as_deref()) {
                                    runner::cleanup_runner_files(&job.id);
                                    let mut failed_job = job;
                                    failed_job.set_status(JobStatus::Failed);
                                    let _ = self.store.update_job(&failed_job);
                                } else {
                                    eprintln!("[Tick] Could not confirm termination of runner/child for job {}. Retaining Running status for safety.", job.id);
                                }
                                continue;
                            }
                            spawned.push(job);
                        }
                        Err(e) => {
                            eprintln!("[Tick] Failed to spawn runner for job {}: {}", job.id, e);
                            runner::cleanup_runner_files(&job.id);
                            let mut failed_job = job;
                            failed_job.set_status(JobStatus::Failed);
                            let _ = self.store.update_job(&failed_job);
                        }
                    }
                }
                return Ok(spawned);
            }
        }

        // Fallback for tests when no real executable is configured: execute in-process
        for job in due_jobs {
            match self.execute_claimed_job(job).await {
                Ok(finished) => spawned.push(finished),
                Err(e) => eprintln!("[Tick] Error executing job: {}", e),
            }
        }

        Ok(spawned)
    }

    /// In-process tick execution for deterministic integration tests.
    pub async fn execute_tick_in_process(&self) -> Result<Vec<Job>, CoreError> {
        let _ = self.reconcile_running_jobs();
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

    // RATIONALE: [SCHED-JOB-006] Independent runner execution with lock-based liveness and streaming logs
    // Executed by the detached runner process. Acquires exclusive OS lock, verifies previous child is not alive,
    // claims job atomically, executes Codex with streaming logs, updates store, and safely exits.
    pub async fn run_job_runner(&self, job_id: &str) -> Result<Job, CoreError> {
        let _lock = runner::RunnerLock::acquire(job_id)?;

        // RATIONALE: [SCHED-JOB-006] Prevent second writer if previous Codex child is still alive or metadata unreadable
        match runner::read_runner_info_checked(job_id) {
            runner::RunnerInfoRead::Present(info) => {
                if runner::check_codex_process_liveness(&info) != runner::LivenessState::Dead {
                    return Err(CoreError::SessionBusy(
                        format!("Previous Codex child process is still alive for job {}", job_id),
                    ));
                }
            }
            runner::RunnerInfoRead::Unreadable(e) => {
                return Err(CoreError::SessionBusy(
                    format!("Previous runner metadata unreadable/corrupt for job {}: {}", job_id, e),
                ));
            }
            runner::RunnerInfoRead::Missing => {}
        }

        let mut job = self
            .store
            .get_job(job_id)?
            .ok_or_else(|| CoreError::JobNotFound(job_id.to_string()))?;

        // Fail-closed guard: unconfirmed execution prohibits handoff and execution
        if job.unconfirmed_execution {
            return Err(CoreError::SessionBusy(format!(
                "Job {} has unconfirmed execution guard, handoff prohibited",
                job_id
            )));
        }

        // If not already claimed to Running (e.g. manual invocation), claim it now
        if job.status == JobStatus::Running {
            // RATIONALE: [SCHED-JOB-007] Handoff handshake invariant before child spawn
            // Detached runner spawned by tick must NOT spawn Codex child until parent tick
            // has persisted the updated HandoffLease with this runner's PID and OS start identity.
            let my_pid = std::process::id();
            let my_start_time = match runner::get_process_start_time(my_pid) {
                Ok(Some(s)) => s,
                _ => {
                    return Err(CoreError::InvalidStateTransition(format!(
                        "Handoff handshake failed: cannot determine current process OS start identity for PID {}",
                        my_pid
                    )));
                }
            };

            let mut handshake_confirmed = false;
            for _ in 0..60 {
                match runner::read_handoff_lease_checked(job_id) {
                    runner::HandoffLeaseRead::Present(lease) => {
                        // lease liveness Unknown / unconfirmed_child prohibits handoff
                        if lease.unconfirmed_child {
                            return Err(CoreError::SessionBusy(format!(
                                "Handoff rejected: lease has unconfirmed_child guard for job {}",
                                job_id
                            )));
                        }
                        match runner::check_lease_liveness(&lease) {
                            runner::LeaseLiveness::Unknown => {
                                return Err(CoreError::SessionBusy(format!(
                                    "Handoff rejected: lease liveness is Unknown for job {}",
                                    job_id
                                )));
                            }
                            _ => {}
                        }

                        // runner_pid + runner_start_time match (detached runner handoff)
                        let is_runner_match = lease.runner_pid == Some(my_pid)
                            && lease.runner_start_time.as_deref() == Some(&my_start_time);
                        // tick_pid + tick_start_time match (manual self-handoff / in-process execution)
                        let is_tick_match = lease.tick_pid == my_pid
                            && lease.tick_start_time.as_deref() == Some(&my_start_time);

                        if is_runner_match || is_tick_match {
                            handshake_confirmed = true;
                            break;
                        }

                        // PID matches but OS start identity mismatches -> PID reuse, reject immediately
                        if lease.runner_pid == Some(my_pid) && lease.runner_start_time.as_deref() != Some(&my_start_time) {
                            return Err(CoreError::InvalidStateTransition(format!(
                                "Handoff rejected: runner PID {} matches but OS start identity mismatches",
                                my_pid
                            )));
                        }
                        if lease.runner_pid.is_none() && lease.tick_pid == my_pid && lease.tick_start_time.as_deref() != Some(&my_start_time) {
                            return Err(CoreError::InvalidStateTransition(format!(
                                "Handoff rejected: tick PID {} matches but OS start identity mismatches",
                                my_pid
                            )));
                        }
                    }
                    runner::HandoffLeaseRead::Unreadable(e) => {
                        return Err(CoreError::SessionBusy(format!(
                            "Handoff rejected: corrupt lease for job {}: {}",
                            job_id, e
                        )));
                    }
                    runner::HandoffLeaseRead::Missing => {}
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            }

            if !handshake_confirmed {
                return Err(CoreError::InvalidStateTransition(format!(
                    "Handoff handshake timeout: updated lease with runner PID {} and start identity was not established by parent tick",
                    my_pid
                )));
            }
        } else {
            job = self.store.claim_job_for_execution(job_id)?;
        }

        self.execute_claimed_job(job).await
    }

    /// Single job manual or specific execution.
    /// Atomically claims the job to prevent duplicate concurrent execution,
    /// acquires runner lock, and executes synchronously.
    pub async fn execute_job(&self, job_id: &str) -> Result<Job, CoreError> {
        let _ = self.reconcile_running_jobs();
        let job = self.store.claim_job_for_execution(job_id)?;
        let _lock = runner::RunnerLock::acquire(job_id)?;
        self.execute_claimed_job(job).await
    }

    /// Shared core job execution logic after the job has been claimed into `Running` status.
    async fn execute_claimed_job(&self, mut job: Job) -> Result<Job, CoreError> {
        let adapter = CodexAdapter::new();
        let attempt_number = (job.execution_history.len() + 1) as u32;
        let started_at = Utc::now();
        let log_path = runner::runner_log_path(&job.id, attempt_number).ok();

        let result = adapter
            .execute_resume_streaming_with_job(
                &job.session_id,
                &job.cwd,
                &job.prompt,
                log_path.as_deref(),
                Some(&job.id),
            )
            .await;

        let finished_at = Utc::now();

        let exec_result = match result {
            Ok(res) => res,
            Err(e @ crate::adapter::AdapterError::UnconfirmedTermination(_)) => {
                // RATIONALE: [SCHED-JOB-006] Fail-closed: unconfirmed termination must NEVER convert to normal Failed
                // Durably guard as unconfirmed in JobStore and lease so second writer is never admitted.
                if let Some(mut lease) = runner::read_handoff_lease(&job.id) {
                    lease.unconfirmed_child = true;
                    let _ = runner::write_handoff_lease(&lease);
                }
                let mut guard_job = job;
                guard_job.unconfirmed_execution = true;
                if let Err(se) = self.store.update_job(&guard_job) {
                    eprintln!("[Runner] Could not persist unconfirmed execution guard in store: {}. Holding lock.", se);
                    // Invariant: Do NOT exit runner without durable guard
                    loop {
                        tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
                    }
                }
                return Err(CoreError::Adapter(e));
            }
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
        // Terminal or retrying state persisted: clean up runner metadata and lease files
        runner::cleanup_runner_files(&job.id);
        Ok(job)
    }
}
