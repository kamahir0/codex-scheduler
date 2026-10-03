use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum RationaleError {
    MissingField {
        file: String,
        line: usize,
        field: &'static str,
    },
    EmptyField {
        file: String,
        line: usize,
        field: &'static str,
    },
    InvalidEvidencePath {
        file: String,
        line: usize,
        path: String,
    },
    InvalidRequirementId {
        file: String,
        line: usize,
        id: String,
    },
}

impl std::fmt::Display for RationaleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RationaleError::MissingField { file, line, field } => {
                write!(f, "{}:{}: missing required field '{}'", file, line, field)
            }
            RationaleError::EmptyField { file, line, field } => {
                write!(f, "{}:{}: field '{}' cannot be empty", file, line, field)
            }
            RationaleError::InvalidEvidencePath { file, line, path } => {
                write!(
                    f,
                    "{}:{}: referenced evidence path '{}' does not exist in repository",
                    file, line, path
                )
            }
            RationaleError::InvalidRequirementId { file, line, id } => {
                write!(
                    f,
                    "{}:{}: referenced requirement ID '{}' is not defined in documentation",
                    file, line, id
                )
            }
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RationaleBlock {
    pub file: String,
    pub start_line: usize,
    pub why: Option<String>,
    pub what_breaks: Option<String>,
    pub evidence: Option<String>,
    pub remove_when: Option<String>,
}

/// Parses rationale blocks from source text.
///
/// A rationale block starts with `// WHY:` and continues through consecutive comment lines.
/// It must specify:
/// - `// WHY:`
/// - `// WHAT BREAKS:` (or `// IF REMOVED:`)
/// - `// EVIDENCE:`
/// Optionally:
/// - `// REMOVE WHEN:` (or `// REMOVAL CONDITION:`)
pub fn parse_rationale_blocks(content: &str, file_name: &str) -> Vec<RationaleBlock> {
    let mut blocks = Vec::new();
    let mut current_block: Option<RationaleBlock> = None;
    let mut current_field: Option<&'static str> = None;

    for (idx, line) in content.lines().enumerate() {
        let line_num = idx + 1;
        let trimmed = line.trim();
        if trimmed.starts_with("///") || trimmed.starts_with("//!") {
            if let Some(b) = current_block.take() {
                blocks.push(b);
            }
            current_field = None;
            continue;
        }

        if let Some(comment_body) = trimmed.strip_prefix("//") {
            let body = comment_body.trim();

            if let Some(rest) = body.strip_prefix("WHY:") {
                if let Some(b) = current_block.take() {
                    blocks.push(b);
                }
                let mut b = RationaleBlock {
                    file: file_name.to_string(),
                    start_line: line_num,
                    ..Default::default()
                };
                let content = rest.trim();
                if !content.is_empty() {
                    b.why = Some(content.to_string());
                } else {
                    b.why = Some(String::new());
                }
                current_block = Some(b);
                current_field = Some("WHY");
            } else if let Some(rest) = body
                .strip_prefix("WHAT BREAKS:")
                .or_else(|| body.strip_prefix("IF REMOVED:"))
            {
                if let Some(ref mut b) = current_block {
                    let content = rest.trim();
                    b.what_breaks = Some(content.to_string());
                    current_field = Some("WHAT BREAKS");
                }
            } else if let Some(rest) = body.strip_prefix("EVIDENCE:") {
                if let Some(ref mut b) = current_block {
                    let content = rest.trim();
                    b.evidence = Some(content.to_string());
                    current_field = Some("EVIDENCE");
                }
            } else if let Some(rest) = body
                .strip_prefix("REMOVE WHEN:")
                .or_else(|| body.strip_prefix("REMOVAL CONDITION:"))
            {
                if let Some(ref mut b) = current_block {
                    let content = rest.trim();
                    b.remove_when = Some(content.to_string());
                    current_field = Some("REMOVE WHEN");
                }
            } else if let Some(field) = current_field {
                // Continuation line of the current field
                if let Some(ref mut b) = current_block {
                    if !body.is_empty() {
                        let target = match field {
                            "WHY" => b.why.as_mut(),
                            "WHAT BREAKS" => b.what_breaks.as_mut(),
                            "EVIDENCE" => b.evidence.as_mut(),
                            "REMOVE WHEN" => b.remove_when.as_mut(),
                            _ => None,
                        };
                        if let Some(text) = target {
                            if !text.is_empty() {
                                text.push(' ');
                            }
                            text.push_str(body);
                        }
                    }
                }
            }
        } else {
            // Non-comment line terminates current block
            if let Some(b) = current_block.take() {
                blocks.push(b);
            }
            current_field = None;
        }
    }

    if let Some(b) = current_block.take() {
        blocks.push(b);
    }

    blocks
}

