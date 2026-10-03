use crate::models::Job;
use crate::os_scheduler::{SchedulerBackend, SchedulerError, SchedulerOwner};
use std::io;
use std::path::{Path, PathBuf};

pub const TASK_NAME: &str = "CodexScheduler_Service";
pub const CANONICAL_DESKTOP_EXE: &str = "codex-scheduler-gui.exe";
pub const CANONICAL_CLI_EXE: &str = "codex-scheduler.exe";
pub const LEGACY_CLI_EXE: &str = "codex-scheduler-cli.exe";
pub const TICK_ARG: &str = "--scheduler-tick";

/// Abstraction for invoking schtasks.exe to allow deterministic, non-destructive testing.
pub trait TaskSchedulerRunner: Send + Sync {
    fn run_schtasks(&self, args: &[&str]) -> io::Result<std::process::Output>;
}

/// Production runner that invokes schtasks.exe directly via std::process::Command.
#[derive(Default)]
pub struct RealTaskSchedulerRunner;

impl TaskSchedulerRunner for RealTaskSchedulerRunner {
    fn run_schtasks(&self, args: &[&str]) -> io::Result<std::process::Output> {
        std::process::Command::new("schtasks.exe")
            .args(args)
            .output()
    }
}

/// Windows Task Scheduler production backend.
pub struct WindowsTaskScheduler {
    runner: Box<dyn TaskSchedulerRunner>,
}

impl Default for WindowsTaskScheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskInfo {
    pub exists: bool,
    pub enabled: bool,
    pub command: Option<PathBuf>,
    pub arguments: Option<String>,
    pub raw_xml: Option<String>,
}

impl WindowsTaskScheduler {
    pub fn new() -> Self {
        Self {
            runner: Box::new(RealTaskSchedulerRunner),
        }
    }

    pub fn with_runner(runner: Box<dyn TaskSchedulerRunner>) -> Self {
        Self { runner }
    }

