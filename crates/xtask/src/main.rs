mod check_all;
mod rationale;

use clap::{Parser, Subcommand};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Parser)]
#[command(name = "xtask")]
#[command(about = "Project maintenance and release automation tasks")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Perform an automated release: bump versions, verify builds/tests, commit, tag, and push.
    Release {
        /// Version target: 'patch', 'minor', 'major', or an explicit version like '0.3.0'
        target: String,

        /// Optional release message / summary
        #[arg(short, long)]
        message: Option<String>,

        /// Do not push to remote origin
        #[arg(long)]
        no_push: bool,

        /// Skip running verification checks before releasing
        #[arg(long)]
        no_test: bool,

        /// Dry-run mode: display planned changes without modifying files or git
        #[arg(long)]
        dry_run: bool,
    },
    /// Verify implementation rationales (WHY / WHAT BREAKS / EVIDENCE) across source files.
    CheckRationale,
    /// Run all mechanical checks (rationale, cargo tests, frontend lint & build).
    CheckAll,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let root = project_root()?;

    match cli.command {
        Commands::Release {
            target,
            message,
            no_push,
            no_test,
            dry_run,
        } => {
            run_release(
                &root,
                &target,
                message.as_deref(),
                no_push,
                no_test,
                dry_run,
            )?;
        }
        Commands::CheckRationale => {
            if let Err(e) = rationale::run_check_rationale(&root) {
                eprintln!("\nError: {}", e);
                std::process::exit(1);
            }
        }
        Commands::CheckAll => {
            if let Err(e) = check_all::run_check_all(&root) {
                eprintln!("\nError: {}", e);
                std::process::exit(1);
            }
        }
    }

    Ok(())
}

fn project_root() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // crates/xtask -> crates -> project root
    let root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf())
        .ok_or("Failed to locate project root from CARGO_MANIFEST_DIR")?;
    Ok(root)
}

fn run_release(
    root: &Path,
    target: &str,
    msg: Option<&str>,
    no_push: bool,
    no_test: bool,
    dry_run: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n=== Codex Scheduler Release Automation ===");

    // 1. Check git status
    if !dry_run {
        let status = run_cmd_output(root, "git", &["status", "--porcelain"])?;
        if !status.trim().is_empty() {
            eprintln!("\nError: Working tree is dirty. Please commit or stash changes first:");
            eprintln!("{}", status);
            std::process::exit(1);
        }
    }

    // 2. Read current version from Cargo.toml
    let cargo_toml_path = root.join("Cargo.toml");
    let cargo_toml_content = fs::read_to_string(&cargo_toml_path)?;
    let current_version = parse_cargo_workspace_version(&cargo_toml_content)
        .ok_or("Failed to parse version from [workspace.package] in Cargo.toml")?;

    let next_version = calculate_next_version(&current_version, target)?;
    println!("Current version: v{}", current_version);
    println!("Target version:  v{}", next_version);

    if dry_run {
        println!("\n[Dry Run] Planned updates:");
        println!("  - Cargo.toml -> v{}", next_version);
        println!("  - package.json -> v{}", next_version);
        println!("  - apps/gui/package.json -> v{}", next_version);
        println!(
            "  - apps/gui/src-tauri/tauri.conf.json -> v{}",
            next_version
        );
        println!("  - cargo check & Cargo.lock sync");
        if !no_test {
            println!("  - Run cargo xtask check-all (tests, lint, build, rationale)");
        }
        println!("  - Commit, Tag v{}, and push to origin", next_version);
        return Ok(());
    }

    // 3. Update version in files
    println!("\n[1/5] Updating version in configuration files...");
    update_cargo_toml(&cargo_toml_path, &next_version)?;
    update_json_version(&root.join("package.json"), &next_version)?;
    update_json_version(&root.join("apps/gui/package.json"), &next_version)?;
    update_json_version(
        &root.join("apps/gui/src-tauri/tauri.conf.json"),
        &next_version,
    )?;

    // 4. Sync Cargo.lock
    println!("[2/5] Synchronizing Cargo.lock...");
    run_cmd(root, "cargo", &["check"])?;

    // 5. Run verification checks
    if !no_test {
        println!("[3/5] Running mechanical verification (check-all)...");
        check_all::run_check_all(root)
            .map_err(|e| format!("Release verification failed: {}", e))?;
    } else {
        println!("[3/5] Skipping verification (--no-test specified)");
    }

    // 6. Git commit & tag
    println!("[4/5] Committing changes and creating Git tag...");
    run_cmd(
        root,
        "git",
        &[
            "add",
            "Cargo.toml",
            "Cargo.lock",
            "package.json",
            "apps/gui/package.json",
            "apps/gui/src-tauri/tauri.conf.json",
        ],
    )?;

    let release_summary = msg.unwrap_or("自動リリースパイプラインによるバージョン更新");
    let commit_msg = format!(
        "chore(release): バージョンを0.2.2に更新\n\n【リリース内容】\n{}",
        release_summary
    )
    .replace("0.2.2", &next_version);

    run_cmd(root, "git", &["commit", "-m", &commit_msg])?;

    // Update execution-state.md Candidate hash
    let head_rev = run_cmd_output(root, "git", &["rev-parse", "HEAD"])?
        .trim()
        .to_string();
    update_execution_state_candidate(root, &head_rev)?;
    run_cmd(root, "git", &["add", "docs/execution-state.md"])?;
    run_cmd(
        root,
        "git",
        &[
            "commit",
            "-m",
            &format!(
                "docs: 実行状態Candidateハッシュをv{}リリースコミットへ更新",
                next_version
            ),
        ],
    )?;

    let tag_name = format!("v{}", next_version);
    let tag_msg = format!("Release v{}\n\n{}", next_version, release_summary);
    run_cmd(root, "git", &["tag", "-a", &tag_name, "-m", &tag_msg])?;
    println!("Created annotated tag: {}", tag_name);

    // 7. Git push
    if !no_push {
        println!(
            "[5/5] Pushing main branch and tag {} to origin...",
            tag_name
        );
        run_cmd(root, "git", &["push", "origin", "main"])?;
        run_cmd(root, "git", &["push", "origin", &tag_name])?;
        println!("\n Successfully released v{}!", next_version);
        println!("GitHub Actions release pipeline will build macOS and Windows installers.");
    } else {
        println!("\n[5/5] Skipped push (--no-push specified).");
        println!(
            "Run manually: git push origin main && git push origin {}",
            tag_name
        );
    }

    Ok(())
}

