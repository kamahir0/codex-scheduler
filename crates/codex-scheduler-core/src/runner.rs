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
    #[serde(default)]
    pub runner_start_time: Option<String>,
    pub runner_started_at: chrono::DateTime<chrono::Utc>,
    pub codex_pid: Option<u32>,
    pub codex_start_time: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HandoffLease {
    pub job_id: String,
    pub tick_pid: u32,
    #[serde(default)]
    pub tick_start_time: Option<String>,
    pub runner_pid: Option<u32>,
    pub runner_start_time: Option<String>,
    #[serde(default)]
    pub boot_id: Option<String>,
    #[serde(default)]
    pub boot_time: Option<u64>,
    #[serde(default)]
    pub uptime_ms: Option<u64>,
    pub claimed_at: chrono::DateTime<chrono::Utc>,
    #[serde(default)]
    pub unconfirmed_child: bool,
    #[serde(default)]
    pub job_object_name: Option<String>,
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

/// Returns the write-ahead execution guard path for a given job: `~/.codex-scheduler/runners/<job_id>.guard`
pub fn runner_guard_path(job_id: &str) -> Result<PathBuf, RunnerError> {
    let dir = runner_lock_dir()?;
    Ok(dir.join(format!("{}.guard", job_id)))
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
    if let Ok(guard_path) = runner_guard_path(job_id) {
        let _ = fs::remove_file(guard_path);
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

// RATIONALE: [SCHED-JOB-007] System boot identity detection for durable reboot recovery
// Enables detecting machine reboots definitively without relying on wall-clock arithmetic.
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
    #[repr(C)]
    struct SystemTimeOfDayInformation {
        boot_time: i64,
        current_time: i64,
        time_zone_bias: i64,
        time_zone_id: u32,
        reserved: u32,
        boot_time_bias: u64,
        sleep_time_bias: u64,
    }

    type NtQuerySystemInformationFn = unsafe extern "system" fn(
        system_information_class: u32,
        system_information: *mut std::ffi::c_void,
        system_information_length: u32,
        return_length: *mut u32,
    ) -> i32;

    use windows::core::s;
    use windows::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};

    unsafe {
        let ntdll = GetModuleHandleA(s!("ntdll.dll")).map_err(|_| ())?;
        let proc = GetProcAddress(ntdll, s!("NtQuerySystemInformation")).ok_or(())?;
        let nt_query_system_information: NtQuerySystemInformationFn = std::mem::transmute(proc);

        let mut info = std::mem::MaybeUninit::<SystemTimeOfDayInformation>::uninit();
        let mut return_length = 0u32;
        let status = nt_query_system_information(
            3, // SystemTimeOfDayInformation
            info.as_mut_ptr() as *mut std::ffi::c_void,
            std::mem::size_of::<SystemTimeOfDayInformation>() as u32,
            &mut return_length,
        );

        if status == 0 {
            let info = info.assume_init();
            if info.boot_time > 0 {
                // KeBootTime is 100-nanosecond intervals since January 1, 1601 UTC.
                let boot_time_100ns = info.boot_time as u64;
                const UNIX_EPOCH_100NS: u64 = 116_444_736_000_000_000;
                if boot_time_100ns >= UNIX_EPOCH_100NS {
                    let sec = (boot_time_100ns - UNIX_EPOCH_100NS) / 10_000_000;
                    return Ok(sec);
                } else {
                    return Ok(boot_time_100ns);
                }
            }
        }
        Err(())
    }
}

#[cfg(windows)]
pub fn get_system_boot_id() -> Option<String> {
    #[repr(C)]
    struct SystemTimeOfDayInformation {
        boot_time: i64,
        current_time: i64,
        time_zone_bias: i64,
        time_zone_id: u32,
        reserved: u32,
        boot_time_bias: u64,
        sleep_time_bias: u64,
    }

    type NtQuerySystemInformationFn = unsafe extern "system" fn(
        system_information_class: u32,
        system_information: *mut std::ffi::c_void,
        system_information_length: u32,
        return_length: *mut u32,
    ) -> i32;

    use windows::core::s;
    use windows::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};

    unsafe {
        let ntdll = GetModuleHandleA(s!("ntdll.dll")).ok()?;
        let proc = GetProcAddress(ntdll, s!("NtQuerySystemInformation"))?;
        let nt_query_system_information: NtQuerySystemInformationFn = std::mem::transmute(proc);

        let mut info = std::mem::MaybeUninit::<SystemTimeOfDayInformation>::uninit();
        let mut return_length = 0u32;
        let status = nt_query_system_information(
            3, // SystemTimeOfDayInformation
            info.as_mut_ptr() as *mut std::ffi::c_void,
            std::mem::size_of::<SystemTimeOfDayInformation>() as u32,
            &mut return_length,
        );

        if status == 0 {
            let info = info.assume_init();
            if info.boot_time > 0 {
                return Some(format!("win-boot-{:x}", info.boot_time));
            }
        }
        None
    }
}

