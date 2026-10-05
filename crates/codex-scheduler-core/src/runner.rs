use fs2::FileExt;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use thiserror::Error;

pub const MAX_BOUNDED_LOG_BYTES: usize = 10 * 1024; // 10 KB
pub const MAX_LOG_FILE_BYTES: u64 = 10 * 1024 * 1024; // 10 MB per attempt

#[derive(Debug, Error)]
pub enum RunnerError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Failed to acquire runner lock for job {0}: already running")]
    LockHeld(String),
    #[error("Home directory not found")]
    HomeNotFound,
    #[error("Executable path not specified")]
    ExecutableNotFound,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RunnerInfo {
    pub job_id: String,
    pub session_id: String,
    pub runner_pid: u32,
    pub runner_started_at: chrono::DateTime<chrono::Utc>,
    pub codex_pid: Option<u32>,
    pub codex_start_time: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HandoffLease {
    pub job_id: String,
    pub tick_pid: u32,
    pub runner_pid: Option<u32>,
    pub runner_start_time: Option<String>,
    pub boot_time: Option<u64>,
    pub claimed_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, PartialEq, Clone)]
pub enum RunnerInfoRead {
    Present(RunnerInfo),
    Missing,
    Unreadable(String),
}

/// Returns the base directory for runner locks: `~/.codex-scheduler/runners`
pub fn runner_lock_dir() -> Result<PathBuf, RunnerError> {
    let home = dirs::home_dir().ok_or(RunnerError::HomeNotFound)?;
    let dir = home.join(".codex-scheduler").join("runners");
    if !dir.exists() {
        fs::create_dir_all(&dir)?;
    }
    Ok(dir)
}

/// Returns the lock file path for a given job: `~/.codex-scheduler/runners/<job_id>.lock`
pub fn runner_lock_path(job_id: &str) -> Result<PathBuf, RunnerError> {
    let dir = runner_lock_dir()?;
    Ok(dir.join(format!("{}.lock", job_id)))
}

/// Returns the runner metadata info path for a given job: `~/.codex-scheduler/runners/<job_id>.json`
pub fn runner_info_path(job_id: &str) -> Result<PathBuf, RunnerError> {
    let dir = runner_lock_dir()?;
    Ok(dir.join(format!("{}.json", job_id)))
}

/// Returns the handoff lease path for a given job: `~/.codex-scheduler/runners/<job_id>.lease`
pub fn runner_lease_path(job_id: &str) -> Result<PathBuf, RunnerError> {
    let dir = runner_lock_dir()?;
    Ok(dir.join(format!("{}.lease", job_id)))
}

// RATIONALE: [SCHED-JOB-006] Atomic persistence of runner metadata via temp-file and rename
// Prevents exposing corrupt or partially written JSON during process interruption or crash.
pub fn write_runner_info(info: &RunnerInfo) -> Result<(), RunnerError> {
    let dir = runner_lock_dir()?;
    let path = dir.join(format!("{}.json", info.job_id));
    let tmp_path = dir.join(format!("{}.json.tmp.{}", info.job_id, std::process::id()));
    let content = serde_json::to_string_pretty(info)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    fs::write(&tmp_path, content)?;
    fs::rename(&tmp_path, &path)?;
    Ok(())
}

// RATIONALE: [SCHED-JOB-006] Tri-state runner info reading to fail-closed on corrupt metadata
// Distinguishes Missing (clean not started/cleaned) from Unreadable (parse error/IO error).
pub fn read_runner_info_checked(job_id: &str) -> RunnerInfoRead {
    let path = match runner_info_path(job_id) {
        Ok(p) => p,
        Err(e) => return RunnerInfoRead::Unreadable(e.to_string()),
    };
    if !path.exists() {
        return RunnerInfoRead::Missing;
    }
    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => return RunnerInfoRead::Unreadable(e.to_string()),
    };
    match serde_json::from_str(&content) {
        Ok(info) => RunnerInfoRead::Present(info),
        Err(e) => RunnerInfoRead::Unreadable(e.to_string()),
    }
}

