//! CLI-only keyboard wizard for named LLM profiles. Credentials remain env references.

use std::io::{self, IsTerminal, Write};

use color_eyre::eyre::{bail, Result};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{self, ClearType},
};
use dialoguer::{Confirm, MultiSelect, Select};
use media_tool::llm_profiles::{self, ProfileStore, PROVIDERS};

/// Interactive edits are staged in memory and persisted only after confirmation.
pub async fn configure() -> Result<()> {
    if !io::stdin().is_terminal() || !io::stderr().is_terminal() {
        bail!("LLM configuration needs a terminal. Use `media-tool llm add`, `llm import`, or `llm list` in scripts.");
    }
    eprintln!("LLM profiles — {}", llm_profiles::config_path()?.display());
    eprintln!("Arrow keys + Enter select; Esc cancels. No API keys are stored here.");
    let mut store = ProfileStore::load()?;
    loop {
        show_profiles(&store);
        let actions = [
            "Add profile (assisted setup)",
            "Edit profile",
            "Choose default profile",
            "Discover available models",
            "Import from existing utilities",
            "Done",
        ];
        let Some(action) = Select::new()
            .with_prompt("LLM configuration")
            .items(&actions)
            .default(0)
            .interact_opt()?
        else {
            return Ok(());
        };
        let result: Result<()> = match action {
            0 => edit_profile(&mut store, None).await,
            1 => match choose_profile(&store, "Edit which profile?")? {
                Some(id) => edit_profile(&mut store, Some(id)).await,
                None => Ok(()),
            },
            2 => set_default(&mut store),
            3 => discover(&mut store).await,
            4 => import_profiles(&mut store),
            _ => return Ok(()),
        };
        if let Err(error) = result {
            // No credential values are part of wizard inputs or diagnostics.
            eprintln!("Configuration was not saved: {error}");
        }
    }
}

fn show_profiles(store: &ProfileStore) {
    eprintln!();
    if store.profiles.is_empty() {
        eprintln!("No profiles yet. Add one or import from your other utilities.");
    }
    for (id, profile) in &store.profiles {
        let marker = if store.default_profile.as_deref() == Some(id.as_str()) {
            " [default]"
        } else {
            ""
        };
        eprintln!("  {id}{marker} — {} / {}", profile.provider, profile.model);
    }
    eprintln!();
}

fn choose_profile(store: &ProfileStore, prompt: &str) -> Result<Option<String>> {
    let ids: Vec<_> = store.profiles.keys().cloned().collect();
    if ids.is_empty() {
        eprintln!("Add a profile first.");
        return Ok(None);
    }
    let labels: Vec<_> = ids
        .iter()
        .map(|id| format!("{} — {}", id, store.profiles[id].model))
        .collect();
    Ok(Select::new()
        .with_prompt(prompt)
        .items(&labels)
        .default(0)
        .interact_opt()?
        .map(|index| ids[index].clone()))
}

fn confirmed(prompt: &str) -> Result<bool> {
    Ok(Confirm::new()
        .with_prompt(prompt)
        .default(false)
        .interact_opt()?
        .unwrap_or(false))
}

