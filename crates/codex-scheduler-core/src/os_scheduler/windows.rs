use crate::models::Job;
use crate::os_scheduler::{SchedulerBackend, SchedulerError, SchedulerOwner};
use std::path::{Path, PathBuf};

pub const TASK_NAME: &str = "CodexScheduler_Service";
pub const CANONICAL_DESKTOP_EXE: &str = "codex-scheduler-gui.exe";
pub const CANONICAL_CLI_EXE: &str = "codex-scheduler.exe";
pub const LEGACY_CLI_EXE: &str = "codex-scheduler-cli.exe";
pub const TICK_ARG: &str = "--scheduler-tick";

/// Abstraction for scheduled task metadata returned by TaskSchedulerRunner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledTaskDetails {
    pub exists: bool,
    pub enabled: bool,
    pub last_result: i32,
    pub xml: Option<String>,
}

/// Abstraction for Task Scheduler operations to allow deterministic, non-destructive testing.
pub trait TaskSchedulerRunner: Send + Sync {
    fn query_task(&self, task_name: &str) -> Result<Option<ScheduledTaskDetails>, SchedulerError>;
    fn register_task(&self, task_name: &str, xml: &str) -> Result<(), SchedulerError>;
    fn delete_task(&self, task_name: &str) -> Result<(), SchedulerError>;
}

#[cfg(target_os = "windows")]
mod com_impl {
    use super::*;
    use windows::Win32::System::TaskScheduler::{
        TaskScheduler, ITaskService, ITaskFolder,
        TASK_CREATE_OR_UPDATE, TASK_LOGON_INTERACTIVE_TOKEN,
        TASK_STATE_DISABLED,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_MULTITHREADED,
    };


    use windows::core::BSTR;

    struct ComGuard;
    impl ComGuard {
        fn new() -> Self {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            }
            ComGuard
        }
    }
    impl Drop for ComGuard {
        fn drop(&mut self) {
            unsafe {
                CoUninitialize();
            }
        }
    }

    pub fn query_task_com(task_name: &str) -> Result<Option<ScheduledTaskDetails>, SchedulerError> {
        let _guard = ComGuard::new();
        unsafe {
            let service: ITaskService = CoCreateInstance(&TaskScheduler, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| SchedulerError::CommandFailed(format!("CoCreateInstance TaskScheduler failed: {e}")))?;
            service.Connect(None, None, None, None)
                .map_err(|e| SchedulerError::CommandFailed(format!("ITaskService::Connect failed: {e}")))?;
            let root_folder: ITaskFolder = service.GetFolder(&BSTR::from("\\"))
                .map_err(|e| SchedulerError::CommandFailed(format!("ITaskService::GetFolder failed: {e}")))?;

            match root_folder.GetTask(&BSTR::from(task_name)) {
                Ok(task) => {
                    let xml = task.Xml().map(|b| b.to_string()).ok();
                    let state = task.State().unwrap_or_default();
                    let enabled = state != TASK_STATE_DISABLED;
                    let last_result = task.LastTaskResult().unwrap_or(0);
                    Ok(Some(ScheduledTaskDetails {
                        exists: true,
                        enabled,
                        last_result,
                        xml,
                    }))
                }
                Err(e) => {
                    if e.code().0 as u32 == 0x80070002 {
                        Ok(None)
                    } else {
                        Err(SchedulerError::CommandFailed(format!("ITaskFolder::GetTask failed: {e}")))
                    }
                }
            }
        }
    }

    pub fn register_task_com(task_name: &str, xml: &str) -> Result<(), SchedulerError> {
        let _guard = ComGuard::new();
        unsafe {
            let service: ITaskService = CoCreateInstance(&TaskScheduler, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| SchedulerError::CommandFailed(format!("CoCreateInstance TaskScheduler failed: {e}")))?;
            service.Connect(None, None, None, None)
                .map_err(|e| SchedulerError::CommandFailed(format!("ITaskService::Connect failed: {e}")))?;
            let root_folder: ITaskFolder = service.GetFolder(&BSTR::from("\\"))
                .map_err(|e| SchedulerError::CommandFailed(format!("ITaskService::GetFolder failed: {e}")))?;

            let flags = TASK_CREATE_OR_UPDATE.0;
            let logon_type = TASK_LOGON_INTERACTIVE_TOKEN;

            root_folder.RegisterTask(
                &BSTR::from(task_name),
                &BSTR::from(xml),
                flags,
                None,
                None,
                logon_type,
                None,
            ).map_err(|e| SchedulerError::CommandFailed(format!("ITaskFolder::RegisterTask failed: {e}")))?;

            Ok(())


        }
    }

    pub fn delete_task_com(task_name: &str) -> Result<(), SchedulerError> {
        let _guard = ComGuard::new();
        unsafe {
            let service: ITaskService = CoCreateInstance(&TaskScheduler, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| SchedulerError::CommandFailed(format!("CoCreateInstance TaskScheduler failed: {e}")))?;
            service.Connect(None, None, None, None)
                .map_err(|e| SchedulerError::CommandFailed(format!("ITaskService::Connect failed: {e}")))?;
            let root_folder: ITaskFolder = service.GetFolder(&BSTR::from("\\"))
                .map_err(|e| SchedulerError::CommandFailed(format!("ITaskService::GetFolder failed: {e}")))?;

            match root_folder.DeleteTask(&BSTR::from(task_name), 0) {
                Ok(_) => Ok(()),
                Err(e) => {
                    if e.code().0 as u32 == 0x80070002 {
                        Ok(())
                    } else {
                        Err(SchedulerError::CommandFailed(format!("ITaskFolder::DeleteTask failed: {e}")))
                    }
                }
            }
        }
    }
}

