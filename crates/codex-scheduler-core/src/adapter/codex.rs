use super::{AdapterError, ExecutionResult, ProviderAdapter};
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::AsyncReadExt;
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

        #[cfg(windows)]
        {
            let known = known_windows_launcher_dirs();
            resolve_windows_launcher(std::env::var_os("PATH").as_deref(), &known)
                .ok_or_else(|| AdapterError::ExecutableNotFound("codex".to_string()))
        }

        #[cfg(not(windows))]
        {
            let known = known_unix_launcher_dirs();
            resolve_unix_launcher(std::env::var_os("PATH").as_deref(), &known)
                .ok_or_else(|| AdapterError::ExecutableNotFound("codex".to_string()))
        }
    }

    pub async fn execute_resume(
        &self,
        session_id: &str,
        cwd: &Path,
        prompt: &str,
    ) -> Result<ExecutionResult, AdapterError> {
        self.execute_resume_streaming(session_id, cwd, prompt, None)
            .await
    }

    // RATIONALE: [CODEX-RESUME-009] Streaming output, safe log persistence, and bounded in-memory storage
    // Prevents unbounded memory allocation and jobs.json bloat for hours-long execution runs while
    // streaming full output to attempt log files on disk for read-only inspection.
    pub async fn execute_resume_streaming(
        &self,
        session_id: &str,
        cwd: &Path,
        prompt: &str,
        log_path: Option<&Path>,
    ) -> Result<ExecutionResult, AdapterError> {
        self.execute_resume_streaming_with_job(session_id, cwd, prompt, log_path, None)
            .await
    }

    pub async fn execute_resume_streaming_with_job(
        &self,
        session_id: &str,
        cwd: &Path,
        prompt: &str,
        log_path: Option<&Path>,
        job_id: Option<&str>,
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

        if let Some(new_path) = augment_path_for_child() {
            cmd.env("PATH", new_path);
        }

        let mut child = cmd.spawn().map_err(AdapterError::ProcessError)?;

        // RATIONALE: [SCHED-JOB-006] Record child PID and OS start-time immediately after spawn
        // Persists RunnerInfo so orphan recovery can distinguish surviving Codex child from total termination.
        if let (Some(jid), Some(c_pid)) = (job_id, child.id()) {
            let start_time = crate::runner::get_process_start_time(c_pid).ok().flatten();
            let r_pid = std::process::id();
            let r_start = crate::runner::get_process_start_time(r_pid).ok().flatten();
            let info = crate::runner::RunnerInfo {
                job_id: jid.to_string(),
                session_id: session_id.to_string(),
                runner_pid: r_pid,
                runner_start_time: r_start,
                runner_started_at: chrono::Utc::now(),
                codex_pid: Some(c_pid),
                codex_start_time: start_time,
            };
            if let Err(e) = crate::runner::write_runner_info(&info) {
                // RATIONALE: [SCHED-JOB-006] Confirm child termination on metadata persistence failure
                // If writing runner info fails, kill child and await/confirm its termination.
                // If termination cannot be confirmed, persist failure evidence to guard as Unknown.
                let _ = child.start_kill();
                let wait_res = tokio::time::timeout(std::time::Duration::from_secs(5), child.wait()).await;
                match wait_res {
                    Ok(Ok(_)) => {
                        return Err(AdapterError::ProcessError(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            format!("Failed to persist runner metadata for job {}: {}. Child terminated successfully.", jid, e),
                        )));
                    }
                    _ => {
                        let marker_res = crate::runner::write_corrupt_runner_info_marker(jid, c_pid);
                        if let Err(ref me) = marker_res {
                            eprintln!("[Runner] Failed to persist corrupt marker for job {}: {}", jid, me);
                        }
                        return Err(AdapterError::UnconfirmedTermination(format!(
                            "Failed to persist runner metadata and could not confirm child termination for job {}: {}. Guarded as Unknown (marker written: {}).",
                            jid, e, marker_res.is_ok()
                        )));
                    }
                }
            }
        }

        // RATIONALE: [CODEX-RESUME-009] Best-effort log file initialization factoring existing file length
        // If the log directory or file cannot be opened, execution proceeds with bounded in-memory log.
        let log_writer = if let Some(path) = log_path {
            if let Some(parent) = path.parent() {
                let _ = tokio::fs::create_dir_all(parent).await;
            }
            match tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .await
            {
                Ok(file) => match file.metadata().await {
                    Ok(meta) => {
                        let existing_len = meta.len();
                        Some(std::sync::Arc::new(tokio::sync::Mutex::new(
                            crate::runner::CappedLogWriter::new(
                                file,
                                crate::runner::MAX_LOG_FILE_BYTES,
                                existing_len,
                            ),
                        )))
                    }
                    Err(e) => {
                        // RATIONALE: [CODEX-RESUME-009] Strict log cap on metadata failure
                        // If file metadata length cannot be determined, do NOT assume 0.
                        // Halt disk append for this attempt and fall back to bounded in-memory log
                        // to guarantee file size NEVER exceeds MAX_LOG_FILE_BYTES.
                        eprintln!(
                            "[Runner] Warning: failed to read log file metadata for {}: {}. Falling back to memory log.",
                            path.display(),
                            e
                        );
                        None
                    }
                },
                Err(_) => None,
            }
        } else {
            None
        };

        let mut stdout_pipe = child.stdout.take().expect("stdout piped");
        let mut stderr_pipe = child.stderr.take().expect("stderr piped");

        let quota_detected = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        let log_writer_stdout = log_writer.clone();
        let quota_stdout = quota_detected.clone();
        let stdout_handle = tokio::spawn(async move {
            let mut bounded = Vec::new();
            let mut buf = [0u8; 4096];
            let mut truncated = false;
            let mut sliding_window = Vec::new();
            while let Ok(n) = stdout_pipe.read(&mut buf).await {
                if n == 0 {
                    break;
                }
                let chunk = &buf[..n];

                // Incremental quota detection across chunk boundary
                sliding_window.extend_from_slice(chunk);
                let window_str = String::from_utf8_lossy(&sliding_window).to_lowercase();
                if window_str.contains("usage limit")
                    || window_str.contains("rate limit")
                    || window_str.contains("rate_limit_exceeded")
                    || window_str.contains("quota exceeded")
                    || window_str.contains("insufficient_quota")
                    || window_str.contains("too many requests")
                    || window_str.contains("status 429")
                    || window_str.contains("http 429")
                    || window_str.contains("429 too many")
                    || window_str.contains("hit your usage limit")
                    || window_str.contains("credit balance is too low")
                    || window_str.contains("monthly limit")
                {
                    quota_stdout.store(true, std::sync::atomic::Ordering::Relaxed);
                }
                if sliding_window.len() > 512 {
                    let drain_len = sliding_window.len() - 256;
                    sliding_window.drain(..drain_len);
                }

                // Strict capped disk log persistence under file lock
                if let Some(ref writer_mutex) = log_writer_stdout {
                    let mut writer = writer_mutex.lock().await;
                    let _ = writer.write_chunk(chunk).await;
                }

                bounded.extend_from_slice(chunk);
                if bounded.len() > crate::runner::MAX_BOUNDED_LOG_BYTES * 2 {
                    let drain_len = bounded.len() - crate::runner::MAX_BOUNDED_LOG_BYTES;
                    bounded.drain(..drain_len);
                    truncated = true;
                }
            }
            (bounded, truncated)
        });

        let log_writer_stderr = log_writer.clone();
        let quota_stderr = quota_detected.clone();
        let stderr_handle = tokio::spawn(async move {
            let mut bounded = Vec::new();
            let mut buf = [0u8; 4096];
            let mut truncated = false;
            let mut sliding_window = Vec::new();
            while let Ok(n) = stderr_pipe.read(&mut buf).await {
                if n == 0 {
                    break;
                }
                let chunk = &buf[..n];

                // Incremental quota detection across chunk boundary
                sliding_window.extend_from_slice(chunk);
                let window_str = String::from_utf8_lossy(&sliding_window).to_lowercase();
                if window_str.contains("usage limit")
                    || window_str.contains("rate limit")
                    || window_str.contains("rate_limit_exceeded")
                    || window_str.contains("quota exceeded")
                    || window_str.contains("insufficient_quota")
                    || window_str.contains("too many requests")
                    || window_str.contains("status 429")
                    || window_str.contains("http 429")
                    || window_str.contains("429 too many")
                    || window_str.contains("hit your usage limit")
                    || window_str.contains("credit balance is too low")
                    || window_str.contains("monthly limit")
                {
                    quota_stderr.store(true, std::sync::atomic::Ordering::Relaxed);
                }
                if sliding_window.len() > 512 {
                    let drain_len = sliding_window.len() - 256;
                    sliding_window.drain(..drain_len);
                }

                // Strict capped disk log persistence under file lock
                if let Some(ref writer_mutex) = log_writer_stderr {
                    let mut writer = writer_mutex.lock().await;
                    let _ = writer.write_chunk(chunk).await;
                }

                bounded.extend_from_slice(chunk);
                if bounded.len() > crate::runner::MAX_BOUNDED_LOG_BYTES * 2 {
                    let drain_len = bounded.len() - crate::runner::MAX_BOUNDED_LOG_BYTES;
                    bounded.drain(..drain_len);
                    truncated = true;
                }
            }
            (bounded, truncated)
        });

        let (stdout_res, stderr_res) = tokio::join!(stdout_handle, stderr_handle);
        let (stdout_raw, stdout_truncated) = stdout_res.unwrap_or_default();
        let (stderr_raw, stderr_truncated) = stderr_res.unwrap_or_default();

        let output_status = child.wait().await.map_err(AdapterError::ProcessError)?;

        let stdout_str = String::from_utf8_lossy(&stdout_raw);
        let stderr_str = String::from_utf8_lossy(&stderr_raw);

        let stdout = crate::runner::bounded_log_tail_with_flag(
            &stdout_str,
            crate::runner::MAX_BOUNDED_LOG_BYTES,
            stdout_truncated,
        );
        let stderr = crate::runner::bounded_log_tail_with_flag(
            &stderr_str,
            crate::runner::MAX_BOUNDED_LOG_BYTES,
            stderr_truncated,
        );
        let combined = format!("{}\n{}", stdout, stderr);

        // RATIONALE: [CODEX-RESUME-009] Incremental streaming quota error detection
        // Retains quota detection even if the initial signature was pushed out of the 10KB bounded tail.
        let is_quota = quota_detected.load(std::sync::atomic::Ordering::Relaxed)
            || self.is_quota_error(&combined);
        let success = output_status.success() && !is_quota;
        let exit_code = output_status.code();

        let error_message = if !success {
            if is_quota {
                Some("Rate limit or usage quota exceeded".to_string())
            } else if !stderr.trim().is_empty() {
                Some(
                    stderr
                        .lines()
                        .last()
                        .unwrap_or("Execution failed")
                        .to_string(),
                )
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

// RATIONALE: [CODEX-RESUME-004] Windows launcher resolution and extension order
//
// 1. `.exe`: Native PE binary takes highest precedence.
// 2. `.cmd`: Official npm global shim on Windows (`cmd-shim`).
// 3. `.bat`: Generic Windows batch script launcher.
// 4. `.com`: Legacy MS-DOS/Win32 executable format.
//
// Extensionless POSIX shims (`codex`) MUST be excluded because Windows `CreateProcessW`
// fails with `ERROR_BAD_EXE_FORMAT` (os error 193) when attempting to execute text scripts directly.
pub const WINDOWS_LAUNCHER_EXTENSIONS: &[&str] = &[".exe", ".cmd", ".bat", ".com"];

/// Resolves the Codex CLI executable path on Windows.
pub fn resolve_windows_launcher(
    path_var: Option<&OsStr>,
    known_dirs: &[PathBuf],
) -> Option<PathBuf> {
    let mut search_dirs: Vec<PathBuf> = Vec::new();

    if let Some(path) = path_var {
        for dir in std::env::split_paths(path) {
            if !search_dirs.contains(&dir) {
                search_dirs.push(dir);
            }
        }
    }

    for dir in known_dirs {
        if !search_dirs.contains(dir) {
            search_dirs.push(dir.clone());
        }
    }

    for dir in search_dirs {
        for ext in WINDOWS_LAUNCHER_EXTENSIONS {
            let candidate = dir.join(format!("codex{}", ext));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    None
}

/// Resolves the Codex CLI executable path on Unix (macOS / Linux).
pub fn resolve_unix_launcher(path_var: Option<&OsStr>, known_dirs: &[PathBuf]) -> Option<PathBuf> {
    let mut search_dirs: Vec<PathBuf> = Vec::new();

    if let Some(path) = path_var {
        for dir in std::env::split_paths(path) {
            if !search_dirs.contains(&dir) {
                search_dirs.push(dir);
            }
        }
    }

    for dir in known_dirs {
        if !search_dirs.contains(dir) {
            search_dirs.push(dir.clone());
        }
    }

    for dir in search_dirs {
        let candidate = dir.join("codex");
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    None
}

/// Known directories where Codex CLI might be installed on Windows.
pub fn known_windows_launcher_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    // %APPDATA%\npm (npm global packages directory on Windows)
    if let Some(app_data) = std::env::var_os("APPDATA") {
        dirs.push(PathBuf::from(app_data).join("npm"));
    } else if let Some(data_dir) = dirs::data_dir() {
        dirs.push(data_dir.join("npm"));
    }

    // %USERPROFILE%\.local\bin and %USERPROFILE%\.cargo\bin
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join(".local").join("bin"));
        dirs.push(home.join(".cargo").join("bin"));
    }

    dirs
}

/// Known directories where Codex CLI might be installed on Unix.
pub fn known_unix_launcher_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/bin"),
    ];

    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join(".local/bin"));
        dirs.push(home.join(".cargo/bin"));
        dirs.push(home.join(".nvm/versions/node/current/bin"));
    }

    dirs
}

