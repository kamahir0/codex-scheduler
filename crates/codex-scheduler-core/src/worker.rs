use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WorkerError {
    #[error("Worker binary not found in bundle, target or system PATH")]
    NotFound,
    #[error("Failed to provision worker: {0}")]
    Io(#[from] std::io::Error),
}

/// Returns the standard binary name for the worker CLI.
pub fn worker_binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "codex-scheduler-cli.exe"
    } else {
        "codex-scheduler-cli"
    }
}

/// Returns the canonical user directory where the worker CLI is installed permanently.
/// - macOS / Linux: `~/.local/share/codex-scheduler/bin`
/// - Windows: `%LOCALAPPDATA%\codex-scheduler\bin`
pub fn canonical_worker_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        dirs::data_local_dir()
            .map(|d| d.join("codex-scheduler").join("bin"))
            .unwrap_or_else(|| PathBuf::from("C:\\codex-scheduler\\bin"))
    }

    #[cfg(not(target_os = "windows"))]
    {
        dirs::home_dir()
            .map(|h| h.join(".local").join("share").join("codex-scheduler").join("bin"))
            .unwrap_or_else(|| PathBuf::from("/tmp/codex-scheduler/bin"))
    }
}

/// Returns the full canonical path to the permanent worker CLI binary.
pub fn canonical_worker_path() -> PathBuf {
    canonical_worker_dir().join(worker_binary_name())
}

/// Checks whether the worker CLI is already installed at the canonical path.
pub fn is_worker_installed() -> bool {
    canonical_worker_path().exists()
}

/// Searches for the worker CLI binary in:
/// 1. The same directory as the currently running process (e.g. `current_exe().parent()`)
/// 2. macOS `.app` bundle `Resources/` or `Resources/bin/`
/// 3. Standard release/debug target directories (for development & testing)
/// 4. System `PATH`
pub fn find_bundled_worker_binary() -> Option<PathBuf> {
    let bin_name = worker_binary_name();

    // 1. Next to current executable
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let next_to_exe = parent.join(bin_name);
            if next_to_exe.is_file() {
                return Some(next_to_exe);
            }

            // 2. macOS bundle Resources
            // Contents/MacOS/codex-scheduler -> Contents/Resources/codex-scheduler-cli
            let resources_dir = parent.join("../Resources").join(bin_name);
            if resources_dir.is_file() {
                return Some(resources_dir);
            }

            let resources_bin = parent.join("../Resources/resources").join(bin_name);
            if resources_bin.is_file() {
                return Some(resources_bin);
            }

            // Also check `resources/` in parent (Windows or unpacked)
            let res_sub = parent.join("resources").join(bin_name);
            if res_sub.is_file() {
                return Some(res_sub);
            }

            // Check ancestor target directories (development mode)
            let mut cur = parent.to_path_buf();
            for _ in 0..5 {
                let target_rel = cur.join("target").join("release").join(bin_name);
                if target_rel.is_file() {
                    return Some(target_rel);
                }
                let target_deb = cur.join("target").join("debug").join(bin_name);
                if target_deb.is_file() {
                    return Some(target_deb);
                }
                if !cur.pop() {
                    break;
                }
            }
        }
    }

    // 3. Current working directory or target
    for candidate in &[
        PathBuf::from(bin_name),
        PathBuf::from("target/release").join(bin_name),
        PathBuf::from("target/debug").join(bin_name),
        PathBuf::from("../target/release").join(bin_name),
        PathBuf::from("../target/debug").join(bin_name),
    ] {
        if candidate.is_file() {
            if let Ok(canon) = fs::canonicalize(candidate) {
                return Some(canon);
            }
            return Some(candidate.clone());
        }
    }

    // 4. Search in PATH
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            let full = dir.join(bin_name);
            if full.is_file() {
                return Some(full);
            }
        }
    }

    None
}

/// Ensures the worker CLI binary is installed at the canonical path.
/// If not installed (or if a newer/different bundled version is available),
/// it copies the bundled binary to the canonical location and sets executable permissions (0o755).
/// Returns the path to the ready-to-use worker executable.
pub fn ensure_worker_installed() -> Result<PathBuf, WorkerError> {
    let canonical = canonical_worker_path();

    let bundled = find_bundled_worker_binary();

    // If canonical already exists
    if canonical.is_file() {
        // If bundled is found and differs, we can refresh it; otherwise existing is good
        if let Some(ref src) = bundled {
            if src != &canonical {
                // If the sizes or modification times differ, update it
                let should_update = match (fs::metadata(src), fs::metadata(&canonical)) {
                    (Ok(src_meta), Ok(dst_meta)) => {
                        src_meta.len() != dst_meta.len()
                            || src_meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH)
                                > dst_meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH)
                    }
                    _ => false,
                };

                if should_update {
                    let _ = copy_and_make_executable(src, &canonical);
                }
            }
        }
        return Ok(canonical);
    }

    // Canonical does not exist: copy from bundled
    if let Some(ref src) = bundled {
        if let Some(parent) = canonical.parent() {
            fs::create_dir_all(parent)?;
        }
        copy_and_make_executable(src, &canonical)?;
        Ok(canonical)
    } else {
        Err(WorkerError::NotFound)
    }
}

/// Helper to copy file and set execute permissions on Unix.
fn copy_and_make_executable(src: &Path, dst: &Path) -> std::io::Result<()> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(src, dst)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(dst)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(dst, perms)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_worker_binary_name() {
        let name = worker_binary_name();
        assert!(name.starts_with("codex-scheduler-cli"));
    }

    #[test]
    fn test_canonical_worker_path() {
        let p = canonical_worker_path();
        assert!(p.to_string_lossy().contains("codex-scheduler"));
        assert!(p.to_string_lossy().contains(worker_binary_name()));
    }

    #[test]
    fn test_copy_and_make_executable() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("mock-cli");
        let dst = dir.path().join("bin/mock-cli");

        fs::write(&src, b"#!/bin/sh\necho ok").unwrap();
        copy_and_make_executable(&src, &dst).unwrap();

        assert!(dst.is_file());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&dst).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o755);
        }
    }
}
