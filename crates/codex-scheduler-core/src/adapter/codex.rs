use super::{AdapterError, ExecutionResult, ProviderAdapter};
use std::ffi::{OsStr, OsString};
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
        for dir in split_windows_paths(path) {
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

pub fn split_windows_paths(path_str: &OsStr) -> Vec<PathBuf> {
    path_str
        .to_string_lossy()
        .split(';')
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .collect()
}

pub fn join_windows_paths(paths: &[PathBuf]) -> OsString {
    let mut joined = String::new();
    for (i, p) in paths.iter().enumerate() {
        if i > 0 {
            joined.push(';');
        }
        joined.push_str(&p.to_string_lossy());
    }
    OsString::from(joined)
}

pub fn augment_path_windows_internal(
    base_path: Option<&OsStr>,
    known_dirs: &[PathBuf],
) -> Option<OsString> {
    let mut paths: Vec<PathBuf> = base_path.map(split_windows_paths).unwrap_or_default();

    for dir in known_dirs {
        if !paths.contains(dir) {
            paths.push(dir.clone());
        }
    }

    Some(join_windows_paths(&paths))
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

    #[test]
    fn test_windows_path_augmentation() {
        let dir1 = PathBuf::from("C:\\Windows\\System32");
        let dir2 = PathBuf::from("C:\\Users\\test\\AppData\\Roaming\\npm");

        let path_os = join_windows_paths(std::slice::from_ref(&dir1));
        let augmented = augment_path_windows_internal(Some(&path_os), std::slice::from_ref(&dir2))
            .expect("augmented path");

        let split: Vec<PathBuf> = split_windows_paths(&augmented);
        assert!(split.contains(&dir1));
        assert!(split.contains(&dir2));

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