async fn edit_profile(store: &mut ProfileStore, existing: Option<String>) -> Result<()> {
    let existing_profile = existing.as_ref().and_then(|id| store.profiles.get(id));
    let default_provider = existing_profile
        .and_then(|p| {
            PROVIDERS
                .iter()
                .position(|candidate| *candidate == p.provider)
        })
        .unwrap_or(0);
    let Some(index) = Select::new()
        .with_prompt("Provider (URL and model can be customized)")
        .items(PROVIDERS)
        .default(default_provider)
        .interact_opt()?
    else {
        return Ok(());
    };
    let provider = PROVIDERS[index];
    let mut profile = match existing_profile {
        Some(profile) if profile.provider == provider => profile.clone(),
        _ => llm_profiles::preset(provider),
    };
    let suggested = existing.unwrap_or_else(|| profile_id(&profile.name));
    let Some(id) = field("Profile ID", &suggested)? else {
        return Ok(());
    };
    let Some(name) = field("Display name", &profile.name)? else {
        return Ok(());
    };
    profile.name = name;
    let Some(url) = field("Base URL", &profile.base_url)? else {
        return Ok(());
    };
    profile.base_url = url;
    let Some(local) = Confirm::new()
        .with_prompt("Local/self-hosted endpoint (allows no API key)")
        .default(profile.local)
        .interact_opt()?
    else {
        return Ok(());
    };
    profile.local = local;
    eprintln!("Enter an environment VARIABLE NAME, not the API key; leave empty for no key.");
    let Some(key_env) = field(
        "API key environment variable",
        profile.api_key_env.as_deref().unwrap_or(""),
    )?
    else {
        return Ok(());
    };
    profile.api_key_env = (!key_env.trim().is_empty()).then(|| key_env.trim().to_string());

    let model_actions = ["Discover models from endpoint", "Enter model ID manually"];
    let Some(model_action) = Select::new()
        .with_prompt("Populate model")
        .items(&model_actions)
        .default(0)
        .interact_opt()?
    else {
        return Ok(());
    };
    if model_action == 0 {
        match llm_profiles::discover_models(&profile).await {
            Ok(models) if !models.is_empty() => {
                let selected = models.iter().position(|m| *m == profile.model).unwrap_or(0);
                let Some(index) = Select::new()
                    .with_prompt("Available model")
                    .items(&models)
                    .default(selected)
                    .interact_opt()?
                else {
                    return Ok(());
                };
                profile.model = models[index].clone();
            }
            Ok(_) => eprintln!("Endpoint returned no models. Enter an exact model ID below."),
            Err(error) => {
                eprintln!("Discovery unavailable: {error}. Enter an exact model ID below.")
            }
        }
    }
    let Some(model) = field("Model ID", &profile.model)? else {
        return Ok(());
    };
    profile.model = model;
    profile.validate()?;
    eprintln!(
        "{id}: {} / {} at {} (key: {})",
        profile.provider,
        profile.model,
        profile.base_url,
        profile.api_key_env.as_deref().unwrap_or("none")
    );
    if !confirmed(if store.profiles.contains_key(&id) {
        "Replace this saved profile?"
    } else {
        "Save this profile?"
    })? {
        return Ok(());
    }
    // Mutate a staged store: even a failed save leaves the visible store untouched.
    let mut staged = store.clone();
    staged.upsert(id.clone(), profile)?;
    if staged.default_profile.is_none() {
        staged.default_profile = Some(id.clone());
    }
    staged.save()?;
    *store = staged;
    eprintln!("Saved {id}. Select it with MEDIA_TOOL_LLM_PROFILE={id}.");
    Ok(())
}

fn set_default(store: &mut ProfileStore) -> Result<()> {
    let Some(id) = choose_profile(store, "Default profile (env/CLI still take precedence)")? else {
        return Ok(());
    };
    if confirmed(&format!("Use {id} by default?"))? {
        let mut staged = store.clone();
        staged.default_profile = Some(id);
        staged.save()?;
        *store = staged;
    }
    Ok(())
}

async fn discover(store: &mut ProfileStore) -> Result<()> {
    let Some(id) = choose_profile(store, "Discover models for which profile?")? else {
        return Ok(());
    };
    let models = llm_profiles::discover_models(&store.profiles[&id]).await?;
    if models.is_empty() {
        eprintln!("Endpoint returned no models.");
        return Ok(());
    }
    let Some(index) = Select::new()
        .with_prompt("Model (Esc leaves saved profile unchanged)")
        .items(&models)
        .default(0)
        .interact_opt()?
    else {
        return Ok(());
    };
    if confirmed(&format!("Set model to {}?", models[index]))? {
        let mut staged = store.clone();
        let profile = staged
            .profiles
            .get_mut(&id)
            .expect("selected saved profile");
        profile.model = models[index].clone();
        profile.validate()?;
        staged.save()?;
        *store = staged;
    }
    Ok(())
}