/// Checks if a token matches the uppercase Requirement ID pattern (e.g., OS-SCHED-001).
pub fn is_requirement_id(token: &str) -> bool {
    let parts: Vec<&str> = token.split('-').collect();
    if parts.len() < 2 {
        return false;
    }
    // Must be uppercase ascii letters or digits, and at least first part should be alphabetic
    for part in &parts {
        if part.is_empty()
            || !part
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        {
            return false;
        }
    }
    parts[0].chars().any(|c| c.is_ascii_uppercase())
}

/// Validates a single rationale block.
pub fn validate_block(
    block: &RationaleBlock,
    root: &Path,
    known_req_ids: &HashSet<String>,
) -> Vec<RationaleError> {
    let mut errors = Vec::new();

    // 1. WHY check
    match &block.why {
        Some(w) if !w.trim().is_empty() => {}
        _ => {
            errors.push(RationaleError::EmptyField {
                file: block.file.clone(),
                line: block.start_line,
                field: "WHY",
            });
        }
    }

    // 2. WHAT BREAKS check
    match &block.what_breaks {
        None => {
            errors.push(RationaleError::MissingField {
                file: block.file.clone(),
                line: block.start_line,
                field: "WHAT BREAKS",
            });
        }
        Some(wb) if wb.trim().is_empty() => {
            errors.push(RationaleError::EmptyField {
                file: block.file.clone(),
                line: block.start_line,
                field: "WHAT BREAKS",
            });
        }
        _ => {}
    }

    // 3. EVIDENCE check
    match &block.evidence {
        None => {
            errors.push(RationaleError::MissingField {
                file: block.file.clone(),
                line: block.start_line,
                field: "EVIDENCE",
            });
        }
        Some(ev) if ev.trim().is_empty() => {
            errors.push(RationaleError::EmptyField {
                file: block.file.clone(),
                line: block.start_line,
                field: "EVIDENCE",
            });
        }
        Some(ev) => {
            // Split evidence into tokens (comma, semicolon, or whitespace separated)
            let tokens = ev
                .split(|c: char| c == ',' || c == ';')
                .map(|t| t.trim())
                .filter(|t| !t.is_empty());

            for raw_token in tokens {
                let token = raw_token.trim_matches(|c: char| c == '.' || c == '"' || c == '\'');
                if token.is_empty() {
                    continue;
                }

                let has_path_separator = token.contains('/') || token.contains('\\');
                let has_file_extension = token.ends_with(".md")
                    || token.ends_with(".rs")
                    || token.ends_with(".toml")
                    || token.ends_with(".json");

                if has_path_separator || has_file_extension {
                    let path = root.join(token);
                    if !path.exists() {
                        errors.push(RationaleError::InvalidEvidencePath {
                            file: block.file.clone(),
                            line: block.start_line,
                            path: token.to_string(),
                        });
                    }
                } else if is_requirement_id(token) {
                    if !known_req_ids.is_empty() && !known_req_ids.contains(token) {
                        errors.push(RationaleError::InvalidRequirementId {
                            file: block.file.clone(),
                            line: block.start_line,
                            id: token.to_string(),
                        });
                    }
                }
            }
        }
    }

    errors
}

/// Determines if a file should be checked for implementation rationales.
pub fn is_target_source_file(rel_path: &Path) -> bool {
    let components: Vec<_> = rel_path
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect();

    let excluded_dirs = [
        "target",
        "node_modules",
        "dist",
        ".git",
        ".github",
        "docs",
        "resources",
        "icons",
        "release-assets",
        "bundle",
    ];

    for comp in &components {
        if excluded_dirs.contains(&comp.as_str()) {
            return false;
        }
    }

    if let Some(ext) = rel_path.extension().and_then(|e| e.to_str()) {
        matches!(ext, "rs" | "ts" | "tsx" | "js")
    } else {
        false
    }
}

