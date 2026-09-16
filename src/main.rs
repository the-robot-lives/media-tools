//! CLI shell for the media-tool library.
//!
//! This binary owns only CLI concerns: clap parsing, terminal rendering (dialoguer prompts,
//! `ui::*` status lines), subprocess launching and process exit codes. All orchestration lives
//! in the `media_tool` library so other front-ends can drive it without a terminal.

use std::path::{Path, PathBuf};
use std::process::Command;

use clap::{Parser, Subcommand};
use dialoguer::MultiSelect;

use media_tool::orchestrator::{self, InputIssue};
use media_tool::pipeline::PipelineConfig;
use media_tool::{provider_config, test_lab, ui};

#[derive(Parser, Debug)]
#[command(
    name = "generate-media-prompt",
    about = "Generate media assets from .media.prompt YAML files",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Prompt files or directories to process. Directories are processed without selection.
    inputs: Vec<PathBuf>,

    /// Recursively scan a directory and choose which prompt files to process
    #[arg(short = 'r', long = "recursive", value_name = "DIR")]
    recursive_dirs: Vec<PathBuf>,

    /// Number of zellij panes to use for selected prompt batches
    #[arg(short = 'j', long = "jobs", default_value_t = 1)]
    jobs: usize,

    /// Number of candidates to generate; best is vision-selected
    #[arg(short = 'n', default_value = "1")]
    variants: usize,

    /// Show generation plan without making API calls
    #[arg(long)]
    dry_run: bool,

    /// Overwrite existing output files
    #[arg(long)]
    force: bool,

    /// Interactive refinement loop after generation
    #[arg(long)]
    refine: bool,

    /// Override generation model
    #[arg(long)]
    model: Option<String>,

    /// Show detailed output
    #[arg(long)]
    verbose: bool,

    /// Quality tier override: low|medium|high (default: per-prompt or medium)
    #[arg(long, value_name = "TIER")]
    quality: Option<String>,

    /// Pin provider service (skips auto-selection)
    #[arg(long, value_name = "SVC")]
    service: Option<String>,

    /// Skip eval grading and provider fallback
    #[arg(long)]
    no_eval: bool,

    /// Skip LLM prompt preparation (send raw prompt text to provider)
    #[arg(long)]
    no_prep: bool,

    /// Disable FIM solution injection into the prompt-prep agent's guidance
    #[arg(long)]
    no_fim: bool,

    /// Override eval endpoint base URL
    #[arg(long, value_name = "URL")]
    eval_url: Option<String>,

    /// Override eval model ID
    #[arg(long, value_name = "ID")]
    eval_model: Option<String>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Interactive web lab: browse types, generate, view, and eval media
    Lab {
        /// Listen port (default 8787)
        #[arg(long, default_value_t = 8787)]
        port: u16,
        /// Curated demos directory (default: package demos/)
        #[arg(long)]
        demos: Option<PathBuf>,
        /// Writable workspace for synthesized prompts + live outputs
        #[arg(long)]
        workspace: Option<PathBuf>,
        /// Open the default browser after start
        #[arg(long, default_value_t = true)]
        open: bool,
        /// Do not open a browser
        #[arg(long, default_value_t = false)]
        no_open: bool,
        /// Verbose logging
        #[arg(long, default_value_t = false)]
        verbose: bool,
    },
}

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    let cli = Cli::parse();

    // Load .envrc.k8.dc for API keys (GEMINI, SUNO, OPENAI, ELEVENLABS, DASHSCOPE)
    orchestrator::load_envrc();

    // Load runtime provider/model overrides (local or remote YAML)
    provider_config::init().await;

    if let Some(Commands::Lab {
        port,
        demos,
        workspace,
        open,
        no_open,
        verbose,
    }) = cli.command
    {
        let open_browser = open && !no_open;
        let cfg = test_lab::LabConfig::resolve(port, demos, workspace, verbose, open_browser)?;
        return test_lab::run_lab(cfg).await;
    }

    if cli.variants < 1 {
        color_eyre::eyre::bail!("Variant count must be at least 1");
    }
    if cli.jobs < 1 {
        color_eyre::eyre::bail!("Job count must be at least 1");
    }

    // Parse quality override
    let quality_override = orchestrator::parse_quality_override(cli.quality.as_deref())?;

    // Expand inputs: plain directories become all *.prompt files within. Directories passed
    // via -r/--recursive are shown in an interactive multi-select first.
    let resolved = orchestrator::expand_inputs(&cli.inputs, false);
    report_input_issues(&resolved.issues);
    let mut prompt_files: Vec<PathBuf> = resolved.files;

    let recursive = orchestrator::expand_inputs(&cli.recursive_dirs, true);
    report_input_issues(&recursive.issues);

    if !recursive.files.is_empty() {
        let selected = select_prompt_files(recursive.files, &cli.recursive_dirs)?;
        prompt_files.extend(selected);
    }

    prompt_files = orchestrator::normalize_prompt_files(prompt_files);

    if prompt_files.is_empty() {
        color_eyre::eyre::bail!("No valid .prompt files to process");
    }

    if cli.jobs > 1 {
        launch_zellij_batches(&prompt_files, &cli)?;
        return Ok(());
    }

    // Parse all prompt files
    ui::step("Loading prompt files");
    let verbose = cli.verbose;
    let prompts = orchestrator::load_prompts(&prompt_files, &mut |path, p| {
        if verbose {
            let svc = p.meta.service.as_deref().unwrap_or("auto");
            ui::verbose(&format!(
                "Loaded: {} ({:?}, service={}, quality={}, schema=v{})",
                path.display(),
                p.meta.asset_type,
                svc,
                p.meta.quality.as_str(),
                p.meta.schema_version
            ));
        }
    })?;
    ui::ok(&format!("Loaded {} prompt file(s)", prompts.len()));

    // Run pipeline
    // FIM solution injection into the prep agent is on by default; disable via --no-fim
    // or MEDIA_FIM_INJECT=0.
    let fim_enabled = !cli.no_fim && std::env::var("MEDIA_FIM_INJECT").ok().as_deref() != Some("0");

    let config = PipelineConfig {
        variant_count: cli.variants,
        dry_run: cli.dry_run,
        force: cli.force,
        model_override: cli.model,
        verbose: cli.verbose,
        refine: cli.refine,
        quality_override,
        service_override: cli.service,
        no_eval: cli.no_eval,
        no_prep: cli.no_prep,
        fim_enabled,
        eval_url: cli.eval_url,
        eval_model: cli.eval_model,
    };

    orchestrator::run_generation(prompts, &config).await?;

    Ok(())
}