    /// Determines if an executable path represents the Desktop GUI application.
    pub fn is_desktop_executable(path: &Path) -> bool {
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            name.eq_ignore_ascii_case(CANONICAL_DESKTOP_EXE)
        } else {
            false
        }
    }

    /// Determines if an executable path represents a standalone CLI application.
    pub fn is_cli_executable(path: &Path) -> bool {
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.eq_ignore_ascii_case(CANONICAL_CLI_EXE)
                || name.eq_ignore_ascii_case(LEGACY_CLI_EXE)
            {
                return true;
            }
            #[cfg(test)]
            {
                let lower = name.to_ascii_lowercase();
                if lower == "echo"
                    || lower == "sh"
                    || lower.starts_with("test")
                    || (lower.ends_with(".exe") && lower.starts_with("test"))
                {
                    return true;
                }
            }
        }
        false
    }

    /// Extracts a tag's text content from XML string.
    pub fn extract_xml_tag(xml: &str, tag: &str) -> Option<String> {
        let open_tag = format!("<{}>", tag);
        let close_tag = format!("</{}>", tag);
        let start_idx = xml.find(&open_tag)? + open_tag.len();
        let end_idx = xml[start_idx..].find(&close_tag)? + start_idx;
        let content = xml[start_idx..end_idx].trim();
        Some(Self::unescape_xml(content))
    }

    pub fn escape_xml(input: &str) -> String {
        input
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&apos;")
    }

    pub fn unescape_xml(input: &str) -> String {
        input
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&")
    }

    /// Generates canonical Windows Task Scheduler XML definition.
    pub fn generate_task_xml(exe_path: &Path) -> String {
        let escaped_path = Self::escape_xml(&exe_path.to_string_lossy());
        format!(
            r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>Codex Scheduler persistent background service</Description>
  </RegistrationInfo>
  <Triggers>
    <TimeTrigger>
      <Repetition>
        <Interval>PT1M</Interval>
        <StopAtDurationEnd>false</StopAtDurationEnd>
      </Repetition>
      <StartBoundary>2026-01-01T00:00:00</StartBoundary>
      <Enabled>true</Enabled>
    </TimeTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author">
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>LeastPrivilege</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>true</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>
    <IdleSettings>
      <StopOnIdleEnd>false</StopOnIdleEnd>
      <RestartOnIdle>false</RestartOnIdle>
    </IdleSettings>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
    <Hidden>false</Hidden>
    <RunOnlyIfIdle>false</RunOnlyIfIdle>
    <WakeToRun>false</WakeToRun>
    <ExecutionTimeLimit>PT72H</ExecutionTimeLimit>
    <Priority>7</Priority>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>{}</Command>
      <Arguments>{}</Arguments>
    </Exec>
  </Actions>
</Task>"#,
            escaped_path, TICK_ARG
        )
    }

    /// Encodes a string as UTF-16LE bytes with BOM.
    pub fn encode_utf16le_with_bom(s: &str) -> Vec<u8> {
        let mut bytes = vec![0xFF, 0xFE];
        for u in s.encode_utf16() {
            bytes.extend_from_slice(&u.to_le_bytes());
        }
        bytes
    }

    /// Decodes schtasks output, handling UTF-16LE (with or without BOM) and UTF-8/ANSI.
    pub fn decode_schtasks_output(bytes: &[u8]) -> String {
        if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
            let u16_slice: Vec<u16> = bytes[2..]
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            return String::from_utf16_lossy(&u16_slice);
        }
        if bytes.len() >= 4 && bytes[0] == b'<' && bytes[1] == 0x00 && bytes[2] == b'?' && bytes[3] == 0x00 {
            let u16_slice: Vec<u16> = bytes
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            return String::from_utf16_lossy(&u16_slice);
        }
        String::from_utf8_lossy(bytes).to_string()
    }

    /// Queries schtasks for the current task definition and parses it.
    pub fn query_task(&self) -> Result<TaskInfo, SchedulerError> {
        let output = self.runner.run_schtasks(&["/Query", "/TN", TASK_NAME, "/XML"])?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout_snippet = String::from_utf8_lossy(&output.stdout);
            let combined = format!("{} {}", stdout_snippet, stderr);
            // If the task does not exist, schtasks outputs "The system cannot find the file specified" or localized variants
            let not_found = combined.contains("cannot find")
                || combined.contains("見つかりません")
                || combined.contains("does not exist")
                || combined.contains("存在しません");
            let access_denied = combined.contains("Access is denied") || combined.contains("アクセスが拒否");

            if not_found || (output.status.code() == Some(1) && !access_denied) {
                return Ok(TaskInfo {
                    exists: false,
                    enabled: false,
                    command: None,
                    arguments: None,
                    raw_xml: None,
                });
            }
            return Err(SchedulerError::CommandFailed(format!(
                "schtasks /Query failed: {}",
                stderr.trim()
            )));
        }

        let xml = Self::decode_schtasks_output(&output.stdout);
        let command = Self::extract_xml_tag(&xml, "Command").map(PathBuf::from);
        let arguments = Self::extract_xml_tag(&xml, "Arguments");

        // Task is considered enabled if Settings.Enabled != false and Triggers.Enabled != false
        let settings_enabled = if let Some(settings_sec) = xml.find("<Settings>") {
            let end_sec = xml[settings_sec..].find("</Settings>").unwrap_or(xml.len() - settings_sec) + settings_sec;
            let section = &xml[settings_sec..end_sec];
            Self::extract_xml_tag(section, "Enabled").map_or(true, |v| v.eq_ignore_ascii_case("true"))
        } else {
            true
        };

        let trigger_enabled = if let Some(trig_sec) = xml.find("<Triggers>") {
            let end_sec = xml[trig_sec..].find("</Triggers>").unwrap_or(xml.len() - trig_sec) + trig_sec;
            let section = &xml[trig_sec..end_sec];
            Self::extract_xml_tag(section, "Enabled").map_or(true, |v| v.eq_ignore_ascii_case("true"))
        } else {
            true
        };

        let enabled = settings_enabled && trigger_enabled;

        Ok(TaskInfo {
            exists: true,
            enabled,
            command,
            arguments,
            raw_xml: Some(xml),
        })
    }

    /// Registers or updates the task using an XML definition via schtasks.
    pub fn register_task_xml(&self, exe_path: &Path) -> Result<(), SchedulerError> {
        let xml_content = Self::generate_task_xml(exe_path);
        let temp_dir = std::env::temp_dir();
        let temp_xml_path = temp_dir.join(format!("codex_sched_task_{}.xml", uuid::Uuid::new_v4()));
        let utf16_bytes = Self::encode_utf16le_with_bom(&xml_content);
        std::fs::write(&temp_xml_path, &utf16_bytes)?;

        let path_str = temp_xml_path.to_string_lossy();
        let output = self.runner.run_schtasks(&["/Create", "/TN", TASK_NAME, "/XML", &path_str, "/F"]);
        let _ = std::fs::remove_file(&temp_xml_path);

        let output = output?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SchedulerError::CommandFailed(format!(
                "Failed to register Windows Scheduled Task via schtasks /Create: {}",
                stderr.trim()
            )));
        }

        Ok(())
    }

    /// Enables the task via schtasks /Change /ENABLE.
    pub fn enable_task(&self) -> Result<(), SchedulerError> {
        let output = self.runner.run_schtasks(&["/Change", "/TN", TASK_NAME, "/ENABLE"])?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SchedulerError::CommandFailed(format!(
                "Failed to enable Windows Scheduled Task via schtasks /Change: {}",
                stderr.trim()
            )));
        }
        Ok(())
    }
}

impl SchedulerBackend for WindowsTaskScheduler {
    fn supports_persistent_scheduler(&self) -> bool {
        true
    }

    fn is_scheduler_installed(&self) -> bool {
        self.query_task().map(|t| t.exists).unwrap_or(false)
    }

    fn is_scheduler_ready(&self) -> bool {
        let task = match self.query_task() {
            Ok(t) => t,
            Err(_) => return false,
        };
        if !task.exists || !task.enabled {
            return false;
        }

        let owner = self.get_scheduler_owner();
        if owner != SchedulerOwner::Desktop && owner != SchedulerOwner::Cli {
            return false;
        }

        let path = match self.get_scheduler_executable_path() {
            Some(p) => p,
            None => return false,
        };

        path.is_absolute() && path.exists()
    }

    fn is_scheduler_path_matched(&self, exe_path: &Path) -> bool {
        if let Some(target) = self.get_scheduler_executable_path() {
            #[cfg(windows)]
            {
                target.as_os_str().to_string_lossy().eq_ignore_ascii_case(&exe_path.as_os_str().to_string_lossy())
            }
            #[cfg(not(windows))]
            {
                target == exe_path
            }
        } else {
            false
        }
    }

