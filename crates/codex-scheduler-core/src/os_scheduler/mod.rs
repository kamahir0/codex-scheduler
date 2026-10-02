pub mod macos;

use crate::models::Job;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SchedulerError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("OS schedule registration failed: {0}")]
    CommandFailed(String),
    #[error("Home directory not found")]
    HomeNotFound,
}

pub trait SchedulerBackend: Send + Sync {
    fn register_job(&self, job: &Job, cli_path: &Path) -> Result<(), SchedulerError>;
    fn unregister_job(&self, job_id: &str) -> Result<(), SchedulerError>;
}

pub fn get_platform_scheduler() -> Box<dyn SchedulerBackend> {
    #[cfg(target_os = "macos")]
    {
        Box::new(macos::MacOsLaunchdScheduler::new())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Box::new(FallbackScheduler)
    }
}

pub struct FallbackScheduler;

impl SchedulerBackend for FallbackScheduler {
    fn register_job(&self, _job: &Job, _cli_path: &Path) -> Result<(), SchedulerError> {
        // Fallback for non-macOS platforms or testing
        Ok(())
    }

    fn unregister_job(&self, _job_id: &str) -> Result<(), SchedulerError> {
        Ok(())
    }
}
