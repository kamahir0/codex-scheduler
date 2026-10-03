use chrono::{DateTime, Utc};
use codex_scheduler_core::models::{Job, ProviderType, RetryPolicy};
use codex_scheduler_core::store::JobStore;
use codex_scheduler_core::SchedulerService;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::State;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum AppExecutionMode {
    Gui,
    HeadlessSchedulerTick,
}

/// Pure function to route execution mode from CLI arguments.
pub fn parse_execution_mode<I, T>(args: I) -> AppExecutionMode
where
    I: IntoIterator<Item = T>,
    T: AsRef<str>,
{
    for arg in args {
        if arg.as_ref() == "--scheduler-tick" {
            return AppExecutionMode::HeadlessSchedulerTick;
        }
    }
    AppExecutionMode::Gui
}

pub struct AppState {
    service: Mutex<SchedulerService>,
    scheduler_error: Mutex<Option<String>>,
}

#[derive(Debug, Deserialize)]
pub struct CreateJobPayload {
    pub provider: Option<String>,
    pub session_id: String,
    pub cwd: String,
    pub prompt: Option<String>,
    pub scheduled_at: String, // ISO 8601
    pub retry_enabled: bool,
    pub retry_interval_seconds: u64,
    pub max_attempts: u32,
}

#[derive(Debug, Serialize)]
pub struct SystemInfo {
    pub os: String,
    pub default_cwd: String,
    pub codex_installed: bool,
    pub codex_path: Option<String>,
    pub scheduler_executable_path: String,
    pub scheduler_installed: bool,
    pub scheduler_ready: bool,
    pub scheduler_path_matched: bool,
    pub scheduler_error: Option<String>,
    pub scheduler_owner: String,
    pub jobs_store_path: String,
    // Backward compatibility fields for UI
    pub cli_worker_path: String,
    pub cli_worker_installed: bool,
}

#[tauri::command]
fn list_jobs(state: State<'_, AppState>) -> Result<Vec<Job>, String> {
    let service = state.service.lock().map_err(|e| e.to_string())?;
    service.store().load_all().map_err(|e| e.to_string())
}

#[tauri::command]
fn get_job(id: String, state: State<'_, AppState>) -> Result<Option<Job>, String> {
    let service = state.service.lock().map_err(|e| e.to_string())?;
    service.store().get_job(&id).map_err(|e| e.to_string())
}

#[tauri::command]
fn create_job(payload: CreateJobPayload, state: State<'_, AppState>) -> Result<Job, String> {
    let service = state.service.lock().map_err(|e| e.to_string())?;

    let scheduled_at = DateTime::parse_from_rfc3339(&payload.scheduled_at)
        .map_err(|e| format!("Invalid scheduled_at format: {}", e))?
        .with_timezone(&Utc);

    let cwd = PathBuf::from(&payload.cwd);
    let policy = RetryPolicy {
        enabled: payload.retry_enabled,
        interval_seconds: payload.retry_interval_seconds,
        max_attempts: payload.max_attempts,
        retry_on_quota_only: true,
    };

    let provider = match payload.provider.as_deref() {
        Some("claude") => ProviderType::Claude,
        Some(custom) if custom != "codex" => ProviderType::Custom(custom.to_string()),
        _ => ProviderType::Codex,
    };

    service
        .schedule_job(
            provider,
            payload.session_id,
            cwd,
            payload.prompt,
            scheduled_at,
            Some(policy),
        )
        .map_err(|e| format!("スケジューラ登録またはジョブ作成に失敗しました: {}", e))
}