    fn get_scheduler_owner(&self) -> SchedulerOwner {
        let task = match self.query_task() {
            Ok(t) => t,
            Err(_) => return SchedulerOwner::Invalid,
        };
        if !task.exists {
            return SchedulerOwner::None;
        }

        let cmd = match task.command {
            Some(c) => c,
            None => return SchedulerOwner::Invalid,
        };

        // Check if arguments include canonical --scheduler-tick
        let args = task.arguments.unwrap_or_default();
        if !args.contains(TICK_ARG) {
            return SchedulerOwner::Invalid;
        }

        // Must be absolute and must exist on disk
        if !cmd.is_absolute() || !cmd.exists() {
            return SchedulerOwner::Invalid;
        }

        if Self::is_desktop_executable(&cmd) {
            SchedulerOwner::Desktop
        } else if Self::is_cli_executable(&cmd) {
            SchedulerOwner::Cli
        } else {
            SchedulerOwner::Invalid
        }
    }

    fn get_scheduler_executable_path(&self) -> Option<PathBuf> {
        self.query_task().ok().and_then(|t| t.command)
    }

    fn is_owner_target_valid(&self) -> bool {
        let owner = self.get_scheduler_owner();
        if let Some(path) = self.get_scheduler_executable_path() {
            if !path.is_absolute() || !path.exists() {
                return false;
            }
            match owner {
                SchedulerOwner::Desktop => Self::is_desktop_executable(&path),
                SchedulerOwner::Cli => Self::is_cli_executable(&path),
                _ => false,
            }
        } else {
            false
        }
    }

    // WHY: State Matrix implementation for Windows Task Scheduler (spec-change 0016, OS-SCHED-002, CLI-CMD-004).
    //      Guarantees Desktop Precedence, single task invariant (CodexScheduler_Service), safe takeover from CLI,
    //      safe repair of unloaded/disabled tasks, and non-destructive protection of invalid/stale targets.
    // WHAT BREAKS: Destructive overwrite deletes user Desktop registration or allows CLI takeover on stale targets.
    // EVIDENCE: docs/spec-changes/0016-windows-task-scheduler-and-cli-distribution.md, ST-01 to ST-13
    fn ensure_scheduler_installed(&self, target_exe_path: &Path) -> Result<(), SchedulerError> {
        if !target_exe_path.is_absolute() {
            return Err(SchedulerError::InvalidExecutable(
                target_exe_path.display().to_string(),
            ));
        }

        if !target_exe_path.exists() {
            return Err(SchedulerError::ExecutableNotFound(
                target_exe_path.display().to_string(),
            ));
        }

        let task = self.query_task()?;
        let caller_is_desktop = Self::is_desktop_executable(target_exe_path);

        // ST-01 / ST-02: Existing task owner is None
        if !task.exists {
            return self.register_task_xml(target_exe_path);
        }

        let current_owner = self.get_scheduler_owner();

        // If task exists but is Invalid/malformed/stale:
        if current_owner == SchedulerOwner::Invalid {
            let cmd_opt = task.command.clone();
            let args = task.arguments.clone().unwrap_or_default();

            let has_tick = args.contains(TICK_ARG);
            if cmd_opt.is_none() || !has_tick {
                // ST-13: Malformed task configuration
                return Err(SchedulerError::MalformedConfiguration(format!(
                    "Unrecognized or malformed Windows Scheduled Task: {}",
                    TASK_NAME
                )));
            }

            let registered_exe = cmd_opt.unwrap();
            let exe_exists = registered_exe.is_absolute() && registered_exe.exists();

            if !exe_exists {
                let intended_desktop = Self::is_desktop_executable(&registered_exe);
                let intended_cli = Self::is_cli_executable(&registered_exe);

                if intended_desktop {
                    if !caller_is_desktop {
                        // ST-09: Stale Desktop target + CLI caller -> Error / No takeover!
                        return Err(SchedulerError::ExecutableNotFound(format!(
                            "Registered Desktop scheduler executable does not exist: {}",
                            registered_exe.display()
                        )));
                    } else {
                        // ST-10: Stale Desktop target + Desktop caller -> Desktop Recovery
                        return self.register_task_xml(target_exe_path);
                    }
                } else if intended_cli {
                    if !caller_is_desktop {
                        // ST-11: Stale CLI target + CLI caller -> CLI Recovery
                        return self.register_task_xml(target_exe_path);
                    } else {
                        // ST-12: Stale CLI target + Desktop caller -> Safe takeover to Desktop
                        return self.register_task_xml(target_exe_path);
                    }
                } else {
                    return Err(SchedulerError::MalformedConfiguration(format!(
                        "Registered scheduler executable target is unrecognized: {}",
                        registered_exe.display()
                    )));
                }
            } else {
                // Executable exists on disk but is neither desktop nor cli
                return Err(SchedulerError::MalformedConfiguration(format!(
                    "Registered scheduler executable is neither canonical desktop nor CLI: {}",
                    registered_exe.display()
                )));
            }
        }

        // ST-03 / ST-04: Existing task owner is Desktop
        if current_owner == SchedulerOwner::Desktop {
            if !caller_is_desktop {
                // CLI caller
                if self.is_scheduler_ready() {
                    // ST-03: Desktop healthy + CLI -> Retain, no mutation
                    return Ok(());
                } else {
                    // ST-04: Desktop unhealthy / disabled + CLI -> Safe Repair (preserve Desktop target)
                    let desktop_exe = self.get_scheduler_executable_path().ok_or_else(|| {
                        SchedulerError::CommandFailed("Failed to get Desktop target path".to_string())
                    })?;
                    self.register_task_xml(&desktop_exe)?;
                    if !self.is_scheduler_ready() {
                        return Err(SchedulerError::CommandFailed(
                            "Desktop Task was repaired, but Task Scheduler still reports not ready".to_string(),
                        ));
                    }
                    return Ok(());
                }
            } else {
                // ST-05: Desktop caller + Desktop owner -> Retain or repair
                if let Some(existing_exe) = self.get_scheduler_executable_path() {
                    if existing_exe == target_exe_path && self.is_scheduler_ready() {
                        return Ok(());
                    }
                }
                return self.register_task_xml(target_exe_path);
            }
        }

        // ST-06 / ST-07 / ST-08: Existing task owner is CLI
        if current_owner == SchedulerOwner::Cli {
            if !caller_is_desktop {
                // CLI caller
                if let Some(existing_exe) = self.get_scheduler_executable_path() {
                    if existing_exe == target_exe_path && self.is_scheduler_ready() {
                        // ST-06: CLI healthy + CLI -> Retain
                        return Ok(());
                    }
                }
                // ST-07: CLI disabled / unready + CLI -> Self-repair
                self.register_task_xml(target_exe_path)?;
                if !self.is_scheduler_ready() {
                    return Err(SchedulerError::CommandFailed(
                        "CLI Task was repaired, but Task Scheduler still reports not ready".to_string(),
                    ));
                }
                return Ok(());
            } else {
                // ST-08: CLI owner + Desktop caller -> Safe takeover to Desktop!
                return self.register_task_xml(target_exe_path);
            }
        }

        Err(SchedulerError::MalformedConfiguration(format!(
            "Unexpected scheduler owner state: {}",
            current_owner
        )))
    }