fn parse_cargo_workspace_version(content: &str) -> Option<String> {
    let mut in_workspace_package = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_workspace_package = trimmed == "[workspace.package]";
            continue;
        }
        if in_workspace_package && trimmed.starts_with("version") {
            if let Some((_, val)) = trimmed.split_once('=') {
                return Some(val.trim().trim_matches('"').to_string());
            }
        }
    }
    None
}

fn calculate_next_version(
    current: &str,
    target: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let parts: Vec<&str> = current.split('.').collect();
    if parts.len() != 3 {
        return Err(format!(
            "Current version '{}' is not valid SemVer (expected X.Y.Z)",
            current
        )
        .into());
    }
    let major: u64 = parts[0].parse()?;
    let minor: u64 = parts[1].parse()?;
    let patch: u64 = parts[2].parse()?;

    match target.to_lowercase().as_str() {
        "patch" => Ok(format!("{}.{}.{}", major, minor, patch + 1)),
        "minor" => Ok(format!("{}.{}.0", major, minor + 1)),
        "major" => Ok(format!("{}.0.0", major + 1)),
        explicit => {
            let exp_parts: Vec<&str> = explicit.split('.').collect();
            if exp_parts.len() == 3 && exp_parts.iter().all(|p| p.parse::<u64>().is_ok()) {
                Ok(explicit.to_string())
            } else {
                Err(format!(
                    "Invalid target '{}'. Expected 'patch', 'minor', 'major', or 'X.Y.Z'",
                    target
                )
                .into())
            }
        }
    }
}

fn update_cargo_toml(path: &Path, new_ver: &str) -> Result<(), Box<dyn std::error::Error>> {
    let content = fs::read_to_string(path)?;
    let mut lines = Vec::new();
    let mut in_workspace_package = false;
    let mut replaced = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_workspace_package = trimmed == "[workspace.package]";
            lines.push(line.to_string());
            continue;
        }
        if in_workspace_package && trimmed.starts_with("version") && !replaced {
            lines.push(format!("version = \"{}\"", new_ver));
            replaced = true;
        } else {
            lines.push(line.to_string());
        }
    }

    fs::write(path, lines.join("\n") + "\n")?;
    Ok(())
}

fn update_json_version(path: &Path, new_ver: &str) -> Result<(), Box<dyn std::error::Error>> {
    let content = fs::read_to_string(path)?;
    let mut val: serde_json::Value = serde_json::from_str(&content)?;
    if let Some(obj) = val.as_object_mut() {
        obj.insert(
            "version".to_string(),
            serde_json::Value::String(new_ver.to_string()),
        );
    }
    let formatted = serde_json::to_string_pretty(&val)?;
    fs::write(path, formatted + "\n")?;
    Ok(())
}

fn update_execution_state_candidate(
    root: &Path,
    head_rev: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = root.join("docs/execution-state.md");
    if !path.exists() {
        return Ok(());
    }
    let content = fs::read_to_string(&path)?;
    let mut lines = Vec::new();
    for line in content.lines() {
        if line.starts_with("Candidate:") {
            lines.push(format!("Candidate: {}", head_rev));
        } else {
            lines.push(line.to_string());
        }
    }
    fs::write(path, lines.join("\n") + "\n")?;
    Ok(())
}

fn run_cmd(root: &Path, cmd: &str, args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let status = Command::new(cmd).current_dir(root).args(args).status()?;

    if !status.success() {
        return Err(format!("Command failed: {} {:?}", cmd, args).into());
    }
    Ok(())
}

fn run_cmd_output(
    root: &Path,
    cmd: &str,
    args: &[&str],
) -> Result<String, Box<dyn std::error::Error>> {
    let output = Command::new(cmd).current_dir(root).args(args).output()?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Command failed: {} {:?}: {}", cmd, args, err).into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}