#[tauri::command]
fn cancel_job(id: String, state: State<'_, AppState>) -> Result<Job, String> {
    let service = state.service.lock().map_err(|e| e.to_string())?;
    service.cancel_job(&id).map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_job(id: String, state: State<'_, AppState>) -> Result<bool, String> {
    let service = state.service.lock().map_err(|e| e.to_string())?;
    service.delete_job(&id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn run_job_now(id: String, state: State<'_, AppState>) -> Result<Job, String> {
    let (store, exe_path) = {
        let service = state.service.lock().map_err(|e| e.to_string())?;
        (service.store().clone(), service.exe_path().to_path_buf())
    };
    let service = SchedulerService::new(store, exe_path);
    service.execute_job(&id).await.map_err(|e| e.to_string())
}

#[tauri::command]
fn provision_worker(state: State<'_, AppState>) -> Result<String, String> {
    let service = state.service.lock().map_err(|e| e.to_string())?;
    service.ensure_scheduler().map_err(|e| e.to_string())?;
    if let Ok(mut err_lock) = state.scheduler_error.lock() {
        *err_lock = None;
    }
    Ok(service.exe_path().to_string_lossy().to_string())
}

#[tauri::command]
fn get_system_info(state: State<'_, AppState>) -> Result<SystemInfo, String> {
    let service = state.service.lock().map_err(|e| e.to_string())?;
    let default_cwd = dirs::home_dir()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|| "/".to_string());

    let adapter = codex_scheduler_core::adapter::codex::CodexAdapter::new();
    let codex_res = adapter.resolve_executable();
    let (codex_installed, codex_path) = match codex_res {
        Ok(p) => {
            let exists = p.exists() || p.to_string_lossy() == "codex";
            (exists, Some(p.to_string_lossy().to_string()))
        }
        Err(_) => (false, None),
    };

    let scheduler_executable_path = service.exe_path().to_string_lossy().to_string();
    let scheduler_installed = service.is_scheduler_installed();
    let scheduler_ready = service.is_scheduler_ready();
    let scheduler_path_matched = service.is_scheduler_path_matched();
    let scheduler_owner = service.get_scheduler_owner().to_string();
    let scheduler_error = state.scheduler_error.lock().ok().and_then(|e| e.clone());
    let jobs_store_path = service.store().path().to_string_lossy().to_string();

    Ok(SystemInfo {
        os: std::env::consts::OS.to_string(),
        default_cwd,
        codex_installed,
        codex_path,
        scheduler_executable_path: scheduler_executable_path.clone(),
        scheduler_installed,
        scheduler_ready,
        scheduler_path_matched,
        scheduler_owner,
        scheduler_error,
        jobs_store_path,
        cli_worker_path: scheduler_executable_path,
        cli_worker_installed: scheduler_installed,
    })
}

/// Runs the headless scheduler tick process.
/// In this mode:
/// - No Tauri GUI window is created
/// - No Dock icon is displayed as a GUI application
/// - No webview or frontend is loaded
/// - No GUI dialog plugins are initialized
/// - Directly executes due jobs via codex-scheduler-core shared semantics and exits cleanly
pub fn run_headless_tick() {
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("[Headless Scheduler] Failed to initialize runtime: {}", e);
            std::process::exit(1);
        }
    };

    rt.block_on(async {
        let store = match JobStore::default_store() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[Headless Scheduler] Failed to open job store: {}", e);
                std::process::exit(1);
            }
        };

        let exe_path = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("codex-scheduler-gui"));
        let service = SchedulerService::new(store, exe_path);

        match service.execute_tick().await {
            Ok(finished) => {
                if !finished.is_empty() {
                    println!("[Headless Scheduler] Executed {} due job(s).", finished.len());
                }
            }
            Err(e) => {
                eprintln!("[Headless Scheduler] Tick execution error: {}", e);
                std::process::exit(1);
            }
        }
    });
}

pub fn run() {
    let store = JobStore::default_store().expect("Failed to initialize job store");
    let exe_path = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("codex-scheduler-gui"));
    let service = SchedulerService::new(store, exe_path);

    // アプリ起動時に単一常設LaunchAgentを確実に登録（登録済みかつ同内容なら再ロードせずスキップ）
    let scheduler_error = match service.ensure_scheduler() {
        Ok(()) => None,
        Err(e) => {
            eprintln!("[Scheduler] Warning: Failed to ensure OS scheduler on startup: {}", e);
            Some(e.to_string())
        }
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            service: Mutex::new(service),
            scheduler_error: Mutex::new(scheduler_error),
        })
        .invoke_handler(tauri::generate_handler![
            list_jobs,
            get_job,
            create_job,
            cancel_job,
            delete_job,
            run_job_now,
            get_system_info,
            provision_worker
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_execution_mode_routing() {
        // Flag absent -> Gui
        assert_eq!(
            parse_execution_mode(vec!["codex-scheduler-gui"]),
            AppExecutionMode::Gui
        );
        assert_eq!(
            parse_execution_mode(vec!["codex-scheduler-gui", "--verbose"]),
            AppExecutionMode::Gui
        );

        // Flag present -> HeadlessSchedulerTick
        assert_eq!(
            parse_execution_mode(vec!["codex-scheduler-gui", "--scheduler-tick"]),
            AppExecutionMode::HeadlessSchedulerTick
        );
        assert_eq!(
            parse_execution_mode(vec!["/Applications/Codex Scheduler.app/Contents/MacOS/codex-scheduler-gui", "--scheduler-tick"]),
            AppExecutionMode::HeadlessSchedulerTick
        );
    }
}
