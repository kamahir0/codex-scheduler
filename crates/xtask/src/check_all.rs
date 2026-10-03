use std::path::Path;
use std::process::Command;

pub struct CheckStep {
    pub name: &'static str,
    pub run: Box<dyn Fn(&Path) -> Result<(), String>>,
}

/// Executes a series of check steps, logging their status and reporting all failures.
pub fn run_check_steps(root: &Path, steps: &[CheckStep]) -> Result<(), Vec<String>> {
    let mut failures = Vec::new();

    println!("\n=== Running Mechanical Verification (check-all) ===");
    for step in steps {
        println!("\n--> Running step: [{}]...", step.name);
        match (step.run)(root) {
            Ok(()) => {
                println!("[{}] PASS", step.name);
            }
            Err(e) => {
                eprintln!("[{}] FAIL: {}", step.name, e);
                failures.push(step.name.to_string());
            }
        }
    }

    if failures.is_empty() {
        println!("\n All mechanical checks passed successfully.");
        Ok(())
    } else {
        eprintln!(
            "\n Mechanical verification failed in {} step(s):",
            failures.len()
        );
        for f in &failures {
            eprintln!("  - [{}] FAIL", f);
        }
        Err(failures)
    }
}

fn run_command_in_root(root: &Path, cmd: &str, args: &[&str]) -> Result<(), String> {
    let status = Command::new(cmd)
        .current_dir(root)
        .args(args)
        .status()
        .map_err(|e| format!("Failed to spawn command '{} {:?}': {}", cmd, args, e))?;

    if !status.success() {
        return Err(format!(
            "Command '{} {:?}' exited with status: {:?}",
            cmd,
            args,
            status.code()
        ));
    }
    Ok(())
}

/// Default sequence of mechanical verification checks required for release readiness.
pub fn default_check_steps() -> Vec<CheckStep> {
    vec![
        CheckStep {
            name: "check-rationale",
            run: Box::new(|root| crate::rationale::run_check_rationale(root)),
        },
        CheckStep {
            name: "cargo-test",
            run: Box::new(|root| {
                println!("Executing: cargo test --workspace");
                run_command_in_root(root, "cargo", &["test", "--workspace"])
            }),
        },
        CheckStep {
            name: "frontend-lint",
            run: Box::new(|root| {
                println!("Executing: npm run frontend:lint");
                run_command_in_root(root, "npm", &["run", "frontend:lint"])
            }),
        },
        CheckStep {
            name: "frontend-build",
            run: Box::new(|root| {
                println!("Executing: npm run frontend:build");
                run_command_in_root(root, "npm", &["run", "frontend:build"])
            }),
        },
    ]
}

/// Runs all mechanical checks under the repository root.
pub fn run_check_all(root: &Path) -> Result<(), String> {
    let steps = default_check_steps();
    run_check_steps(root, &steps)
        .map_err(|failures| format!("Verification failed for steps: {:?}", failures))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn test_run_check_steps_all_pass() {
        let counter = Arc::new(AtomicUsize::new(0));
        let c1 = Arc::clone(&counter);
        let c2 = Arc::clone(&counter);

        let steps = vec![
            CheckStep {
                name: "step-1",
                run: Box::new(move |_| {
                    c1.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }),
            },
            CheckStep {
                name: "step-2",
                run: Box::new(move |_| {
                    c2.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }),
            },
        ];

        let res = run_check_steps(Path::new("."), &steps);
        assert!(res.is_ok());
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn test_run_check_steps_failure_propagation() {
        let counter = Arc::new(AtomicUsize::new(0));
        let c1 = Arc::clone(&counter);
        let c2 = Arc::clone(&counter);

        let steps = vec![
            CheckStep {
                name: "step-ok",
                run: Box::new(move |_| {
                    c1.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }),
            },
            CheckStep {
                name: "step-fail",
                run: Box::new(move |_| {
                    c2.fetch_add(1, Ordering::SeqCst);
                    Err("Synthetic test error".to_string())
                }),
            },
        ];

        let res = run_check_steps(Path::new("."), &steps);
        assert!(res.is_err());
        let failures = res.unwrap_err();
        assert_eq!(failures, vec!["step-fail".to_string()]);
        // All steps should have been attempted
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn test_run_check_steps_aggregates_multiple_failures() {
        let steps = vec![
            CheckStep {
                name: "step-fail-1",
                run: Box::new(|_| Err("Error 1".to_string())),
            },
            CheckStep {
                name: "step-ok",
                run: Box::new(|_| Ok(())),
            },
            CheckStep {
                name: "step-fail-2",
                run: Box::new(|_| Err("Error 2".to_string())),
            },
        ];

        let res = run_check_steps(Path::new("."), &steps);
        assert!(res.is_err());
        let failures = res.unwrap_err();
        assert_eq!(failures.len(), 2);
        assert_eq!(failures, vec!["step-fail-1".to_string(), "step-fail-2".to_string()]);
    }
}
