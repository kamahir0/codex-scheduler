use super::{SchedulerBackend, SchedulerError};
use crate::models::Job;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const SCHEDULER_LABEL: &str = "dev.codexscheduler.scheduler";
pub const SCHEDULER_PLIST_FILENAME: &str = "dev.codexscheduler.scheduler.plist";
pub const LEGACY_PLIST_PREFIX: &str = "com.codexscheduler.job.";

pub struct MacOsLaunchdScheduler {
    launch_agents_dir: PathBuf,
}

impl MacOsLaunchdScheduler {
    pub fn new() -> Self {
        let dir = dirs::home_dir()
            .map(|h| h.join("Library/LaunchAgents"))
            .unwrap_or_else(|| PathBuf::from("/tmp/LaunchAgents"));
        Self {
            launch_agents_dir: dir,
        }
    }

    pub fn with_dir(dir: PathBuf) -> Self {
        Self {
            launch_agents_dir: dir,
        }
    }

    pub fn scheduler_plist_path(&self) -> PathBuf {
        self.launch_agents_dir.join(SCHEDULER_PLIST_FILENAME)
    }

    pub fn legacy_job_plist_path(&self, job_id: &str) -> PathBuf {
        self.launch_agents_dir
            .join(format!("{}{}.plist", LEGACY_PLIST_PREFIX, job_id))
    }

    // WHY: LaunchAgent must execute the app bundle's main executable with `--scheduler-tick`
    //      instead of a separate CLI worker binary, ensuring Gatekeeper authorization granted
    //      by the user to the GUI app covers background execution under ad-hoc distribution.
    // WHAT BREAKS: Invoking a separate binary causes macOS Gatekeeper rejection ("codex-scheduler-cli is not open")
    //              and breaks scheduled runs even when GUI app was authorized.
    // EVIDENCE: docs/adr/0003-macos-single-executable-headless-scheduler.md, OS-SCHED-001, DELIVERY-BUNDLE-002
    pub fn generate_scheduler_plist_content(app_executable_path: &Path) -> String {
        let app_str = app_executable_path.to_string_lossy();
        let home_dir = dirs::home_dir()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|| "/Users".to_string());
        let path_env = format!(
            "{}/.local/bin:{}/.cargo/bin:/opt/homebrew/bin:/opt/homebrew/sbin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin",
            home_dir, home_dir
        );

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

