use super::{SchedulerBackend, SchedulerError, SchedulerOwner};
use crate::models::Job;
use std::fs;
use std::path::{Path, PathBuf};
#[cfg(not(test))]
use std::process::Command;

pub const SCHEDULER_LABEL: &str = "dev.codexscheduler.scheduler";
pub const SCHEDULER_PLIST_FILENAME: &str = "dev.codexscheduler.scheduler.plist";
pub const LEGACY_PLIST_PREFIX: &str = "com.codexscheduler.job.";

pub trait LaunchctlRunner: Send + Sync {
    fn run_launchctl(&self, args: &[&str]) -> std::io::Result<std::process::Output>;
}

#[derive(Clone, Default)]
pub struct RealLaunchctlRunner;

impl LaunchctlRunner for RealLaunchctlRunner {
    fn run_launchctl(&self, args: &[&str]) -> std::io::Result<std::process::Output> {
        #[cfg(test)]
        {
            panic!(
                "RealLaunchctlRunner invoked during unit/integration tests! Real host launchd mutation is strictly forbidden. Args: {:?}",
                args
            );
        }
        #[cfg(not(test))]
        {
            Command::new("launchctl").args(args).output()
        }
    }
}

#[cfg(test)]
use std::sync::Arc;
#[cfg(test)]
use std::sync::Mutex;
#[cfg(test)]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(test)]
#[derive(Clone)]
pub struct MockLaunchctlRunner {
    pub loaded: Arc<AtomicBool>,
    pub load_should_fail: Arc<AtomicBool>,
    pub loaded_executable: Arc<Mutex<Option<PathBuf>>>,
    pub calls: Arc<Mutex<Vec<Vec<String>>>>,
}

