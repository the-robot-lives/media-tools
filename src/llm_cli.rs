//! Terminal commands for named LLM profiles. Kept out of the library's I/O surface.
use clap::Subcommand;
use color_eyre::eyre::{bail, eyre, Result};
use media_tool::llm_profiles::{self, ProfileStore};
use media_tool::providers::{ChatProvider, GenerationOptions};
use std::path::PathBuf;

#[derive(Subcommand, Debug)]
pub enum LlmCommand {
    /// Keyboard-driven profile setup, import, model discovery and default selection
    Configure,
    /// List named profiles (never prints credentials)
    List,
    /// Add a profile from provider presets; exact endpoint/model may be overridden
    Add {
        name: String,
        #[arg(long)]
        provider: String,
        #[arg(long)]
        model: String,
        #[arg(long)]
        base_url: Option<String>,
        #[arg(long)]
        api_key_env: Option<String>,
        /// Endpoint requires no key (e.g. a private local inference server)
        #[arg(long)]
        local: bool,
        /// Make this the default for text-backed prompt files
        #[arg(long)]
        default: bool,
    },
    /// Select the stored default profile
    Default { name: String },
    /// Inspect/import profiles from LLM Toolkit and queue-populator (no secret copying)
    Import {
        /// Import only this candidate ID, otherwise list candidates without changing config
        #[arg(long)]
        name: Option<String>,
        /// Optional target profile ID
        #[arg(long, requires = "name")]
        as_name: Option<String>,
    },
    /// Query the selected endpoint for available model IDs
    Discover,
    /// Test the selected profile with a small completion
    Test,
    /// Generate XML, HTML, JS or other text directly into a file
    Generate {
        #[arg(
            long,
            conflicts_with = "prompt_file",
            required_unless_present = "prompt_file"
        )]
        prompt: Option<String>,
        #[arg(long)]
        prompt_file: Option<PathBuf>,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long, default_value_t = 8192)]
        max_tokens: u64,
        #[arg(long)]
        force: bool,
    },
}

fn active() -> Result<&'static llm_profiles::Profile> {
    llm_profiles::selected().ok_or_else(|| {
        eyre!("Select --llm-profile NAME, set MEDIA_TOOL_LLM_PROFILE, or run llm configure")
    })
}

pub async fn run(command: LlmCommand, flag: Option<&str>, model: Option<&str>) -> Result<()> {
    match command {
        LlmCommand::Configure => crate::llm_tui::configure().await,
        LlmCommand::List => {
            let store = ProfileStore::load()?;
            println!("Configuration: {}", llm_profiles::config_path()?.display());
            for (id, p) in store.profiles {
                println!(
                    "{}{}\t{}\t{}\t{}\tkey: {}",
                    if store.default_profile.as_deref() == Some(&id) {
                        "* "
                    } else {
                        "  "
                    },
                    id,
                    p.provider,
                    p.model,
                    p.base_url,
                    p.api_key_env.as_deref().unwrap_or("none (local)")
                );
            }
            Ok(())
        }
        LlmCommand::Add {
            name,
            provider,
            model,
            base_url,
            api_key_env,
            local,
            default,
        } => {
            let mut store = ProfileStore::load()?;
            if store.profiles.contains_key(&name) {
                bail!("Profile already exists; edit with llm configure");
            }
            let mut p = llm_profiles::preset(&provider);
            p.name = name.clone();
            p.model = model;
            if let Some(url) = base_url {
                p.base_url = url;
            }
            if local {
                p.local = true;
                p.api_key_env = None;
            }
            if let Some(key) = api_key_env {
                p.api_key_env = Some(key);
            }
            store.upsert(name.clone(), p)?;
            if default {
                store.default_profile = Some(name.clone());
            }
            store.save()?;
            println!("Saved profile {name}");
            Ok(())
        }
        LlmCommand::Default { name } => {
            let mut store = ProfileStore::load()?;
            if !store.profiles.contains_key(&name) {
                bail!("Unknown profile: {name}");
            }
            store.default_profile = Some(name.clone());
            store.save()?;
            println!("Default LLM profile: {name}");
            Ok(())
        }
        LlmCommand::Import { name, as_name } => {
            let candidates = llm_profiles::import_candidates()?;
            let Some(id) = name else {
                for (id, p) in candidates {
                    println!("{id}\t{}\t{}\t{}", p.provider, p.model, p.base_url);
                }
                println!("Use llm import --name ID to import without overwriting an existing profile. Raw/encrypted keys are never copied.");
                return Ok(());
            };
            let (_, p) = candidates
                .into_iter()
                .find(|(k, _)| k == &id)
                .ok_or_else(|| eyre!("Import candidate not found: {id}"))?;
            let id = as_name.unwrap_or(id);
            let mut store = ProfileStore::load()?;
            if store.profiles.contains_key(&id) {
                bail!("Profile {id} already exists; choose --as-name");
            }
            store.upsert(id.clone(), p)?;
            store.save()?;
            println!("Imported {id}; set its API key environment variable if remote");
            Ok(())
        }
        command => {
            llm_profiles::init(flag)?;
            let p = active()?;
            match command {
                LlmCommand::Discover => {
                    for id in llm_profiles::discover_models(p).await? {
                        println!("{id}");
                    }
                    Ok(())
                }
                LlmCommand::Test => {
                    let content = llm_profiles::complete(
                        p,
                        "Reply with only OK.",
                        "Connection test",
                        &p.model,
                        &Default::default(),
                        &[],
                    )
                    .await?;
                    println!(
                        "Profile connected: {} / {} ({} response characters)",
                        p.name,
                        p.model,
                        content.chars().count()
                    );
                    Ok(())
                }
                LlmCommand::Generate {
                    prompt,
                    prompt_file,
                    output,
                    max_tokens,
                    force,
                } => {
                    if max_tokens == 0 {
                        bail!("max-tokens must be positive");
                    }
                    if output.exists() && !force {
                        bail!("Output exists; use --force to replace it");
                    }
                    let text = match (prompt, prompt_file) {
                        (Some(t), _) => t,
                        (_, Some(path)) if path.to_str() == Some("-") => {
                            use std::io::Read;
                            let mut t = String::new();
                            std::io::stdin().read_to_string(&mut t)?;
                            t
                        }
                        (_, Some(path)) => std::fs::read_to_string(path)?,
                        _ => unreachable!(),
                    };
                    if text.trim().is_empty() {
                        bail!("Prompt must not be empty");
                    }
                    let extension = output
                        .extension()
                        .and_then(|x| x.to_str())
                        .unwrap_or("text");
                    let system=format!("Generate a complete {extension} document/file. Return only its raw content, with no Markdown fences or explanations. Follow the requested format exactly.");
                    let options = GenerationOptions {
                        model: model.unwrap_or(&p.model).into(),
                        aspect_ratio: None,
                        negative_prompt: None,
                        provider_options: std::collections::HashMap::from([(
                            "max_tokens".into(),
                            serde_yaml::Value::from(max_tokens),
                        )]),
                        verbose: false,
                        duration_seconds: None,
                        audio_kind: media_tool::schema::AudioKind::default(),
                    };
                    llm_profiles::ProfileProvider(p.clone())
                        .generate(&system, &text, &output, "", &options, &[])
                        .await?;
                    println!("Generated {} with {}", output.display(), p.name);
                    Ok(())
                }
                _ => unreachable!(),
            }
        }
    }
}
