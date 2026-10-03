use crate::models::Job;
use chrono::{DateTime, Utc};
use fs2::FileExt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization/Deserialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("Unable to determine user home directory")]
    HomeDirNotFound,
    #[error("Job not found: {0}")]
    NotFound(String),
    #[error("Job is already running: {0}")]
    AlreadyRunning(String),
}

#[derive(Debug, Clone)]
pub struct JobStore {
    path: PathBuf,
}

impl JobStore {
    pub fn new_with_path<P: AsRef<Path>>(path: P) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    pub fn default_store() -> Result<Self, StoreError> {
        let home = dirs::home_dir().ok_or(StoreError::HomeDirNotFound)?;
        let dir = home.join(".codex-scheduler");
        if !dir.exists() {
            fs::create_dir_all(&dir)?;
        }
        Ok(Self::new_with_path(dir.join("jobs.json")))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn lock_path(&self) -> PathBuf {
        self.path.with_extension("lock")
    }

    /// Executes a closure while holding an exclusive file lock on `<store>.lock`.
    pub fn with_lock<F, R>(&self, f: F) -> Result<R, StoreError>
    where
        F: FnOnce() -> Result<R, StoreError>,
    {
        if let Some(parent) = self.path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }
        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.lock_path())?;

        lock_file.lock_exclusive()?;
        let res = f();
        let _ = lock_file.unlock();
        res
    }

    pub fn load_all(&self) -> Result<Vec<Job>, StoreError> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let content = fs::read_to_string(&self.path)?;
        if content.trim().is_empty() {
            return Ok(Vec::new());
        }
        let jobs: Vec<Job> = serde_json::from_str(&content)?;
        Ok(jobs)
    }

    pub fn save_all(&self, jobs: &[Job]) -> Result<(), StoreError> {
        if let Some(parent) = self.path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }

        let json = serde_json::to_string_pretty(jobs)?;

        // Atomic write via tempfile in the same parent directory to ensure rename works across filesystems
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        temp.write_all(json.as_bytes())?;
        temp.flush()?;
        temp.persist(&self.path).map_err(|e| e.error)?;

        Ok(())
    }

    pub fn get_job(&self, id: &str) -> Result<Option<Job>, StoreError> {
        let jobs = self.load_all()?;
        Ok(jobs.into_iter().find(|j| j.id == id))
    }

    pub fn insert_job(&self, job: Job) -> Result<(), StoreError> {
        self.with_lock(|| {
            let mut jobs = self.load_all()?;
            jobs.retain(|j| j.id != job.id);
            jobs.push(job);
            self.save_all(&jobs)
        })
    }

    pub fn update_job(&self, job: &Job) -> Result<(), StoreError> {
        self.with_lock(|| {
            let mut jobs = self.load_all()?;
            let mut found = false;
            for existing in jobs.iter_mut() {
                if existing.id == job.id {
                    *existing = job.clone();
                    found = true;
                    break;
                }
            }
            if !found {
                return Err(StoreError::NotFound(job.id.clone()));
            }
            self.save_all(&jobs)
        })
    }

    pub fn delete_job(&self, id: &str) -> Result<bool, StoreError> {
        self.with_lock(|| {
            let mut jobs = self.load_all()?;
            let initial_len = jobs.len();
            jobs.retain(|j| j.id != id);
            if jobs.len() != initial_len {
                self.save_all(&jobs)?;
                Ok(true)
            } else {
                Ok(false)
            }
        })
    }

    pub fn list_active_jobs(&self) -> Result<Vec<Job>, StoreError> {
        let jobs = self.load_all()?;
        Ok(jobs.into_iter().filter(|j| j.status.is_active()).collect())
    }

    /// Atomically finds and claims jobs that are Scheduled and due (scheduled_at <= now),
    /// changing their status to Running and persisting them under an exclusive file lock.
    /// Returns the claimed jobs. This prevents multiple tick processes or race conditions from executing the same job.
    pub fn claim_due_jobs(&self, now: DateTime<Utc>) -> Result<Vec<Job>, StoreError> {
        self.with_lock(|| {
            let mut jobs = self.load_all()?;
            let mut claimed = Vec::new();

            for job in jobs.iter_mut() {
                if job.status == crate::models::JobStatus::Scheduled && job.scheduled_at <= now {
                    job.set_status(crate::models::JobStatus::Running);
                    claimed.push(job.clone());
                }
            }

            if !claimed.is_empty() {
                self.save_all(&jobs)?;
            }

            Ok(claimed)
        })
    }

    /// Atomically claims a single job for execution, ensuring it is not already running.
    pub fn claim_job_for_execution(&self, id: &str) -> Result<Job, StoreError> {
        self.with_lock(|| {
            let mut jobs = self.load_all()?;
            for job in jobs.iter_mut() {
                if job.id == id {
                    if job.status == crate::models::JobStatus::Running {
                        return Err(StoreError::AlreadyRunning(id.to_string()));
                    }
                    job.set_status(crate::models::JobStatus::Running);
                    let claimed = job.clone();
                    self.save_all(&jobs)?;
                    return Ok(claimed);
                }
            }
            Err(StoreError::NotFound(id.to_string()))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{JobStatus, ProviderType};
    use chrono::{Duration, Utc};

    #[test]
    fn test_store_crud_lifecycle() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store_file = temp_dir.path().join("jobs.json");
        let store = JobStore::new_with_path(&store_file);

        // Initially empty
        assert_eq!(store.load_all().unwrap().len(), 0);

        // Insert
        let job = Job::new(
            ProviderType::Codex,
            "session-1".to_string(),
            temp_dir.path().to_path_buf(),
            Some("continue".to_string()),
            Utc::now(),
            None,
        )
        .unwrap();
        let job_id = job.id.clone();
        store.insert_job(job.clone()).unwrap();

        // Retrieve
        let retrieved = store.get_job(&job_id).unwrap().expect("Job should exist");
        assert_eq!(retrieved.session_id, "session-1");
        assert_eq!(retrieved.status, JobStatus::Scheduled);

        // Update
        let mut updated = retrieved;
        updated.set_status(JobStatus::Running);
        store.update_job(&updated).unwrap();

        let after_update = store.get_job(&job_id).unwrap().unwrap();
        assert_eq!(after_update.status, JobStatus::Running);

        // Delete
        let deleted = store.delete_job(&job_id).unwrap();
        assert!(deleted);
        assert!(store.get_job(&job_id).unwrap().is_none());
    }

    #[test]
    fn test_claim_due_jobs_atomic_exclusion() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store_file = temp_dir.path().join("jobs.json");
        let store = JobStore::new_with_path(&store_file);

        let now = Utc::now();

        // 1. Past due job
        let due_job = Job::new(
            ProviderType::Codex,
            "session-due".to_string(),
            temp_dir.path().to_path_buf(),
            None,
            now - Duration::minutes(5),
            None,
        )
        .unwrap();

        // 2. Future job
        let future_job = Job::new(
            ProviderType::Codex,
            "session-future".to_string(),
            temp_dir.path().to_path_buf(),
            None,
            now + Duration::minutes(30),
            None,
        )
        .unwrap();

        store.insert_job(due_job.clone()).unwrap();
        store.insert_job(future_job.clone()).unwrap();

        // First tick claim: should claim due_job only
        let claimed_1 = store.claim_due_jobs(now).unwrap();
        assert_eq!(claimed_1.len(), 1);
        assert_eq!(claimed_1[0].id, due_job.id);
        assert_eq!(claimed_1[0].status, JobStatus::Running);

        // Store should show due_job is now Running
        let in_store = store.get_job(&due_job.id).unwrap().unwrap();
        assert_eq!(in_store.status, JobStatus::Running);

        // Second tick claim immediately after: should return 0 jobs (already running)
        let claimed_2 = store.claim_due_jobs(now).unwrap();
        assert_eq!(claimed_2.len(), 0);

        // Test claim_job_for_execution already running error
        let err = store.claim_job_for_execution(&due_job.id).unwrap_err();
        match err {
            StoreError::AlreadyRunning(id) => assert_eq!(id, due_job.id),
            _ => panic!("Expected AlreadyRunning error"),
        }
    }
}
