use crate::adapter::ExecutionResult;
use crate::models::{Job, JobStatus};
use chrono::{DateTime, Duration, Utc};

#[derive(Debug, PartialEq, Eq)]
pub enum NextAction {
    CompleteSuccess,
    RetryAfter(DateTime<Utc>),
    CompleteFailure(String),
}

pub struct RetryEngine;

impl RetryEngine {
    pub fn evaluate(job: &Job, result: &ExecutionResult) -> NextAction {
        if result.success {
            return NextAction::CompleteSuccess;
        }

        let current_attempts = job.execution_history.len() as u32; // includes the one just completed if already added, or about to be added
        let policy = &job.retry_policy;

        if !policy.enabled {
            return NextAction::CompleteFailure(
                result
                    .error_message
                    .clone()
                    .unwrap_or_else(|| "Execution failed and retry is disabled".to_string()),
            );
        }

        // Check if only retrying on quota errors
        if policy.retry_on_quota_only && !result.is_quota_error {
            return NextAction::CompleteFailure(
                result
                    .error_message
                    .clone()
                    .unwrap_or_else(|| "Execution failed due to non-quota error".to_string()),
            );
        }

        // Check maximum attempts
        if current_attempts >= policy.max_attempts {
            return NextAction::CompleteFailure(format!(
                "Max attempts ({}) reached without success. Last error: {}",
                policy.max_attempts,
                result.error_message.as_deref().unwrap_or("Unknown")
            ));
        }

        // Compute next retry time
        let delay_seconds = policy.interval_seconds as i64;
        let next_time = Utc::now() + Duration::seconds(delay_seconds);

        NextAction::RetryAfter(next_time)
    }

    pub fn apply_evaluation(job: &mut Job, result: &ExecutionResult) -> NextAction {
        let action = Self::evaluate(job, result);
        match &action {
            NextAction::CompleteSuccess => {
                job.set_status(JobStatus::Succeeded);
            }
            NextAction::RetryAfter(_) => {
                job.set_status(JobStatus::Retrying);
            }
            NextAction::CompleteFailure(_) => {
                job.set_status(JobStatus::Failed);
            }
        }
        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ExecutionAttempt, ProviderType, RetryPolicy};

    fn dummy_job() -> Job {
        Job::new(
            ProviderType::Codex,
            "session-test".to_string(),
            std::env::temp_dir(),
            None,
            Utc::now(),
            Some(RetryPolicy {
                enabled: true,
                interval_seconds: 60,
                max_attempts: 3,
                retry_on_quota_only: true,
            }),
        )
        .unwrap()
    }

    #[test]
    fn test_retry_on_quota_error() {
        let mut job = dummy_job();
        let quota_result = ExecutionResult {
            exit_code: Some(1),
            stdout: "".to_string(),
            stderr: "Rate limit exceeded".to_string(),
            is_quota_error: true,
            success: false,
            error_message: Some("Rate limit".to_string()),
        };

        // 1st attempt: should retry
        let action = RetryEngine::apply_evaluation(&mut job, &quota_result);
        assert!(matches!(action, NextAction::RetryAfter(_)));
        assert_eq!(job.status, JobStatus::Retrying);

        // Record attempts to simulate reaching max attempts
        job.add_attempt(ExecutionAttempt {
            attempt_number: 1,
            started_at: Utc::now(),
            finished_at: Utc::now(),
            exit_code: Some(1),
            stdout: "".to_string(),
            stderr: "".to_string(),
            is_quota_error: true,
            error_message: None,
        });
        job.add_attempt(ExecutionAttempt {
            attempt_number: 2,
            started_at: Utc::now(),
            finished_at: Utc::now(),
            exit_code: Some(1),
            stdout: "".to_string(),
            stderr: "".to_string(),
            is_quota_error: true,
            error_message: None,
        });
        job.add_attempt(ExecutionAttempt {
            attempt_number: 3,
            started_at: Utc::now(),
            finished_at: Utc::now(),
            exit_code: Some(1),
            stdout: "".to_string(),
            stderr: "".to_string(),
            is_quota_error: true,
            error_message: None,
        });

        // 4th attempt after max: should fail
        let action2 = RetryEngine::apply_evaluation(&mut job, &quota_result);
        assert!(matches!(action2, NextAction::CompleteFailure(_)));
        assert_eq!(job.status, JobStatus::Failed);
    }

    #[test]
    fn test_no_retry_on_non_quota_error() {
        let mut job = dummy_job();
        let non_quota_result = ExecutionResult {
            exit_code: Some(127),
            stdout: "".to_string(),
            stderr: "Command not found".to_string(),
            is_quota_error: false,
            success: false,
            error_message: Some("Command not found".to_string()),
        };

        let action = RetryEngine::apply_evaluation(&mut job, &non_quota_result);
        assert!(matches!(action, NextAction::CompleteFailure(_)));
        assert_eq!(job.status, JobStatus::Failed);
    }
}