    /// Safely executes `launchctl unload <plist>` and ensures errors other than "not loaded" are strictly propagated.
    pub fn safe_launchctl_unload(plist_path: &Path) -> Result<(), SchedulerError> {
        let output = Command::new("launchctl")
            .arg("unload")
            .arg(plist_path)
            .output()?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            let err_trimmed = err.trim();
            // In macOS launchctl, unloading a service that is not currently registered/loaded
            // outputs messages like "Could not find specified service", "Not loaded", or similar.
            let is_not_loaded = err_trimmed.contains("Could not find specified service")
                || err_trimmed.contains("Not loaded")
                || err_trimmed.contains("No such process");
            if !is_not_loaded && !err_trimmed.is_empty() {
                return Err(SchedulerError::CommandFailed(format!(
                    "launchctl unload failed for {}: {}",
                    plist_path.display(),
                    err_trimmed
                )));
            }
        }
        Ok(())
    }

    /// 過去バージョンで作成されたジョブ個別plist（com.codexscheduler.job.*.plist）を検出してアンロード・削除
    pub fn cleanup_legacy_job_plists(&self) -> Result<(), SchedulerError> {
        if !self.launch_agents_dir.exists() {
            return Ok(());
        }

        let entries = match fs::read_dir(&self.launch_agents_dir) {
            Ok(e) => e,
            Err(_) => return Ok(()),
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                if file_name.starts_with(LEGACY_PLIST_PREFIX) && file_name.ends_with(".plist") {
                    Self::safe_launchctl_unload(&path)?;
                    fs::remove_file(&path)?;
                }
            }
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
    fn ensure_scheduler_installed(&self, app_executable_path: &Path) -> Result<(), SchedulerError> {
        // Guard against registering CLI binary as LaunchAgent target (which causes Gatekeeper rejections)
        if let Some(name) = app_executable_path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with("codex-scheduler-cli") {
                return Err(SchedulerError::DesktopAppRequired);
            }
        }

        if !app_executable_path.is_absolute() {
            return Err(SchedulerError::InvalidExecutable(
                app_executable_path.display().to_string(),
            ));
        }

        if !app_executable_path.exists() {
            return Err(SchedulerError::ExecutableNotFound(
                app_executable_path.display().to_string(),
            ));
        }

        if !self.launch_agents_dir.exists() {
            fs::create_dir_all(&self.launch_agents_dir)?;
        }

        // レガシーな個別ジョブplistがあれば一括クリーンアップ
        self.cleanup_legacy_job_plists()?;

        let plist_path = self.scheduler_plist_path();
        let expected_content = Self::generate_scheduler_plist_content(app_executable_path);

        // 既に同内容のplistが存在し、かつ正常にlaunchctlに登録されていれば再登録せずスキップ（通知抑制）
        if plist_path.exists() {
            if let Ok(existing) = fs::read_to_string(&plist_path) {
                if existing == expected_content {
                    let check = Command::new("launchctl")
                        .arg("list")
                        .arg(SCHEDULER_LABEL)
                        .output();
                    if let Ok(out) = check {
                        if out.status.success() {
                            return Ok(());
                        }
                    }
                }
            }
        }

        // 既存の登録があれば確実にアンロード（古いWorkerパスからの移行や移動時）
        if plist_path.exists() {
            Self::safe_launchctl_unload(&plist_path)?;
        }

        // 新規作成または内容更新時のみ書き込み＆ロード
        fs::write(&plist_path, &expected_content)?;

        let output = Command::new("launchctl")
            .arg("load")
            .arg("-w")
            .arg(&plist_path)
            .output()?;

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

    fn is_scheduler_path_matched(&self, app_executable_path: &Path) -> bool {
        let plist_path = self.scheduler_plist_path();
        if !plist_path.exists() {
            return false;
        }
        if let Ok(content) = fs::read_to_string(&plist_path) {
            let path_str = app_executable_path.to_string_lossy();
            content.contains(&*path_str) && content.contains("<string>--scheduler-tick</string>")
        } else {
            false
        }
    }

    fn uninstall_scheduler(&self) -> Result<(), SchedulerError> {
        let plist_path = self.scheduler_plist_path();
        if plist_path.exists() {
            Self::safe_launchctl_unload(&plist_path)?;
            fs::remove_file(&plist_path)?;
        }
        Ok(())
    }

    fn register_job(&self, _job: &Job, app_executable_path: &Path) -> Result<(), SchedulerError> {
        self.ensure_scheduler_installed(app_executable_path)
    }

    fn unregister_job(&self, job_id: &str) -> Result<(), SchedulerError> {
        let legacy_path = self.legacy_job_plist_path(job_id);
        if legacy_path.exists() {
            Self::safe_launchctl_unload(&legacy_path)?;
            fs::remove_file(&legacy_path)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scheduler_plist_generation() {
        let app_path = Path::new("/Applications/Codex Scheduler.app/Contents/MacOS/codex-scheduler-gui");
        let plist = MacOsLaunchdScheduler::generate_scheduler_plist_content(app_path);
        assert!(plist.contains(SCHEDULER_LABEL));
        assert!(plist.contains("<string>/Applications/Codex Scheduler.app/Contents/MacOS/codex-scheduler-gui</string>"));
        assert!(plist.contains("<string>--scheduler-tick</string>"));
        assert!(!plist.contains("codex-scheduler-cli"));
        assert!(plist.contains("<key>StartInterval</key>\n    <integer>60</integer>"));
        assert!(plist.contains("<key>RunAtLoad</key>\n    <true/>"));
        assert!(plist.contains("<key>EnvironmentVariables</key>"));
        assert!(plist.contains("<key>PATH</key>"));
        assert!(plist.contains("<key>HOME</key>"));
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
        let temp_dir = std::env::temp_dir().join(format!("test-launchd-val-{}", uuid::Uuid::new_v4()));
        let scheduler = MacOsLaunchdScheduler::with_dir(temp_dir.clone());

        // 1. Non-absolute path
        let err1 = scheduler.ensure_scheduler_installed(Path::new("relative/path")).unwrap_err();
        match err1 {
            SchedulerError::InvalidExecutable(_) => {}
            _ => panic!("Expected InvalidExecutable error, got: {:?}", err1),
        }

        // 2. Missing executable
        let err2 = scheduler.ensure_scheduler_installed(Path::new("/non/existent/path/binary")).unwrap_err();
        match err2 {
            SchedulerError::ExecutableNotFound(_) => {}
            _ => panic!("Expected ExecutableNotFound error, got: {:?}", err2),
        }

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_legacy_worker_plist_migration() {
        let temp_dir = std::env::temp_dir().join(format!("test-migration-{}", uuid::Uuid::new_v4()));
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
    fn test_rejects_cli_binary_registration() {
        let temp_dir = std::env::temp_dir().join(format!("test-cli-reject-{}", uuid::Uuid::new_v4()));
        let scheduler = MacOsLaunchdScheduler::with_dir(temp_dir.clone());
        let cli_path = Path::new("/usr/local/bin/codex-scheduler-cli");

        let err = scheduler.ensure_scheduler_installed(cli_path).unwrap_err();
        match err {
            SchedulerError::DesktopAppRequired => {}
            _ => panic!("Expected DesktopAppRequired error, got: {:?}", err),
        }

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