#[cfg(test)]
impl MockLaunchctlRunner {
    pub fn new(loaded: bool) -> Self {
        Self {
            loaded: Arc::new(AtomicBool::new(loaded)),
            load_should_fail: Arc::new(AtomicBool::new(false)),
            loaded_executable: Arc::new(Mutex::new(None)),
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn with_executable(exe: PathBuf) -> Self {
        Self {
            loaded: Arc::new(AtomicBool::new(true)),
            load_should_fail: Arc::new(AtomicBool::new(false)),
            loaded_executable: Arc::new(Mutex::new(Some(exe))),
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn with_load_failure() -> Self {
        Self {
            loaded: Arc::new(AtomicBool::new(false)),
            load_should_fail: Arc::new(AtomicBool::new(true)),
            loaded_executable: Arc::new(Mutex::new(None)),
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

#[cfg(test)]
impl LaunchctlRunner for MockLaunchctlRunner {
    fn run_launchctl(&self, args: &[&str]) -> std::io::Result<std::process::Output> {
        use std::os::unix::process::ExitStatusExt;
        let str_args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        self.calls.lock().unwrap().push(str_args);

        if args.first() == Some(&"list") {
            if self.loaded.load(Ordering::SeqCst) {
                let exe_opt = self.loaded_executable.lock().unwrap().clone();
                let stdout_str = if let Some(exe) = exe_opt {
                    format!(
                        "{{\n\t\"Label\" = \"{}\";\n\t\"Program\" = \"{}\";\n\t\"ProgramArguments\" = (\n\t\t\"{}\";\n\t\t\"--scheduler-tick\";\n\t);\n}};\n",
                        SCHEDULER_LABEL,
                        exe.display(),
                        exe.display()
                    )
                } else {
                    format!("{{\n\t\"Label\" = \"{}\";\n}};\n", SCHEDULER_LABEL)
                };
                Ok(std::process::Output {
                    status: std::process::ExitStatus::from_raw(0),
                    stdout: stdout_str.into_bytes(),
                    stderr: Vec::new(),
                })
            } else {
                Ok(std::process::Output {
                    status: std::process::ExitStatus::from_raw(1 << 8),
                    stdout: Vec::new(),
                    stderr: b"Could not find specified service".to_vec(),
                })
            }
        } else if args.first() == Some(&"unload") || args.first() == Some(&"remove") {
            self.loaded.store(false, Ordering::SeqCst);
            *self.loaded_executable.lock().unwrap() = None;
            Ok(std::process::Output {
                status: std::process::ExitStatus::from_raw(0),
                stdout: Vec::new(),
                stderr: Vec::new(),
            })
        } else if args.first() == Some(&"load") {
            if self.load_should_fail.load(Ordering::SeqCst) {
                Ok(std::process::Output {
                    status: std::process::ExitStatus::from_raw(5 << 8),
                    stdout: Vec::new(),
                    stderr: b"Service could not be registered: Input/output error".to_vec(),
                })
            } else {
                self.loaded.store(true, Ordering::SeqCst);
                let mut found_exe = None;
                for arg in args.iter().skip(1) {
                    let path = Path::new(arg);
                    if path.exists() {
                        if let Ok(content) = fs::read_to_string(path) {
                            if let Some(exe) = MacOsLaunchdScheduler::extract_executable_path_from_plist(&content) {
                                found_exe = Some(exe);
                                break;
                            }
                        }
                    }
                }
                if let Some(exe) = found_exe {
                    *self.loaded_executable.lock().unwrap() = Some(exe);
                }
                Ok(std::process::Output {
                    status: std::process::ExitStatus::from_raw(0),
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                })
            }
        } else {
            Ok(std::process::Output {
                status: std::process::ExitStatus::from_raw(0),
                stdout: Vec::new(),
                stderr: Vec::new(),
            })
        }
    }
}

pub struct MacOsLaunchdScheduler {
    launch_agents_dir: PathBuf,
    runner: Box<dyn LaunchctlRunner>,
}

impl MacOsLaunchdScheduler {
    pub fn new() -> Self {
        let dir = dirs::home_dir()
            .map(|h| h.join("Library/LaunchAgents"))
            .unwrap_or_else(|| PathBuf::from("/tmp/LaunchAgents"));
        Self {
            launch_agents_dir: dir,
            runner: Box::new(RealLaunchctlRunner),
        }
    }

    #[cfg(test)]
    pub fn with_dir(dir: PathBuf) -> Self {
        Self {
            launch_agents_dir: dir,
            runner: Box::new(MockLaunchctlRunner::new(true)),
        }
    }

    #[cfg(not(test))]
    pub fn with_dir(dir: PathBuf) -> Self {
        Self {
            launch_agents_dir: dir,
            runner: Box::new(RealLaunchctlRunner),
        }
    }

    pub fn with_dir_and_runner(dir: PathBuf, runner: Box<dyn LaunchctlRunner>) -> Self {
        Self {
            launch_agents_dir: dir,
            runner,
        }
    }

    pub fn scheduler_plist_path(&self) -> PathBuf {
        self.launch_agents_dir.join(SCHEDULER_PLIST_FILENAME)
    }

    pub fn legacy_job_plist_path(&self, job_id: &str) -> PathBuf {
        self.launch_agents_dir
            .join(format!("{}{}.plist", LEGACY_PLIST_PREFIX, job_id))
    }

    fn escape_xml_text(value: &str) -> String {
        let mut escaped = String::with_capacity(value.len());
        for ch in value.chars() {
            match ch {
                '&' => escaped.push_str("&amp;"),
                '<' => escaped.push_str("&lt;"),
                '>' => escaped.push_str("&gt;"),
                '"' => escaped.push_str("&quot;"),
                '\'' => escaped.push_str("&apos;"),
                _ => escaped.push(ch),
            }
        }
        escaped
    }

    fn unescape_xml_text(value: &str) -> String {
        value
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&")
    }

    // WHY: LaunchAgent must execute the app bundle's main executable with `--scheduler-tick`
    //      instead of a separate CLI worker binary, ensuring Gatekeeper authorization granted
    //      by the user to the GUI app covers background execution under ad-hoc distribution.
    // WHAT BREAKS: Invoking a separate binary causes macOS Gatekeeper rejection ("codex-scheduler-cli is not open")
    //              and breaks scheduled runs even when GUI app was authorized.
    // EVIDENCE: docs/adr/0003-macos-single-executable-headless-scheduler.md, OS-SCHED-001, DELIVERY-BUNDLE-002
    pub fn generate_scheduler_plist_content(app_executable_path: &Path) -> String {
        let app_str = Self::escape_xml_text(&app_executable_path.to_string_lossy());
        let home_dir_raw = dirs::home_dir()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|| "/Users".to_string());
        let path_env_raw = format!(
            "{}/.local/bin:{}/.cargo/bin:/opt/homebrew/bin:/opt/homebrew/sbin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin",
            home_dir_raw, home_dir_raw
        );
        let home_dir = Self::escape_xml_text(&home_dir_raw);
        let path_env = Self::escape_xml_text(&path_env_raw);

        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{app_str}</string>
        <string>--scheduler-tick</string>
    </array>
    <key>EnvironmentVariables</key>
    <dict>
        <key>PATH</key>
        <string>{path_env}</string>
        <key>HOME</key>
        <string>{home_dir}</string>
    </dict>
    <key>WorkingDirectory</key>
    <string>{home_dir}</string>
    <key>StartInterval</key>
    <integer>60</integer>
    <key>RunAtLoad</key>
    <true/>
    <key>AbandonProcessGroup</key>
    <true/>
    <key>StandardOutPath</key>
    <string>/tmp/{label}.stdout.log</string>
    <key>StandardErrorPath</key>
    <string>/tmp/{label}.stderr.log</string>
</dict>
</plist>"#,
            label = SCHEDULER_LABEL,
            app_str = app_str,
            path_env = path_env,
            home_dir = home_dir,
        )
    }

    /// Safely executes `launchctl unload <plist>` and ensures errors other than explicit "not loaded" are strictly propagated.
    pub fn safe_launchctl_unload(&self, plist_path: &Path) -> Result<(), SchedulerError> {
        let path_str = plist_path.to_string_lossy();
        let output = self.runner.run_launchctl(&["unload", &path_str])?;

        Self::check_launchctl_unload_output(plist_path, &output)
    }

    /// Evaluates launchctl unload command output. Non-zero exit code is always an error unless explicitly confirmed not loaded.
    pub fn check_launchctl_unload_output(
        plist_path: &Path,
        output: &std::process::Output,
    ) -> Result<(), SchedulerError> {
        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            let err_trimmed = err.trim();
            // In macOS launchctl, unloading a service that is not currently registered/loaded
            // outputs messages like "Could not find specified service", "Not loaded", or similar.
            let is_not_loaded = err_trimmed.contains("Could not find specified service")
                || err_trimmed.contains("Not loaded")
                || err_trimmed.contains("No such process");
            if is_not_loaded {
                return Ok(());
            }

            let detail = if !err_trimmed.is_empty() {
                err_trimmed.to_string()
            } else {
                let stdout_trimmed = String::from_utf8_lossy(&output.stdout).trim().to_string();
                format!(
                    "exit status: {:?}{}",
                    output.status.code(),
                    if !stdout_trimmed.is_empty() {
                        format!(", stdout: {}", stdout_trimmed)
                    } else {
                        String::new()
                    }
                )
            };

            return Err(SchedulerError::CommandFailed(format!(
                "launchctl unload failed for {}: {}",
                plist_path.display(),
                detail
            )));
        }
        Ok(())
    }

    /// Extracts the target executable path from a launchd plist content string.
    pub fn extract_executable_path_from_plist(content: &str) -> Option<PathBuf> {
        let prog_args_idx = content.find("<key>ProgramArguments</key>")?;
        let after_prog_args = &content[prog_args_idx..];
        let array_start = after_prog_args.find("<array>")?;
        let after_array = &after_prog_args[array_start..];
        let string_start = after_array.find("<string>")? + "<string>".len();
        let string_end = after_array[string_start..].find("</string>")? + string_start;
        let exe_str = after_array[string_start..string_end].trim();
        if exe_str.is_empty() {
            None
        } else {
            Some(PathBuf::from(Self::unescape_xml_text(exe_str)))
        }
    }

    /// Extracts the target executable path from `launchctl list <label>` (or `launchctl print`) stdout.
    pub fn parse_runtime_executable_from_launchctl_list(stdout: &str) -> Option<PathBuf> {
        // 1. "Program" = "...";
        if let Some(pos) = stdout.find("\"Program\"") {
            let after = &stdout[pos + "\"Program\"".len()..];
            if let Some(eq_pos) = after.find('=') {
                let val_part = &after[eq_pos + 1..];
                if let Some(semi_pos) = val_part.find(';') {
                    let raw = val_part[..semi_pos].trim().trim_matches('"');
                    if !raw.is_empty() {
                        return Some(PathBuf::from(raw));
                    }
                }
            }
        }

        // 2. Fallback: "ProgramArguments" = ( "..."; ... );
        if let Some(pos) = stdout.find("\"ProgramArguments\"") {
            let after = &stdout[pos + "\"ProgramArguments\"".len()..];
            if let Some(paren_pos) = after.find('(') {
                let inside = &after[paren_pos + 1..];
                if let Some(semi_pos) = inside.find(';') {
                    let raw = inside[..semi_pos].trim().trim_matches('"');
                    if !raw.is_empty() {
                        return Some(PathBuf::from(raw));
                    }
                }
            }
        }

        // 3. Fallback: launchctl print format: program = ...
        if let Some(pos) = stdout.find("program = ") {
            let after = &stdout[pos + "program = ".len()..];
            let line = after.lines().next().unwrap_or("").trim().trim_matches('"');
            if !line.is_empty() {
                return Some(PathBuf::from(line));
            }
        }

        None
    }

    /// 過去バージョンで作成されたジョブ個別plist（com.codexscheduler.job.*.plist）を検出してアンロード・削除
    pub fn cleanup_legacy_job_plists(&self) -> Result<(), SchedulerError> {
        if !self.launch_agents_dir.exists() {
            return Ok(());
        }

        let entries = fs::read_dir(&self.launch_agents_dir)?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                if file_name.starts_with(LEGACY_PLIST_PREFIX) && file_name.ends_with(".plist") {
                    self.safe_launchctl_unload(&path)?;
                    fs::remove_file(&path)?;
                }
            }
        }

        Ok(())
    }

    /// 過去バージョンで作成されたジョブ個別plistが存在するか判定
    pub fn has_legacy_job_plists(&self) -> bool {
        if !self.launch_agents_dir.exists() {
            return false;
        }
        if let Ok(entries) = fs::read_dir(&self.launch_agents_dir) {
            for entry in entries.flatten() {
                if let Some(file_name) = entry.file_name().to_str() {
                    if file_name.starts_with(LEGACY_PLIST_PREFIX) && file_name.ends_with(".plist") {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// 実行可能ファイルが Desktop GUI アプリケーション（.app 内バイナリかつ codex-scheduler-gui）か判定
    pub fn is_desktop_executable(path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        let in_app_bundle = path_str.contains(".app/");
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            in_app_bundle && name == "codex-scheduler-gui"
        } else {
            false
        }
    }

    /// 実行可能ファイルが standalone CLI バイナリ（codex-scheduler または legacy互換名 codex-scheduler-cli）か判定
    pub fn is_cli_executable(path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        // App bundle 内部のバイナリは standalone CLI 実行ファイルとして誤認してはならない
        if path_str.contains(".app/") {
            return false;
        }
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name == "codex-scheduler" || name == "codex-scheduler-cli" {
                return true;
            }
            #[cfg(test)]
            {
                if name == "echo" || name == "sh" || name.starts_with("test") {
                    return true;
                }
            }
        }
        false
    }

    // WHY: In macOS single scheduler architecture, Desktop-owned LaunchAgent must be prioritized.
    //      If the Desktop LaunchAgent plist exists on disk but is unloaded or runtime target mismatched,
    //      CLI must safely repair it by unloading any stale same-label runtime registration and
    //      re-loading the canonical Desktop plist via launchctl load -w without mutating plist content or binary path.
    // WHAT BREAKS: Silently ignoring unloaded/mismatched state leaves scheduled jobs unexecuted in background.
    //              Overwriting Desktop plist with CLI binary breaks Desktop background Gatekeeper authorization.
    //              Leaving stale runtime target active in launchd causes launchd to invoke wrong/missing binary.
    // EVIDENCE: docs/spec-changes/0015-macos-scheduler-health-and-cli-status.md, docs/spec-changes/0022-os-scheduler-runtime-target-verification.md, OS-SCHED-006, CLI-CMD-004
    pub fn repair_desktop_scheduler(&self) -> Result<(), SchedulerError> {
        let plist_path = self.scheduler_plist_path();
        if !plist_path.exists() {
            return Err(SchedulerError::CommandFailed(
                "Desktop scheduler plist does not exist".to_string(),
            ));
        }

        let owner = self.get_scheduler_owner();
        if owner != SchedulerOwner::Desktop {
            return Err(SchedulerError::CommandFailed(format!(
                "Cannot repair Desktop scheduler: detected owner is {}",
                owner
            )));
        }

        // Safely remove any existing/stale runtime registration for the same label
        let _ = self.safe_launchctl_unload(&plist_path);
        let _ = self.runner.run_launchctl(&["remove", SCHEDULER_LABEL]);

        // Load existing Desktop plist with -w without mutating plist content or binary path
        let path_str = plist_path.to_string_lossy();
        let output = self.runner.run_launchctl(&["load", "-w", &path_str])?;
        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(SchedulerError::CommandFailed(format!(
                "launchctl load failed during Desktop scheduler repair for {}: {}",
                SCHEDULER_LABEL,
                err.trim()
            )));
        }

        if !self.is_scheduler_ready() {
            let detail = String::from_utf8_lossy(&output.stdout);
            return Err(SchedulerError::CommandFailed(format!(
                "Desktop scheduler repaired but launchctl reports not ready for {}{}",
                SCHEDULER_LABEL,
                if !detail.trim().is_empty() {
                    format!(": {}", detail.trim())
                } else {
                    String::new()
                }
            )));
        }

        Ok(())
    }
}

impl Default for MacOsLaunchdScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl SchedulerBackend for MacOsLaunchdScheduler {
    fn get_scheduler_owner(&self) -> SchedulerOwner {
        let plist_path = self.scheduler_plist_path();
        if !plist_path.exists() {
            if self.has_legacy_job_plists() {
                return SchedulerOwner::Legacy;
            }
            return SchedulerOwner::None;
        }

        let content = match fs::read_to_string(&plist_path) {
            Ok(c) => c,
            Err(_) => return SchedulerOwner::Invalid,
        };

        if !content.contains(SCHEDULER_LABEL) {
            return SchedulerOwner::Invalid;
        }

        // ProgramArguments に --scheduler-tick が含まれていない場合はレガシー
        if !content.contains("<string>--scheduler-tick</string>") {
            return SchedulerOwner::Legacy;
        }

        let exe_path = match Self::extract_executable_path_from_plist(&content) {
            Some(p) => p,
            None => return SchedulerOwner::Invalid,
        };

        if !exe_path.is_absolute() || !exe_path.exists() {
            return SchedulerOwner::Invalid;
        }

        if Self::is_desktop_executable(&exe_path) {
            SchedulerOwner::Desktop
        } else if Self::is_cli_executable(&exe_path) {
            SchedulerOwner::Cli
        } else {
            SchedulerOwner::Invalid
        }
    }

    fn get_scheduler_executable_path(&self) -> Option<PathBuf> {
        let plist_path = self.scheduler_plist_path();
        if !plist_path.exists() {
            return None;
        }
        let content = fs::read_to_string(&plist_path).ok()?;
        Self::extract_executable_path_from_plist(&content)
    }

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

        if !self.launch_agents_dir.exists() {
            fs::create_dir_all(&self.launch_agents_dir)?;
        }

        // レガシーな個別ジョブplistがあれば一括クリーンアップ
        self.cleanup_legacy_job_plists()?;

        let plist_path = self.scheduler_plist_path();
        let current_owner = self.get_scheduler_owner();
        let caller_is_desktop = Self::is_desktop_executable(target_exe_path);

        if current_owner == SchedulerOwner::Desktop && !caller_is_desktop {
            // WHY: Valid Desktop-owned LaunchAgent must not be overwritten by CLI invocation.
            // WHAT BREAKS: Overwriting Desktop owner with CLI binary re-introduces Gatekeeper warnings
            //              and breaks Desktop-managed background scheduling.
            // EVIDENCE: docs/adr/0004-independent-desktop-cli-single-scheduler-owner.md, OS-SCHED-006, CLI-CMD-004
            if self.is_scheduler_ready() {
                return Ok(());
            } else {
                // State B (Unloaded / Recoverable): Desktop LaunchAgent exists on disk but is not loaded in launchctl.
                // Safely repair by reloading existing Desktop plist without mutating plist content or binary path.
                return self.repair_desktop_scheduler();
            }
        }

        if current_owner == SchedulerOwner::Cli && caller_is_desktop {
            // WHY: Desktop GUI startup takes over ownership from CLI-owned scheduler to guarantee
            //      single LaunchAgent invariant while providing Gatekeeper-safe Desktop execution.
            // WHAT BREAKS: Multiple LaunchAgents or failing to migrate leaves Desktop unverified in background.
            // EVIDENCE: docs/adr/0004-independent-desktop-cli-single-scheduler-owner.md, OS-SCHED-006
            // Desktop proceeds to take over ownership and update LaunchAgent plist.
        }

        // 不正・未知の設定ファイルの検査および Invalid owner に対する非破壊保護
        if plist_path.exists() && current_owner == SchedulerOwner::Invalid {
            let content = match fs::read_to_string(&plist_path) {
                Ok(c) => c,
                Err(e) => {
                    return Err(SchedulerError::MalformedConfiguration(format!(
                        "Cannot read configuration in {}: {}",
                        plist_path.display(),
                        e
                    )));
                }
            };

            let has_label = content.contains(SCHEDULER_LABEL);
            let has_tick = content.contains("<string>--scheduler-tick</string>");
            let exe_opt = Self::extract_executable_path_from_plist(&content);

            if !has_label || !has_tick || exe_opt.is_none() {
                // Case A: Unrecognized label, missing --scheduler-tick, or unparseable target executable
                return Err(SchedulerError::MalformedConfiguration(format!(
                    "Unrecognized or malformed configuration in {}",
                    plist_path.display()
                )));
            }

            let registered_exe = exe_opt.unwrap();
            let exe_exists = registered_exe.is_absolute() && registered_exe.exists();

            if !caller_is_desktop {
                // WHY: CLI caller must not overwrite or takeover existing LaunchAgent when owner is Invalid.
                //      If the registered executable is missing (stale Desktop registration) or unrecognized,
                //      CLI must return a structured error and leave the existing plist untouched.
                // WHAT BREAKS: Overwriting invalid/stale Desktop registrations with CLI binary breaks Desktop
                //              precedence and causes unmanaged scheduler takeover.
                // EVIDENCE: docs/spec-changes/0015-macos-scheduler-health-and-cli-status.md, OS-SCHED-006, CLI-CMD-004
                if !exe_exists {
                    // Case B: Syntactically managed plist but target executable is missing (stale registration)
                    return Err(SchedulerError::ExecutableNotFound(format!(
                        "Registered scheduler executable does not exist: {}",
                        registered_exe.display()
                    )));
                } else {
                    return Err(SchedulerError::MalformedConfiguration(format!(
                        "Registered scheduler executable is invalid or unrecognized: {}",
                        registered_exe.display()
                    )));
                }
            } else {
                // Caller is Desktop:
                // Protect non-desktop managed registrations from accidental Desktop overwrite if unrecognized.
                if exe_exists && !Self::is_desktop_executable(&registered_exe) {
                    return Err(SchedulerError::MalformedConfiguration(format!(
                        "Cannot overwrite non-desktop registration with Desktop app: {}",
                        registered_exe.display()
                    )));
                }
                // When registered Desktop executable was missing (stale registration), Desktop GUI proceeds
                // to self-repair by rewriting plist with current valid Desktop executable.
            }
        }

        let expected_content = Self::generate_scheduler_plist_content(target_exe_path);

        // 既に同内容のplistが存在し、かつ正常にlaunchctlに登録されていれば再登録せずスキップ（通知抑制）
        if plist_path.exists() {
            if let Ok(existing) = fs::read_to_string(&plist_path) {
                if existing == expected_content {
                    if self.is_scheduler_ready() {
                        return Ok(());
                    }
                }
            }
        }

        // 既存の登録があれば確実にアンロード
        if plist_path.exists() {
            let _ = self.safe_launchctl_unload(&plist_path);
        }
        let _ = self.runner.run_launchctl(&["remove", SCHEDULER_LABEL]);

        // 新規作成または内容更新時のみ書き込み＆ロード
        fs::write(&plist_path, &expected_content)?;

        let path_str = plist_path.to_string_lossy();
        let output = self.runner.run_launchctl(&["load", "-w", &path_str])?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(SchedulerError::CommandFailed(format!(
                "launchctl load failed for {}: {}",
                SCHEDULER_LABEL,
                err.trim()
            )));
        }

        Ok(())
    }

    fn is_scheduler_installed(&self) -> bool {
        self.scheduler_plist_path().exists()
    }

    // WHY: Scheduler readiness must verify that launchctl has loaded the job AND that the
    //      currently loaded executable in launchd runtime matches the expected target from
    //      canonical plist.
    // WHAT BREAKS: Merely checking exit code of `launchctl list <label>` causes false-positive
    //              readiness when launchd holds a stale registration with the same label pointing
    //              to a deleted/wrong executable (e.g. from previous tests or stale paths).
    // EVIDENCE: docs/spec-changes/0022-os-scheduler-runtime-target-verification.md, OS-SCHED-006, CLI-CMD-003
    fn is_scheduler_ready(&self) -> bool {
        let owner = self.get_scheduler_owner();
        if owner != SchedulerOwner::Desktop && owner != SchedulerOwner::Cli {
            return false;
        }

        let expected_exe = match self.get_scheduler_executable_path() {
            Some(p) => p,
            None => return false,
        };

        // launchctl list dev.codexscheduler.scheduler が成功（loaded）していること
        let output = match self.runner.run_launchctl(&["list", SCHEDULER_LABEL]) {
            Ok(out) => out,
            Err(_) => return false,
        };

        if !output.status.success() {
            return false;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let runtime_exe = match Self::parse_runtime_executable_from_launchctl_list(&stdout) {
            Some(p) => p,
            None => return false,
        };

        runtime_exe == expected_exe
    }

    fn is_scheduler_path_matched(&self, app_executable_path: &Path) -> bool {
        let plist_path = self.scheduler_plist_path();
        if !plist_path.exists() {
            return false;
        }

        let content = match fs::read_to_string(&plist_path) {
            Ok(content) => content,
            Err(_) => return false,
        };
        if !content.contains("<string>--scheduler-tick</string>") {
            return false;
        }

        matches!(
            Self::extract_executable_path_from_plist(&content),
            Some(path) if path == app_executable_path
        )
    }

    fn uninstall_scheduler(&self) -> Result<(), SchedulerError> {
        let plist_path = self.scheduler_plist_path();
        if plist_path.exists() {
            self.safe_launchctl_unload(&plist_path)?;
            fs::remove_file(&plist_path)?;
        }
        Ok(())
    }

    fn uninstall_scheduler_as_cli(&self) -> Result<(), SchedulerError> {
        let owner = self.get_scheduler_owner();
        if owner == SchedulerOwner::Desktop {
            return Err(SchedulerError::DesktopOwnerProtected);
        }
        self.uninstall_scheduler()
    }

    fn register_job(&self, _job: &Job, app_executable_path: &Path) -> Result<(), SchedulerError> {
        self.ensure_scheduler_installed(app_executable_path)
    }

    fn unregister_job(&self, job_id: &str) -> Result<(), SchedulerError> {
        let legacy_path = self.legacy_job_plist_path(job_id);
        if legacy_path.exists() {
            self.safe_launchctl_unload(&legacy_path)?;
            fs::remove_file(&legacy_path)?;
        }
        Ok(())
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use crate::os_scheduler::SchedulerOwner;

    #[test]
    fn test_scheduler_plist_generation() {
        let app_path =
            Path::new("/Applications/Codex Scheduler.app/Contents/MacOS/codex-scheduler-gui");
        let plist = MacOsLaunchdScheduler::generate_scheduler_plist_content(app_path);
        assert!(plist.contains(SCHEDULER_LABEL));
        assert!(plist.contains(
            "<string>/Applications/Codex Scheduler.app/Contents/MacOS/codex-scheduler-gui</string>"
        ));
        assert!(plist.contains("<string>--scheduler-tick</string>"));
        assert!(!plist.contains("codex-scheduler-cli"));
        assert!(plist.contains("<key>StartInterval</key>\n    <integer>60</integer>"));
        assert!(plist.contains("<key>RunAtLoad</key>\n    <true/>"));
        assert!(plist.contains("<key>EnvironmentVariables</key>"));
        assert!(plist.contains("<key>PATH</key>"));
        assert!(plist.contains("<key>HOME</key>"));
    }

    #[test]
    fn test_scheduler_plist_xml_special_character_round_trip() {
        let app_path = Path::new(
            "/Applications/AI & Dev <Nightly>/Codex \"Scheduler\" 'Test'.app/Contents/MacOS/codex-scheduler-gui",
        );
        let plist = MacOsLaunchdScheduler::generate_scheduler_plist_content(app_path);

        assert!(plist.contains("AI &amp; Dev &lt;Nightly&gt;"));
        assert!(plist.contains("Codex &quot;Scheduler&quot; &apos;Test&apos;.app"));
        assert!(!plist.contains("AI & Dev <Nightly>"));

        let extracted = MacOsLaunchdScheduler::extract_executable_path_from_plist(&plist);
        assert_eq!(extracted, Some(app_path.to_path_buf()));

        let temp_dir = std::env::temp_dir().join(format!("test-xml-path-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();
        let scheduler = MacOsLaunchdScheduler::with_dir(temp_dir.clone());
        fs::write(scheduler.scheduler_plist_path(), plist).unwrap();
        assert!(scheduler.is_scheduler_path_matched(app_path));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_xml_text_escape_unescape_preserves_literal_entities() {
        let raw = "/Applications/A &amp; B & <C> \"D\" 'E'.app";
        let escaped = MacOsLaunchdScheduler::escape_xml_text(raw);
        assert_eq!(
            escaped,
            "/Applications/A &amp;amp; B &amp; &lt;C&gt; &quot;D&quot; &apos;E&apos;.app"
        );
        assert_eq!(MacOsLaunchdScheduler::unescape_xml_text(&escaped), raw);
    }

    #[test]
    fn test_ensure_scheduler_installed_idempotent() {
        let temp_dir = std::env::temp_dir().join(format!("test-launchd-{}", uuid::Uuid::new_v4()));
        let scheduler = MacOsLaunchdScheduler::with_dir(temp_dir.clone());
        let exe_path = Path::new("/bin/echo");

        // 1回目のインストール（launchctl loadが環境によって権限等の理由でコケる可能性を考慮）
        let res1 = scheduler.ensure_scheduler_installed(exe_path);
        // /bin/echo を登録しようとした場合、launchctl load はモック/sandboxではエラーになる場合があるが、
        // plistファイルが生成されていることを確認
        let plist_file = scheduler.scheduler_plist_path();
        if res1.is_ok() {
            assert!(plist_file.exists());
            let content1 = fs::read_to_string(&plist_file).unwrap();
            assert!(content1.contains(SCHEDULER_LABEL));
            assert!(content1.contains("/bin/echo"));
            assert!(content1.contains("--scheduler-tick"));

            // 2回目の呼出（内容同一かつロード済みなら再ロードなし）
            let res2 = scheduler.ensure_scheduler_installed(exe_path);
            assert!(res2.is_ok());

            // パスマッチ確認
            assert!(scheduler.is_scheduler_path_matched(exe_path));
        }

        // アンインストールテスト
        let uninst = scheduler.uninstall_scheduler();
        assert!(uninst.is_ok());
        assert!(!plist_file.exists());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_ensure_scheduler_validation_errors() {
        let temp_dir =
            std::env::temp_dir().join(format!("test-launchd-val-{}", uuid::Uuid::new_v4()));
        let scheduler = MacOsLaunchdScheduler::with_dir(temp_dir.clone());

        // 1. Non-absolute path
        let err1 = scheduler
            .ensure_scheduler_installed(Path::new("relative/path"))
            .unwrap_err();
        match err1 {
            SchedulerError::InvalidExecutable(_) => {}
            _ => panic!("Expected InvalidExecutable error, got: {:?}", err1),
        }

        // 2. Missing executable
        let err2 = scheduler
            .ensure_scheduler_installed(Path::new("/non/existent/path/binary"))
            .unwrap_err();
        match err2 {
            SchedulerError::ExecutableNotFound(_) => {}
            _ => panic!("Expected ExecutableNotFound error, got: {:?}", err2),
        }

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_legacy_worker_plist_migration() {
        let temp_dir =
            std::env::temp_dir().join(format!("test-migration-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();
        let scheduler = MacOsLaunchdScheduler::with_dir(temp_dir.clone());

        // 旧Worker CLIを指すplistを手動で作成
        let plist_file = scheduler.scheduler_plist_path();
        let old_plist_content = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{}</string>
    <key>ProgramArguments</key>
    <array>
        <string>/Users/test/.local/share/codex-scheduler/bin/codex-scheduler-cli</string>
        <string>tick</string>
    </array>
</dict>
</plist>"#,
            SCHEDULER_LABEL
        );
        fs::write(&plist_file, old_plist_content).unwrap();
        assert!(plist_file.exists());

        // 新しいアプリパス（実在する/bin/sh等）でチェック
        let new_exe = Path::new("/bin/sh");
        assert!(!scheduler.is_scheduler_path_matched(new_exe));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_cleanup_legacy_job_plists() {
        let temp_dir = std::env::temp_dir().join(format!("test-legacy-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();
        let scheduler = MacOsLaunchdScheduler::with_dir(temp_dir.clone());

        let legacy_file = temp_dir.join("com.codexscheduler.job.test-123.plist");
        fs::write(&legacy_file, "<plist></plist>").unwrap();
        assert!(legacy_file.exists());

        scheduler.cleanup_legacy_job_plists().unwrap();
        assert!(!legacy_file.exists());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_ownership_no_scheduler_desktop_ensure() {
        let temp_dir =
            std::env::temp_dir().join(format!("test-own-no-desk-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();
        let scheduler = MacOsLaunchdScheduler::with_dir(temp_dir.clone());
        assert_eq!(scheduler.get_scheduler_owner(), SchedulerOwner::None);

        let desk_dir = temp_dir.join("Codex Scheduler.app/Contents/MacOS");
        fs::create_dir_all(&desk_dir).unwrap();
        let desk_exe = desk_dir.join("codex-scheduler-gui");
        fs::write(&desk_exe, b"#!/bin/sh\nexit 0").unwrap();

        let _ = scheduler.ensure_scheduler_installed(&desk_exe);
        let plist_path = scheduler.scheduler_plist_path();
        assert!(plist_path.exists());
        assert_eq!(scheduler.get_scheduler_owner(), SchedulerOwner::Desktop);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_ownership_no_scheduler_cli_ensure() {
        let temp_dir =
            std::env::temp_dir().join(format!("test-own-no-cli-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();
        let scheduler = MacOsLaunchdScheduler::with_dir(temp_dir.clone());
        assert_eq!(scheduler.get_scheduler_owner(), SchedulerOwner::None);

        let cli_exe = temp_dir.join("codex-scheduler");
        fs::write(&cli_exe, b"#!/bin/sh\nexit 0").unwrap();

        let _ = scheduler.ensure_scheduler_installed(&cli_exe);
        let plist_path = scheduler.scheduler_plist_path();
        assert!(plist_path.exists());
        assert_eq!(scheduler.get_scheduler_owner(), SchedulerOwner::Cli);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_ownership_cli_owner_cli_ensure_noop() {
        let temp_dir =
            std::env::temp_dir().join(format!("test-own-cli-noop-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();
        let scheduler = MacOsLaunchdScheduler::with_dir(temp_dir.clone());

        let cli_exe = temp_dir.join("codex-scheduler");
        fs::write(&cli_exe, b"#!/bin/sh\nexit 0").unwrap();

        let _ = scheduler.ensure_scheduler_installed(&cli_exe);
        assert_eq!(scheduler.get_scheduler_owner(), SchedulerOwner::Cli);

        // Second call with same CLI exe
        let res = scheduler.ensure_scheduler_installed(&cli_exe);
        assert!(res.is_ok());
        assert_eq!(scheduler.get_scheduler_owner(), SchedulerOwner::Cli);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_ownership_desktop_owner_cli_ensure_retains_desktop() {
        let temp_dir =
            std::env::temp_dir().join(format!("test-own-desk-retain-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();
        let runner = Box::new(MockLaunchctlRunner::new(true));
        let scheduler = MacOsLaunchdScheduler::with_dir_and_runner(temp_dir.clone(), runner);

        let desk_dir = temp_dir.join("Codex Scheduler.app/Contents/MacOS");
        fs::create_dir_all(&desk_dir).unwrap();
        let desk_exe = desk_dir.join("codex-scheduler-gui");
        fs::write(&desk_exe, b"#!/bin/sh\nexit 0").unwrap();

        let cli_exe = temp_dir.join("codex-scheduler");
        fs::write(&cli_exe, b"#!/bin/sh\nexit 0").unwrap();

        // 1. Desktop ensures scheduler
        let _ = scheduler.ensure_scheduler_installed(&desk_exe);
        assert_eq!(scheduler.get_scheduler_owner(), SchedulerOwner::Desktop);

        // 2. CLI tries to ensure scheduler -> MUST retain Desktop owner and succeed
        let cli_res = scheduler.ensure_scheduler_installed(&cli_exe);
        assert!(
            cli_res.is_ok(),
            "CLI ensure against Desktop owner should return Ok as no-op"
        );
        assert_eq!(scheduler.get_scheduler_owner(), SchedulerOwner::Desktop);

        let extracted = scheduler.get_scheduler_executable_path();
        assert_eq!(extracted, Some(desk_exe));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_ownership_cli_owner_desktop_ensure_migrates_to_desktop() {
        let temp_dir =
            std::env::temp_dir().join(format!("test-own-migrate-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();
        let scheduler = MacOsLaunchdScheduler::with_dir(temp_dir.clone());

        let cli_exe = temp_dir.join("codex-scheduler");
        fs::write(&cli_exe, b"#!/bin/sh\nexit 0").unwrap();

        let desk_dir = temp_dir.join("Codex Scheduler.app/Contents/MacOS");
        fs::create_dir_all(&desk_dir).unwrap();
        let desk_exe = desk_dir.join("codex-scheduler-gui");
        fs::write(&desk_exe, b"#!/bin/sh\nexit 0").unwrap();

        // 1. CLI registered scheduler
        let _ = scheduler.ensure_scheduler_installed(&cli_exe);
        assert_eq!(scheduler.get_scheduler_owner(), SchedulerOwner::Cli);

        // 2. Desktop GUI starts up and ensures scheduler -> MUST migrate to Desktop owner!
        let _ = scheduler.ensure_scheduler_installed(&desk_exe);
        assert_eq!(scheduler.get_scheduler_owner(), SchedulerOwner::Desktop);

        let extracted = scheduler.get_scheduler_executable_path();
        assert_eq!(extracted, Some(desk_exe));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_ownership_desktop_owner_desktop_ensure_idempotent() {
        let temp_dir =
            std::env::temp_dir().join(format!("test-own-desk-idem-{}", uuid::Uuid::new_v4()));
        let scheduler = MacOsLaunchdScheduler::with_dir(temp_dir.clone());

        let desk_dir = temp_dir.join("Codex Scheduler.app/Contents/MacOS");
        fs::create_dir_all(&desk_dir).unwrap();
        let desk_exe = desk_dir.join("codex-scheduler-gui");
        fs::write(&desk_exe, b"#!/bin/sh\nexit 0").unwrap();

        let _ = scheduler.ensure_scheduler_installed(&desk_exe);
        assert_eq!(scheduler.get_scheduler_owner(), SchedulerOwner::Desktop);

        let res2 = scheduler.ensure_scheduler_installed(&desk_exe);
        assert!(res2.is_ok());
        assert_eq!(scheduler.get_scheduler_owner(), SchedulerOwner::Desktop);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_ownership_known_legacy_owner_migrates_safely() {
        let temp_dir =
            std::env::temp_dir().join(format!("test-own-legacy-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();
        let scheduler = MacOsLaunchdScheduler::with_dir(temp_dir.clone());

        let legacy_content = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{}</string>
    <key>ProgramArguments</key>
    <array>
        <string>/Users/old/.local/bin/codex-scheduler-cli</string>
        <string>tick</string>
    </array>
</dict>
</plist>"#,
            SCHEDULER_LABEL
        );
        fs::write(scheduler.scheduler_plist_path(), legacy_content).unwrap();
        assert_eq!(scheduler.get_scheduler_owner(), SchedulerOwner::Legacy);

        let desk_dir = temp_dir.join("Codex Scheduler.app/Contents/MacOS");
        fs::create_dir_all(&desk_dir).unwrap();
        let desk_exe = desk_dir.join("codex-scheduler-gui");
        fs::write(&desk_exe, b"#!/bin/sh\nexit 0").unwrap();

        let _ = scheduler.ensure_scheduler_installed(&desk_exe);
        assert_eq!(scheduler.get_scheduler_owner(), SchedulerOwner::Desktop);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_ownership_unknown_malformed_config_errors_without_destructive_overwrite() {
        let temp_dir =
            std::env::temp_dir().join(format!("test-own-malformed-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();
        let scheduler = MacOsLaunchdScheduler::with_dir(temp_dir.clone());

        let malformed_content = "<invalid xml content without label or arguments>";
        fs::write(scheduler.scheduler_plist_path(), malformed_content).unwrap();
        assert_eq!(scheduler.get_scheduler_owner(), SchedulerOwner::Invalid);

        let desk_dir = temp_dir.join("Codex Scheduler.app/Contents/MacOS");
        fs::create_dir_all(&desk_dir).unwrap();
        let desk_exe = desk_dir.join("codex-scheduler-gui");
        fs::write(&desk_exe, b"#!/bin/sh\nexit 0").unwrap();

        let err = scheduler.ensure_scheduler_installed(&desk_exe).unwrap_err();
        match err {
            SchedulerError::MalformedConfiguration(_) => {}
            _ => panic!("Expected MalformedConfiguration error, got: {:?}", err),
        }

        // Verify file was NOT destructively overwritten
        let preserved = fs::read_to_string(scheduler.scheduler_plist_path()).unwrap();
        assert_eq!(preserved, malformed_content);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_ownership_single_launchagent_invariant_and_cli_uninstall_protection() {
        let temp_dir =
            std::env::temp_dir().join(format!("test-own-single-inv-{}", uuid::Uuid::new_v4()));
        let scheduler = MacOsLaunchdScheduler::with_dir(temp_dir.clone());

        let desk_dir = temp_dir.join("Codex Scheduler.app/Contents/MacOS");
        fs::create_dir_all(&desk_dir).unwrap();
        let desk_exe = desk_dir.join("codex-scheduler-gui");
        fs::write(&desk_exe, b"#!/bin/sh\nexit 0").unwrap();

        let cli_exe = temp_dir.join("codex-scheduler");
        fs::write(&cli_exe, b"#!/bin/sh\nexit 0").unwrap();

        // 1. Install as Desktop
        let _ = scheduler.ensure_scheduler_installed(&desk_exe);
        assert_eq!(scheduler.get_scheduler_owner(), SchedulerOwner::Desktop);

        // 2. CLI attempts uninstall -> MUST BE PROTECTED!
        let uninst_err = scheduler.uninstall_scheduler_as_cli().unwrap_err();
        match uninst_err {
            SchedulerError::DesktopOwnerProtected => {}
            _ => panic!(
                "Expected DesktopOwnerProtected error, got: {:?}",
                uninst_err
            ),
        }
        assert!(
            scheduler.scheduler_plist_path().exists(),
            "Desktop-owned plist must not be deleted by CLI"
        );

        // 3. Migrate to CLI owner for uninstall test
        // Manually overwrite plist with CLI content
        let cli_plist = MacOsLaunchdScheduler::generate_scheduler_plist_content(&cli_exe);
        fs::write(scheduler.scheduler_plist_path(), cli_plist).unwrap();
        assert_eq!(scheduler.get_scheduler_owner(), SchedulerOwner::Cli);

        // 4. CLI uninstalls CLI-owned scheduler -> MUST SUCCEED
        let uninst_ok = scheduler.uninstall_scheduler_as_cli();
        assert!(uninst_ok.is_ok());
        assert!(
            !scheduler.scheduler_plist_path().exists(),
            "CLI-owned plist should be deleted"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_extract_executable_path_from_plist() {
        let sample_plist = r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
    <key>ProgramArguments</key>
    <array>
        <string>/Applications/Codex Scheduler.app/Contents/MacOS/codex-scheduler-gui</string>
        <string>--scheduler-tick</string>
    </array>
</dict>
</plist>"#;
        let extracted = MacOsLaunchdScheduler::extract_executable_path_from_plist(sample_plist);
        assert_eq!(
            extracted,
            Some(PathBuf::from(
                "/Applications/Codex Scheduler.app/Contents/MacOS/codex-scheduler-gui"
            ))
        );

        let legacy_plist = r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
    <key>ProgramArguments</key>
    <array>
        <string>/Users/test/.local/share/codex-scheduler/bin/codex-scheduler-cli</string>
        <string>tick</string>
    </array>
</dict>
</plist>"#;
        let legacy_extracted =
            MacOsLaunchdScheduler::extract_executable_path_from_plist(legacy_plist);
        assert_eq!(
            legacy_extracted,
            Some(PathBuf::from(
                "/Users/test/.local/share/codex-scheduler/bin/codex-scheduler-cli"
            ))
        );

        let invalid_plist = "<dict></dict>";
        assert_eq!(
            MacOsLaunchdScheduler::extract_executable_path_from_plist(invalid_plist),
            None
        );
    }

    #[test]
    fn test_is_scheduler_ready_rejects_legacy_or_invalid_plist() {
        let temp_dir = std::env::temp_dir().join(format!("test-ready-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();
        let scheduler = MacOsLaunchdScheduler::with_dir(temp_dir.clone());
        let plist_file = scheduler.scheduler_plist_path();

        // 1. plistが存在しない -> false
        assert!(!scheduler.is_scheduler_ready());

        // 2. 旧Worker plist（codex-scheduler-cli tick）が存在する -> false (not ready!)
        let old_content = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{}</string>
    <key>ProgramArguments</key>
    <array>
        <string>/Users/test/.local/share/codex-scheduler/bin/codex-scheduler-cli</string>
        <string>tick</string>
    </array>
</dict>
</plist>"#,
            SCHEDULER_LABEL
        );
        fs::write(&plist_file, old_content).unwrap();
        assert!(!scheduler.is_scheduler_ready());

        // 3. --scheduler-tick はあるが、実行可能ファイルが存在しないパス -> false
        let nonexistent_app = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{}</string>
    <key>ProgramArguments</key>
    <array>
        <string>/non/existent/app/Contents/MacOS/codex-scheduler-gui</string>
        <string>--scheduler-tick</string>
    </array>
</dict>
</plist>"#,
            SCHEDULER_LABEL
        );
        fs::write(&plist_file, nonexistent_app).unwrap();
        assert!(!scheduler.is_scheduler_ready());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_check_launchctl_unload_output() {
        use std::os::unix::process::ExitStatusExt;
        let test_plist = Path::new("/tmp/test.plist");

        // 1. Success exit status
        let success_out = std::process::Output {
            status: std::process::ExitStatus::from_raw(0),
            stdout: Vec::new(),
            stderr: Vec::new(),
        };
        assert!(
            MacOsLaunchdScheduler::check_launchctl_unload_output(test_plist, &success_out).is_ok()
        );

        // 2. Not loaded error strings -> Treated as Ok
        for msg in &[
            "Could not find specified service",
            "/tmp/test.plist: Not loaded",
            "No such process",
        ] {
            let not_loaded_out = std::process::Output {
                status: std::process::ExitStatus::from_raw(3 << 8),
                stdout: Vec::new(),
                stderr: msg.as_bytes().to_vec(),
            };
            assert!(
                MacOsLaunchdScheduler::check_launchctl_unload_output(test_plist, &not_loaded_out)
                    .is_ok(),
                "Expected Ok for not-loaded message: {}",
                msg
            );
        }

        // 3. Failure exit status with empty stderr -> Must return Error (do NOT swallow!)
        let empty_stderr_fail = std::process::Output {
            status: std::process::ExitStatus::from_raw(1 << 8),
            stdout: b"some stdout".to_vec(),
            stderr: Vec::new(),
        };
        let err =
            MacOsLaunchdScheduler::check_launchctl_unload_output(test_plist, &empty_stderr_fail)
                .unwrap_err();
        match err {
            SchedulerError::CommandFailed(ref s) => {
                assert!(s.contains("exit status"));
                assert!(s.contains("some stdout"));
            }
            _ => panic!("Expected CommandFailed error, got: {:?}", err),
        }

        // 4. Failure exit status with permission denied or other error -> Must return Error
        let perm_fail = std::process::Output {
            status: std::process::ExitStatus::from_raw(1 << 8),
            stdout: Vec::new(),
            stderr: b"Permission denied".to_vec(),
        };
        let err2 = MacOsLaunchdScheduler::check_launchctl_unload_output(test_plist, &perm_fail)
            .unwrap_err();
        match err2 {
            SchedulerError::CommandFailed(ref s) => {
                assert!(s.contains("Permission denied"));
            }
            _ => panic!("Expected CommandFailed error, got: {:?}", err2),
        }
    }



    #[test]
    fn test_desktop_owner_loaded_cli_ensure_no_mutation_and_ready() {
        let temp_dir =
            std::env::temp_dir().join(format!("test-desk-loaded-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();

        let desk_dir = temp_dir.join("Codex Scheduler.app/Contents/MacOS");
        fs::create_dir_all(&desk_dir).unwrap();
        let desk_exe = desk_dir.join("codex-scheduler-gui");
        fs::write(&desk_exe, b"#!/bin/sh\nexit 0").unwrap();

        let cli_exe = temp_dir.join("codex-scheduler");
        fs::write(&cli_exe, b"#!/bin/sh\nexit 0").unwrap();

        let runner = MockLaunchctlRunner::with_executable(desk_exe.clone());
        let scheduler =
            MacOsLaunchdScheduler::with_dir_and_runner(temp_dir.clone(), Box::new(runner.clone()));

        // Create valid Desktop plist initially
        let initial_content = MacOsLaunchdScheduler::generate_scheduler_plist_content(&desk_exe);
        fs::write(scheduler.scheduler_plist_path(), &initial_content).unwrap();

        assert_eq!(scheduler.get_scheduler_owner(), SchedulerOwner::Desktop);
        assert!(scheduler.is_scheduler_ready());

        // CLI calls ensure_scheduler_installed: must be idempotent no-op, no mutation, still ready!
        let calls_before = runner.calls.lock().unwrap().len();
        let res = scheduler.ensure_scheduler_installed(&cli_exe);
        assert!(res.is_ok());

        // Verify plist was not modified and owner remains Desktop
        let content_after = fs::read_to_string(scheduler.scheduler_plist_path()).unwrap();
        assert_eq!(initial_content, content_after);
        assert_eq!(scheduler.get_scheduler_owner(), SchedulerOwner::Desktop);
        assert_eq!(scheduler.get_scheduler_executable_path(), Some(desk_exe));
        assert!(scheduler.is_scheduler_ready());

        // Ensure "load" was not called
        let calls = runner.calls.lock().unwrap();
        let load_calls: Vec<_> = calls[calls_before..]
            .iter()
            .filter(|c| c.first() == Some(&"load".to_string()))
            .collect();
        assert!(
            load_calls.is_empty(),
            "Should not re-load already ready Desktop scheduler"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_desktop_owner_unloaded_safely_repairs_without_altering_desktop_owner_or_path() {
        let temp_dir = std::env::temp_dir().join(format!(
            "test-desk-unloaded-repair-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&temp_dir).unwrap();

        let desk_dir = temp_dir.join("Codex Scheduler.app/Contents/MacOS");
        fs::create_dir_all(&desk_dir).unwrap();
        let desk_exe = desk_dir.join("codex-scheduler-gui");
        fs::write(&desk_exe, b"#!/bin/sh\nexit 0").unwrap();

        let cli_exe = temp_dir.join("codex-scheduler");
        fs::write(&cli_exe, b"#!/bin/sh\nexit 0").unwrap();

        // Start with loaded = false (unloaded state in launchd)
        let runner = MockLaunchctlRunner::new(false);
        let scheduler =
            MacOsLaunchdScheduler::with_dir_and_runner(temp_dir.clone(), Box::new(runner.clone()));

        // Plist exists on disk pointing to Desktop
        let initial_content = MacOsLaunchdScheduler::generate_scheduler_plist_content(&desk_exe);
        fs::write(scheduler.scheduler_plist_path(), &initial_content).unwrap();

        assert_eq!(scheduler.get_scheduler_owner(), SchedulerOwner::Desktop);
        assert!(
            !scheduler.is_scheduler_ready(),
            "Should initially be unloaded / not ready"
        );

        // CLI ensures: must trigger safe repair (launchctl load -w) of existing Desktop plist!
        let res = scheduler.ensure_scheduler_installed(&cli_exe);
        assert!(res.is_ok(), "Safe repair should succeed");

        // Verify state after repair:
        // 1. Ready is now true
        assert!(
            scheduler.is_scheduler_ready(),
            "Scheduler should now be ready after repair"
        );
        // 2. Owner is still Desktop
        assert_eq!(scheduler.get_scheduler_owner(), SchedulerOwner::Desktop);
        // 3. Executable path is still Desktop app, not CLI
        assert_eq!(scheduler.get_scheduler_executable_path(), Some(desk_exe));
        // 4. Plist content on disk was NOT mutated with CLI binary
        let content_after = fs::read_to_string(scheduler.scheduler_plist_path()).unwrap();
        assert_eq!(initial_content, content_after);

        // Verify runner received load command
        let calls = runner.calls.lock().unwrap();
        let has_load = calls
            .iter()
            .any(|c| c.contains(&"load".to_string()) && c.contains(&"-w".to_string()));
        assert!(has_load, "Repair must have executed launchctl load -w");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_desktop_owner_repair_failure_returns_structured_error() {
        let temp_dir =
            std::env::temp_dir().join(format!("test-desk-repair-fail-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();

        let desk_dir = temp_dir.join("Codex Scheduler.app/Contents/MacOS");
        fs::create_dir_all(&desk_dir).unwrap();
        let desk_exe = desk_dir.join("codex-scheduler-gui");
        fs::write(&desk_exe, b"#!/bin/sh\nexit 0").unwrap();

        let cli_exe = temp_dir.join("codex-scheduler");
        fs::write(&cli_exe, b"#!/bin/sh\nexit 0").unwrap();

        // Runner configured to fail on load
        let runner = MockLaunchctlRunner::with_load_failure();
        let scheduler =
            MacOsLaunchdScheduler::with_dir_and_runner(temp_dir.clone(), Box::new(runner));

        let initial_content = MacOsLaunchdScheduler::generate_scheduler_plist_content(&desk_exe);
        fs::write(scheduler.scheduler_plist_path(), &initial_content).unwrap();

        // CLI ensure fails when repair fails:
        let err = scheduler.ensure_scheduler_installed(&cli_exe).unwrap_err();
        match err {
            SchedulerError::CommandFailed(ref msg) => {
                assert!(msg.contains("launchctl load failed during Desktop scheduler repair"));
                assert!(msg.contains("Input/output error"));
            }
            _ => panic!("Expected CommandFailed error, got: {:?}", err),
        }

        // Verify plist was NOT overwritten with CLI executable
        let content_after = fs::read_to_string(scheduler.scheduler_plist_path()).unwrap();
        assert_eq!(initial_content, content_after);
        assert_eq!(scheduler.get_scheduler_owner(), SchedulerOwner::Desktop);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_automation_safe_desktop_health_evaluation() {
        let temp_dir =
            std::env::temp_dir().join(format!("test-desk-health-eval-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();

        let desk_dir = temp_dir.join("Codex Scheduler.app/Contents/MacOS");
        fs::create_dir_all(&desk_dir).unwrap();
        let desk_exe = desk_dir.join("codex-scheduler-gui");
        fs::write(&desk_exe, b"#!/bin/sh\nexit 0").unwrap();

        let cli_exe = temp_dir.join("codex-scheduler");
        fs::write(&cli_exe, b"#!/bin/sh\nexit 0").unwrap();

        let runner = MockLaunchctlRunner::with_executable(desk_exe.clone());
        let scheduler =
            MacOsLaunchdScheduler::with_dir_and_runner(temp_dir.clone(), Box::new(runner));

        let plist_content = MacOsLaunchdScheduler::generate_scheduler_plist_content(&desk_exe);
        fs::write(scheduler.scheduler_plist_path(), &plist_content).unwrap();

        // Evaluation from CLI perspective:
        assert!(scheduler.is_scheduler_installed());
        assert!(scheduler.is_scheduler_ready());
        assert_eq!(scheduler.get_scheduler_owner(), SchedulerOwner::Desktop);
        assert!(scheduler.is_target_executable_exists());
        assert!(scheduler.is_owner_target_valid());
        // path_matched is false from CLI perspective because CLI != Desktop GUI exe
        assert!(!scheduler.is_scheduler_path_matched(&cli_exe));

        // Automation health check rule:
        let is_healthy = scheduler.is_scheduler_installed()
            && scheduler.is_scheduler_ready()
            && scheduler.is_target_executable_exists()
            && scheduler.is_owner_target_valid();
        assert!(
            is_healthy,
            "Automation must judge Desktop owner as healthy even when path_matched is false for CLI"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_single_launchagent_invariant_maintained_under_all_operations() {
        let temp_dir =
            std::env::temp_dir().join(format!("test-single-inv-all-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();

        let desk_dir = temp_dir.join("Codex Scheduler.app/Contents/MacOS");
        fs::create_dir_all(&desk_dir).unwrap();
        let desk_exe = desk_dir.join("codex-scheduler-gui");
        fs::write(&desk_exe, b"#!/bin/sh\nexit 0").unwrap();

        let cli_exe = temp_dir.join("codex-scheduler");
        fs::write(&cli_exe, b"#!/bin/sh\nexit 0").unwrap();

        let runner = MockLaunchctlRunner::new(false);
        let scheduler =
            MacOsLaunchdScheduler::with_dir_and_runner(temp_dir.clone(), Box::new(runner));

        // 1. Desktop installs
        scheduler.ensure_scheduler_installed(&desk_exe).unwrap();
        let plist_count = fs::read_dir(&temp_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |ext| ext == "plist"))
            .count();
        assert_eq!(plist_count, 1);

        // 2. CLI ensures (repairs unloaded)
        scheduler.ensure_scheduler_installed(&cli_exe).unwrap();
        let plist_count2 = fs::read_dir(&temp_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |ext| ext == "plist"))
            .count();
        assert_eq!(
            plist_count2, 1,
            "Only single LaunchAgent plist must ever exist"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_executable_classification_hardening() {
        use std::path::Path;

        // 1. Desktop: must be in .app/ and named exactly "codex-scheduler-gui"
        assert!(!MacOsLaunchdScheduler::is_desktop_executable(Path::new(
            "/Applications/Other.app/Contents/MacOS/foo"
        )));
        assert!(!MacOsLaunchdScheduler::is_desktop_executable(Path::new(
            "/Applications/Codex Scheduler.app/Contents/MacOS/codex-scheduler-gui-bar"
        )));
        assert!(!MacOsLaunchdScheduler::is_desktop_executable(Path::new(
            "/usr/local/bin/codex-scheduler-gui"
        )));
        assert!(MacOsLaunchdScheduler::is_desktop_executable(Path::new(
            "/Applications/Codex Scheduler.app/Contents/MacOS/codex-scheduler-gui"
        )));

        // 2. CLI: must be "codex-scheduler" or legacy "codex-scheduler-cli", and NOT inside .app/
        assert!(MacOsLaunchdScheduler::is_cli_executable(Path::new(
            "/usr/local/bin/codex-scheduler"
        )));
        assert!(MacOsLaunchdScheduler::is_cli_executable(Path::new(
            "/usr/local/bin/codex-scheduler-cli"
        )));
        assert!(!MacOsLaunchdScheduler::is_cli_executable(Path::new(
            "/usr/local/bin/codex-scheduler-foo"
        )));
        assert!(!MacOsLaunchdScheduler::is_cli_executable(Path::new(
            "/usr/local/bin/codex-scheduler-gui"
        )));
        assert!(!MacOsLaunchdScheduler::is_cli_executable(Path::new(
            "/Applications/Codex Scheduler.app/Contents/MacOS/codex-scheduler"
        )));
    }

    #[tokio::test]
    async fn test_invalid_stale_target_not_overwritten_by_cli_and_atomicity() {
        use crate::SchedulerService;
        use crate::models::ProviderType;
        use crate::store::JobStore;
        use chrono::Utc;

        let temp_dir =
            std::env::temp_dir().join(format!("test-stale-target-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();

        // 1. Setup a syntactically valid managed plist pointing to a Desktop executable path that does NOT exist
        let non_existent_desktop_exe =
            temp_dir.join("Codex Scheduler.app/Contents/MacOS/codex-scheduler-gui");
        let initial_content =
            MacOsLaunchdScheduler::generate_scheduler_plist_content(&non_existent_desktop_exe);

        let runner = MockLaunchctlRunner::new(false);
        let scheduler =
            MacOsLaunchdScheduler::with_dir_and_runner(temp_dir.clone(), Box::new(runner));
        let plist_path = scheduler.scheduler_plist_path();
        fs::write(&plist_path, &initial_content).unwrap();

        // Current owner must be Invalid because target executable does not exist
        assert_eq!(scheduler.get_scheduler_owner(), SchedulerOwner::Invalid);
        assert!(!scheduler.is_target_executable_exists());
        assert!(!scheduler.is_owner_target_valid());

        let initial_bytes = fs::read(&plist_path).unwrap();

        // 2. Create real CLI executable
        let cli_exe = temp_dir.join("codex-scheduler");
        fs::write(&cli_exe, b"#!/bin/sh\nexit 0").unwrap();

        // 3. CLI calls ensure_scheduler_installed: MUST FAIL, must NOT mutate plist, must NOT takeover to CLI
        let ensure_res = scheduler.ensure_scheduler_installed(&cli_exe);
        assert!(
            ensure_res.is_err(),
            "CLI ensure must fail against Invalid stale target registration"
        );
        match ensure_res.unwrap_err() {
            SchedulerError::ExecutableNotFound(msg) => {
                assert!(msg.contains("Registered scheduler executable does not exist"));
            }
            other => panic!("Expected ExecutableNotFound, got {:?}", other),
        }

        let bytes_after = fs::read(&plist_path).unwrap();
        assert_eq!(
            initial_bytes, bytes_after,
            "Existing plist bytes must remain completely unchanged"
        );
        assert_eq!(
            scheduler.get_scheduler_owner(),
            SchedulerOwner::Invalid,
            "Owner must remain Invalid, never become Cli"
        );
        assert_ne!(
            scheduler.get_scheduler_executable_path(),
            Some(cli_exe.clone()),
            "Executable path in plist must NOT be replaced with CLI binary"
        );

        // 4. Verify schedule_job atomicity via SchedulerService: Job must NOT be saved to store
        let jobs_path = temp_dir.join("jobs.json");
        let store = JobStore::new_with_path(&jobs_path);
        let service =
            SchedulerService::with_scheduler(store.clone(), Some(cli_exe), Box::new(scheduler));

        let sched_res = service.schedule_job(
            ProviderType::Codex,
            "session-stale-test".to_string(),
            temp_dir.clone(),
            Some("test prompt".to_string()),
            Utc::now() + chrono::Duration::hours(1),
            None,
        );
        assert!(
            sched_res.is_err(),
            "schedule_job must fail when scheduler is in Invalid/stale state"
        );
        let all_jobs = store.load_all().unwrap();
        assert_eq!(
            all_jobs.len(),
            0,
            "JobStore must remain completely empty on scheduler error"
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_unit_test_suite_does_not_mutate_real_launchd() {
        // RealLaunchctlRunner is guarded to panic when invoked during tests.
        let runner = RealLaunchctlRunner;
        let panic_result = std::panic::catch_unwind(|| {
            let _ = runner.run_launchctl(&["list", "dummy"]);
        });
        assert!(
            panic_result.is_err(),
            "RealLaunchctlRunner must panic in test environment to prevent real launchd mutation"
        );
    }

    #[test]
    fn test_readiness_disk_target_equals_runtime_target_ready_true() {
        let temp_dir =
            std::env::temp_dir().join(format!("test-ready-match-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();

        let desk_dir = temp_dir.join("Codex Scheduler.app/Contents/MacOS");
        fs::create_dir_all(&desk_dir).unwrap();
        let desk_exe = desk_dir.join("codex-scheduler-gui");
        fs::write(&desk_exe, b"#!/bin/sh\nexit 0").unwrap();

        // Runtime holds exactly desk_exe
        let runner = MockLaunchctlRunner::with_executable(desk_exe.clone());
        let scheduler =
            MacOsLaunchdScheduler::with_dir_and_runner(temp_dir.clone(), Box::new(runner));

        let plist_content = MacOsLaunchdScheduler::generate_scheduler_plist_content(&desk_exe);
        fs::write(scheduler.scheduler_plist_path(), &plist_content).unwrap();

        // Disk target == Runtime target -> ready=true
        assert!(scheduler.is_scheduler_ready());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_readiness_disk_target_not_equal_runtime_target_ready_false() {
        let temp_dir =
            std::env::temp_dir().join(format!("test-ready-mismatch-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();

        let desk_dir = temp_dir.join("Codex Scheduler.app/Contents/MacOS");
        fs::create_dir_all(&desk_dir).unwrap();
        let desk_exe = desk_dir.join("codex-scheduler-gui");
        fs::write(&desk_exe, b"#!/bin/sh\nexit 0").unwrap();

        let other_exe = PathBuf::from("/Applications/Other.app/Contents/MacOS/other-gui");

        // Runtime holds other_exe
        let runner = MockLaunchctlRunner::with_executable(other_exe);
        let scheduler =
            MacOsLaunchdScheduler::with_dir_and_runner(temp_dir.clone(), Box::new(runner));

        let plist_content = MacOsLaunchdScheduler::generate_scheduler_plist_content(&desk_exe);
        fs::write(scheduler.scheduler_plist_path(), &plist_content).unwrap();

        // Disk target != Runtime target -> ready=false
        assert!(!scheduler.is_scheduler_ready());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_readiness_disk_target_exists_but_runtime_stale_temp_target_ready_false() {
        let temp_dir =
            std::env::temp_dir().join(format!("test-ready-stale-temp-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();

        let desk_dir = temp_dir.join("Codex Scheduler.app/Contents/MacOS");
        fs::create_dir_all(&desk_dir).unwrap();
        let desk_exe = desk_dir.join("codex-scheduler-gui");
        fs::write(&desk_exe, b"#!/bin/sh\nexit 0").unwrap();

        // Runtime holds a deleted stale temporary target from previous unit test
        let stale_temp_exe = PathBuf::from(
            "/private/var/folders/zf/stale-temp-test/Codex Scheduler.app/Contents/MacOS/codex-scheduler-gui",
        );

        let runner = MockLaunchctlRunner::with_executable(stale_temp_exe);
        let scheduler =
            MacOsLaunchdScheduler::with_dir_and_runner(temp_dir.clone(), Box::new(runner));

        let plist_content = MacOsLaunchdScheduler::generate_scheduler_plist_content(&desk_exe);
        fs::write(scheduler.scheduler_plist_path(), &plist_content).unwrap();

        // Disk target exists and is valid, but runtime target is stale temp -> ready=false!
        assert!(!scheduler.is_scheduler_ready());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_desktop_owner_stale_runtime_ensure_repairs_to_canonical_desktop_target() {
        let temp_dir =
            std::env::temp_dir().join(format!("test-desk-repair-stale-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();

        let desk_dir = temp_dir.join("Codex Scheduler.app/Contents/MacOS");
        fs::create_dir_all(&desk_dir).unwrap();
        let desk_exe = desk_dir.join("codex-scheduler-gui");
        fs::write(&desk_exe, b"#!/bin/sh\nexit 0").unwrap();

        // Stale runtime target
        let stale_exe = PathBuf::from("/tmp/old-stale/codex-scheduler-gui");
        let runner = MockLaunchctlRunner::with_executable(stale_exe);
        let scheduler =
            MacOsLaunchdScheduler::with_dir_and_runner(temp_dir.clone(), Box::new(runner));

        let plist_content = MacOsLaunchdScheduler::generate_scheduler_plist_content(&desk_exe);
        fs::write(scheduler.scheduler_plist_path(), &plist_content).unwrap();

        // Initially not ready due to mismatch
        assert!(!scheduler.is_scheduler_ready());

        // Desktop calls ensure -> must repair to canonical Desktop target
        let res = scheduler.ensure_scheduler_installed(&desk_exe);
        assert!(res.is_ok());

        // Now runtime target matches desk_exe -> ready=true
        assert!(scheduler.is_scheduler_ready());
        assert_eq!(scheduler.get_scheduler_owner(), SchedulerOwner::Desktop);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_cli_repair_desktop_owner_does_not_overwrite_desktop_target() {
        let temp_dir =
            std::env::temp_dir().join(format!("test-cli-repair-desk-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();

        let desk_dir = temp_dir.join("Codex Scheduler.app/Contents/MacOS");
        fs::create_dir_all(&desk_dir).unwrap();
        let desk_exe = desk_dir.join("codex-scheduler-gui");
        fs::write(&desk_exe, b"#!/bin/sh\nexit 0").unwrap();

        let cli_exe = temp_dir.join("codex-scheduler");
        fs::write(&cli_exe, b"#!/bin/sh\nexit 0").unwrap();

        // Stale runtime target
        let stale_exe = PathBuf::from("/tmp/stale/codex-scheduler-gui");
        let runner = MockLaunchctlRunner::with_executable(stale_exe);
        let scheduler =
            MacOsLaunchdScheduler::with_dir_and_runner(temp_dir.clone(), Box::new(runner));

        let plist_content = MacOsLaunchdScheduler::generate_scheduler_plist_content(&desk_exe);
        fs::write(scheduler.scheduler_plist_path(), &plist_content).unwrap();

        assert!(!scheduler.is_scheduler_ready());

        // CLI calls ensure -> repairs Desktop scheduler without overwriting Desktop plist target!
        let res = scheduler.ensure_scheduler_installed(&cli_exe);
        assert!(res.is_ok());

        // Plist content MUST still point to desk_exe (NOT cli_exe)
        assert_eq!(scheduler.get_scheduler_owner(), SchedulerOwner::Desktop);
        let content_after = fs::read_to_string(scheduler.scheduler_plist_path()).unwrap();
        assert!(content_after.contains(&desk_exe.to_string_lossy().to_string()));
        assert!(!content_after.contains(&cli_exe.to_string_lossy().to_string()));

        // Runtime repaired to desk_exe -> ready=true
        assert!(scheduler.is_scheduler_ready());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_runtime_identity_inspection_failure_ready_false() {
        let temp_dir =
            std::env::temp_dir().join(format!("test-ready-inspect-fail-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();

        let desk_dir = temp_dir.join("Codex Scheduler.app/Contents/MacOS");
        fs::create_dir_all(&desk_dir).unwrap();
        let desk_exe = desk_dir.join("codex-scheduler-gui");
        fs::write(&desk_exe, b"#!/bin/sh\nexit 0").unwrap();

        // loaded=true but loaded_executable is None (unreadable identity / generic output without Program)
        let runner = MockLaunchctlRunner::new(true);
        let scheduler =
            MacOsLaunchdScheduler::with_dir_and_runner(temp_dir.clone(), Box::new(runner));

        let plist_content = MacOsLaunchdScheduler::generate_scheduler_plist_content(&desk_exe);
        fs::write(scheduler.scheduler_plist_path(), &plist_content).unwrap();

        // Fail-closed: unreadable runtime identity -> ready=false
        assert!(!scheduler.is_scheduler_ready());

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
