use fs2::FileExt;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use thiserror::Error;

pub const MAX_BOUNDED_LOG_BYTES: usize = 10 * 1024; // 10 KB

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

// RATIONALE: [SCHED-JOB-006] Lock-based liveness verification using kernel-managed file lock
// Relying only on PID checks is prone to PID wrap-around errors or stale processes.
// An OS-level exclusive file lock (flock / LockFileEx via fs2) is automatically released
// by the kernel if the process terminates, crashes, or the machine reboots.
/// Represents an acquired exclusive lock for a running job runner.
pub struct RunnerLock {
    file: File,
    path: PathBuf,
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

        Ok(Self { file, path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for RunnerLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
        let _ = fs::remove_file(&self.path);
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
) -> Result<(), RunnerError> {
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

    cmd.spawn()?;
    Ok(())
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
