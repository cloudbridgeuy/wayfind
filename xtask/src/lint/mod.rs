//! The lint pipeline.
//!
//! The same split the two library crates use applies here. The top half is a
//! functional core: it decides which checks to run, what to call them, how to
//! read an exit status, and whether a set of collected facts passes. The bottom
//! half is the shell: it spawns processes, walks directories, reads files, and
//! writes the log.
//!
//! Two checks have no external tool. File length and the ban on
//! `#[allow(clippy::too_many_arguments)]` are decided by pure functions over
//! collected facts, so they are unit tested without touching the filesystem.

pub mod hooks;

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use clap::Args;
use color_eyre::eyre::Result;

/// No source file may pass this many lines.
const MAX_FILE_LINES: usize = 1000;

/// Where the full transcript of a run is written.
const LOG_NAME: &str = "xtask-lint.log";

// ---------------------------------------------------------------------------
// Functional core — pure types and logic, no input or output
// ---------------------------------------------------------------------------

/// Names one check, so skip flags and fix-mode overrides can match on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CheckId {
    Fmt,
    Check,
    Clippy,
    Test,
    FileLength,
    TooManyArgsAllow,
}

/// One check in the pipeline.
///
/// A check with the sentinel program `__builtin__` runs in this crate instead
/// of in a child process.
struct Check {
    id: CheckId,
    name: &'static str,
    program: &'static str,
    args: &'static [&'static str],
    optional: bool,
}

/// What a check decided.
enum CheckOutcome {
    Passed { output: String },
    Failed { output: String },
    Skipped,
}

/// A check and its outcome, ready to log.
struct CheckResult {
    name: String,
    outcome: CheckOutcome,
}

/// Command line for `cargo run -p xtask -- lint`.
#[derive(Args)]
pub struct LintArgs {
    /// Print all output, not only failures
    #[arg(long)]
    pub verbose: bool,

    /// Skip the format check
    #[arg(long)]
    pub no_fmt: bool,

    /// Skip the compile check
    #[arg(long)]
    pub no_check: bool,

    /// Skip Clippy
    #[arg(long)]
    pub no_clippy: bool,

    /// Skip the tests
    #[arg(long)]
    pub no_test: bool,

    /// Skip the file-length check
    #[arg(long)]
    pub no_file_length: bool,

    /// Skip the `#[allow(clippy::too_many_arguments)]` ban
    #[arg(long)]
    pub no_too_many_args: bool,

    /// Repair what can be repaired: apply formatting and Clippy fixes
    #[arg(long)]
    pub fix: bool,

    /// Pre-commit hook mode: implies `--fix` and stages the repaired files again
    #[arg(long, hide = true)]
    pub staged_only: bool,

    /// Install the git pre-commit hook
    #[arg(long, conflicts_with_all = ["uninstall_hooks", "hooks_status"])]
    pub install_hooks: bool,

    /// Remove the git pre-commit hook
    #[arg(long, conflicts_with_all = ["install_hooks", "hooks_status"])]
    pub uninstall_hooks: bool,

    /// Report whether the git pre-commit hook is installed
    #[arg(long, conflicts_with_all = ["install_hooks", "uninstall_hooks"])]
    pub hooks_status: bool,
}

/// The pipeline, in order. The cheapest check that fails most often comes first.
const CHECKS: &[Check] = &[
    Check {
        id: CheckId::Fmt,
        name: "cargo fmt -- --check",
        program: "cargo",
        args: &["fmt", "--", "--check"],
        optional: false,
    },
    Check {
        id: CheckId::Check,
        name: "cargo check --workspace --all-targets",
        program: "cargo",
        args: &["check", "--workspace", "--all-targets"],
        optional: false,
    },
    Check {
        id: CheckId::Clippy,
        name: "cargo clippy --workspace --all-targets -- -D warnings",
        program: "cargo",
        args: &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
        optional: false,
    },
    Check {
        id: CheckId::Test,
        name: "cargo test --workspace --all-targets",
        program: "cargo",
        args: &["test", "--workspace", "--all-targets"],
        optional: false,
    },
    Check {
        id: CheckId::FileLength,
        name: "file length (<= 1000 lines)",
        program: "__builtin__",
        args: &[],
        optional: false,
    },
    Check {
        id: CheckId::TooManyArgsAllow,
        name: "forbid #[allow(clippy::too_many_arguments)]",
        program: "__builtin__",
        args: &[],
        optional: false,
    },
];

