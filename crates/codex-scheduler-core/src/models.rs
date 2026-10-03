use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("Session ID must not be empty")]
    EmptySessionId,
    #[error("Working directory '{0}' does not exist or is not a directory")]
    InvalidWorkingDirectory(PathBuf),
    #[error("Scheduled time '{0}' is in the past")]
    ScheduledTimeInPast(DateTime<Utc>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderType {
    Codex,
    Claude,
    Custom(String),
}

impl Default for ProviderType {
    fn default() -> Self {
        Self::Codex
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Scheduled,
    Running,
    Retrying,
    Succeeded,
    Failed,
    Cancelled,
}

impl JobStatus {
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Scheduled | Self::Running | Self::Retrying)
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RetryPolicy {
    pub enabled: bool,
    pub interval_seconds: u64,
    pub max_attempts: u32,
    pub retry_on_quota_only: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_seconds: 300, // 5 minutes
            max_attempts: 6,       // up to 30 mins
            retry_on_quota_only: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionAttempt {
    pub attempt_number: u32,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub is_quota_error: bool,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Job {
    pub id: String,
    pub provider: ProviderType,
    pub session_id: String,
    pub cwd: PathBuf,
    pub prompt: String,
    pub scheduled_at: DateTime<Utc>,
    pub status: JobStatus,
    pub retry_policy: RetryPolicy,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub execution_history: Vec<ExecutionAttempt>,
}

impl Job {
    pub fn new(
        provider: ProviderType,
        session_id: String,
        cwd: PathBuf,
        prompt: Option<String>,
        scheduled_at: DateTime<Utc>,
        retry_policy: Option<RetryPolicy>,
    ) -> Result<Self, ValidationError> {
        let trimmed_session = session_id.trim();
        if trimmed_session.is_empty() {
            return Err(ValidationError::EmptySessionId);
        }

        // Validate cwd exists if path is provided
        if !cwd.exists() || !cwd.is_dir() {
            return Err(ValidationError::InvalidWorkingDirectory(cwd));
        }

        let now = Utc::now();
        let prompt_text = prompt
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .unwrap_or_else(|| "continue".to_string());

        Ok(Self {
            id: Uuid::new_v4().to_string(),
            provider,
            session_id: trimmed_session.to_string(),
            cwd,
            prompt: prompt_text,
            scheduled_at,
            status: JobStatus::Scheduled,
            retry_policy: retry_policy.unwrap_or_default(),
            created_at: now,
            updated_at: now,
            execution_history: Vec::new(),
        })
    }

    pub fn add_attempt(&mut self, attempt: ExecutionAttempt) {
        self.updated_at = Utc::now();
        self.execution_history.push(attempt);
    }

    pub fn set_status(&mut self, status: JobStatus) {
        self.updated_at = Utc::now();
        self.status = status;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_validation_empty_session() {
        let res = Job::new(
            ProviderType::Codex,
            "".to_string(),
            PathBuf::from("/"),
            None,
            Utc::now(),
            None,
        );
        assert!(matches!(res, Err(ValidationError::EmptySessionId)));
    }

    #[test]
    fn test_job_validation_invalid_dir() {
        let res = Job::new(
            ProviderType::Codex,
            "sess-123".to_string(),
            PathBuf::from("/nonexistent/directory/12345"),
            None,
            Utc::now(),
            None,
        );
        assert!(matches!(
            res,
            Err(ValidationError::InvalidWorkingDirectory(_))
        ));
    }

    #[test]
    fn test_job_success_creation() {
        let res = Job::new(
            ProviderType::Codex,
            "sess-abc".to_string(),
            std::env::temp_dir(),
            None,
            Utc::now(),
            None,
        );
        assert!(res.is_ok());
        let job = res.unwrap();
        assert_eq!(job.prompt, "continue");
        assert_eq!(job.status, JobStatus::Scheduled);
        assert_eq!(job.retry_policy.max_attempts, 6);
    }
}