/// Production runner that invokes Windows Task Scheduler 2.0 COM API directly.
#[derive(Default)]
pub struct RealTaskSchedulerRunner;

impl TaskSchedulerRunner for RealTaskSchedulerRunner {
    #[cfg(target_os = "windows")]
    fn query_task(&self, task_name: &str) -> Result<Option<ScheduledTaskDetails>, SchedulerError> {
        com_impl::query_task_com(task_name)
    }

    #[cfg(not(target_os = "windows"))]
    fn query_task(&self, _task_name: &str) -> Result<Option<ScheduledTaskDetails>, SchedulerError> {
        Ok(None)
    }

    #[cfg(target_os = "windows")]
    fn register_task(&self, task_name: &str, xml: &str) -> Result<(), SchedulerError> {
        com_impl::register_task_com(task_name, xml)
    }

    #[cfg(not(target_os = "windows"))]
    fn register_task(&self, _task_name: &str, _xml: &str) -> Result<(), SchedulerError> {
        Err(SchedulerError::CommandFailed(
            "Task Scheduler COM API is only available on Windows".to_string(),
        ))
    }

    #[cfg(target_os = "windows")]
    fn delete_task(&self, task_name: &str) -> Result<(), SchedulerError> {
        com_impl::delete_task_com(task_name)
    }

