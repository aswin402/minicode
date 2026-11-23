//! Standalone `minidoctor` CLI binary — universal native Rust health diagnostic engine.

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use minidoctor::{
    explain_rule, format_neededfix_markdown, format_sarif_json, format_terminal_report, list_rules,
    run_doctor, DoctorOptions, ScanScope,
};
use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser, Debug)]
#[command(
    name = "minidoctor",
    version = env!("CARGO_PKG_VERSION"),
    about = "Universal multi-stack native Rust codebase health doctor & AI remediation CLI"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<DoctorSubcommand>,

    /// Target workspace directory to scan (defaults to current directory)
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Show detailed fix suggestions and expanded diagnostics
    #[arg(short = 'v', long)]
    verbose: bool,

    /// Scan only files changed relative to a git ref (defaults to working tree changes if omitted)
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    diff: Option<String>,

    /// Output format
    #[arg(long, value_enum, default_value = "terminal")]
    format: OutputFormat,

    /// Output only the integer health score (0–100) for CI scripts
    #[arg(long)]
    score: bool,

    /// Minimum health score required to exit 0 (overrides config threshold, default 75)
    #[arg(long)]
    threshold: Option<u8>,

    /// Skip cross-file dead code and duplicate detection
    #[arg(long)]
    no_dead_code: bool,

    /// Skip security and secret checks
    #[arg(long)]
    no_security: bool,

    /// Skip writing `.minicode/minidoctor/NEEDEDFIX.md` and workspace telemetry
    #[arg(long)]
    no_md: bool,

    /// Export an additional copy of `NEEDEDFIX.md` to a custom file path
    #[arg(long)]
    export: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
enum DoctorSubcommand {
    /// List or explain built-in diagnostic rules
    Rules {
        #[command(subcommand)]
        action: RulesAction,
    },
}

#[derive(Subcommand, Debug)]
enum RulesAction {
    /// List all built-in diagnostic rules across all stacks
    List,
    /// Show detailed documentation and remediation advice for a rule ID
    Explain {
        /// Rule ID (e.g. `fastapi/sync-in-async-route`, `react/async-use-effect`)
        rule: String,
    },
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
enum OutputFormat {
    Terminal,
    Json,
    Sarif,
    Markdown,
}

fn main() -> ExitCode {
    match run_cli() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("minidoctor error: {e}");
            ExitCode::from(2)
        }
    }
}

fn run_cli() -> Result<ExitCode> {
    let cli = Cli::parse();

    if let Some(DoctorSubcommand::Rules { action }) = cli.command {
        match action {
            RulesAction::List => {
                println!(
                    "{:<34} {:<10} {:<15} {:<8} SUMMARY",
                    "RULE ID", "STACK", "CATEGORY", "SEVERITY"
                );
                println!("{}", "─".repeat(104));
                for r in list_rules() {
                    println!(
                        "{:<34} {:<10} {:<15} {:<8} {}",
                        r.id,
                        r.stack,
                        r.category.label(),
                        r.default_severity.badge(),
                        r.summary
                    );
                }
                return Ok(ExitCode::SUCCESS);
            }
            RulesAction::Explain { rule } => {
                if let Some(meta) = explain_rule(&rule) {
                    println!("Rule:     {}", meta.id);
                    println!("Stack:    {}", meta.stack);
                    println!("Category: {}", meta.category.label());
                    println!("Severity: {}", meta.default_severity.badge());
                    println!("\nSummary:\n  {}", meta.summary);
                    println!("\nWhy it matters:\n  {}", meta.why);
                    println!("\nHow to fix:\n  {}", meta.how_to_fix);
                    return Ok(ExitCode::SUCCESS);
                }
                eprintln!(
                    "Unknown rule `{rule}`. Run `minidoctor rules list` to see available rules."
                );
                return Ok(ExitCode::from(1));
            }
        }
    }

    let workspace = if cli.path.is_absolute() {
        cli.path.clone()
    } else {
        std::env::current_dir()?.join(&cli.path)
    };

    let scope = match cli.diff {
        Some(ref r) if !r.trim().is_empty() => ScanScope::Changed(Some(r.trim().to_string())),
        Some(_) => ScanScope::Changed(None),
        None => ScanScope::Full,
    };

    let opts = DoctorOptions {
        scope,
        run_dead_code: !cli.no_dead_code,
        run_security: !cli.no_security,
        write_workspace_artifacts: !cli.no_md,
        export_md_path: cli.export,
        threshold_override: cli.threshold,
        max_files: 2500,
    };

    let report = run_doctor(&workspace, &opts)?;

    if cli.score {
        println!("{}", report.score.overall);
    } else {
        match cli.format {
            OutputFormat::Terminal => {
                let use_color = std::io::stdout().is_terminal();
                print!(
                    "{}",
                    format_terminal_report(&report, cli.verbose, use_color)
                );
            }
            OutputFormat::Json => {
                println!("{}", serde_json::to_string_pretty(&report)?);
            }
            OutputFormat::Sarif => {
                println!("{}", format_sarif_json(&report));
            }
            OutputFormat::Markdown => {
                print!("{}", format_neededfix_markdown(&report));
            }
        }
    }

    if report.passed_threshold() {
        Ok(ExitCode::SUCCESS)
    } else {
        Ok(ExitCode::from(1))
    }
}