/// Did the operator ask for this check to be left out?
fn should_skip(id: CheckId, args: &LintArgs) -> bool {
    match id {
        CheckId::Fmt => args.no_fmt,
        CheckId::Check => args.no_check,
        CheckId::Clippy => args.no_clippy,
        CheckId::Test => args.no_test,
        CheckId::FileLength => args.no_file_length,
        CheckId::TooManyArgsAllow => args.no_too_many_args,
    }
}

/// The arguments a check takes in `--fix` mode, if it can repair anything.
///
/// - `fmt` drops `--check`, so the formatting is written.
/// - `clippy` gains `--fix --allow-dirty`, so the machine-applicable lints are
///   written.
/// - Every other check reports only, and returns `None`.
fn fix_args(id: CheckId) -> Option<Vec<&'static str>> {
    match id {
        CheckId::Fmt => Some(vec!["fmt"]),
        CheckId::Clippy => Some(vec![
            "clippy",
            "--workspace",
            "--all-targets",
            "--fix",
            "--allow-dirty",
            "--",
            "-D",
            "warnings",
        ]),
        _ => None,
    }
}

/// The name to print for a check whose arguments were overridden.
fn check_display_name(program: &str, args: &[&str]) -> String {
    if args.is_empty() {
        program.to_string()
    } else {
        format!("{program} {}", args.join(" "))
    }
}

/// Does this output say the tool is absent rather than unhappy?
fn is_tool_not_found(output: &str) -> bool {
    let lower = output.to_lowercase();
    lower.contains("not found")
        || lower.contains("no such file or directory")
        || lower.contains("unrecognized subcommand")
        || lower.contains("no such command")
}

/// Read an exit status and its output as an outcome.
fn determine_outcome(success: bool, output: String, optional: bool) -> CheckOutcome {
    if optional && !success && is_tool_not_found(&output) {
        return CheckOutcome::Skipped;
    }
    if success {
        CheckOutcome::Passed { output }
    } else {
        CheckOutcome::Failed { output }
    }
}

/// Render one result as a log entry.
fn format_log_entry(result: &CheckResult) -> String {
    match &result.outcome {
        CheckOutcome::Skipped => {
            format!("=== {} ===\n[skipped — tool not installed]\n", result.name)
        }
        CheckOutcome::Passed { output } | CheckOutcome::Failed { output } => {
            format!("=== {} ===\n{}\n", result.name, output)
        }
    }
}

/// Decide the file-length check from collected line counts.
fn evaluate_file_lengths(files: &[(String, usize)], max_lines: usize) -> CheckOutcome {
    let violations: Vec<String> = files
        .iter()
        .filter(|(_, count)| *count > max_lines)
        .map(|(path, count)| format!("  {path} ({count} lines)"))
        .collect();

    if violations.is_empty() {
        CheckOutcome::Passed {
            output: String::new(),
        }
    } else {
        CheckOutcome::Failed {
            output: format!(
                "Files longer than {max_lines} lines:\n{}\n",
                violations.join("\n")
            ),
        }
    }
}

/// One forbidden `#[allow(clippy::too_many_arguments)]` attribute.
struct TooManyArgsFinding {
    path: String,
    line: usize,
    text: String,
}

/// Does this line declare `#[allow(...)]` or `#![allow(...)]` with
/// `clippy::too_many_arguments` among the allowed lints?
///
/// The test anchors on `#[` or `#![` at the start of the trimmed line, so prose
/// in a doc comment or a string literal cannot match.
fn line_forbids_too_many_args(line: &str) -> bool {
    let trimmed = line.trim_start();
    let rest = trimmed
        .strip_prefix("#![")
        .or_else(|| trimmed.strip_prefix("#["));
    let Some(rest) = rest else {
        return false;
    };
    let rest = rest.trim_start();
    let Some(rest) = rest.strip_prefix("allow") else {
        return false;
    };
    let rest = rest.trim_start();
    let Some(rest) = rest.strip_prefix('(') else {
        return false;
    };
    let Some(end) = rest.find(')') else {
        return false;
    };
    rest[..end]
        .split(',')
        .any(|token| token.trim() == "clippy::too_many_arguments")
}