#[cfg(target_os = "linux")]
pub fn get_system_boot_id() -> Option<String> {
    std::fs::read_to_string("/proc/sys/kernel/random/boot_id")
        .ok()
        .map(|s| s.trim().to_string())
}

#[cfg(all(not(target_os = "linux"), not(windows)))]
pub fn get_system_boot_id() -> Option<String> {
    None
}

#[cfg(windows)]
pub fn get_system_uptime_ms() -> Result<u64, ()> {
    use windows::Win32::System::SystemInformation::GetTickCount64;
    unsafe { Ok(GetTickCount64()) }
}

#[cfg(target_os = "linux")]
pub fn get_system_uptime_ms() -> Result<u64, ()> {
    if let Ok(content) = std::fs::read_to_string("/proc/uptime") {
        if let Some(sec_str) = content.split_whitespace().next() {
            if let Ok(sec) = sec_str.parse::<f64>() {
                return Ok((sec * 1000.0) as u64);
            }
        }
    }
    Err(())
}

#[cfg(all(unix, not(target_os = "linux")))]
pub fn get_system_uptime_ms() -> Result<u64, ()> {
    // macOS uses get_system_boot_time via sysctl kern.boottime
    Err(())
}

pub fn current_boot_identity() -> (Option<String>, Option<u64>, Option<u64>) {
    (
        get_system_boot_id(),
        get_system_boot_time().ok(),
        get_system_uptime_ms().ok(),
    )
}

pub fn job_object_name_for(job_id: &str) -> String {
    format!(r"Local\codex-scheduler-job-{}", job_id)
}

impl HandoffLease {
    pub fn new_initial(job_id: &str, tick_pid: u32, claimed_at: chrono::DateTime<chrono::Utc>) -> Self {
        let tick_start_time = get_process_start_time(tick_pid).ok().flatten();
        let (boot_id, boot_time, uptime_ms) = current_boot_identity();
        #[cfg(windows)]
        let job_object_name = Some(job_object_name_for(job_id));
        #[cfg(not(windows))]
        let job_object_name = None;
        Self {
            job_id: job_id.to_string(),
            tick_pid,
            tick_start_time,
            runner_pid: None,
            runner_start_time: None,
            boot_id,
            boot_time,
            uptime_ms,
            claimed_at,
            unconfirmed_child: false,
            job_object_name,
        }
    }

    pub fn with_runner(&self, runner_pid: u32, runner_start_time: Option<String>) -> Self {
        let mut updated = self.clone();
        updated.runner_pid = Some(runner_pid);
        updated.runner_start_time = runner_start_time;
        #[cfg(windows)]
        if updated.job_object_name.is_none() {
            updated.job_object_name = Some(job_object_name_for(&self.job_id));
        }
        updated
    }
}

// RATIONALE: [SCHED-JOB-007] Stable machine reboot detection
// Evaluates reboot evidence objectively:
// 1. Linux & Windows: kernel boot identifier (boot_id) mismatch.
// 2. macOS, Linux & Windows: kernel boot timestamp (boot_time) mismatch.
// Reboot is NEVER proved by GetTickCount64 / uptime rollback alone;
// boot-level OS evidence is strictly required to prove reboot.
// Returns false (fail-closed) if reboot cannot be proved objectively.
pub fn is_reboot_detected(lease: &HandoffLease) -> bool {
    // Check boot_id change (Linux & Windows)
    if let (Some(lease_bid), Some(cur_bid)) = (&lease.boot_id, &get_system_boot_id()) {
        if lease_bid != cur_bid {
            return true;
        }
    }

    // Check kernel boot timestamp change (macOS, Linux & Windows)
    if let (Some(lease_boot), Ok(cur_boot)) = (lease.boot_time, get_system_boot_time()) {
        if lease_boot != cur_boot {
            return true;
        }
    }

    false
}