// RATIONALE: [CODEX-RESUME-004] Platform-correct PATH augmentation for child processes
// When spawned from background services (launchd / Task Scheduler), PATH can be stripped.
// On Windows, directories must be separated by semicolon (`;`) and avoid Unix paths.
// On Unix, directories must be separated by colon (`:`) and include system paths.

#[cfg(windows)]
pub fn augment_path_for_child() -> Option<OsString> {
    let known = known_windows_launcher_dirs();
    augment_path_windows_internal(std::env::var_os("PATH").as_deref(), &known)
}

#[cfg(not(windows))]
pub fn augment_path_for_child() -> Option<OsString> {
    let known = known_unix_launcher_dirs();
    augment_path_unix_internal(std::env::var_os("PATH").as_deref(), &known)
}

pub fn augment_path_windows_internal(
    base_path: Option<&OsStr>,
    known_dirs: &[PathBuf],
) -> Option<OsString> {
    let mut paths: Vec<PathBuf> = base_path
        .map(|p| std::env::split_paths(p).collect())
        .unwrap_or_default();

    for dir in known_dirs {
        if !paths.contains(dir) {
            paths.push(dir.clone());
        }
    }

    std::env::join_paths(paths).ok()
}

pub fn augment_path_unix_internal(
    base_path: Option<&OsStr>,
    known_dirs: &[PathBuf],
) -> Option<OsString> {
    let mut paths: Vec<PathBuf> = base_path
        .map(|p| std::env::split_paths(p).collect())
        .unwrap_or_default();

    let system_candidates = [
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/opt/homebrew/sbin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/bin"),
        PathBuf::from("/bin"),
        PathBuf::from("/usr/sbin"),
        PathBuf::from("/sbin"),
    ];

    for c in &system_candidates {
        if !paths.contains(c) {
            paths.push(c.clone());
        }
    }

    for dir in known_dirs {
        if !paths.contains(dir) {
            paths.push(dir.clone());
        }
    }

    std::env::join_paths(paths).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

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
        assert!(!adapter.is_quota_error("Failed to execute process: (os error 193)"));
    }

    #[test]
    fn test_windows_launcher_reproduction_and_fix() {
        // Reproduce real incident:
        // Directory contains:
        // 1. `codex` (extensionless POSIX script from npm global install on Windows)
        // 2. `codex.cmd` (valid Windows cmd-shim)
        let dir = tempdir().expect("create tempdir");
        let dir_path = dir.path();

        let posix_shim = dir_path.join("codex");
        let mut f1 = File::create(&posix_shim).expect("create posix shim");
        writeln!(f1, "#!/bin/sh\necho 'posix shim'").expect("write posix shim");

        let cmd_shim = dir_path.join("codex.cmd");
        let mut f2 = File::create(&cmd_shim).expect("create cmd shim");
        writeln!(f2, "@echo off\necho cmd shim").expect("write cmd shim");

        // Old buggy behavior demonstration:
        // If checking `dir.join("codex")`, it would return the extensionless file!
        let buggy_pick = dir_path.join("codex");
        assert!(buggy_pick.is_file());
        assert_eq!(buggy_pick, posix_shim);

        // Fixed Windows resolution:
        // Must ignore extensionless `codex` and pick `codex.cmd`
        let resolved = resolve_windows_launcher(None, &[dir_path.to_path_buf()]);
        assert_eq!(resolved, Some(cmd_shim.clone()));
    }

    #[test]
    fn test_windows_launcher_precedence_exe_over_cmd() {
        let dir = tempdir().expect("create tempdir");
        let dir_path = dir.path();

        let posix_shim = dir_path.join("codex");
        File::create(&posix_shim).expect("create posix shim");

        let cmd_shim = dir_path.join("codex.cmd");
        File::create(&cmd_shim).expect("create cmd shim");

        let exe_binary = dir_path.join("codex.exe");
        File::create(&exe_binary).expect("create exe binary");

        // Resolution must prioritize .exe over .cmd
        let resolved = resolve_windows_launcher(None, &[dir_path.to_path_buf()]);
        assert_eq!(resolved, Some(exe_binary));
    }

    #[test]
    fn test_windows_launcher_not_found() {
        let dir = tempdir().expect("create tempdir");
        let dir_path = dir.path();

        // Only unrelated files exist
        let unrelated = dir_path.join("unrelated.exe");
        File::create(&unrelated).expect("create unrelated");

        let resolved = resolve_windows_launcher(None, &[dir_path.to_path_buf()]);
        assert_eq!(resolved, None);
    }

    #[test]
    fn test_unix_launcher_finds_extensionless_binary() {
        let dir = tempdir().expect("create tempdir");
        let dir_path = dir.path();

        let binary = dir_path.join("codex");
        File::create(&binary).expect("create binary");

        let resolved = resolve_unix_launcher(None, &[dir_path.to_path_buf()]);
        assert_eq!(resolved, Some(binary));
    }

    #[cfg(windows)]
    #[test]
    fn test_windows_path_augmentation() {
        let dir1 = PathBuf::from("C:\\Windows\\System32");
        let dir2 = PathBuf::from("C:\\Users\\テスト\\AppData\\Roaming\\npm");
        let dir3 = PathBuf::from("C:\\Users\\Test User\\AppData\\Roaming\\npm");

        let path_os = std::env::join_paths([&dir1, &dir2]).expect("join paths");
        let augmented =
            augment_path_windows_internal(Some(&path_os), &[dir2.clone(), dir3.clone()])
                .expect("augmented path");

        let split: Vec<PathBuf> = std::env::split_paths(&augmented).collect();
        assert!(split.contains(&dir1));
        assert!(split.contains(&dir2));
        assert!(split.contains(&dir3));

        // Deduplication: dir2 should appear only once
        let dir2_count = split.iter().filter(|&p| p == &dir2).count();
        assert_eq!(dir2_count, 1);

        // Must not contain macOS paths
        assert!(!split.contains(&PathBuf::from("/opt/homebrew/bin")));
    }

    #[test]
    fn test_unix_path_augmentation() {
        let dir1 = PathBuf::from("/usr/bin");
        let dir2 = PathBuf::from("/home/user/.local/bin");

        let path_os = std::env::join_paths([&dir1]).expect("join paths");
        let augmented = augment_path_unix_internal(Some(&path_os), std::slice::from_ref(&dir2))
            .expect("augmented path");

        let split: Vec<PathBuf> = std::env::split_paths(&augmented).collect();
        assert!(split.contains(&dir1));
        assert!(split.contains(&dir2));
        assert!(split.contains(&PathBuf::from("/opt/homebrew/bin")));
    }
}
