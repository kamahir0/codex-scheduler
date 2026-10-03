pub mod macos;

use crate::models::Job;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SchedulerOwner {
    Desktop,
    Cli,
    None,
    Legacy,
    Invalid,
}

impl std::fmt::Display for SchedulerOwner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SchedulerOwner::Desktop => write!(f, "desktop"),
            SchedulerOwner::Cli => write!(f, "cli"),
            SchedulerOwner::None => write!(f, "none"),
            SchedulerOwner::Legacy => write!(f, "legacy"),
            SchedulerOwner::Invalid => write!(f, "invalid"),
        }
    }
}

#[derive(Debug, Error)]
pub enum SchedulerError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("OS schedule registration failed: {0}")]
    CommandFailed(String),
    #[error("Home directory not found")]
    HomeNotFound,
    #[error("Executable path must be absolute: {0}")]
    InvalidExecutable(String),
    #[error("Expected executable does not exist: {0}")]
    ExecutableNotFound(String),
    #[error("On macOS, LaunchAgent is owned by Desktop application and cannot be uninstalled via CLI")]
    DesktopOwnerProtected,
    #[error("Malformed or unknown scheduler configuration: {0}")]
    MalformedConfiguration(String),
    #[error("On macOS, scheduler registration must be performed via Codex Scheduler.app before using CLI scheduling")]
    DesktopAppRequired,
}

pub trait SchedulerBackend: Send + Sync {
    /// 常設スケジューラサービス（LaunchAgent等）が登録され最新であることを保証する
    fn ensure_scheduler_installed(&self, exe_path: &Path) -> Result<(), SchedulerError>;

    /// 常設スケジューラサービスが登録されているか確認する
    fn is_scheduler_installed(&self) -> bool;

    /// 常設スケジューラサービスが現在のアーキテクチャで実際に予約実行可能（正常に稼働可能）な状態か判定する
    fn is_scheduler_ready(&self) -> bool {
        self.is_scheduler_installed()
    }

    /// 登録済みのスケジューラサービスが指定された実行ファイルパスと一致しているか確認する
    fn is_scheduler_path_matched(&self, exe_path: &Path) -> bool;

    /// 現在のスケジューラ所有者を取得する
    fn get_scheduler_owner(&self) -> SchedulerOwner {
        SchedulerOwner::None
    }

    /// 登録されているスケジューラ実行ファイルパスを取得する
    fn get_scheduler_executable_path(&self) -> Option<std::path::PathBuf> {
        None
    }

    /// 常設スケジューラサービスを登録解除・アンインストールする
    fn uninstall_scheduler(&self) -> Result<(), SchedulerError>;

    /// CLIツールからの安全なアンインストール（Desktop所有時は保護エラーを返す）
    fn uninstall_scheduler_as_cli(&self) -> Result<(), SchedulerError> {
        if self.get_scheduler_owner() == SchedulerOwner::Desktop {
            return Err(SchedulerError::DesktopOwnerProtected);
        }
        self.uninstall_scheduler()
    }

    /// ジョブごとの登録（互換用）
    fn register_job(&self, job: &Job, exe_path: &Path) -> Result<(), SchedulerError>;
    fn unregister_job(&self, job_id: &str) -> Result<(), SchedulerError>;
}

pub fn get_platform_scheduler() -> Box<dyn SchedulerBackend> {
    #[cfg(target_os = "macos")]
    {
        Box::new(macos::MacOsLaunchdScheduler::new())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Box::new(FallbackScheduler)
    }
}

/// non-macOS環境向けのフォールバック実装。
/// 現行バージョンでは常設OSスケジューラ（Windows Task Scheduler等）は未対応であり、
/// 診断系（is_scheduler_installed, is_scheduler_ready, is_scheduler_path_matched）はfalseを返し、
/// 所有者はSchedulerOwner::Noneを返す。
///
/// WHY: Windowsなど未実装のプラットフォームで存在しないスケジューラをReadyと誤認させないため。
/// 一方で、ジョブ作成や手動実行などのJobStore操作自体を阻害しないよう、
/// ensure_scheduler_installedやregister_jobはno-op Ok(())を返す。
/// EVIDENCE: test_fallback_scheduler_diagnostics_unsupported
pub struct FallbackScheduler;

impl SchedulerBackend for FallbackScheduler {
    fn ensure_scheduler_installed(&self, _exe_path: &Path) -> Result<(), SchedulerError> {
        Ok(())
    }

    fn is_scheduler_installed(&self) -> bool {
        false
    }

    fn is_scheduler_ready(&self) -> bool {
        false
    }

    fn is_scheduler_path_matched(&self, _exe_path: &Path) -> bool {
        false
    }

    fn get_scheduler_owner(&self) -> SchedulerOwner {
        SchedulerOwner::None
    }

    fn get_scheduler_executable_path(&self) -> Option<std::path::PathBuf> {
        None
    }

    fn uninstall_scheduler(&self) -> Result<(), SchedulerError> {
        Ok(())
    }

    fn register_job(&self, _job: &Job, _exe_path: &Path) -> Result<(), SchedulerError> {
        Ok(())
    }

    fn unregister_job(&self, _job_id: &str) -> Result<(), SchedulerError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_fallback_scheduler_diagnostics_unsupported() {
        let scheduler = FallbackScheduler;
        let dummy_path = Path::new("/dummy/path/codex-scheduler");

        assert!(!scheduler.is_scheduler_installed());
        assert!(!scheduler.is_scheduler_ready());
        assert!(!scheduler.is_scheduler_path_matched(dummy_path));
        assert_eq!(scheduler.get_scheduler_owner(), SchedulerOwner::None);
        assert_eq!(scheduler.get_scheduler_executable_path(), None);

        // JobStore登録や実行を阻害しないよう、no-opとして成功すること
        assert!(scheduler.ensure_scheduler_installed(dummy_path).is_ok());
        assert!(scheduler.uninstall_scheduler().is_ok());
        assert!(scheduler.unregister_job("job-1").is_ok());
    }
}