/// Writes an unconfirmed termination marker for a job when child termination cannot be verified.
/// Distinguishes Unknown state from normal Dead state.
pub fn write_corrupt_runner_info_marker(job_id: &str, unconfirmed_pid: u32) -> Result<(), RunnerError> {
    let dir = runner_lock_dir()?;
    let path = dir.join(format!("{}.json", job_id));
    let content = format!(
        "{{\"job_id\":\"{}\",\"unconfirmed_child_pid\":{},\"status\":\"unconfirmed_termination\"}}",
        job_id, unconfirmed_pid
    );
    fs::write(path, content)?;
    Ok(())
}

// Test injection hooks for deterministic failure simulation
static INJECT_GUARD_WRITE_FAIL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static INJECT_GUARD_CLEAR_FAIL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static INJECT_JOB_OBJECT_ASSIGN_FAIL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_inject_guard_write_failure(fail: bool) {
    INJECT_GUARD_WRITE_FAIL.store(fail, std::sync::atomic::Ordering::SeqCst);
}

pub fn set_inject_guard_clear_failure(fail: bool) {
    INJECT_GUARD_CLEAR_FAIL.store(fail, std::sync::atomic::Ordering::SeqCst);
}

pub fn set_inject_job_object_assign_failure(fail: bool) {
    INJECT_JOB_OBJECT_ASSIGN_FAIL.store(fail, std::sync::atomic::Ordering::SeqCst);
}

// RATIONALE: [SCHED-JOB-006] Write-ahead durable execution guard before child process spawn
// Persists durable evidence that a child process may exist BEFORE calling spawn().
// Prevents the untracked child race where runner crashes between child spawn and RunnerInfo persistence.
// Cleared only after RunnerInfo atomic persistence succeeds.
pub fn write_execution_guard(job_id: &str) -> Result<(), RunnerError> {
    if INJECT_GUARD_WRITE_FAIL.load(std::sync::atomic::Ordering::SeqCst) {
        return Err(RunnerError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Injected guard write failure",
        )));
    }
    let path = runner_guard_path(job_id)?;
    let tmp_path = path.with_extension(format!("tmp.{}", std::process::id()));
    let content = serde_json::json!({
        "job_id": job_id,
        "guard": "unconfirmed_execution",
        "created_at": chrono::Utc::now().to_rfc3339(),
    });
    let content_str = serde_json::to_string(&content)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    fs::write(&tmp_path, content_str)?;
    fs::rename(&tmp_path, &path)?;
    Ok(())
}

/// Clears the write-ahead execution guard after RunnerInfo has been atomically persisted.
pub fn clear_execution_guard(job_id: &str) -> Result<(), RunnerError> {
    if INJECT_GUARD_CLEAR_FAIL.load(std::sync::atomic::Ordering::SeqCst) {
        return Err(RunnerError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Injected guard clear failure",
        )));
    }
    let path = runner_guard_path(job_id)?;
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