/// Scan one file for forbidden allows, letting `#[cfg(test)]` code through.
///
/// Test gating is tracked with a brace-depth stack. Brace counting is naive: a
/// brace inside a string literal skews it. Rust source that compiles balances
/// braces in the usual case, and on a mismatch the scan errs toward reporting,
/// which shows a violation instead of hiding one.
fn scan_file_for_too_many_args(path: &str, content: &str) -> Vec<TooManyArgsFinding> {
    let mut findings = Vec::new();
    let mut depth: i32 = 0;
    let mut test_frames: Vec<i32> = Vec::new();
    let mut pending_cfg_test = false;

    for (index, line) in content.lines().enumerate() {
        let trimmed = line.trim_start();
        let is_attribute = trimmed.starts_with("#[") || trimmed.starts_with("#![");
        let is_cfg_test =
            trimmed.starts_with("#[cfg(test)]") || trimmed.starts_with("#![cfg(test)]");

        if test_frames.is_empty() && line_forbids_too_many_args(line) {
            findings.push(TooManyArgsFinding {
                path: path.to_string(),
                line: index + 1,
                text: line.to_string(),
            });
        }

        let opens = line.matches('{').count() as i32;
        let closes = line.matches('}').count() as i32;

        if pending_cfg_test {
            if opens > 0 {
                test_frames.push(depth + 1);
                pending_cfg_test = false;
            } else if !is_attribute && !trimmed.is_empty() && !trimmed.starts_with("//") {
                pending_cfg_test = false;
            }
        }
        if is_cfg_test {
            pending_cfg_test = true;
        }

        depth += opens - closes;

        while let Some(&frame) = test_frames.last() {
            if depth < frame {
                test_frames.pop();
            } else {
                break;
            }
        }
    }

    findings
}