    #[cfg(not(target_os = "windows"))]
    fn delete_task(&self, _task_name: &str) -> Result<(), SchedulerError> {
        Err(SchedulerError::CommandFailed(
            "Task Scheduler COM API is only available on Windows".to_string(),
        ))
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
    pub last_result: i32,
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

    /// Queries the Task Scheduler for the current task definition and parses it.
    pub fn query_task(&self) -> Result<TaskInfo, SchedulerError> {
        let details = self.runner.query_task(TASK_NAME)?;
        match details {
            Some(d) if d.exists => {
                let xml = d.xml;
                let command = xml.as_deref().and_then(|x| Self::extract_xml_tag(x, "Command")).map(PathBuf::from);
                let arguments = xml.as_deref().and_then(|x| Self::extract_xml_tag(x, "Arguments"));
                Ok(TaskInfo {
                    exists: true,
                    enabled: d.enabled,
                    last_result: d.last_result,
                    command,
                    arguments,
                    raw_xml: xml,
                })
            }
            _ => Ok(TaskInfo {
                exists: false,
                enabled: false,
                last_result: 0,
                command: None,
                arguments: None,
                raw_xml: None,
            }),
        }
    }

    /// Registers or updates the task using an XML definition via COM API.
    pub fn register_task_xml(&self, exe_path: &Path) -> Result<(), SchedulerError> {
        let xml_content = Self::generate_task_xml(exe_path);
        self.runner.register_task(TASK_NAME, &xml_content)
    }

    fn canonical_path_str(path: &Path) -> String {
        let s = path.to_string_lossy().to_string();
        s.replace('/', "\\").to_ascii_lowercase()
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
        let owner = self.get_scheduler_owner();
        if owner != SchedulerOwner::Desktop && owner != SchedulerOwner::Cli {
            return false;
        }

        let task = match self.query_task() {
            Ok(t) => t,
            Err(_) => return false,
        };
        if !task.exists || !task.enabled {
            return false;
        }

        // Arguments must contain canonical --scheduler-tick
        task.arguments.as_deref() == Some(TICK_ARG)
    }


    fn is_scheduler_path_matched(&self, exe_path: &Path) -> bool {
        let task = match self.query_task() {
            Ok(t) => t,
            Err(_) => return false,
        };
        if let Some(cmd) = task.command {
            Self::canonical_path_str(&cmd) == Self::canonical_path_str(exe_path)
        } else {
            false
        }
    }

    fn is_target_executable_exists(&self) -> bool {
        self.query_task()
            .ok()
            .and_then(|t| t.command)
            .map(|p| p.is_absolute() && p.exists())
            .unwrap_or(false)
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

        let args = task.arguments.unwrap_or_default();
        if !args.contains(TICK_ARG) {
            return SchedulerOwner::Invalid;
        }

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
        self.runner.delete_task(TASK_NAME)
    }

    fn uninstall_scheduler_as_cli(&self) -> Result<(), SchedulerError> {
        if self.get_scheduler_owner() == SchedulerOwner::Desktop {
            return Err(SchedulerError::DesktopOwnerProtected);
        }
        self.uninstall_scheduler()
    }

    fn register_job(&self, _job: &Job, _exe_path: &Path) -> Result<(), SchedulerError> {
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

    #[derive(Clone, Default)]
    pub struct MockTaskSchedulerRunner {
        pub task: Arc<Mutex<Option<ScheduledTaskDetails>>>,
        pub calls: Arc<Mutex<Vec<String>>>,
    }

    impl MockTaskSchedulerRunner {
        pub fn new_empty() -> Self {
            Self::default()
        }

        pub fn with_task(xml: String, enabled: bool) -> Self {
            let runner = Self::default();
            *runner.task.lock().unwrap() = Some(ScheduledTaskDetails {
                exists: true,
                enabled,
                last_result: 0,
                xml: Some(xml),
            });
            runner
        }

        pub fn with_task_and_result(xml: String, enabled: bool, last_result: i32) -> Self {
            let runner = Self::default();
            *runner.task.lock().unwrap() = Some(ScheduledTaskDetails {
                exists: true,
                enabled,
                last_result,
                xml: Some(xml),
            });
            runner
        }
    }

    impl TaskSchedulerRunner for MockTaskSchedulerRunner {
        fn query_task(&self, task_name: &str) -> Result<Option<ScheduledTaskDetails>, SchedulerError> {
            self.calls.lock().unwrap().push(format!("query:{}", task_name));
            Ok(self.task.lock().unwrap().clone())
        }

        fn register_task(&self, task_name: &str, xml: &str) -> Result<(), SchedulerError> {
            self.calls.lock().unwrap().push(format!("register:{}", task_name));
            *self.task.lock().unwrap() = Some(ScheduledTaskDetails {
                exists: true,
                enabled: true,
                last_result: 0,
                xml: Some(xml.to_string()),
            });
            Ok(())
        }

        fn delete_task(&self, task_name: &str) -> Result<(), SchedulerError> {
            self.calls.lock().unwrap().push(format!("delete:{}", task_name));
            *self.task.lock().unwrap() = None;
            Ok(())
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

        // No mutation call (register) should have been issued
        let calls = runner.calls.lock().unwrap();
        for call in &calls[calls_before..] {
            assert!(!call.starts_with("register"));
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
        if let Some(ref mut d) = *runner.task.lock().unwrap() {
            d.enabled = false;
        }
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
        let task_xml = runner.task.lock().unwrap().as_ref().and_then(|t| t.xml.clone());
        assert_eq!(task_xml, Some(malformed_xml));

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
        assert!(runner.task.lock().unwrap().is_some());

        // Unconditional uninstall (GUI) -> Succeeds!
        let uninst_gui = scheduler.uninstall_scheduler();
        assert!(uninst_gui.is_ok());
        assert!(runner.task.lock().unwrap().is_none());

        // 2. CLI owned task
        let cli_xml = WindowsTaskScheduler::generate_task_xml(&cli_exe);
        let runner_cli = MockTaskSchedulerRunner::with_task(cli_xml, true);
        let scheduler_cli = WindowsTaskScheduler::with_runner(Box::new(runner_cli.clone()));

        // CLI uninstall attempt on CLI owned scheduler -> Succeeds!
        let uninst_cli_ok = scheduler_cli.uninstall_scheduler_as_cli();
        assert!(uninst_cli_ok.is_ok());
        assert!(runner_cli.task.lock().unwrap().is_none());

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