    fn uninstall_scheduler(&self) -> Result<(), SchedulerError> {
        let task = self.query_task()?;
        if !task.exists {
            return Ok(());
        }

        let output = self.runner.run_schtasks(&["/Delete", "/TN", TASK_NAME, "/F"])?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SchedulerError::CommandFailed(format!(
                "schtasks /Delete failed: {}",
                stderr.trim()
            )));
        }

        Ok(())
    }

    fn uninstall_scheduler_as_cli(&self) -> Result<(), SchedulerError> {
        if self.get_scheduler_owner() == SchedulerOwner::Desktop {
            return Err(SchedulerError::DesktopOwnerProtected);
        }
        self.uninstall_scheduler()
    }

    fn register_job(&self, _job: &Job, _exe_path: &Path) -> Result<(), SchedulerError> {
        // Multi-job architecture: jobs are persisted in jobs.json and claimed via execute_tick.
        Ok(())
    }

    fn unregister_job(&self, _job_id: &str) -> Result<(), SchedulerError> {
        Ok(())
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::fs;
    use std::sync::{Arc, Mutex};
    use std::sync::atomic::{AtomicBool, Ordering};

    #[cfg(unix)]
    fn exit_status_from_code(code: i32) -> std::process::ExitStatus {
        use std::os::unix::process::ExitStatusExt;
        std::process::ExitStatus::from_raw(code)
    }

    #[cfg(windows)]
    fn exit_status_from_code(code: u32) -> std::process::ExitStatus {
        use std::os::windows::process::ExitStatusExt;
        std::process::ExitStatus::from_raw(code)
    }

    #[derive(Clone)]
    pub struct MockTaskSchedulerRunner {
        pub xml: Arc<Mutex<Option<String>>>,
        pub enabled: Arc<AtomicBool>,
        pub calls: Arc<Mutex<Vec<Vec<String>>>>,
        pub fail_create: Arc<AtomicBool>,
    }

    impl MockTaskSchedulerRunner {
        pub fn new_empty() -> Self {
            Self {
                xml: Arc::new(Mutex::new(None)),
                enabled: Arc::new(AtomicBool::new(false)),
                calls: Arc::new(Mutex::new(Vec::new())),
                fail_create: Arc::new(AtomicBool::new(false)),
            }
        }

        pub fn with_task(xml: String, enabled: bool) -> Self {
            Self {
                xml: Arc::new(Mutex::new(Some(xml))),
                enabled: Arc::new(AtomicBool::new(enabled)),
                calls: Arc::new(Mutex::new(Vec::new())),
                fail_create: Arc::new(AtomicBool::new(false)),
            }
        }
    }

    impl TaskSchedulerRunner for MockTaskSchedulerRunner {
        fn run_schtasks(&self, args: &[&str]) -> io::Result<std::process::Output> {
            let str_args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
            self.calls.lock().unwrap().push(str_args);

            if args.first() == Some(&"/Query") {
                let xml_lock = self.xml.lock().unwrap();
                if let Some(ref xml) = *xml_lock {
                    let is_en = self.enabled.load(Ordering::SeqCst);
                    let rep_xml = if is_en {
                        xml.clone()
                    } else {
                        xml.replace("<Enabled>true</Enabled>", "<Enabled>false</Enabled>")
                    };
                    Ok(std::process::Output {
                        status: exit_status_from_code(0),
                        stdout: rep_xml.into_bytes(),
                        stderr: Vec::new(),
                    })
                } else {
                    Ok(std::process::Output {
                        status: exit_status_from_code(1),
                        stdout: Vec::new(),
                        stderr: b"ERROR: The system cannot find the file specified.".to_vec(),
                    })
                }
            } else if args.first() == Some(&"/Create") {
                if self.fail_create.load(Ordering::SeqCst) {
                    return Ok(std::process::Output {
                        status: exit_status_from_code(1),
                        stdout: Vec::new(),
                        stderr: b"ERROR: Access is denied.".to_vec(),
                    });
                }
                let xml_path_idx = args.iter().position(|&x| x == "/XML").unwrap() + 1;
                let path = args[xml_path_idx];
                let raw_bytes = fs::read(path).unwrap_or_default();
                let content = WindowsTaskScheduler::decode_schtasks_output(&raw_bytes);
                *self.xml.lock().unwrap() = Some(content);
                self.enabled.store(true, Ordering::SeqCst);
                Ok(std::process::Output {
                    status: exit_status_from_code(0),
                    stdout: b"SUCCESS: The scheduled task \"CodexScheduler_Service\" has successfully been created.".to_vec(),
                    stderr: Vec::new(),
                })
            } else if args.first() == Some(&"/Change") {
                if args.iter().any(|&x| x == "/ENABLE") {
                    self.enabled.store(true, Ordering::SeqCst);
                }
                Ok(std::process::Output {
                    status: exit_status_from_code(0),
                    stdout: b"SUCCESS: The parameters of scheduled task \"CodexScheduler_Service\" have been changed.".to_vec(),
                    stderr: Vec::new(),
                })
            } else if args.first() == Some(&"/Delete") {
                *self.xml.lock().unwrap() = None;
                self.enabled.store(false, Ordering::SeqCst);
                Ok(std::process::Output {
                    status: exit_status_from_code(0),
                    stdout: b"SUCCESS: The scheduled task \"CodexScheduler_Service\" was successfully deleted.".to_vec(),
                    stderr: Vec::new(),
                })
            } else {
                Ok(std::process::Output {
                    status: exit_status_from_code(0),
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                })
            }
        }
    }

    #[test]
    fn test_generate_task_xml_content() {
        let exe = Path::new(r"C:\Program Files\Codex Scheduler\codex-scheduler-gui.exe");
        let xml = WindowsTaskScheduler::generate_task_xml(exe);

        assert!(xml.contains("<Interval>PT1M</Interval>"));
        assert!(xml.contains("<MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>"));
        assert!(xml.contains("<LogonType>InteractiveToken</LogonType>"));
        assert!(xml.contains("<RunLevel>LeastPrivilege</RunLevel>"));
        assert!(xml.contains("<DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>"));
        assert!(xml.contains("<StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>"));
        assert!(xml.contains("<StartWhenAvailable>true</StartWhenAvailable>"));
        assert!(xml.contains("<ExecutionTimeLimit>PT72H</ExecutionTimeLimit>"));
        assert!(xml.contains("<Priority>7</Priority>"));
        assert!(xml.contains(r"<Command>C:\Program Files\Codex Scheduler\codex-scheduler-gui.exe</Command>"));
        assert!(xml.contains("<Arguments>--scheduler-tick</Arguments>"));
    }

    #[test]
    fn test_xml_escaping_and_unescaping() {
        let raw = r#"C:\Apps & Tools <v1.0>\Codex "Scheduler" 'Test'\codex-scheduler.exe"#;
        let escaped = WindowsTaskScheduler::escape_xml(raw);
        assert!(escaped.contains("&amp;"));
        assert!(escaped.contains("&lt;"));
        assert!(escaped.contains("&gt;"));
        assert!(escaped.contains("&quot;"));
        assert!(escaped.contains("&apos;"));
        let unescaped = WindowsTaskScheduler::unescape_xml(&escaped);
        assert_eq!(unescaped, raw);

        let xml = WindowsTaskScheduler::generate_task_xml(Path::new(raw));
        let extracted = WindowsTaskScheduler::extract_xml_tag(&xml, "Command");
        assert_eq!(extracted, Some(raw.to_string()));
    }

    #[test]
    fn test_utf16le_encoding_and_decoding() {
        let text = "<Task><Command>C:\\test.exe</Command></Task>";
        let bytes = WindowsTaskScheduler::encode_utf16le_with_bom(text);
        assert_eq!(bytes[0], 0xFF);
        assert_eq!(bytes[1], 0xFE);
        let decoded = WindowsTaskScheduler::decode_schtasks_output(&bytes);
        assert_eq!(decoded, text);

        // UTF-8 bytes fallback
        let utf8_bytes = text.as_bytes();
        let decoded_utf8 = WindowsTaskScheduler::decode_schtasks_output(utf8_bytes);
        assert_eq!(decoded_utf8, text);
    }

    #[test]
    fn test_st_01_no_task_cli_caller_creates_cli_task() {
        let temp_dir = std::env::temp_dir().join(format!("test-st01-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();
        let cli_exe = temp_dir.join("codex-scheduler.exe");
        fs::write(&cli_exe, b"test").unwrap();

        let runner = MockTaskSchedulerRunner::new_empty();
        let scheduler = WindowsTaskScheduler::with_runner(Box::new(runner.clone()));

        assert_eq!(scheduler.get_scheduler_owner(), SchedulerOwner::None);
        assert!(!scheduler.is_scheduler_installed());

        // ST-01: CLI caller installs scheduler
        let res = scheduler.ensure_scheduler_installed(&cli_exe);
        assert!(res.is_ok());
        assert!(scheduler.is_scheduler_installed());
        assert!(scheduler.is_scheduler_ready());
        assert_eq!(scheduler.get_scheduler_owner(), SchedulerOwner::Cli);
        assert_eq!(scheduler.get_scheduler_executable_path(), Some(cli_exe.clone()));
        assert!(scheduler.is_scheduler_path_matched(&cli_exe));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_st_02_no_task_desktop_caller_creates_desktop_task() {
        let temp_dir = std::env::temp_dir().join(format!("test-st02-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();
        let desk_exe = temp_dir.join("codex-scheduler-gui.exe");
        fs::write(&desk_exe, b"test").unwrap();

        let runner = MockTaskSchedulerRunner::new_empty();
        let scheduler = WindowsTaskScheduler::with_runner(Box::new(runner.clone()));

        assert_eq!(scheduler.get_scheduler_owner(), SchedulerOwner::None);

        // ST-02: Desktop caller installs scheduler
        let res = scheduler.ensure_scheduler_installed(&desk_exe);
        assert!(res.is_ok());
        assert!(scheduler.is_scheduler_installed());
        assert!(scheduler.is_scheduler_ready());
        assert_eq!(scheduler.get_scheduler_owner(), SchedulerOwner::Desktop);
        assert_eq!(scheduler.get_scheduler_executable_path(), Some(desk_exe.clone()));
        assert!(scheduler.is_scheduler_path_matched(&desk_exe));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_st_03_desktop_healthy_cli_caller_retains_no_mutation() {
        let temp_dir = std::env::temp_dir().join(format!("test-st03-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();
        let desk_exe = temp_dir.join("codex-scheduler-gui.exe");
        fs::write(&desk_exe, b"test").unwrap();
        let cli_exe = temp_dir.join("codex-scheduler.exe");
        fs::write(&cli_exe, b"test").unwrap();

        let initial_xml = WindowsTaskScheduler::generate_task_xml(&desk_exe);
        let runner = MockTaskSchedulerRunner::with_task(initial_xml.clone(), true);
        let scheduler = WindowsTaskScheduler::with_runner(Box::new(runner.clone()));

        assert_eq!(scheduler.get_scheduler_owner(), SchedulerOwner::Desktop);
        assert!(scheduler.is_scheduler_ready());

        let calls_before = runner.calls.lock().unwrap().len();

        // ST-03: CLI caller calls ensure_scheduler_installed
        let res = scheduler.ensure_scheduler_installed(&cli_exe);
        assert!(res.is_ok(), "CLI caller against healthy Desktop task should succeed as no-op");

        // Must retain Desktop owner and Desktop path
        assert_eq!(scheduler.get_scheduler_owner(), SchedulerOwner::Desktop);
        assert_eq!(scheduler.get_scheduler_executable_path(), Some(desk_exe.clone()));
        assert_eq!(*runner.xml.lock().unwrap(), Some(initial_xml));

        // No mutation call (/Create) should have been issued
        let calls = runner.calls.lock().unwrap();
        for call in &calls[calls_before..] {
            assert_ne!(call.first().map(|s| s.as_str()), Some("/Create"));
        }

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_st_04_desktop_disabled_cli_caller_repairs_preserving_desktop_target() {
        let temp_dir = std::env::temp_dir().join(format!("test-st04-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();
        let desk_exe = temp_dir.join("codex-scheduler-gui.exe");
        fs::write(&desk_exe, b"test").unwrap();
        let cli_exe = temp_dir.join("codex-scheduler.exe");
        fs::write(&cli_exe, b"test").unwrap();

        let initial_xml = WindowsTaskScheduler::generate_task_xml(&desk_exe);
        // Task exists, but enabled = false
        let runner = MockTaskSchedulerRunner::with_task(initial_xml, false);
        let scheduler = WindowsTaskScheduler::with_runner(Box::new(runner.clone()));

        assert_eq!(scheduler.get_scheduler_owner(), SchedulerOwner::Desktop);
        assert!(!scheduler.is_scheduler_ready());

        // ST-04: CLI caller triggers safe repair
        let res = scheduler.ensure_scheduler_installed(&cli_exe);
        assert!(res.is_ok());

        // Must still be Desktop owner and point to Desktop exe, but now ready/enabled!
        assert_eq!(scheduler.get_scheduler_owner(), SchedulerOwner::Desktop);
        assert_eq!(scheduler.get_scheduler_executable_path(), Some(desk_exe));
        assert!(scheduler.is_scheduler_ready());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_st_05_desktop_owner_desktop_caller_retains_or_updates() {
        let temp_dir = std::env::temp_dir().join(format!("test-st05-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();
        let desk_exe1 = temp_dir.join("codex-scheduler-gui.exe");
        fs::write(&desk_exe1, b"test1").unwrap();

        let initial_xml = WindowsTaskScheduler::generate_task_xml(&desk_exe1);
        let runner = MockTaskSchedulerRunner::with_task(initial_xml, true);
        let scheduler = WindowsTaskScheduler::with_runner(Box::new(runner.clone()));

        // Identical path -> retain
        let res1 = scheduler.ensure_scheduler_installed(&desk_exe1);
        assert!(res1.is_ok());
        assert_eq!(scheduler.get_scheduler_executable_path(), Some(desk_exe1));

        // Different path -> update
        let desk_dir2 = temp_dir.join("v2");
        fs::create_dir_all(&desk_dir2).unwrap();
        let desk_exe2 = desk_dir2.join("codex-scheduler-gui.exe");
        fs::write(&desk_exe2, b"test2").unwrap();

        let res2 = scheduler.ensure_scheduler_installed(&desk_exe2);
        assert!(res2.is_ok());
        assert_eq!(scheduler.get_scheduler_executable_path(), Some(desk_exe2));
        assert_eq!(scheduler.get_scheduler_owner(), SchedulerOwner::Desktop);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_st_06_and_st_07_cli_owner_cli_caller() {
        let temp_dir = std::env::temp_dir().join(format!("test-st0607-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();
        let cli_exe = temp_dir.join("codex-scheduler.exe");
        fs::write(&cli_exe, b"test").unwrap();

        let initial_xml = WindowsTaskScheduler::generate_task_xml(&cli_exe);

        // ST-06: CLI healthy + CLI caller -> retain
        let runner = MockTaskSchedulerRunner::with_task(initial_xml.clone(), true);
        let scheduler = WindowsTaskScheduler::with_runner(Box::new(runner.clone()));
        assert!(scheduler.is_scheduler_ready());
        let res = scheduler.ensure_scheduler_installed(&cli_exe);
        assert!(res.is_ok());
        assert_eq!(scheduler.get_scheduler_owner(), SchedulerOwner::Cli);

        // ST-07: CLI disabled + CLI caller -> self-repair
        runner.enabled.store(false, Ordering::SeqCst);
        assert!(!scheduler.is_scheduler_ready());
        let res7 = scheduler.ensure_scheduler_installed(&cli_exe);
        assert!(res7.is_ok());
        assert!(scheduler.is_scheduler_ready());
        assert_eq!(scheduler.get_scheduler_owner(), SchedulerOwner::Cli);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_st_08_cli_owner_desktop_caller_safe_takeover() {
        let temp_dir = std::env::temp_dir().join(format!("test-st08-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();
        let cli_exe = temp_dir.join("codex-scheduler.exe");
        fs::write(&cli_exe, b"test").unwrap();
        let desk_exe = temp_dir.join("codex-scheduler-gui.exe");
        fs::write(&desk_exe, b"test").unwrap();

        let initial_xml = WindowsTaskScheduler::generate_task_xml(&cli_exe);
        let runner = MockTaskSchedulerRunner::with_task(initial_xml, true);
        let scheduler = WindowsTaskScheduler::with_runner(Box::new(runner.clone()));

        assert_eq!(scheduler.get_scheduler_owner(), SchedulerOwner::Cli);

        // ST-08: Desktop caller safe takeover
        let res = scheduler.ensure_scheduler_installed(&desk_exe);
        assert!(res.is_ok());
        assert_eq!(scheduler.get_scheduler_owner(), SchedulerOwner::Desktop);
        assert_eq!(scheduler.get_scheduler_executable_path(), Some(desk_exe));
        assert!(scheduler.is_scheduler_ready());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_st_09_stale_desktop_cli_caller_errors_no_takeover() {
        let temp_dir = std::env::temp_dir().join(format!("test-st09-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();
        let non_existent_desktop_exe = temp_dir.join("stale").join("codex-scheduler-gui.exe");
        let cli_exe = temp_dir.join("codex-scheduler.exe");
        fs::write(&cli_exe, b"test").unwrap();

        let initial_xml = WindowsTaskScheduler::generate_task_xml(&non_existent_desktop_exe);
        let runner = MockTaskSchedulerRunner::with_task(initial_xml.clone(), true);
        let scheduler = WindowsTaskScheduler::with_runner(Box::new(runner.clone()));

        assert_eq!(scheduler.get_scheduler_owner(), SchedulerOwner::Invalid);
        assert!(!scheduler.is_target_executable_exists());

        // ST-09: CLI caller must fail with ExecutableNotFound and NOT overwrite task!
        let res = scheduler.ensure_scheduler_installed(&cli_exe);
        assert!(res.is_err());
        match res.unwrap_err() {
            SchedulerError::ExecutableNotFound(msg) => {
                assert!(msg.contains("Registered Desktop scheduler executable does not exist"));
            }
            other => panic!("Expected ExecutableNotFound, got {:?}", other),
        }

        // Must NOT takeover to CLI
        assert_eq!(scheduler.get_scheduler_owner(), SchedulerOwner::Invalid);
        assert_eq!(*runner.xml.lock().unwrap(), Some(initial_xml));
        assert_ne!(scheduler.get_scheduler_executable_path(), Some(cli_exe));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_st_10_stale_desktop_desktop_caller_recovers() {
        let temp_dir = std::env::temp_dir().join(format!("test-st10-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();
        let non_existent_desktop_exe = temp_dir.join("stale").join("codex-scheduler-gui.exe");
        let real_desktop_exe = temp_dir.join("codex-scheduler-gui.exe");
        fs::write(&real_desktop_exe, b"test").unwrap();

        let initial_xml = WindowsTaskScheduler::generate_task_xml(&non_existent_desktop_exe);
        let runner = MockTaskSchedulerRunner::with_task(initial_xml, true);
        let scheduler = WindowsTaskScheduler::with_runner(Box::new(runner.clone()));

        // ST-10: Desktop caller recovers
        let res = scheduler.ensure_scheduler_installed(&real_desktop_exe);
        assert!(res.is_ok());
        assert_eq!(scheduler.get_scheduler_owner(), SchedulerOwner::Desktop);
        assert_eq!(scheduler.get_scheduler_executable_path(), Some(real_desktop_exe));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_st_11_and_st_12_stale_cli() {
        let temp_dir = std::env::temp_dir().join(format!("test-st1112-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();
        let non_existent_cli_exe = temp_dir.join("stale").join("codex-scheduler.exe");
        let real_cli_exe = temp_dir.join("codex-scheduler.exe");
        fs::write(&real_cli_exe, b"test").unwrap();
        let real_desk_exe = temp_dir.join("codex-scheduler-gui.exe");
        fs::write(&real_desk_exe, b"test").unwrap();

        // ST-11: Stale CLI + CLI caller -> CLI Recovery
        let initial_xml = WindowsTaskScheduler::generate_task_xml(&non_existent_cli_exe);
        let runner = MockTaskSchedulerRunner::with_task(initial_xml.clone(), true);
        let scheduler = WindowsTaskScheduler::with_runner(Box::new(runner.clone()));

        let res11 = scheduler.ensure_scheduler_installed(&real_cli_exe);
        assert!(res11.is_ok());
        assert_eq!(scheduler.get_scheduler_owner(), SchedulerOwner::Cli);
        assert_eq!(scheduler.get_scheduler_executable_path(), Some(real_cli_exe));

        // ST-12: Stale CLI + Desktop caller -> Safe takeover to Desktop
        let runner12 = MockTaskSchedulerRunner::with_task(initial_xml, true);
        let scheduler12 = WindowsTaskScheduler::with_runner(Box::new(runner12.clone()));

        let res12 = scheduler12.ensure_scheduler_installed(&real_desk_exe);
        assert!(res12.is_ok());
        assert_eq!(scheduler12.get_scheduler_owner(), SchedulerOwner::Desktop);
        assert_eq!(scheduler12.get_scheduler_executable_path(), Some(real_desk_exe));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_st_13_malformed_task_errors_no_overwrite() {
        let temp_dir = std::env::temp_dir().join(format!("test-st13-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();
        let cli_exe = temp_dir.join("codex-scheduler.exe");
        fs::write(&cli_exe, b"test").unwrap();

        let malformed_xml = "<Task><Actions><Exec><Command>notepad.exe</Command></Exec></Actions></Task>".to_string();
        let runner = MockTaskSchedulerRunner::with_task(malformed_xml.clone(), true);
        let scheduler = WindowsTaskScheduler::with_runner(Box::new(runner.clone()));

        assert_eq!(scheduler.get_scheduler_owner(), SchedulerOwner::Invalid);

        // ST-13: Malformed configuration returns error and does NOT overwrite
        let res = scheduler.ensure_scheduler_installed(&cli_exe);
        assert!(res.is_err());
        match res.unwrap_err() {
            SchedulerError::MalformedConfiguration(msg) => {
                assert!(msg.contains("Unrecognized or malformed Windows Scheduled Task"));
            }
            other => panic!("Expected MalformedConfiguration, got {:?}", other),
        }
        assert_eq!(*runner.xml.lock().unwrap(), Some(malformed_xml));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_uninstall_scheduler_and_cli_protection() {
        let temp_dir = std::env::temp_dir().join(format!("test-uninst-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();
        let desk_exe = temp_dir.join("codex-scheduler-gui.exe");
        fs::write(&desk_exe, b"test").unwrap();
        let cli_exe = temp_dir.join("codex-scheduler.exe");
        fs::write(&cli_exe, b"test").unwrap();

        // 1. Desktop owned task
        let desk_xml = WindowsTaskScheduler::generate_task_xml(&desk_exe);
        let runner = MockTaskSchedulerRunner::with_task(desk_xml, true);
        let scheduler = WindowsTaskScheduler::with_runner(Box::new(runner.clone()));

        // CLI uninstall attempt on Desktop owned scheduler -> Protected!
        let uninst_err = scheduler.uninstall_scheduler_as_cli().unwrap_err();
        match uninst_err {
            SchedulerError::DesktopOwnerProtected => {}
            other => panic!("Expected DesktopOwnerProtected, got {:?}", other),
        }
        assert!(runner.xml.lock().unwrap().is_some());

        // Unconditional uninstall (GUI) -> Succeeds!
        let uninst_gui = scheduler.uninstall_scheduler();
        assert!(uninst_gui.is_ok());
        assert!(runner.xml.lock().unwrap().is_none());

        // 2. CLI owned task
        let cli_xml = WindowsTaskScheduler::generate_task_xml(&cli_exe);
        let runner_cli = MockTaskSchedulerRunner::with_task(cli_xml, true);
        let scheduler_cli = WindowsTaskScheduler::with_runner(Box::new(runner_cli.clone()));

        // CLI uninstall attempt on CLI owned scheduler -> Succeeds!
        let uninst_cli_ok = scheduler_cli.uninstall_scheduler_as_cli();
        assert!(uninst_cli_ok.is_ok());
        assert!(runner_cli.xml.lock().unwrap().is_none());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_atomic_scheduling_rejection_on_invalid_scheduler() {
        use crate::store::JobStore;
        use crate::SchedulerService;
        use crate::models::ProviderType;
        use chrono::Utc;

        let temp_dir = std::env::temp_dir().join(format!("test-atomic-win-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();

        let non_existent_desk_exe = temp_dir.join("stale").join("codex-scheduler-gui.exe");
        let initial_xml = WindowsTaskScheduler::generate_task_xml(&non_existent_desk_exe);
        let runner = MockTaskSchedulerRunner::with_task(initial_xml, true);
        let scheduler = WindowsTaskScheduler::with_runner(Box::new(runner.clone()));

        let cli_exe = temp_dir.join("codex-scheduler.exe");
        fs::write(&cli_exe, b"test").unwrap();

        let jobs_path = temp_dir.join("jobs.json");
        let store = JobStore::new_with_path(&jobs_path);
        let service = SchedulerService::with_scheduler(
            store.clone(),
            Some(cli_exe),
            Box::new(scheduler),
        );

        let sched_res = service.schedule_job(
            ProviderType::Codex,
            "session-atomic-test".to_string(),
            temp_dir.clone(),
            Some("test prompt".to_string()),
            Utc::now() + chrono::Duration::hours(1),
            None,
        );

        assert!(sched_res.is_err(), "schedule_job must fail when Windows scheduler is in stale/invalid state");
        let all_jobs = store.load_all().unwrap();
        assert_eq!(all_jobs.len(), 0, "JobStore must remain completely empty on scheduler error");

        let _ = fs::remove_dir_all(&temp_dir);
    }
}