/// Checks whether an execution guard is actively in place.
pub fn has_execution_guard(job_id: &str) -> bool {
    if let Ok(path) = runner_guard_path(job_id) {
        if path.exists() {
            return true;
        }
    }
    if let Some(lease) = read_handoff_lease(job_id) {
        if lease.unconfirmed_child {
            return true;
        }
    }
    false
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LeaseLiveness {
    Active,
    Dead,
    Unknown,
}

/// Evaluates tri-state liveness for a HandoffLease holder process (Active, Dead, Unknown).
/// Invariants:
/// - unconfirmed_child guard -> Unknown (fail-closed unless reboot detected)
/// - Reboot detected -> Dead
/// - Identity missing or OS inspection error -> Unknown (fail-closed, never assumed Dead)
/// - Process does not exist or start identity mismatch (PID reuse) -> Dead
/// - Process exists and start identity matches -> Active
pub fn check_lease_liveness(lease: &HandoffLease) -> LeaseLiveness {
    if is_reboot_detected(lease) {
        return LeaseLiveness::Dead;
    }

    if lease.unconfirmed_child {
        return LeaseLiveness::Unknown;
    }

    if let Some(r_pid) = lease.runner_pid {
        match get_process_start_time(r_pid) {
            Ok(Some(actual_start)) => {
                let expected_start = match lease.runner_start_time.as_deref() {
                    Some(s) => s,
                    None => return LeaseLiveness::Unknown,
                };
                if actual_start == expected_start {
                    LeaseLiveness::Active
                } else {
                    LeaseLiveness::Dead
                }
            }
            Ok(None) => LeaseLiveness::Dead,
            Err(()) => LeaseLiveness::Unknown,
        }
    } else {
        match get_process_start_time(lease.tick_pid) {
            Ok(Some(actual_start)) => {
                let expected_tick_start = match lease.tick_start_time.as_deref() {
                    Some(s) => s,
                    None => return LeaseLiveness::Unknown,
                };
                if actual_start == expected_tick_start {
                    LeaseLiveness::Active
                } else {
                    LeaseLiveness::Dead
                }
            }
            Ok(None) => LeaseLiveness::Dead,
            Err(()) => LeaseLiveness::Unknown,
        }
    }
}

pub fn is_lease_active(lease: &HandoffLease) -> bool {
    check_lease_liveness(lease) == LeaseLiveness::Active
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessIdentityCheck {
    Matches,
    DeadOrMismatch,
    Unknown,
}

/// Verifies whether a process exists and its start identity matches expected identity.
/// Invariants:
/// - Missing expected identity -> Unknown
/// - OS inspection error -> Unknown
/// - Process not found or start identity mismatch -> DeadOrMismatch
/// - Start identity matches -> Matches
pub fn check_process_identity(pid: u32, expected_start: Option<&str>) -> ProcessIdentityCheck {
    let expected = match expected_start {
        Some(s) if !s.is_empty() => s,
        _ => return ProcessIdentityCheck::Unknown,
    };
    match get_process_start_time(pid) {
        Ok(Some(actual)) => {
            if actual == expected {
                ProcessIdentityCheck::Matches
            } else {
                ProcessIdentityCheck::DeadOrMismatch
            }
        }
        Ok(None) => ProcessIdentityCheck::DeadOrMismatch,
        Err(()) => ProcessIdentityCheck::Unknown,
    }
}

/// Safely terminates and confirms exit of a process matching expected start identity.
/// Invariants:
/// - Verifies PID + start identity BEFORE sending kill signal. NEVER kills mismatched or reused PIDs.
/// - If process is already dead or PID mismatched, returns Ok(true) without calling kill_process.
/// - If identity cannot be confirmed (missing expected identity or OS query error), returns Err(()) without killing (fail-closed Unknown).
/// - If identity matches, calls kill_process(pid) and polls up to 3 seconds for exit confirmation.
pub fn safe_terminate_and_confirm(pid: u32, expected_start: Option<&str>) -> Result<bool, ()> {
    match check_process_identity(pid, expected_start) {
        ProcessIdentityCheck::DeadOrMismatch => return Ok(true),
        ProcessIdentityCheck::Unknown => return Err(()),
        ProcessIdentityCheck::Matches => {
            kill_process(pid);
        }
    }

    // Poll up to 3.0s (60 iterations x 50ms) for confirmation
    for _ in 0..60 {
        match check_process_identity(pid, expected_start) {
            ProcessIdentityCheck::DeadOrMismatch => return Ok(true),
            ProcessIdentityCheck::Matches => {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            ProcessIdentityCheck::Unknown => {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
    }

    Ok(false)
}

#[cfg(not(windows))]
pub fn find_child_pids_of(parent_pid: u32) -> Vec<u32> {
    if parent_pid == 0 {
        return Vec::new();
    }
    let output = match std::process::Command::new("pgrep")
        .arg("-P")
        .arg(parent_pid.to_string())
        .output()
    {
        Ok(out) => out,
        Err(_) => return Vec::new(),
    };
    if output.status.success() {
        let text = String::from_utf8_lossy(&output.stdout);
        text.lines()
            .filter_map(|l| l.trim().parse::<u32>().ok())
            .collect()
    } else {
        Vec::new()
    }
}

#[cfg(windows)]
pub fn find_child_pids_of(parent_pid: u32) -> Vec<u32> {
    if parent_pid == 0 {
        return Vec::new();
    }
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32First, Process32Next, PROCESSENTRY32, TH32CS_SNAPPROCESS,
    };
    use windows::Win32::Foundation::CloseHandle;

    let mut children = Vec::new();
    unsafe {
        if let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
            let mut entry = PROCESSENTRY32::default();
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32>() as u32;
            if Process32First(snapshot, &mut entry).is_ok() {
                loop {
                    if entry.th32ParentProcessID == parent_pid {
                        children.push(entry.th32ProcessID);
                    }
                    if Process32Next(snapshot, &mut entry).is_err() {
                        break;
                    }
                }
            }
            let _ = CloseHandle(snapshot);
        }
    }
    children
}

/// Attempts to terminate both the detached runner process and any spawned Codex child processes,
/// verifying process start identities BEFORE killing to prevent terminating reused PIDs.
/// Pre-enumerates child processes of runner_pid to close the window where a child was spawned
/// but RunnerInfo was not yet persisted.
/// Returns true only if BOTH runner and all child processes are confirmed terminated.
/// Returns false (fail-closed) if termination cannot be confirmed or identity is Unknown.
pub fn terminate_and_confirm_runner_and_child(
    job_id: &str,
    runner_pid: u32,
    runner_start: Option<&str>,
) -> bool {
    // 1. Check runner process identity
    let runner_check = check_process_identity(runner_pid, runner_start);

    // 2. Gather all target child processes BEFORE terminating runner
    let mut child_targets: Vec<(u32, Option<String>)> = Vec::new();

    match read_runner_info_checked(job_id) {
        RunnerInfoRead::Present(info) => {
            if let Some(c_pid) = info.codex_pid {
                child_targets.push((c_pid, info.codex_start_time));
            }
        }
        RunnerInfoRead::Unreadable(_) => return false, // Metadata corrupt: fail-closed Unknown
        RunnerInfoRead::Missing => {
            // Only inspect process tree of runner_pid if runner_pid actually matches our runner!
            if runner_check == ProcessIdentityCheck::Matches {
                for child_pid in find_child_pids_of(runner_pid) {
                    let start = get_process_start_time(child_pid).ok().flatten();
                    child_targets.push((child_pid, start));
                }
            }
        }
    }

    // 3. Terminate and confirm runner process
    match safe_terminate_and_confirm(runner_pid, runner_start) {
        Ok(true) => {}
        _ => return false, // Failed to terminate, timed out, or identity Unknown -> fail-closed
    }

    // 4. Terminate and confirm all child processes
    for (c_pid, c_start) in child_targets {
        match safe_terminate_and_confirm(c_pid, c_start.as_deref()) {
            Ok(true) => {}
            _ => return false, // Failed to terminate child -> fail-closed
        }
    }

    true
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
    use windows::Win32::System::Threading::{OpenProcess, GetProcessTimes, GetExitCodeProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    use windows::Win32::Foundation::FILETIME;
    unsafe {
        match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(handle) => {
                let mut exit_code = 0u32;
                if GetExitCodeProcess(handle, &mut exit_code).is_ok() && exit_code != 259 {
                    let _ = CloseHandle(handle);
                    return Ok(None); // Process has exited!
                }
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
        .arg("stat=,lstart=")
        .output()
    {
        Ok(out) => out,
        Err(_) => return Err(()),
    };

    if output.status.success() {
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if s.is_empty() {
            return Ok(None);
        }
        // If the process is a zombie (defunct), it is definitively terminated.
        if s.starts_with('Z') {
            return Ok(None);
        }
        let parts: Vec<&str> = s.split_whitespace().collect();
        if parts.len() >= 2 {
            let lstart = parts[1..].join(" ");
            Ok(Some(lstart))
        } else {
            Ok(Some(s))
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
        // RATIONALE: [SCHED-JOB-006] Lock release MUST NOT delete execution evidence
        // RunnerInfo and HandoffLease are owned by the execution lifecycle.
        // They must NOT be removed when a lock is released due to conflict or foreign execution guard.
        // Cleanup happens only when execution reaches terminal status or during orphan recovery.
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

/// Kills a process by PID across platforms (SIGKILL on Unix, TerminateProcess on Windows).
pub fn kill_process(pid: u32) {
    if pid == 0 {
        return;
    }
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};
        unsafe {
            if let Ok(handle) = OpenProcess(PROCESS_TERMINATE, false, pid) {
                let _ = TerminateProcess(handle, 1);
                let _ = CloseHandle(handle);
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("kill")
            .arg("-9")
            .arg(pid.to_string())
            .output();
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

// RATIONALE: [SCHED-JOB-006] Process Group container setup on Unix
// Ensures current runner process is leader of its own process group (PGID == runner_pid).
// Detached runner spawned with process_group(0) is already leader; manual run-job path
// calls this before spawning Codex child to guarantee runner-owned container.
#[cfg(unix)]
pub fn ensure_runner_process_group() -> Result<u32, RunnerError> {
    unsafe extern "C" {
        fn setpgid(pid: i32, pgid: i32) -> i32;
        fn getpgrp() -> i32;
        fn getpid() -> i32;
    }
    unsafe {
        let pid = getpid();
        let pgrp = getpgrp();
        if pgrp != pid {
            let res = setpgid(0, 0);
            if res != 0 {
                let err = std::io::Error::last_os_error();
                let new_pgrp = getpgrp();
                if new_pgrp != pid {
                    return Err(RunnerError::Io(err));
                }
            }
        }
        Ok(getpgrp() as u32)
    }
}

#[cfg(not(unix))]
pub fn ensure_runner_process_group() -> Result<u32, RunnerError> {
    Ok(std::process::id())
}

// RATIONALE: [SCHED-JOB-006] Process Group container inspection on Unix
// Queries surviving processes belonging to PGID == pgid using pgrep -g.
// Verifies runner process identity beforehand to prevent misattributing unrelated processes on PID reuse.
#[cfg(unix)]
pub fn find_surviving_pgid_pids(pgid: u32, expected_runner_start: Option<&str>) -> Result<Vec<u32>, ()> {
    if pgid == 0 {
        return Ok(Vec::new());
    }

    // Step 1: Check runner PID identity to detect PID reuse
    match get_process_start_time(pgid) {
        Ok(Some(actual_start)) => {
            // A process with PID == pgid is currently running on the system.
            match expected_runner_start {
                Some(expected) => {
                    if actual_start != expected {
                        // PID reuse detected: pgid was reused by a new unrelated process.
                        // Any process group led by this unrelated process does NOT belong to our job.
                        return Ok(Vec::new());
                    }
                }
                None => {
                    // Expected runner start time is missing: cannot safely verify identity -> fail-closed Unknown
                    return Err(());
                }
            }
        }
        Ok(None) => {
            // Runner process is definitively dead, and PID has not been reused by any running process.
        }
        Err(()) => {
            // OS inspection failed: fail-closed Unknown
            return Err(());
        }
    }

    // Step 2: Query all processes belonging to PGID == pgid using pgrep -g
    let output = match std::process::Command::new("pgrep")
        .arg("-g")
        .arg(pgid.to_string())
        .output()
    {
        Ok(out) => out,
        Err(_) => return Err(()), // If pgrep fails to execute, fail-closed Unknown
    };

    if !output.status.success() {
        if output.status.code() == Some(1) {
            // pgrep returns 1 when no matching processes exist
            return Ok(Vec::new());
        }
        return Err(());
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut surviving = Vec::new();
    for line in text.lines() {
        if let Ok(pid) = line.trim().parse::<u32>() {
            match get_process_start_time(pid) {
                Ok(Some(_)) => {
                    surviving.push(pid);
                }
                _ => {} // Dead or defunct
            }
        }
    }

    Ok(surviving)
}

#[cfg(not(unix))]
pub fn find_surviving_pgid_pids(_pgid: u32, _expected_runner_start: Option<&str>) -> Result<Vec<u32>, ()> {
    Ok(Vec::new())
}

#[cfg(windows)]
#[derive(Clone, Copy)]
struct SendHandle(windows::Win32::Foundation::HANDLE);
#[cfg(windows)]
unsafe impl Send for SendHandle {}

#[cfg(windows)]
static RUNNER_JOB_OBJECT_HANDLE: std::sync::Mutex<Option<SendHandle>> =
    std::sync::Mutex::new(None);

#[cfg(windows)]
pub fn setup_runner_job_object(job_id: &str) -> Result<String, RunnerError> {
    if INJECT_JOB_OBJECT_ASSIGN_FAIL.load(std::sync::atomic::Ordering::SeqCst) {
        return Err(RunnerError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Injected Job Object assign failure",
        )));
    }

    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::JobObjects::{AssignProcessToJobObject, CreateJobObjectW};
    use windows::Win32::System::Threading::GetCurrentProcess;

    let name = job_object_name_for(job_id);
    let wide_name = HSTRING::from(&name);

    unsafe {
        let handle = CreateJobObjectW(None, PCWSTR(wide_name.as_ptr()))
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

        let assign_res = AssignProcessToJobObject(handle, GetCurrentProcess());
        if let Err(e) = assign_res {
            let _ = CloseHandle(handle);
            return Err(RunnerError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to assign runner to Job Object {}: {}", name, e),
            )));
        }

        if let Ok(mut lock) = RUNNER_JOB_OBJECT_HANDLE.lock() {
            if let Some(old) = lock.replace(SendHandle(handle)) {
                let _ = CloseHandle(old.0);
            }
        }

        Ok(name)
    }
}

#[cfg(windows)]
pub fn find_surviving_job_object_pids(job_object_name: &str) -> Result<Vec<u32>, ()> {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_FILE_NOT_FOUND};
    use windows::Win32::System::JobObjects::{
        OpenJobObjectW, QueryInformationJobObject, JobObjectBasicProcessIdList,
        JOBOBJECT_BASIC_PROCESS_ID_LIST,
    };
    const JOB_OBJECT_QUERY: u32 = 0x0004;

    let wide_name = HSTRING::from(job_object_name);

    unsafe {
        let handle = match OpenJobObjectW(JOB_OBJECT_QUERY, false, PCWSTR(wide_name.as_ptr())) {
            Ok(h) => h,
            Err(_) => {
                let err = GetLastError();
                if err == ERROR_FILE_NOT_FOUND {
                    return Ok(Vec::new());
                }
                return Err(());
            }
        };

        let mut buffer = vec![0u8; 4096];
        let mut return_len = 0u32;
        let query_res = QueryInformationJobObject(
            handle,
            JobObjectBasicProcessIdList,
            buffer.as_mut_ptr() as *mut std::ffi::c_void,
            buffer.len() as u32,
            Some(&mut return_len),
        );
        let _ = CloseHandle(handle);

        if query_res.is_err() {
            return Err(());
        }

        let list = &*(buffer.as_ptr() as *const JOBOBJECT_BASIC_PROCESS_ID_LIST);
        let count = list.NumberOfProcessIdsInList as usize;
        let mut pids = Vec::new();
        let ptr_list = std::slice::from_raw_parts(list.ProcessIdList.as_ptr(), count);
        for &pid in ptr_list {
            pids.push(pid as u32);
        }
        Ok(pids)
    }
}

#[cfg(not(windows))]
pub fn setup_runner_job_object(job_id: &str) -> Result<String, RunnerError> {
    if INJECT_JOB_OBJECT_ASSIGN_FAIL.load(std::sync::atomic::Ordering::SeqCst) {
        return Err(RunnerError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Injected Job Object assign failure",
        )));
    }
    Ok(job_object_name_for(job_id))
}

#[cfg(not(windows))]
pub fn find_surviving_job_object_pids(_job_object_name: &str) -> Result<Vec<u32>, ()> {
    Ok(Vec::new())
}

// RATIONALE: [SCHED-JOB-006] Unified execution container setup before Codex child process spawn
// On Unix: establishes runner-owned process group (PGID == runner_pid) so child inherits PGID.
// On Windows: creates named Job Object and assigns runner process so child inherits Job Object.
// Fails closed if container setup or assignment fails, preventing child spawn.
pub fn setup_runner_execution_container(_job_id: &str) -> Result<(), RunnerError> {
    #[cfg(unix)]
    {
        ensure_runner_process_group()?;
        if INJECT_JOB_OBJECT_ASSIGN_FAIL.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(RunnerError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Injected Job Object assign failure",
            )));
        }
        Ok(())
    }

    #[cfg(windows)]
    {
        let name = setup_runner_job_object(_job_id)?;
        if let Some(mut lease) = read_handoff_lease(_job_id) {
            lease.job_object_name = Some(name);
            let _ = write_handoff_lease(&lease);
        }
        Ok(())
    }

    #[cfg(all(not(unix), not(windows)))]
    {
        if INJECT_JOB_OBJECT_ASSIGN_FAIL.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(RunnerError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Injected Job Object assign failure",
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerLiveness {
    ActiveMembers(usize),
    NoMembers,
    Unknown,
}

// RATIONALE: [SCHED-JOB-006, SCHED-JOB-007] Execution container liveness query
// Objectively distinguishes Case A (0 surviving members -> safe recovery) from
// Case B (active members exist -> retain Running / block second writer).
// Evaluates machine reboot first (boot identity mismatch guarantees all pre-reboot containers dead).
pub fn check_execution_container_liveness(
    job_id: &str,
    lease: Option<&HandoffLease>,
) -> ContainerLiveness {
    // 1. Proven machine reboot: all previous OS containers are dead
    if let Some(l) = lease {
        if is_reboot_detected(l) {
            return ContainerLiveness::NoMembers;
        }
    }

    // 2. Windows: query named Job Object
    #[cfg(windows)]
    {
        let job_obj_name = lease
            .and_then(|l| l.job_object_name.clone())
            .unwrap_or_else(|| job_object_name_for(job_id));

        match find_surviving_job_object_pids(&job_obj_name) {
            Ok(pids) => {
                if pids.is_empty() {
                    ContainerLiveness::NoMembers
                } else {
                    ContainerLiveness::ActiveMembers(pids.len())
                }
            }
            Err(()) => ContainerLiveness::Unknown,
        }
    }

    // 3. Unix: query runner-owned Process Group
    #[cfg(unix)]
    {
        let (r_pid, r_start) = match lease {
            Some(l) => match l.runner_pid {
                Some(pid) => (pid, l.runner_start_time.clone()),
                None => {
                    return ContainerLiveness::NoMembers;
                }
            },
            None => {
                match read_runner_info_checked(job_id) {
                    RunnerInfoRead::Present(info) => (info.runner_pid, info.runner_start_time),
                    RunnerInfoRead::Unreadable(_) => return ContainerLiveness::Unknown,
                    RunnerInfoRead::Missing => return ContainerLiveness::NoMembers,
                }
            }
        };

        match find_surviving_pgid_pids(r_pid, r_start.as_deref()) {
            Ok(pids) => {
                if pids.is_empty() {
                    ContainerLiveness::NoMembers
                } else {
                    ContainerLiveness::ActiveMembers(pids.len())
                }
            }
            Err(()) => ContainerLiveness::Unknown,
        }
    }

    #[cfg(all(not(unix), not(windows)))]
    {
        ContainerLiveness::NoMembers
    }
}

/// Evaluates comprehensive execution liveness for a job across Runner lock, OS container, and Codex child.
pub fn get_job_liveness(job_id: &str) -> LivenessState {
    if is_runner_active(job_id) {
        return LivenessState::RunnerActive;
    }
    let lease_opt = read_handoff_lease(job_id);
    if let Some(ref lease) = lease_opt {
        if is_reboot_detected(lease) {
            return LivenessState::Dead;
        }
    }

    match check_execution_container_liveness(job_id, lease_opt.as_ref()) {
        ContainerLiveness::ActiveMembers(_) => return LivenessState::ChildActive,
        ContainerLiveness::Unknown => return LivenessState::Unknown,
        ContainerLiveness::NoMembers => {}
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