/// Decide the ban check from collected findings.
fn evaluate_too_many_args(findings: &[TooManyArgsFinding]) -> CheckOutcome {
    if findings.is_empty() {
        return CheckOutcome::Passed {
            output: String::new(),
        };
    }
    let body = findings
        .iter()
        .map(|finding| {
            format!(
                "  {}:{}: {}",
                finding.path,
                finding.line,
                finding.text.trim()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    CheckOutcome::Failed {
        output: format!(
            "Forbidden `#[allow(clippy::too_many_arguments)]` \
             (group the arguments into a struct instead):\n{body}\n"
        ),
    }
}

// ---------------------------------------------------------------------------
// Imperative shell — processes, filesystem, orchestration
// ---------------------------------------------------------------------------

/// Run the pipeline, or manage the hooks and return.
pub fn run(args: &LintArgs) -> Result<()> {
    if args.install_hooks {
        return hooks::install_hooks();
    }
    if args.uninstall_hooks {
        return hooks::uninstall_hooks();
    }
    if args.hooks_status {
        return hooks::show_status();
    }

    let fix = args.fix || args.staged_only;

    // The staged list is read before anything can repair a file, so the same
    // files are staged again afterwards.
    let staged_files = if args.staged_only {
        Some(collect_staged_rust_files()?)
    } else {
        None
    };

    let log_path = resolve_log_path()?;
    let mut log_file = fs::File::create(&log_path)?;

    let mut failed_check: Option<String> = None;

    for check in CHECKS {
        if should_skip(check.id, args) {
            continue;
        }

        let result = match check.id {
            CheckId::FileLength => {
                let files = collect_file_lengths()?;
                CheckResult {
                    name: check.name.to_string(),
                    outcome: evaluate_file_lengths(&files, MAX_FILE_LINES),
                }
            }
            CheckId::TooManyArgsAllow => {
                let findings = collect_too_many_args()?;
                CheckResult {
                    name: check.name.to_string(),
                    outcome: evaluate_too_many_args(&findings),
                }
            }
            _ => {
                let overrides: Option<Vec<&str>> = if fix { fix_args(check.id) } else { None };
                let name = match &overrides {
                    Some(effective) => check_display_name(check.program, effective),
                    None => check.name.to_string(),
                };
                run_check(check, name, overrides.as_deref())?
            }
        };

        write!(log_file, "{}", format_log_entry(&result))?;

        match result.outcome {
            CheckOutcome::Skipped => {
                if args.verbose {
                    println!("[skip] {} (not installed)", result.name);
                }
            }
            CheckOutcome::Passed { ref output } => {
                if args.verbose {
                    print!("{output}");
                }
            }
            CheckOutcome::Failed { ref output } => {
                print!("{output}");
                failed_check = Some(result.name.clone());
                break;
            }
        }
    }

    if let Some(name) = failed_check {
        println!("\nlint failed at: {name}");
        println!("log: {log_path}");
        drop(log_file);
        std::process::exit(1);
    }

    if let Some(files) = staged_files {
        restage_files(&files)?;
    }

    Ok(())
}

/// Spawn one check and read its result.
fn run_check(check: &Check, name: String, override_args: Option<&[&str]>) -> Result<CheckResult> {
    let args: &[&str] = override_args.unwrap_or(check.args);

    let output = Command::new(check.program).args(args).output();

    let (success, text) = match output {
        Ok(output) => {
            let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
            text.push_str(&String::from_utf8_lossy(&output.stderr));
            (output.status.success(), text)
        }
        // A missing program is reported the same way a failing one is, so
        // `is_tool_not_found` can still turn it into a skip for an optional check.
        Err(error) => (false, format!("failed to run {}: {error}\n", check.program)),
    };

    let outcome = determine_outcome(success, text, check.optional);
    Ok(CheckResult { name, outcome })
}

/// The absolute path of the log file inside `target/`.
fn resolve_log_path() -> Result<String> {
    let target_dir = std::env::current_dir()?.join("target");
    fs::create_dir_all(&target_dir)?;
    Ok(target_dir.join(LOG_NAME).to_string_lossy().into_owned())
}

/// Every directory holding Rust source this task judges.
fn source_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(entries) = fs::read_dir("crates") {
        for entry in entries.flatten() {
            let src = entry.path().join("src");
            if src.is_dir() {
                roots.push(src);
            }
        }
    }
    let xtask_src = PathBuf::from("xtask/src");
    if xtask_src.is_dir() {
        roots.push(xtask_src);
    }
    roots.sort();
    roots
}

/// Collect every `.rs` file under one directory, deepest last.
fn rust_files_under(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            rust_files_under(&path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

/// Every Rust source file in the workspace, in a settled order.
fn workspace_rust_files() -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for root in source_roots() {
        rust_files_under(&root, &mut files)?;
    }
    Ok(files)
}

/// Read the line count of every source file.
fn collect_file_lengths() -> Result<Vec<(String, usize)>> {
    let mut results = Vec::new();
    for path in workspace_rust_files()? {
        let content = fs::read_to_string(&path)?;
        results.push((path.display().to_string(), content.lines().count()));
    }
    Ok(results)
}

/// Scan every source file for forbidden allows.
fn collect_too_many_args() -> Result<Vec<TooManyArgsFinding>> {
    let mut findings = Vec::new();
    for path in workspace_rust_files()? {
        let content = fs::read_to_string(&path)?;
        findings.extend(scan_file_for_too_many_args(
            &path.display().to_string(),
            &content,
        ));
    }
    Ok(findings)
}

/// The staged Rust files, as the index holds them right now.
fn collect_staged_rust_files() -> Result<Vec<String>> {
    let output = Command::new("git")
        .args(["diff", "--cached", "--name-only", "--diff-filter=ACM"])
        .output()?;
    let listing = String::from_utf8_lossy(&output.stdout);
    Ok(listing
        .lines()
        .filter(|line| line.ends_with(".rs"))
        .map(String::from)
        .collect())
}

/// Stage the repaired files again, so the commit carries the repair.
fn restage_files(files: &[String]) -> Result<()> {
    if files.is_empty() {
        return Ok(());
    }
    let mut args = vec!["add".to_string()];
    args.extend(files.iter().cloned());
    Command::new("git").args(&args).status()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::{
        check_display_name, determine_outcome, evaluate_file_lengths, evaluate_too_many_args,
        fix_args, format_log_entry, is_tool_not_found, line_forbids_too_many_args,
        scan_file_for_too_many_args, CheckId, CheckOutcome, CheckResult, MAX_FILE_LINES,
    };

    fn output_of(outcome: &CheckOutcome) -> String {
        match outcome {
            CheckOutcome::Passed { output } | CheckOutcome::Failed { output } => output.clone(),
            CheckOutcome::Skipped => String::new(),
        }
    }

    fn is_failure(outcome: &CheckOutcome) -> bool {
        matches!(outcome, CheckOutcome::Failed { .. })
    }

    #[test]
    fn a_missing_tool_reads_as_absent_not_unhappy() {
        assert!(is_tool_not_found("error: no such command: `nextest`\n"));
        assert!(is_tool_not_found("bash: cargo-nextest: not found\n"));
        assert!(is_tool_not_found("No such file or directory\n"));
        assert!(is_tool_not_found("error: unrecognized subcommand 'lint'\n"));
    }

    #[test]
    fn an_ordinary_failure_does_not_read_as_a_missing_tool() {
        assert!(!is_tool_not_found("error[E0308]: mismatched types\n"));
        assert!(!is_tool_not_found("warning: unused variable\n"));
        assert!(!is_tool_not_found(""));
    }

    #[test]
    fn only_an_optional_check_can_skip() {
        let optional = determine_outcome(false, "not found\n".to_string(), true);
        assert!(matches!(optional, CheckOutcome::Skipped));

        let required = determine_outcome(false, "not found\n".to_string(), false);
        assert!(is_failure(&required));

        let happy = determine_outcome(true, String::new(), true);
        assert!(matches!(happy, CheckOutcome::Passed { .. }));
    }

    #[test]
    fn a_log_entry_carries_the_name_and_the_output() {
        let result = CheckResult {
            name: "cargo fmt -- --check".to_string(),
            outcome: CheckOutcome::Passed {
                output: "all good\n".to_string(),
            },
        };
        assert_eq!(
            format_log_entry(&result),
            "=== cargo fmt -- --check ===\nall good\n\n"
        );

        let skipped = CheckResult {
            name: "cargo nextest".to_string(),
            outcome: CheckOutcome::Skipped,
        };
        assert!(format_log_entry(&skipped).contains("[skipped"));
    }

    #[test]
    fn fix_mode_writes_for_fmt_and_clippy_only() {
        assert_eq!(fix_args(CheckId::Fmt), Some(vec!["fmt"]));
        assert!(fix_args(CheckId::Clippy)
            .unwrap()
            .contains(&"--allow-dirty"));
        assert_eq!(fix_args(CheckId::Check), None);
        assert_eq!(fix_args(CheckId::Test), None);
        assert_eq!(fix_args(CheckId::FileLength), None);
        assert_eq!(fix_args(CheckId::TooManyArgsAllow), None);
    }

    #[test]
    fn a_display_name_joins_the_program_and_its_arguments() {
        assert_eq!(check_display_name("cargo", &["fmt"]), "cargo fmt");
        assert_eq!(check_display_name("cargo", &[]), "cargo");
    }

    #[test]
    fn a_file_at_the_limit_passes_and_one_over_it_fails() {
        let at_limit = [("crates/core/src/render.rs".to_string(), MAX_FILE_LINES)];
        assert!(!is_failure(&evaluate_file_lengths(
            &at_limit,
            MAX_FILE_LINES
        )));

        let over = [("crates/core/src/render.rs".to_string(), MAX_FILE_LINES + 1)];
        let outcome = evaluate_file_lengths(&over, MAX_FILE_LINES);
        assert!(is_failure(&outcome));
        assert!(output_of(&outcome).contains("render.rs (1001 lines)"));
    }

    #[test]
    fn an_empty_workspace_passes_the_length_check() {
        assert!(!is_failure(&evaluate_file_lengths(&[], MAX_FILE_LINES)));
    }

    #[test]
    fn the_ban_reads_an_attribute_and_ignores_prose() {
        assert!(line_forbids_too_many_args(
            "#[allow(clippy::too_many_arguments)]"
        ));
        assert!(line_forbids_too_many_args(
            "    #[allow(dead_code, clippy::too_many_arguments)]"
        ));
        assert!(line_forbids_too_many_args(
            "#![allow(clippy::too_many_arguments)]"
        ));

        assert!(!line_forbids_too_many_args(
            "/// Never write #[allow(clippy::too_many_arguments)] here."
        ));
        assert!(!line_forbids_too_many_args(
            "let text = \"#[allow(clippy::too_many_arguments)]\";"
        ));
        assert!(!line_forbids_too_many_args("#[allow(dead_code)]"));
        assert!(!line_forbids_too_many_args(
            "#[deny(clippy::too_many_arguments)]"
        ));
    }

    #[test]
    fn a_test_module_may_allow_what_the_crate_may_not() {
        let source = "\
#[allow(clippy::too_many_arguments)]
fn wide(a: u8) -> u8 { a }

#[cfg(test)]
mod tests {
    #[allow(clippy::too_many_arguments)]
    fn also_wide(a: u8) -> u8 { a }
}
";
        let findings = scan_file_for_too_many_args("lib.rs", source);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].line, 1);

        let outcome = evaluate_too_many_args(&findings);
        assert!(is_failure(&outcome));
        assert!(output_of(&outcome).contains("lib.rs:1"));
    }

    #[test]
    fn a_clean_file_passes_the_ban() {
        let findings = scan_file_for_too_many_args("lib.rs", "fn narrow() {}\n");
        assert!(findings.is_empty());
        assert!(!is_failure(&evaluate_too_many_args(&findings)));
    }
}