pub fn read_runner_info(job_id: &str) -> Option<RunnerInfo> {
    match read_runner_info_checked(job_id) {
        RunnerInfoRead::Present(info) => Some(info),
        _ => None,
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum HandoffLeaseRead {
    Present(HandoffLease),
    Missing,
    Unreadable(String),
}

// RATIONALE: [SCHED-JOB-007] Durable handoff lease persistence
pub fn write_handoff_lease(lease: &HandoffLease) -> Result<(), RunnerError> {
    let dir = runner_lock_dir()?;
    let path = dir.join(format!("{}.lease", lease.job_id));
    let tmp_path = dir.join(format!("{}.lease.tmp.{}", lease.job_id, std::process::id()));
    let content = serde_json::to_string_pretty(lease)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    fs::write(&tmp_path, content)?;
    fs::rename(&tmp_path, &path)?;
    Ok(())
}

pub fn read_handoff_lease_checked(job_id: &str) -> HandoffLeaseRead {
    let path = match runner_lease_path(job_id) {
        Ok(p) => p,
        Err(e) => return HandoffLeaseRead::Unreadable(e.to_string()),
    };
    if !path.exists() {
        return HandoffLeaseRead::Missing;
    }
    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => return HandoffLeaseRead::Unreadable(e.to_string()),
    };
    match serde_json::from_str(&content) {
        Ok(lease) => HandoffLeaseRead::Present(lease),
        Err(e) => HandoffLeaseRead::Unreadable(e.to_string()),
    }
}

pub fn read_handoff_lease(job_id: &str) -> Option<HandoffLease> {
    match read_handoff_lease_checked(job_id) {
        HandoffLeaseRead::Present(lease) => Some(lease),
        _ => None,
    }
}

pub fn cleanup_runner_files(job_id: &str) {
    if let Ok(lock_path) = runner_lock_path(job_id) {
        let _ = fs::remove_file(lock_path);
    }
    if let Ok(info_path) = runner_info_path(job_id) {
        let _ = fs::remove_file(info_path);
    }
    if let Ok(lease_path) = runner_lease_path(job_id) {
        let _ = fs::remove_file(lease_path);
    }
}

/// Removes all log files and directory for a deleted job: `~/.codex-scheduler/logs/<job_id>`
pub fn cleanup_job_logs(job_id: &str) -> Result<(), RunnerError> {
    let home = dirs::home_dir().ok_or(RunnerError::HomeNotFound)?;
    let dir = home.join(".codex-scheduler").join("logs").join(job_id);
    if dir.exists() {
        fs::remove_dir_all(dir)?;
    }
    Ok(())
}

/// Returns the base directory for job execution logs: `~/.codex-scheduler/logs/<job_id>`
pub fn runner_log_dir(job_id: &str) -> Result<PathBuf, RunnerError> {
    let home = dirs::home_dir().ok_or(RunnerError::HomeNotFound)?;
    let dir = home.join(".codex-scheduler").join("logs").join(job_id);
    if !dir.exists() {
        fs::create_dir_all(&dir)?;
    }
    Ok(dir)
}

/// Returns the log file path for a specific attempt: `~/.codex-scheduler/logs/<job_id>/attempt-<attempt_number>.log`
pub fn runner_log_path(job_id: &str, attempt_number: u32) -> Result<PathBuf, RunnerError> {
    let dir = runner_log_dir(job_id)?;
    Ok(dir.join(format!("attempt-{}.log", attempt_number)))
}

// RATIONALE: [SCHED-JOB-007] System boot time detection for durable reboot recovery
// Enables detecting machine reboots definitively, ensuring all prior processes are dead.
#[cfg(target_os = "macos")]
pub fn get_system_boot_time() -> Result<u64, ()> {
    let output = std::process::Command::new("sysctl")
        .arg("-n")
        .arg("kern.boottime")
        .output()
        .map_err(|_| ())?;
    if !output.status.success() {
        return Err(());
    }
    let s = String::from_utf8_lossy(&output.stdout);
    if let Some(pos) = s.find("sec = ") {
        let after = &s[pos + 6..];
        let num_str: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        num_str.parse::<u64>().map_err(|_| ())
    } else {
        Err(())
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
pub fn get_system_boot_time() -> Result<u64, ()> {
    if let Ok(content) = std::fs::read_to_string("/proc/stat") {
        for line in content.lines() {
            if line.starts_with("btime ") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    if let Ok(t) = parts[1].parse::<u64>() {
                        return Ok(t);
                    }
                }
            }
        }
    }
    Err(())
}

#[cfg(windows)]
pub fn get_system_boot_time() -> Result<u64, ()> {
    use windows::Win32::System::SystemInformation::GetTickCount64;
    unsafe {
        let uptime_ms = GetTickCount64();
        let uptime_secs = uptime_ms / 1000;
        let now_secs = chrono::Utc::now().timestamp().max(0) as u64;
        Ok(now_secs.saturating_sub(uptime_secs))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LivenessState {
    RunnerActive,
    ChildActive,
    Dead,
    Unknown,
}

#[cfg(windows)]
pub fn get_process_start_time(pid: u32) -> Result<Option<String>, ()> {
    use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_INVALID_PARAMETER};
    use windows::Win32::System::Threading::{OpenProcess, GetProcessTimes, PROCESS_QUERY_LIMITED_INFORMATION};
    use windows::Win32::Foundation::FILETIME;
    unsafe {
        match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(handle) => {
                let mut creation = FILETIME::default();
                let mut exit = FILETIME::default();
                let mut kernel = FILETIME::default();
                let mut user = FILETIME::default();
                let res = GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user);
                let _ = CloseHandle(handle);
                if res.is_ok() {
                    let time_u64 = ((creation.dwHighDateTime as u64) << 32) | (creation.dwLowDateTime as u64);
                    Ok(Some(time_u64.to_string()))
                } else {
                    Err(())
                }
            }
            Err(_) => {
                let err_code = GetLastError();
                if err_code == ERROR_INVALID_PARAMETER {
                    Ok(None)
                } else {
                    Err(())
                }
            }
        }
    }
}

