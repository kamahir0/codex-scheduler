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

    pub fn generate_scheduler_plist_content(cli_path: &Path) -> String {
        let cli_str = cli_path.to_string_lossy();
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{cli_str}</string>
        <string>tick</string>
    </array>
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
            cli_str = cli_str,
        )
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
                    let _ = Command::new("launchctl")
                        .arg("unload")
                        .arg(&path)
                        .output();
                    let _ = fs::remove_file(&path);
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
    fn ensure_scheduler_installed(&self, cli_path: &Path) -> Result<(), SchedulerError> {
        if !self.launch_agents_dir.exists() {
            fs::create_dir_all(&self.launch_agents_dir)?;
        }

        // レガシーな個別ジョブplistがあれば一括クリーンアップ
        let _ = self.cleanup_legacy_job_plists();

        let plist_path = self.scheduler_plist_path();
        let expected_content = Self::generate_scheduler_plist_content(cli_path);

        // 既に同内容のplistが存在する場合は launchctl load をスキップ（冪等性 & 通知抑制）
        if plist_path.exists() {
            if let Ok(existing) = fs::read_to_string(&plist_path) {
                if existing == expected_content {
                    return Ok(());
                }
            }
        }

        // 新規作成または内容更新時のみ書き込み＆ロード
        fs::write(&plist_path, &expected_content)?;

        let output = Command::new("launchctl")
            .arg("load")
            .arg("-w")
            .arg(&plist_path)
            .output();

        if let Ok(out) = output {
            if !out.status.success() {
                let err = String::from_utf8_lossy(&out.stderr);
                eprintln!("Notice: launchctl load result for {}: {}", SCHEDULER_LABEL, err);
            }
        }

        Ok(())
    }

    fn is_scheduler_installed(&self) -> bool {
        self.scheduler_plist_path().exists()
    }

    fn uninstall_scheduler(&self) -> Result<(), SchedulerError> {
        let plist_path = self.scheduler_plist_path();
        if plist_path.exists() {
            let _ = Command::new("launchctl")
                .arg("unload")
                .arg(&plist_path)
                .output();
            let _ = fs::remove_file(&plist_path);
        }
        Ok(())
    }

    fn register_job(&self, _job: &Job, cli_path: &Path) -> Result<(), SchedulerError> {
        // 個別plistは作らず、単一常設スケジューラをensureするのみ
        self.ensure_scheduler_installed(cli_path)
    }

    fn unregister_job(&self, job_id: &str) -> Result<(), SchedulerError> {
        // 旧plistがもし残っていれば削除
        let legacy_path = self.legacy_job_plist_path(job_id);
        if legacy_path.exists() {
            let _ = Command::new("launchctl")
                .arg("unload")
                .arg(&legacy_path)
                .output();
            let _ = fs::remove_file(&legacy_path);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scheduler_plist_generation() {
        let plist = MacOsLaunchdScheduler::generate_scheduler_plist_content(Path::new(
            "/Users/test/.local/share/codex-scheduler/bin/codex-scheduler-cli",
        ));
        assert!(plist.contains(SCHEDULER_LABEL));
        assert!(plist.contains("<string>tick</string>"));
        assert!(plist.contains("<key>StartInterval</key>\n    <integer>60</integer>"));
        assert!(plist.contains("<key>RunAtLoad</key>\n    <true/>"));
    }

    #[test]
    fn test_ensure_scheduler_installed_idempotent() {
        let temp_dir = std::env::temp_dir().join(format!("test-launchd-{}", uuid::Uuid::new_v4()));
        let scheduler = MacOsLaunchdScheduler::with_dir(temp_dir.clone());
        let cli_path = Path::new("/bin/echo");

        // 1回目のインストール
        let res1 = scheduler.ensure_scheduler_installed(cli_path);
        assert!(res1.is_ok());
        let plist_file = scheduler.scheduler_plist_path();
        assert!(plist_file.exists());

        let content1 = fs::read_to_string(&plist_file).unwrap();
        assert!(content1.contains(SCHEDULER_LABEL));

        // 2回目の呼出（内容同一なら再ロードなしで成功）
        let res2 = scheduler.ensure_scheduler_installed(cli_path);
        assert!(res2.is_ok());

        // アンインストールテスト
        let uninst = scheduler.uninstall_scheduler();
        assert!(uninst.is_ok());
        assert!(!plist_file.exists());

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
}
