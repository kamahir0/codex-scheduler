use chrono::{DateTime, Duration, TimeZone, Utc};
use clap::{Parser, Subcommand};
use codex_scheduler_core::models::{ProviderType, RetryPolicy};
use codex_scheduler_core::store::JobStore;
use codex_scheduler_core::SchedulerService;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "codex-scheduler")]
#[command(about = "Scheduled resume and prompt automation for Codex sessions", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Execute a scheduled job as a worker process (invoked by OS scheduler)
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
    },
    /// Cancel a scheduled or retrying job
    Cancel {
        /// Job UUID
        job_id: String,
    },
    /// Delete a job from the database
    Delete {
        /// Job UUID
        job_id: String,
    },
    /// Check and execute any scheduled jobs that have reached their target time (called periodically by LaunchAgent)
    Tick,
    /// Ensure the persistent OS scheduler LaunchAgent/Task is installed
    InstallScheduler,
    /// Uninstall the persistent OS scheduler LaunchAgent/Task
    UninstallScheduler,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let store = JobStore::default_store()?;
    let service = SchedulerService::new_for_cli(store);

    match cli.command {
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
        } => {
            let scheduled_at = parse_time_arg(&at)?;
            let canonical_cwd = std::fs::canonicalize(&cwd).unwrap_or(cwd);

            let policy = RetryPolicy {
                enabled: true,
                interval_seconds: retry_interval,
                max_attempts,
                retry_on_quota_only: true,
            };

            let job = service.schedule_job(
                ProviderType::Codex,
                session_id,
                canonical_cwd,
                Some(prompt),
                scheduled_at,
                Some(policy),
            )?;

            println!("Job successfully scheduled!");
            println!("  ID:           {}", job.id);
            println!("  Session ID:   {}", job.session_id);
            println!("  Working dir:  {}", job.cwd.display());
            println!("  Prompt:       {}", job.prompt);
            println!("  Scheduled at: {} ({})", job.scheduled_at, relative_time(job.scheduled_at));
        }
        Commands::List { json } => {
            let jobs = service.store().load_all()?;
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
        Commands::Show { job_id } => {
            if let Some(job) = service.store().get_job(&job_id)? {
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
            } else {
                eprintln!("Error: Job '{}' not found", job_id);
                std::process::exit(1);
            }
        }
        Commands::Cancel { job_id } => {
            let job = service.cancel_job(&job_id)?;
            println!("Job {} has been cancelled.", job.id);
        }
        Commands::Delete { job_id } => {
            let deleted = service.delete_job(&job_id)?;
            if deleted {
                println!("Job {} has been deleted.", job_id);
            } else {
                eprintln!("Job {} was not found.", job_id);
            }
        }
        Commands::Tick => {
            let executed = service.execute_tick().await?;
            if !executed.is_empty() {
                println!("[Tick] Completed execution for {} due job(s).", executed.len());
            }
        }
        Commands::InstallScheduler => {
            #[cfg(target_os = "macos")]
            {
                println!("Notice: macOS環境でのLaunchAgent登録はCodex Scheduler.app本体の初回起動時に安全に行われます。CLI自身をLaunchAgentに登録することはできません。Codex Scheduler.appを起動してください。");
            }
            #[cfg(not(target_os = "macos"))]
            {
                service.ensure_scheduler()?;
                println!("Persistent OS scheduler service installed and active.");
            }
        }
        Commands::UninstallScheduler => {
            let os_sched = codex_scheduler_core::os_scheduler::get_platform_scheduler();
            os_sched.uninstall_scheduler()?;
            println!("Persistent OS scheduler service uninstalled.");
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
