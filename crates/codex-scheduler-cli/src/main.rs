use chrono::{DateTime, Duration, TimeZone, Utc};
use clap::{Parser, Subcommand};
use codex_scheduler_core::models::{ProviderType, RetryPolicy};
use codex_scheduler_core::os_scheduler::SchedulerOwner;
use codex_scheduler_core::store::JobStore;
use codex_scheduler_core::{CoreError, SchedulerService};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "codex-scheduler")]
#[command(about = "Scheduled resume and prompt automation for Codex sessions", long_about = None)]
#[command(version)]
struct Cli {
    /// Run headless scheduler tick (invoked periodically by OS scheduler)
    #[arg(long = "scheduler-tick", global = true)]
    scheduler_tick: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, PartialEq, Eq, Debug)]
enum Commands {
    /// Execute a scheduled job as a worker process (manual or worker debug)
    RunJob {
        /// Job UUID
        job_id: String,
    },
    /// Schedule a new prompt execution for a session
    Schedule {
        /// Session ID to resume
        #[arg(short, long)]
        session_id: String,

        /// Working directory of the project
        #[arg(short, long)]
        cwd: PathBuf,

        /// Prompt to send (default: "continue")
        #[arg(short, long, default_value = "continue")]
        prompt: String,

        /// Target schedule time in ISO 8601 (e.g. 2026-10-03T02:05:00Z) or offset in minutes (e.g. +120)
        #[arg(short, long)]
        at: String,

        /// Retry interval in seconds (default: 300 = 5 min)
        #[arg(long, default_value_t = 300)]
        retry_interval: u64,

        /// Max attempts (default: 6)
        #[arg(long, default_value_t = 6)]
        max_attempts: u32,

        /// Output as raw JSON
        #[arg(long)]
        json: bool,
    },
    /// List all scheduled and past jobs
    List {
        /// Output as raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Show details and execution history of a specific job
    Show {
        /// Job UUID
        job_id: String,

        /// Output as raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Cancel a scheduled or retrying job
    Cancel {
        /// Job UUID
        job_id: String,

        /// Output as raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Delete a job from the database
    Delete {
        /// Job UUID
        job_id: String,

        /// Output as raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Show system, scheduler, and ownership status
    Status {
        /// Output as raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Check and execute any scheduled jobs that have reached their target time
    Tick {
        /// Output as raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Ensure the persistent OS scheduler LaunchAgent/Task is installed
    InstallScheduler {
        /// Output as raw JSON
        #[arg(long)]
        json: bool,
    },
    /// Uninstall the persistent OS scheduler LaunchAgent/Task
    UninstallScheduler {
        /// Output as raw JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Serialize)]
struct StatusOutput {
    version: String,
    store_path: String,
    scheduler: SchedulerStatusOutput,
    platform: String,
}

#[derive(Serialize)]
struct SchedulerStatusOutput {
    installed: bool,
    ready: bool,
    owner: SchedulerOwner,
    executable: Option<String>,
    path_matched: bool,
}

fn print_error_and_exit(code: &str, msg: &str, json: bool) -> ! {
    if json {
        let err_json = serde_json::json!({
            "error": {
                "code": code,
                "message": msg,
            }
        });
        eprintln!("{}", serde_json::to_string_pretty(&err_json).unwrap());
    } else {
        eprintln!("Error: {}", msg);
    }
    std::process::exit(1);
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let store = match JobStore::default_store() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error initializing job store: {}", e);
            std::process::exit(1);
        }
    };
    let service = SchedulerService::new_for_cli(store);

    // If --scheduler-tick flag is supplied at root level, execute tick immediately
    if cli.scheduler_tick {
        let executed = service.execute_tick().await?;
        if !executed.is_empty() {
            println!("[Tick] Completed execution for {} due job(s).", executed.len());
        }
        return Ok(());
    }

    let command = match cli.command {
        Some(cmd) => cmd,
        None => {
            eprintln!("No command specified. Use --help to see available commands.");
            std::process::exit(1);
        }
    };

    match command {
        Commands::RunJob { job_id } => {
            println!("[Worker] Starting execution for job: {}", job_id);
            let job = service.execute_job(&job_id).await?;
            println!("[Worker] Job execution finished. Final status: {:?}", job.status);
            if let Some(last) = job.execution_history.last() {
                println!("[Worker] Exit code: {:?}", last.exit_code);
                if last.is_quota_error {
                    println!("[Worker] Quota error detected. Next retry: {:?}", job.scheduled_at);
                }
            }
        }
        Commands::Schedule {
            session_id,
            cwd,
            prompt,
            at,
            retry_interval,
            max_attempts,
            json,
        } => {
            let scheduled_at = match parse_time_arg(&at) {
                Ok(dt) => dt,
                Err(e) => print_error_and_exit("invalid_time_format", &e.to_string(), json),
            };
            let canonical_cwd = std::fs::canonicalize(&cwd).unwrap_or(cwd);

            let policy = RetryPolicy {
                enabled: true,
                interval_seconds: retry_interval,
                max_attempts,
                retry_on_quota_only: true,
            };

            let job = match service.schedule_job(
                ProviderType::Codex,
                session_id,
                canonical_cwd,
                Some(prompt),
                scheduled_at,
                Some(policy),
            ) {
                Ok(j) => j,
                Err(e) => print_error_and_exit("schedule_failed", &e.to_string(), json),
            };

            if json {
                println!("{}", serde_json::to_string_pretty(&job)?);
            } else {
                println!("Job successfully scheduled!");
                println!("  ID:           {}", job.id);
                println!("  Session ID:   {}", job.session_id);
                println!("  Working dir:  {}", job.cwd.display());
                println!("  Prompt:       {}", job.prompt);
                println!("  Scheduled at: {} ({})", job.scheduled_at, relative_time(job.scheduled_at));
            }
        }
        Commands::List { json } => {
            match service.store().load_all() {
                Ok(jobs) => {
                    if json {
                        println!("{}", serde_json::to_string_pretty(&jobs)?);
                    } else if jobs.is_empty() {
                        println!("No jobs found. Use 'schedule' to add a new job.");
                    } else {
                        println!("{:<36} {:<10} {:<16} {:<20} {:<10}", "JOB ID", "STATUS", "SESSION", "SCHEDULED AT", "ATTEMPTS");
                        println!("{}", "-".repeat(96));
                        for j in jobs {
                            let attempts = format!("{}/{}", j.execution_history.len(), j.retry_policy.max_attempts);
                            let session_short = if j.session_id.len() > 14 {
                                format!("{}...", &j.session_id[..12])
                            } else {
                                j.session_id.clone()
                            };
                            println!(
                                "{:<36} {:<10?} {:<16} {:<20} {:<10}",
                                j.id,
                                j.status,
                                session_short,
                                j.scheduled_at.format("%Y-%m-%d %H:%M"),
                                attempts
                            );
                        }
                    }
                }
                Err(e) => print_error_and_exit("store_error", &e.to_string(), json),
            }
        }
        Commands::Show { job_id, json } => {
            match service.store().get_job(&job_id) {
                Ok(Some(job)) => {
                    if json {
                        println!("{}", serde_json::to_string_pretty(&job)?);
                    } else {
                        println!("Job Details:");
                        println!("  ID:           {}", job.id);
                        println!("  Provider:     {:?}", job.provider);
                        println!("  Session ID:   {}", job.session_id);
                        println!("  Working dir:  {}", job.cwd.display());
                        println!("  Prompt:       {}", job.prompt);
                        println!("  Status:       {:?}", job.status);
                        println!("  Scheduled at: {}", job.scheduled_at);
                        println!("  Created at:   {}", job.created_at);
                        println!("  Updated at:   {}", job.updated_at);
                        println!("  History:      {} attempts", job.execution_history.len());
                        for (i, att) in job.execution_history.iter().enumerate() {
                            println!("\n  --- Attempt #{} ---", i + 1);
                            println!("    Started:        {}", att.started_at);
                            println!("    Finished:       {}", att.finished_at);
                            println!("    Exit Code:      {:?}", att.exit_code);
                            println!("    Quota Error:    {}", att.is_quota_error);
                            if let Some(ref err) = att.error_message {
                                println!("    Error Message:  {}", err);
                            }
                            if !att.stdout.trim().is_empty() {
                                println!("    Stdout:         {}", att.stdout.trim());
                            }
                            if !att.stderr.trim().is_empty() {
                                println!("    Stderr:         {}", att.stderr.trim());
                            }
                        }
                    }
                }
                Ok(None) => print_error_and_exit("job_not_found", &format!("Job '{}' not found", job_id), json),
                Err(e) => print_error_and_exit("store_error", &e.to_string(), json),
            }
        }
        Commands::Cancel { job_id, json } => {
            match service.cancel_job(&job_id) {
                Ok(job) => {
                    if json {
                        println!("{}", serde_json::to_string_pretty(&job)?);
                    } else {
                        println!("Job {} has been cancelled.", job.id);
                    }
                }
                Err(CoreError::JobNotFound(_)) => {
                    print_error_and_exit("job_not_found", &format!("Job '{}' not found", job_id), json);
                }
                Err(e) => print_error_and_exit("cancel_failed", &e.to_string(), json),
            }
        }
        Commands::Delete { job_id, json } => {
            match service.delete_job(&job_id) {
                Ok(true) => {
                    if json {
                        println!("{}", serde_json::json!({
                            "deleted": true,
                            "job_id": job_id
                        }));
                    } else {
                        println!("Job {} has been deleted.", job_id);
                    }
                }
                Ok(false) => {
                    print_error_and_exit("job_not_found", &format!("Job '{}' was not found.", job_id), json);
                }
                Err(e) => print_error_and_exit("delete_failed", &e.to_string(), json),
            }
        }
        Commands::Status { json } => {
            let output = StatusOutput {
                version: env!("CARGO_PKG_VERSION").to_string(),
                store_path: service.store().path().display().to_string(),
                scheduler: SchedulerStatusOutput {
                    installed: service.is_scheduler_installed(),
                    ready: service.is_scheduler_ready(),
                    owner: service.get_scheduler_owner(),
                    executable: service.get_scheduler_executable_path().map(|p| p.display().to_string()),
                    path_matched: service.is_scheduler_path_matched(),
                },
                platform: std::env::consts::OS.to_string(),
            };

            if json {
                println!("{}", serde_json::to_string_pretty(&output)?);
            } else {
                println!("Codex Scheduler v{}", output.version);
                println!("Platform:        {}", output.platform);
                println!("Store Path:      {}", output.store_path);
                println!("Scheduler:");
                println!("  Installed:     {}", output.scheduler.installed);
                println!("  Ready:         {}", output.scheduler.ready);
                println!("  Owner:         {}", output.scheduler.owner);
                if let Some(ref exe) = output.scheduler.executable {
                    println!("  Executable:    {}", exe);
                }
                println!("  Path Matched:  {}", output.scheduler.path_matched);
            }
        }
        Commands::Tick { json } => {
            let executed = service.execute_tick().await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&executed)?);
            } else if !executed.is_empty() {
                println!("[Tick] Completed execution for {} due job(s).", executed.len());
            }
        }
        Commands::InstallScheduler { json } => {
            let owner = service.get_scheduler_owner();
            if owner == SchedulerOwner::Desktop {
                if json {
                    println!("{}", serde_json::json!({
                        "status": "retained",
                        "owner": "desktop",
                        "message": "Desktop-owned scheduler is already active. Retaining Desktop ownership."
                    }));
                } else {
                    println!("Notice: Desktop-owned scheduler is already active. Retaining Desktop ownership.");
                }
            } else {
                match service.ensure_scheduler() {
                    Ok(()) => {
                        let new_owner = service.get_scheduler_owner();
                        if json {
                            println!("{}", serde_json::json!({
                                "status": "installed",
                                "owner": new_owner,
                                "message": "Persistent OS scheduler service installed and active."
                            }));
                        } else {
                            println!("Persistent OS scheduler service installed and active ({}).", new_owner);
                        }
                    }
                    Err(e) => print_error_and_exit("install_scheduler_failed", &e.to_string(), json),
                }
            }
        }
        Commands::UninstallScheduler { json } => {
            match service.scheduler().uninstall_scheduler_as_cli() {
                Ok(()) => {
                    if json {
                        println!("{}", serde_json::json!({
                            "uninstalled": true,
                            "message": "Persistent OS scheduler service uninstalled."
                        }));
                    } else {
                        println!("Persistent OS scheduler service uninstalled.");
                    }
                }
                Err(codex_scheduler_core::os_scheduler::SchedulerError::DesktopOwnerProtected) => {
                    print_error_and_exit(
                        "desktop_owner_protected",
                        "LaunchAgent is owned by Desktop application and cannot be uninstalled via CLI.",
                        json,
                    );
                }
                Err(e) => print_error_and_exit("uninstall_failed", &e.to_string(), json),
            }
        }
    }

    Ok(())
}

fn parse_time_arg(at: &str) -> Result<DateTime<Utc>, Box<dyn std::error::Error>> {
    let trimmed = at.trim();
    if trimmed.starts_with('+') {
        let mins: i64 = trimmed[1..].parse()?;
        return Ok(Utc::now() + Duration::minutes(mins));
    }

    // Try ISO 8601
    if let Ok(dt) = DateTime::parse_from_rfc3339(trimmed) {
        return Ok(dt.with_timezone(&Utc));
    }

    // Try local format YYYY-MM-DD HH:MM
    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M") {
        if let Some(local_dt) = chrono::Local::now().timezone().from_local_datetime(&naive).single() {
            return Ok(local_dt.with_timezone(&Utc));
        }
    }

    Err(format!("Unable to parse schedule time '{}'. Use ISO 8601 or +<minutes>", at).into())
}

fn relative_time(dt: DateTime<Utc>) -> String {
    let now = Utc::now();
    let diff = dt.signed_duration_since(now);
    if diff.num_seconds() < 0 {
        "in the past".to_string()
    } else if diff.num_hours() > 0 {
        format!("in ~{} hours", diff.num_hours())
    } else {
        format!("in ~{} minutes", diff.num_minutes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_time_arg() {
        let plus = parse_time_arg("+30").unwrap();
        let diff = plus.signed_duration_since(Utc::now()).num_minutes();
        assert!(diff >= 29 && diff <= 31);

        let rfc = parse_time_arg("2026-10-03T12:00:00Z").unwrap();
        assert_eq!(rfc.to_rfc3339(), "2026-10-03T12:00:00+00:00");
    }

    #[test]
    fn test_cli_parse_subcommands() {
        let parsed = Cli::try_parse_from(["codex-scheduler", "list", "--json"]).unwrap();
        assert_eq!(parsed.command, Some(Commands::List { json: true }));

        let parsed_tick = Cli::try_parse_from(["codex-scheduler", "--scheduler-tick"]).unwrap();
        assert!(parsed_tick.scheduler_tick);

        let parsed_status = Cli::try_parse_from(["codex-scheduler", "status", "--json"]).unwrap();
        assert_eq!(parsed_status.command, Some(Commands::Status { json: true }));

        let parsed_sched = Cli::try_parse_from([
            "codex-scheduler",
            "schedule",
            "--session-id",
            "sess-123",
            "--cwd",
            "/tmp",
            "--prompt",
            "test prompt",
            "--at",
            "+10",
            "--json",
        ])
        .unwrap();
        assert_eq!(
            parsed_sched.command,
            Some(Commands::Schedule {
                session_id: "sess-123".to_string(),
                cwd: PathBuf::from("/tmp"),
                prompt: "test prompt".to_string(),
                at: "+10".to_string(),
                retry_interval: 300,
                max_attempts: 6,
                json: true,
            })
        );

        let parsed_cancel = Cli::try_parse_from(["codex-scheduler", "cancel", "job-xyz", "--json"]).unwrap();
        assert_eq!(
            parsed_cancel.command,
            Some(Commands::Cancel {
                job_id: "job-xyz".to_string(),
                json: true,
            })
        );

        let parsed_install = Cli::try_parse_from(["codex-scheduler", "install-scheduler", "--json"]).unwrap();
        assert_eq!(
            parsed_install.command,
            Some(Commands::InstallScheduler { json: true })
        );

        let parsed_uninstall = Cli::try_parse_from(["codex-scheduler", "uninstall-scheduler", "--json"]).unwrap();
        assert_eq!(
            parsed_uninstall.command,
            Some(Commands::UninstallScheduler { json: true })
        );
    }

    #[test]
    fn test_cli_status_serialization() {
        let status = StatusOutput {
            version: "0.3.0".to_string(),
            store_path: "/path/to/jobs.json".to_string(),
            scheduler: SchedulerStatusOutput {
                installed: true,
                ready: true,
                owner: SchedulerOwner::Desktop,
                executable: Some("/Applications/Codex Scheduler.app/Contents/MacOS/codex-scheduler-gui".to_string()),
                path_matched: true,
            },
            platform: "macos".to_string(),
        };
        let json_str = serde_json::to_string(&status).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(value["scheduler"]["owner"], "desktop");
        assert_eq!(value["scheduler"]["installed"], true);
        assert_eq!(value["scheduler"]["ready"], true);
        assert_eq!(value["version"], "0.3.0");
        assert_eq!(value["platform"], "macos");
    }
}
