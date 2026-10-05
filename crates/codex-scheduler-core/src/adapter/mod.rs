pub mod codex;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AdapterError {
    #[error("Executable not found: {0}")]
    ExecutableNotFound(String),
    #[error("Failed to execute process: {0}")]
    ProcessError(#[from] std::io::Error),
    #[error("Execution timed out after {0} seconds")]
    Timeout(u64),
    #[error("Unconfirmed child termination: {0}")]
    UnconfirmedTermination(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionResult {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub is_quota_error: bool,
    pub success: bool,
    pub error_message: Option<String>,
}

pub trait ProviderAdapter: Send + Sync {
    fn provider_name(&self) -> &str;
    fn is_quota_error(&self, output: &str) -> bool;
}