fn report_input_issues(issues: &[InputIssue]) {
    for issue in issues {
        match issue {
            InputIssue::EmptyDirectory(path) => ui::warn_msg(&format!(
                "No *.prompt files found in directory: {}",
                path.display()
            )),
            InputIssue::NotADirectory(path) => ui::fail_msg(&format!(
                "Recursive input is not a directory: {}",
                path.display()
            )),
            InputIssue::NotFound(path) => {
                ui::fail_msg(&format!("File or directory not found: {}", path.display()))
            }
        }
    }
}

fn select_prompt_files(files: Vec<PathBuf>, roots: &[PathBuf]) -> color_eyre::Result<Vec<PathBuf>> {
    let files = orchestrator::normalize_prompt_files(files);
    let labels: Vec<String> = files
        .iter()
        .map(|path| selection_label(path, roots))
        .collect();
    let defaults = vec![true; labels.len()];

    ui::step(&format!(
        "Select prompt files ({} found; Space toggles, Enter accepts)",
        labels.len()
    ));

    let selected = MultiSelect::new()
        .with_prompt("Prompt files to process")
        .items(&labels)
        .defaults(&defaults)
        .interact_opt()?;

    let selected = selected.ok_or_else(|| color_eyre::eyre::eyre!("Prompt selection cancelled"))?;

    let selected_files: Vec<PathBuf> = selected
        .into_iter()
        .filter_map(|index| files.get(index).cloned())
        .collect();

    if selected_files.is_empty() {
        color_eyre::eyre::bail!("No prompt files selected");
    }

    ui::ok(&format!("Selected {} prompt file(s)", selected_files.len()));
    Ok(selected_files)
}