#[cfg(not(windows))]
pub fn get_process_start_time(pid: u32) -> Result<Option<String>, ()> {
    let output = match std::process::Command::new("ps")
        .arg("-p")
        .arg(pid.to_string())
        .arg("-o")
        .arg("lstart=")
        .output()
    {
        Ok(out) => out,
        Err(_) => return Err(()),
    };

    if output.status.success() {
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !s.is_empty() {
            Ok(Some(s))
        } else {
            Ok(None)
        }
    } else {
        Ok(None)
    }
}

// RATIONALE: [SCHED-JOB-006] Lock-based liveness verification using kernel-managed file lock
// Relying only on PID checks is prone to PID wrap-around errors or stale processes.
// An OS-level exclusive file lock (flock / LockFileEx via fs2) is automatically released
// by the kernel if the process terminates, crashes, or the machine reboots.
static CURRENT_PROCESS_RUNNER_LOCKS: std::sync::Mutex<Option<std::collections::HashSet<String>>> = std::sync::Mutex::new(None);

fn record_current_process_lock(job_id: &str) {
    if let Ok(mut guard) = CURRENT_PROCESS_RUNNER_LOCKS.lock() {
        guard.get_or_insert_with(std::collections::HashSet::new).insert(job_id.to_string());
    }
}

fn remove_current_process_lock(job_id: &str) {
    if let Ok(mut guard) = CURRENT_PROCESS_RUNNER_LOCKS.lock() {
        if let Some(ref mut set) = *guard {
            set.remove(job_id);
        }
    }
}

/// Checks whether the runner lock for a given job is actively held by the current process.
pub fn is_runner_lock_held_by_current_process(job_id: &str) -> bool {
    if let Ok(guard) = CURRENT_PROCESS_RUNNER_LOCKS.lock() {
        if guard.as_ref().map(|set| set.contains(job_id)).unwrap_or(false) {
            return true;
        }
    }
    // Fallback check against persisted lock file metadata
    get_runner_lock_holder(job_id) == Some(std::process::id())
}

/// Represents an acquired exclusive lock for a running job runner.
pub struct RunnerLock {
    file: File,
    path: PathBuf,
    job_id: String,
}

