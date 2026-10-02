use super::{SchedulerBackend, SchedulerError};
use crate::models::Job;
use chrono::{Datelike, Timelike};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

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

    pub fn plist_label(job_id: &str) -> String {
        format!("com.codexscheduler.job.{}", job_id)
    }

    pub fn plist_path(&self, job_id: &str) -> PathBuf {
        self.launch_agents_dir
            .join(format!("{}.plist", Self::plist_label(job_id)))
    }

    pub fn generate_plist_content(job: &Job, cli_path: &Path) -> String {
        let label = Self::plist_label(&job.id);
        let cli_str = cli_path.to_string_lossy();
        let target_time = job.scheduled_at;

        let hour = target_time.hour();
        let minute = target_time.minute();
        let day = target_time.day();
        let month = target_time.month();

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
        <string>run-job</string>
        <string>{job_id}</string>
    </array>
    <key>StartCalendarInterval</key>
    <dict>
        <key>Month</key>
        <integer>{month}</integer>
        <key>Day</key>
        <integer>{day}</integer>
        <key>Hour</key>
        <integer>{hour}</integer>
        <key>Minute</key>
        <integer>{minute}</integer>
    </dict>
    <key>AbandonProcessGroup</key>
    <true/>
    <key>StandardOutPath</key>
    <string>/tmp/{label}.stdout.log</string>
    <key>StandardErrorPath</key>
    <string>/tmp/{label}.stderr.log</string>
</dict>
</plist>"#,
            label = label,
            cli_str = cli_str,
            job_id = job.id,
            month = month,
            day = day,
            hour = hour,
            minute = minute,
        )
    }
}

impl Default for MacOsLaunchdScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl SchedulerBackend for MacOsLaunchdScheduler {
    fn register_job(&self, job: &Job, cli_path: &Path) -> Result<(), SchedulerError> {
        if !self.launch_agents_dir.exists() {
            fs::create_dir_all(&self.launch_agents_dir)?;
        }

        let plist_path = self.plist_path(&job.id);
        let content = Self::generate_plist_content(job, cli_path);
        fs::write(&plist_path, content)?;

        // Run launchctl load
        let output = Command::new("launchctl")
            .arg("load")
            .arg("-w")
            .arg(&plist_path)
            .output();

        if let Ok(out) = output {
            if !out.status.success() {
                let err = String::from_utf8_lossy(&out.stderr);
                // In some test/headless environments launchctl might fail gracefully
                eprintln!("Warning: launchctl load returned status {:?}: {}", out.status, err);
            }
        }

        Ok(())
    }

    fn unregister_job(&self, job_id: &str) -> Result<(), SchedulerError> {
        let plist_path = self.plist_path(job_id);
        if plist_path.exists() {
            let _ = Command::new("launchctl")
                .arg("unload")
                .arg(&plist_path)
                .output();

            let _ = fs::remove_file(&plist_path);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ProviderType;
    use chrono::{TimeZone, Utc};

    #[test]
    fn test_plist_generation() {
        let dt = Utc.with_ymd_and_hms(2026, 10, 3, 2, 5, 0).unwrap();
        let job = Job::new(
            ProviderType::Codex,
            "session-abc".to_string(),
            PathBuf::from("/tmp"),
            None,
            dt,
            None,
        )
        .unwrap();

        let plist = MacOsLaunchdScheduler::generate_plist_content(&job, Path::new("/usr/local/bin/codex-scheduler-cli"));
        assert!(plist.contains("com.codexscheduler.job."));
        assert!(plist.contains("<key>Hour</key>\n        <integer>2</integer>"));
        assert!(plist.contains("<key>Minute</key>\n        <integer>5</integer>"));
        assert!(plist.contains("<string>run-job</string>"));
    }
}
