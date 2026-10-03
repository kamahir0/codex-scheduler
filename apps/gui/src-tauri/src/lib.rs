use chrono::{DateTime, Utc};
use codex_scheduler_core::models::{Job, ProviderType, RetryPolicy};
use codex_scheduler_core::store::JobStore;
use codex_scheduler_core::SchedulerService;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::State;

pub struct AppState {
    service: Mutex<SchedulerService>,
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
    pub cli_worker_path: String,
    pub cli_worker_installed: bool,
    pub scheduler_installed: bool,
    pub jobs_store_path: String,
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
        .map_err(|e| e.to_string())
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
    let (store, cli_path) = {
        let service = state.service.lock().map_err(|e| e.to_string())?;
        (service.store().clone(), service.cli_path().to_path_buf())
    };
    let service = SchedulerService::new(store, cli_path);
    service.execute_job(&id).await.map_err(|e| e.to_string())
}

#[tauri::command]
fn provision_worker(state: State<'_, AppState>) -> Result<String, String> {
    let path = codex_scheduler_core::worker::ensure_worker_installed()
        .map_err(|e| e.to_string())?;
    let mut service = state.service.lock().map_err(|e| e.to_string())?;
    let store = service.store().clone();
    *service = SchedulerService::new(store, path.clone());
    let _ = service.ensure_scheduler();
    Ok(path.to_string_lossy().to_string())
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

    let cli_worker_path = service.cli_path().to_string_lossy().to_string();
    let cli_worker_installed = service.cli_path().is_file();
    let scheduler_installed = service.is_scheduler_installed();
    let jobs_store_path = service.store().path().to_string_lossy().to_string();

    Ok(SystemInfo {
        os: std::env::consts::OS.to_string(),
        default_cwd,
        codex_installed,
        codex_path,
        cli_worker_path,
        cli_worker_installed,
        scheduler_installed,
        jobs_store_path,
    })
}

pub fn run() {
    let store = JobStore::default_store().expect("Failed to initialize job store");
    let worker_path = codex_scheduler_core::worker::ensure_worker_installed()
        .unwrap_or_else(|_| codex_scheduler_core::worker::canonical_worker_path());
    let service = SchedulerService::new(store, worker_path);
    // アプリ起動時に単一常設LaunchAgentを確実に登録（登録済みなら再ロードせずスキップ）
    let _ = service.ensure_scheduler();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            service: Mutex::new(service),
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