impl RunnerLock {
    /// Attempts to acquire an exclusive lock for the given job.
    /// Fails with `RunnerError::LockHeld` if another runner process currently holds the lock.
    pub fn acquire(job_id: &str) -> Result<Self, RunnerError> {
        let path = runner_lock_path(job_id)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;

        file.try_lock_exclusive()
            .map_err(|_| RunnerError::LockHeld(job_id.to_string()))?;

        // Write PID metadata to the lock file for diagnostic inspection
        let mut f_clone = file.try_clone()?;
        let _ = f_clone.set_len(0);
        let meta = serde_json::json!({
            "pid": std::process::id(),
            "started_at": chrono::Utc::now().to_rfc3339(),
        });
        let _ = writeln!(f_clone, "{}", meta);
        let _ = f_clone.flush();

        record_current_process_lock(job_id);

        Ok(Self { file, path, job_id: job_id.to_string() })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for RunnerLock {
    fn drop(&mut self) {
        remove_current_process_lock(&self.job_id);
        let _ = self.file.unlock();
        cleanup_runner_files(&self.job_id);
    }
}

// RATIONALE: [SCHED-JOB-006] Verify child process execution liveness using PID and OS start-time
// Prevents PID wrap-around errors and ensures second writer is never spawned while child process is alive.
pub fn is_codex_process_alive(info: &RunnerInfo) -> bool {
    check_codex_process_liveness(info) == LivenessState::ChildActive
}

/// Evaluates three-state liveness (ChildActive, Dead, Unknown) for recorded Codex child process.
pub fn check_codex_process_liveness(info: &RunnerInfo) -> LivenessState {
    let pid = match info.codex_pid {
        Some(p) => p,
        None => return LivenessState::Unknown,
    };
    let expected_start = match info.codex_start_time.as_deref() {
        Some(t) => t,
        None => return LivenessState::Unknown,
    };
    match get_process_start_time(pid) {
        Ok(Some(actual_start)) => {
            if actual_start == expected_start {
                LivenessState::ChildActive
            } else {
                LivenessState::Dead
            }
        }
        Ok(None) => LivenessState::Dead,
        Err(()) => LivenessState::Unknown,
    }
}

// RATIONALE: [SCHED-JOB-006] Checking runner liveness via non-blocking lock acquisition
// If the lock file can be locked exclusively, no runner process is holding it.
// If locking fails with an error indicating would-block / locked, the runner is actively running.
pub fn is_runner_active(job_id: &str) -> bool {
    let path = match runner_lock_path(job_id) {
        Ok(p) => p,
        Err(_) => return false,
    };

    if !path.exists() {
        return false;
    }

    match OpenOptions::new().read(true).write(true).open(&path) {
        Ok(file) => match file.try_lock_exclusive() {
            Ok(()) => {
                let _ = file.unlock();
                false
            }
            Err(_) => true,
        },
        Err(_) => false,
    }
}

// RATIONALE: [CLI-CMD-002, OS-SCHED-003] Identify runner lock holder to distinguish self-lock from conflict
// Reads written lock metadata to safely detect if the current process holds the lock.
pub fn get_runner_lock_holder(job_id: &str) -> Option<u32> {
    let path = runner_lock_path(job_id).ok()?;
    if !path.exists() {
        return None;
    }
    let content = fs::read_to_string(path).ok()?;
    let val: serde_json::Value = serde_json::from_str(&content).ok()?;
    val.get("pid").and_then(|p| p.as_u64()).map(|p| p as u32)
}

/// Evaluates comprehensive execution liveness for a job across Runner lock and Codex child process.
pub fn get_job_liveness(job_id: &str) -> LivenessState {
    if is_runner_active(job_id) {
        return LivenessState::RunnerActive;
    }
    match read_runner_info_checked(job_id) {
        RunnerInfoRead::Present(info) => check_codex_process_liveness(&info),
        RunnerInfoRead::Unreadable(_) => LivenessState::Unknown,
        RunnerInfoRead::Missing => LivenessState::Dead,
    }
}

// RATIONALE: [CODEX-RESUME-009] Strict 10MB capped log persistence factoring existing file length
// Ensures disk log files never exceed MAX_LOG_FILE_BYTES (10MB) combined across stdout and stderr,
// even when appending to an existing log file.
pub struct CappedLogWriter {
    file: tokio::fs::File,
    written_bytes: u64,
    max_bytes: u64,
}

impl CappedLogWriter {
    pub fn new(file: tokio::fs::File, max_bytes: u64, existing_len: u64) -> Self {
        let written_bytes = existing_len.min(max_bytes);
        Self {
            file,
            written_bytes,
            max_bytes,
        }
    }

    pub async fn write_chunk(&mut self, chunk: &[u8]) -> std::io::Result<()> {
        use tokio::io::AsyncWriteExt;
        if self.written_bytes >= self.max_bytes {
            return Ok(());
        }

        let remaining = (self.max_bytes - self.written_bytes) as usize;
        let cap_marker = b"\n[...log file reached max size 10MB, further output capped...]\n";

        if chunk.len() < remaining {
            self.file.write_all(chunk).await?;
            self.written_bytes += chunk.len() as u64;
        } else {
            // chunk reaches or exceeds cap
            if remaining > cap_marker.len() {
                let chunk_fit = remaining - cap_marker.len();
                if chunk_fit > 0 {
                    self.file.write_all(&chunk[..chunk_fit]).await?;
                }
                self.file.write_all(cap_marker).await?;
            } else {
                self.file.write_all(&chunk[..remaining]).await?;
            }
            self.written_bytes = self.max_bytes;
        }
        self.file.flush().await?;
        Ok(())
    }
}


// RATIONALE: [SCHED-JOB-004] Bounded memory and store footprint with streaming file log persistence
// Prevents memory exhaustion and unbounded jobs.json growth by trimming to the latest bounded window.
pub fn bounded_log_tail(content: &str, max_bytes: usize) -> String {
    bounded_log_tail_with_flag(content, max_bytes, false)
}

pub fn bounded_log_tail_with_flag(content: &str, max_bytes: usize, was_truncated: bool) -> String {
    if !was_truncated && content.len() <= max_bytes {
        return content.to_string();
    }

    let tail_slice = if content.len() > max_bytes {
        let start = content.len() - max_bytes;
        let mut boundary = start;
        while boundary < content.len() && !content.is_char_boundary(boundary) {
            boundary += 1;
        }
        &content[boundary..]
    } else {
        content
    };

    format!("[...truncated...]\n{}", tail_slice)
}

// RATIONALE: [OS-SCHED-003] Detached process spawning to decouple persistent tick from long-running execution
// Spawns runner in a detached process group without blocking the calling scheduler tick.
// On Windows, DETACHED_PROCESS (0x8) and CREATE_NEW_PROCESS_GROUP (0x200) ensure the process
// outlives Task Scheduler instances. On Unix, process_group(0) / AbandonProcessGroup in launchd
// allows the child to continue independently.
pub fn spawn_detached_runner(
    exe_path: &Path,
    job_id: &str,
    is_desktop: bool,
) -> Result<u32, RunnerError> {
    let mut cmd = std::process::Command::new(exe_path);

    if is_desktop {
        cmd.arg("--run-job").arg(job_id);
    } else {
        cmd.arg("run-job").arg(job_id);
    }

    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x00000008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }

    #[cfg(not(windows))]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    let child = cmd.spawn()?;
    Ok(child.id())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bounded_log_tail() {
        let short = "Hello World";
        assert_eq!(bounded_log_tail(short, 100), "Hello World");

        let long = "1234567890abcdefghijklmnopqrstuvwxyz";
        let trimmed = bounded_log_tail(long, 10);
        assert!(trimmed.contains("[...truncated...]"));
        assert!(trimmed.ends_with("rstuvwxyz") || trimmed.ends_with("qrstuvwxyz"));
    }

    #[test]
    fn test_runner_lock_lifecycle() {
        let temp = tempfile::tempdir().unwrap();
        let job_id = "test-job-lifecycle-1";
        let lock_path = temp.path().join(format!("{}.lock", job_id));

        // Acquire lock
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .unwrap();
        file.try_lock_exclusive().unwrap();

        // Second acquire should fail
        let file2 = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&lock_path)
            .unwrap();
        assert!(file2.try_lock_exclusive().is_err());

        // Release first lock
        file.unlock().unwrap();

        // Now second acquire should succeed
        assert!(file2.try_lock_exclusive().is_ok());
        file2.unlock().unwrap();
    }
}
