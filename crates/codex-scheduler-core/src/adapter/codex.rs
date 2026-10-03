use super::{AdapterError, ExecutionResult, ProviderAdapter};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;

pub struct CodexAdapter {
    executable_override: Option<PathBuf>,
}

impl CodexAdapter {
    pub fn new() -> Self {
        Self {
            executable_override: None,
        }
    }

    pub fn with_executable(path: PathBuf) -> Self {
        Self {
            executable_override: Some(path),
        }
    }

    pub fn resolve_executable(&self) -> Result<PathBuf, AdapterError> {
        if let Some(ref path) = self.executable_override {
            return Ok(path.clone());
        }

        // Standard PATH check first
        if let Ok(path) = which_executable("codex") {
            return Ok(path);
        }

        // Check common locations on macOS / Linux
        let candidates = [
            "/opt/homebrew/bin/codex",
            "/usr/local/bin/codex",
            "/usr/bin/codex",
        ];

        for c in &candidates {
            let p = PathBuf::from(c);
            if p.exists() {
                return Ok(p);
            }
        }

        // Home directory paths
        if let Some(home) = dirs::home_dir() {
            let user_candidates = [
                home.join(".local/bin/codex"),
                home.join(".cargo/bin/codex"),
                home.join(".nvm/versions/node/current/bin/codex"),
            ];
            for p in &user_candidates {
                if p.exists() {
                    return Ok(p.clone());
                }
            }
        }

        // Fallback to "codex" string to let shell try
        Ok(PathBuf::from("codex"))
    }

    pub async fn execute_resume(
        &self,
        session_id: &str,
        cwd: &Path,
        prompt: &str,
    ) -> Result<ExecutionResult, AdapterError> {
        let exe = self.resolve_executable()?;

        let mut cmd = Command::new(&exe);
        cmd.arg("exec")
            .arg("resume")
            .arg(session_id)
            .arg(prompt)
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Augment PATH with common directories in case launchd stripped it
        let current_path = std::env::var("PATH").unwrap_or_default();
        let home_bin = dirs::home_dir()
            .map(|h| format!("{}:{}", h.join(".local/bin").display(), h.join(".cargo/bin").display()))
            .unwrap_or_default();
        let new_path = format!("{}:/opt/homebrew/bin:/opt/homebrew/sbin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin:{}", current_path, home_bin);
        cmd.env("PATH", new_path);

        let output = cmd.output().await.map_err(AdapterError::ProcessError)?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let combined = format!("{}\n{}", stdout, stderr);

        let is_quota = self.is_quota_error(&combined);
        let success = output.status.success() && !is_quota;
        let exit_code = output.status.code();

        let error_message = if !success {
            if is_quota {
                Some("Rate limit or usage quota exceeded".to_string())
            } else if !stderr.trim().is_empty() {
                Some(stderr.lines().last().unwrap_or("Execution failed").to_string())
            } else {
                Some(format!("Command exited with status {:?}", exit_code))
            }
        } else {
            None
        };

        Ok(ExecutionResult {
            exit_code,
            stdout,
            stderr,
            is_quota_error: is_quota,
            success,
            error_message,
        })
    }
}

impl Default for CodexAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderAdapter for CodexAdapter {
    fn provider_name(&self) -> &str {
        "codex"
    }

    fn is_quota_error(&self, output: &str) -> bool {
        let lower = output.to_lowercase();
        lower.contains("usage limit")
            || lower.contains("rate limit")
            || lower.contains("rate_limit_exceeded")
            || lower.contains("quota exceeded")
            || lower.contains("insufficient_quota")
            || lower.contains("too many requests")
            || lower.contains("status 429")
            || lower.contains("http 429")
            || lower.contains("429 too many")
            || lower.contains("hit your usage limit")
            || lower.contains("credit balance is too low")
            || lower.contains("monthly limit")
    }
}

fn which_executable(name: &str) -> Result<PathBuf, ()> {
    if let Ok(path) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path) {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    Err(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quota_error_detection() {
        let adapter = CodexAdapter::new();

        assert!(adapter.is_quota_error("Error: You have hit your usage limit for this period."));
        assert!(adapter.is_quota_error("HTTP 429: Rate limit exceeded. Try again at 02:00."));
        assert!(adapter.is_quota_error("insufficient_quota: Credit balance is too low."));
        assert!(adapter.is_quota_error("Status 429 Too Many Requests"));

        // Non-quota errors
        assert!(!adapter.is_quota_error("Session not found with ID 019abc"));
        assert!(!adapter.is_quota_error("SyntaxError: Unexpected token"));
        assert!(!adapter.is_quota_error("Success: Task completed"));
    }
}