fn selection_label(path: &Path, roots: &[PathBuf]) -> String {
    for root in roots {
        if let Ok(canonical_root) = root.canonicalize() {
            if let Ok(relative) = path.strip_prefix(&canonical_root) {
                return format!("{}/{}", root.display(), relative.display());
            }
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        if let Ok(relative) = path.strip_prefix(cwd) {
            return relative.display().to_string();
        }
    }

    path.display().to_string()
}

fn launch_zellij_batches(prompt_files: &[PathBuf], cli: &Cli) -> color_eyre::Result<()> {
    if std::env::var_os("ZELLIJ").is_none() {
        color_eyre::eyre::bail!("-j/--jobs requires running inside a zellij session");
    }

    let pane_count = cli.jobs.min(prompt_files.len());
    let batches = orchestrator::build_batches(prompt_files, pane_count)?;

    if batches.is_empty() {
        color_eyre::eyre::bail!("No prompt batches to launch");
    }

    ui::step(&format!(
        "Launching {} zellij batch pane(s) for {} prompt file(s)",
        batches.len(),
        prompt_files.len()
    ));

    let exe = std::env::current_exe()
        .map_err(|e| color_eyre::eyre::eyre!("Failed to resolve current executable: {}", e))?;
    let cwd = std::env::current_dir()
        .map_err(|e| color_eyre::eyre::eyre!("Failed to resolve current directory: {}", e))?;

    for (index, batch) in batches.iter().enumerate() {
        let pane_name = format!("media-batch-{}-of-{}", index + 1, batches.len());
        let mut command = Command::new("zellij");
        command
            .arg("run")
            .arg("--name")
            .arg(&pane_name)
            .arg("--cwd")
            .arg(&cwd)
            .arg("--")
            .arg(&exe);

        append_worker_args(&mut command, cli);
        for file in batch {
            command.arg(file);
        }

        let status = command.status().map_err(|e| {
            color_eyre::eyre::eyre!("Failed to launch zellij pane '{}': {}", pane_name, e)
        })?;

        if !status.success() {
            color_eyre::eyre::bail!("zellij failed to launch pane '{}'", pane_name);
        }

        ui::ok(&format!("{}: {} prompt file(s)", pane_name, batch.len()));
    }

    Ok(())
}

fn append_worker_args(command: &mut Command, cli: &Cli) {
    command.arg("-n").arg(cli.variants.to_string());

    if cli.dry_run {
        command.arg("--dry-run");
    }
    if cli.force {
        command.arg("--force");
    }
    if cli.refine {
        command.arg("--refine");
    }
    if let Some(model) = &cli.model {
        command.arg("--model").arg(model);
    }
    if cli.verbose {
        command.arg("--verbose");
    }
    if let Some(quality) = &cli.quality {
        command.arg("--quality").arg(quality);
    }
    if let Some(service) = &cli.service {
        command.arg("--service").arg(service);
    }
    if cli.no_eval {
        command.arg("--no-eval");
    }
    if cli.no_prep {
        command.arg("--no-prep");
    }
    if cli.no_fim {
        command.arg("--no-fim");
    }
    if let Some(eval_url) = &cli.eval_url {
        command.arg("--eval-url").arg(eval_url);
    }
    if let Some(eval_model) = &cli.eval_model {
        command.arg("--eval-model").arg(eval_model);
    }
}