/// Collects all known Requirement IDs from markdown documents under docs/.
pub fn collect_known_requirement_ids(root: &Path) -> HashSet<String> {
    let mut req_ids = HashSet::new();
    let docs_dir = root.join("docs");
    if !docs_dir.exists() {
        return req_ids;
    }

    let mut stack = vec![docs_dir];
    while let Some(dir) = stack.pop() {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        for token in content
                            .split(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_')
                        {
                            let trimmed = token.trim_matches(|c: char| c == '-' || c == '_');
                            if is_requirement_id(trimmed) {
                                req_ids.insert(trimmed.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    req_ids
}

/// Recursively finds all target source files under root.
pub fn find_source_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Ok(rel) = path.strip_prefix(root) {
                    if is_target_source_file(rel) {
                        if path.is_file() {
                            files.push(path);
                        }
                    } else if path.is_dir() {
                        // Check if directory should be traversed
                        let comp = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                        let skip = matches!(
                            comp,
                            "target"
                                | "node_modules"
                                | "dist"
                                | ".git"
                                | ".github"
                                | "docs"
                                | "resources"
                                | "icons"
                                | "release-assets"
                                | "bundle"
                        );
                        if !skip {
                            stack.push(path);
                        }
                    }
                }
            }
        }
    }

    files.sort();
    files
}

/// Runs rationale verification across repository source files.
pub fn check_rationale(root: &Path) -> Result<usize, Vec<RationaleError>> {
    let known_req_ids = collect_known_requirement_ids(root);
    let files = find_source_files(root);

    let mut total_blocks = 0;
    let mut all_errors = Vec::new();

    for file_path in files {
        let rel_file = file_path
            .strip_prefix(root)
            .unwrap_or(&file_path)
            .display()
            .to_string();

        if let Ok(content) = fs::read_to_string(&file_path) {
            let blocks = parse_rationale_blocks(&content, &rel_file);
            total_blocks += blocks.len();

            for block in blocks {
                let errors = validate_block(&block, root, &known_req_ids);
                all_errors.extend(errors);
            }
        }
    }

    if all_errors.is_empty() {
        Ok(total_blocks)
    } else {
        Err(all_errors)
    }
}

/// Entry point helper for CLI.
pub fn run_check_rationale(root: &Path) -> Result<(), String> {
    println!("\n=== Checking Implementation Rationales (check-rationale) ===");
    match check_rationale(root) {
        Ok(count) => {
            println!(
                "[check-rationale] PASS: Verified {} implementation rationale block(s) across codebase.",
                count
            );
            Ok(())
        }
        Err(errors) => {
            eprintln!(
                "[check-rationale] FAIL: Found {} rationale defect(s):",
                errors.len()
            );
            for err in &errors {
                eprintln!("  - {}", err);
            }
            Err(format!("Found {} rationale defect(s)", errors.len()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_rationale_block() {
        let content = concat!(
            "\n",
            "/",
            "/ WHY: Single executable with 2 modes avoids Gatekeeper re-evaluation.\n",
            "/",
            "/ WHAT BREAKS: Separating worker binary causes Gatekeeper rejection.\n",
            "/",
            "/ EVIDENCE: docs/adr/0003-macos-single-executable-headless-scheduler.md, OS-SCHED-001\n",
            "fn dummy() {}\n"
        );
        let blocks = parse_rationale_blocks(content, "test.rs");
        assert_eq!(blocks.len(), 1);
        let b = &blocks[0];
        assert_eq!(b.start_line, 2);
        assert!(b.why.as_ref().unwrap().contains("Single executable"));
        assert!(
            b.what_breaks
                .as_ref()
                .unwrap()
                .contains("Separating worker")
        );
        assert!(b.evidence.as_ref().unwrap().contains("OS-SCHED-001"));

        // Setup mock root
        let temp_dir = tempfile::tempdir().unwrap();
        let adr_path = temp_dir
            .path()
            .join("docs/adr/0003-macos-single-executable-headless-scheduler.md");
        fs::create_dir_all(adr_path.parent().unwrap()).unwrap();
        fs::write(&adr_path, "mock ADR").unwrap();

        let mut req_ids = HashSet::new();
        req_ids.insert("OS-SCHED-001".to_string());

        let errors = validate_block(b, temp_dir.path(), &req_ids);
        assert!(errors.is_empty(), "Expected no errors, got: {:?}", errors);
    }

    #[test]
    fn test_missing_field_detection() {
        let content = concat!("\n", "/", "/ WHY: Some explanation\n", "fn dummy() {}\n");
        let blocks = parse_rationale_blocks(content, "test.rs");
        assert_eq!(blocks.len(), 1);
        let b = &blocks[0];

        let temp_dir = tempfile::tempdir().unwrap();
        let req_ids = HashSet::new();
        let errors = validate_block(b, temp_dir.path(), &req_ids);

        assert_eq!(errors.len(), 2);
        assert!(errors.contains(&RationaleError::MissingField {
            file: "test.rs".to_string(),
            line: 2,
            field: "WHAT BREAKS",
        }));
        assert!(errors.contains(&RationaleError::MissingField {
            file: "test.rs".to_string(),
            line: 2,
            field: "EVIDENCE",
        }));
    }

    #[test]
    fn test_empty_field_detection() {
        let content = concat!(
            "\n",
            "/",
            "/ WHY: \n",
            "/",
            "/ WHAT BREAKS: \n",
            "/",
            "/ EVIDENCE: \n",
            "fn dummy() {}\n"
        );
        let blocks = parse_rationale_blocks(content, "test.rs");
        assert_eq!(blocks.len(), 1);
        let b = &blocks[0];

        let temp_dir = tempfile::tempdir().unwrap();
        let req_ids = HashSet::new();
        let errors = validate_block(b, temp_dir.path(), &req_ids);

        assert_eq!(errors.len(), 3);
        assert!(errors.contains(&RationaleError::EmptyField {
            file: "test.rs".to_string(),
            line: 2,
            field: "WHY",
        }));
        assert!(errors.contains(&RationaleError::EmptyField {
            file: "test.rs".to_string(),
            line: 2,
            field: "WHAT BREAKS",
        }));
        assert!(errors.contains(&RationaleError::EmptyField {
            file: "test.rs".to_string(),
            line: 2,
            field: "EVIDENCE",
        }));
    }

    #[test]
    fn test_missing_local_evidence_path() {
        let content = concat!(
            "\n",
            "/",
            "/ WHY: Important performance optimization\n",
            "/",
            "/ WHAT BREAKS: Severe latency spike\n",
            "/",
            "/ EVIDENCE: docs/adr/9999-does-not-exist.md\n",
            "fn dummy() {}\n"
        );
        let blocks = parse_rationale_blocks(content, "test.rs");
        assert_eq!(blocks.len(), 1);

        let temp_dir = tempfile::tempdir().unwrap();
        let req_ids = HashSet::new();
        let errors = validate_block(&blocks[0], temp_dir.path(), &req_ids);

        assert_eq!(errors.len(), 1);
        assert_eq!(
            errors[0],
            RationaleError::InvalidEvidencePath {
                file: "test.rs".to_string(),
                line: 2,
                path: "docs/adr/9999-does-not-exist.md".to_string(),
            }
        );
    }

    #[test]
    fn test_missing_requirement_id() {
        let content = concat!(
            "\n",
            "/",
            "/ WHY: Necessary invariant\n",
            "/",
            "/ WHAT BREAKS: Data corruption\n",
            "/",
            "/ EVIDENCE: UNKNOWN-REQ-999\n",
            "fn dummy() {}\n"
        );
        let blocks = parse_rationale_blocks(content, "test.rs");
        assert_eq!(blocks.len(), 1);

        let temp_dir = tempfile::tempdir().unwrap();
        let mut req_ids = HashSet::new();
        req_ids.insert("KNOWN-REQ-001".to_string());
        let errors = validate_block(&blocks[0], temp_dir.path(), &req_ids);

        assert_eq!(errors.len(), 1);
        assert_eq!(
            errors[0],
            RationaleError::InvalidRequirementId {
                file: "test.rs".to_string(),
                line: 2,
                id: "UNKNOWN-REQ-999".to_string(),
            }
        );
    }

    #[test]
    fn test_excluded_path_handling() {
        assert!(!is_target_source_file(Path::new("target/debug/build.rs")));
        assert!(!is_target_source_file(Path::new(
            "node_modules/pkg/index.js"
        )));
        assert!(!is_target_source_file(Path::new("dist/assets/index.js")));
        assert!(!is_target_source_file(Path::new(".git/hooks/pre-commit")));
        assert!(!is_target_source_file(Path::new(
            "docs/contributing/implementation-rationale.md"
        )));
        assert!(!is_target_source_file(Path::new("Cargo.lock")));
        assert!(!is_target_source_file(Path::new("package.json")));

        assert!(is_target_source_file(Path::new(
            "crates/codex-scheduler-core/src/lib.rs"
        )));
        assert!(is_target_source_file(Path::new("apps/gui/src/main.tsx")));
        assert!(is_target_source_file(Path::new(
            "apps/gui/src-tauri/src/main.rs"
        )));
    }

    #[test]
    fn test_doc_comments_do_not_continue_rationale_block() {
        let content = concat!(
            "/",
            "/ WHY: Needed for safety\n",
            "/",
            "/ WHAT BREAKS: Invariant violation\n",
            "/",
            "/ EVIDENCE: KNOWN-REQ-001\n",
            "/",
            "/",
            "/ This is a doc comment that should not be swallowed into EVIDENCE.\n",
            "pub fn safe_fn() {}\n"
        );
        let blocks = parse_rationale_blocks(content, "test.rs");
        assert_eq!(blocks.len(), 1);
        let b = &blocks[0];
        assert_eq!(b.evidence.as_deref(), Some("KNOWN-REQ-001"));
    }
}