fn import_profiles(store: &mut ProfileStore) -> Result<()> {
    let candidates = llm_profiles::import_candidates()?;
    if candidates.is_empty() {
        eprintln!("No importable utility profiles found. Add a profile manually instead.");
        return Ok(());
    }
    eprintln!(
        "Imports copy settings and env references only; stored API key values are never copied."
    );
    let labels: Vec<_> = candidates
        .iter()
        .map(|(id, p)| format!("{id} — {} / {}", p.provider, p.model))
        .collect();
    let Some(selected) = MultiSelect::new()
        .with_prompt("Profiles to import (Space toggles, Enter continues, Esc cancels)")
        .items(&labels)
        .interact_opt()?
    else {
        return Ok(());
    };
    let mut staged = store.clone();
    let mut imported = 0;
    for index in selected {
        let (id, profile) = &candidates[index];
        if staged.profiles.contains_key(id) {
            match Confirm::new()
                .with_prompt(format!("Replace existing profile {id}?"))
                .default(false)
                .interact_opt()?
            {
                Some(true) => {}
                Some(false) => continue,
                None => return Ok(()),
            }
        }
        staged.upsert(id.clone(), profile.clone())?;
        imported += 1;
    }
    if imported == 0 || !confirmed(&format!("Save {imported} imported profile(s)?"))? {
        return Ok(());
    }
    staged.save()?;
    *store = staged;
    eprintln!("Saved {imported} profile(s).");
    Ok(())
}

fn profile_id(name: &str) -> String {
    let id = name
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>()
        .join("-");
    if id.is_empty() {
        "local-llm".to_string()
    } else {
        id
    }
}

struct RawInput;

impl Drop for RawInput {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
    }
}

/// A single-line editor with real Escape/Ctrl-C/Ctrl-D cancellation. Its viewport
/// scrolls horizontally, so long URLs remain editable in narrow terminals.
fn field(label: &str, initial: &str) -> Result<Option<String>> {
    eprintln!("{label} (Enter accepts; Esc cancels)");
    terminal::enable_raw_mode()?;
    let guard = RawInput;
    let mut output = io::stderr();
    let mut value: Vec<char> = initial.chars().collect();
    let mut position = value.len();
    loop {
        let columns = terminal::size()
            .map(|(cols, _)| usize::from(cols))
            .unwrap_or(80);
        let width = columns.saturating_sub(4).max(1);
        let start = position.saturating_sub(width - 1);
        let visible: String = value.iter().skip(start).take(width).collect();
        execute!(
            output,
            cursor::MoveToColumn(0),
            terminal::Clear(ClearType::CurrentLine)
        )?;
        write!(output, "> {visible}")?;
        execute!(output, cursor::MoveToColumn((2 + position - start) as u16))?;
        output.flush()?;
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind == KeyEventKind::Release {
            continue;
        }
        match key.code {
            KeyCode::Esc => {
                drop(guard);
                eprintln!();
                return Ok(None);
            }
            KeyCode::Char(c) if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if matches!(c, 'c' | 'd') {
                    drop(guard);
                    eprintln!();
                    return Ok(None);
                }
                if c == 'a' {
                    position = 0;
                }
                if c == 'e' {
                    position = value.len();
                }
                if c == 'u' {
                    value.clear();
                    position = 0;
                }
            }
            KeyCode::Enter => {
                drop(guard);
                eprintln!();
                return Ok(Some(value.iter().collect()));
            }
            KeyCode::Char(c) => {
                value.insert(position, c);
                position += 1;
            }
            KeyCode::Backspace if position > 0 => {
                position -= 1;
                value.remove(position);
            }
            KeyCode::Delete if position < value.len() => {
                value.remove(position);
            }
            KeyCode::Left => position = position.saturating_sub(1),
            KeyCode::Right => position = (position + 1).min(value.len()),
            KeyCode::Home => position = 0,
            KeyCode::End => position = value.len(),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::profile_id;

    #[test]
    fn suggested_ids_are_portable_and_nonempty() {
        assert_eq!(profile_id("Local Qwen 3.8"), "local-qwen-3-8");
        assert_eq!(profile_id("  LM Studio / local  "), "lm-studio-local");
        assert_eq!(profile_id("模型"), "local-llm");
    }
}
