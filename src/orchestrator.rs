//! Orchestration extracted from the CLI binary.
//!
//! These functions carry the *decisions and work* a front-end needs: resolving inputs into
//! prompt files, parsing quality overrides, loading prompts, grouping dependent prompts into
//! batches, and running the generation pipeline. None of them touch a terminal — where the
//! CLI used to print inline, the function either returns the information ([`ResolvedInputs`]'s
//! issue list) or invokes a caller-supplied callback ([`load_prompts`]).
//!
//! The one exception is [`explain_prompt`], which is the terminal rendering of
//! [`explain`]; front-ends without a terminal call [`explain`] and render the
//! returned [`Explanation`] themselves.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::pipeline::{self, PipelineConfig};
use crate::preferences;
use crate::schema::{parse_prompt_file, ParsedPrompt, Quality};
use crate::telemetry as tel;

/// A non-fatal problem encountered while resolving CLI-style inputs into prompt files.
///
/// The caller decides how (or whether) to surface these; the CLI prints them as warnings /
/// failures exactly as it did when the logic lived in `main.rs`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputIssue {
    /// A directory was scanned but contained no `*.prompt` files.
    EmptyDirectory(PathBuf),
    /// A path was passed that is neither a file nor a directory.
    NotFound(PathBuf),
    /// A path passed as a recursive root exists but is not a directory.
    NotADirectory(PathBuf),
}

/// Prompt files discovered from a set of inputs, plus any non-fatal issues.
#[derive(Debug, Default, Clone)]
pub struct ResolvedInputs {
    /// Prompt files found, in discovery order (directories sorted).
    pub files: Vec<PathBuf>,
    /// Non-fatal issues, in the order they were encountered.
    pub issues: Vec<InputIssue>,
}

/// Load `.envrc.k8.dc` API keys into the process environment.
///
/// Looks at `$INFRA_ROOT/.envrc.k8.dc` then `$HOME/.envrc.k8.dc`; existing variables win.
pub fn load_envrc() {
    let candidates = [
        std::env::var("INFRA_ROOT")
            .ok()
            .map(|r| PathBuf::from(r).join(".envrc.k8.dc")),
        std::env::var("HOME")
            .ok()
            .map(|h| PathBuf::from(h).join(".envrc.k8.dc")),
    ];

    for candidate in candidates.into_iter().flatten() {
        if candidate.is_file() {
            if let Ok(content) = std::fs::read_to_string(&candidate) {
                for line in content.lines() {
                    let line = line.trim();
                    if let Some(rest) = line.strip_prefix("export ") {
                        if let Some((key, val)) = rest.split_once('=') {
                            let key = key.trim();
                            let val = val.trim().trim_matches('"').trim_matches('\'');
                            if std::env::var(key).is_err() {
                                std::env::set_var(key, val);
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Recursively collect `*.prompt` files under `dir`, skipping hidden directories.
pub fn collect_prompt_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            // Skip hidden directories (.genai.*, .DS_Store dirs, etc.)
            if path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with('.'))
                .unwrap_or(false)
            {
                continue;
            }
            collect_prompt_files(&path, out);
        } else if path.is_file()
            && path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.ends_with(".prompt"))
                .unwrap_or(false)
        {
            out.push(path);
        }
    }
}

/// Expand a list of inputs into prompt files.
///
/// Directories are scanned recursively; plain files are taken as-is. When `require_dirs` is
/// true (the `-r/--recursive` roots), a non-directory input is reported as
/// [`InputIssue::NotADirectory`] rather than accepted as a file.
pub fn expand_inputs(inputs: &[PathBuf], require_dirs: bool) -> ResolvedInputs {
    let mut resolved = ResolvedInputs::default();

    for input in inputs {
        if input.is_dir() {
            let mut found: Vec<PathBuf> = Vec::new();
            collect_prompt_files(input, &mut found);
            found.sort();
            if found.is_empty() {
                resolved.issues.push(InputIssue::EmptyDirectory(input.clone()));
            }
            resolved.files.extend(found);
        } else if require_dirs {
            if input.exists() {
                resolved.issues.push(InputIssue::NotADirectory(input.clone()));
            } else {
                resolved.issues.push(InputIssue::NotFound(input.clone()));
            }
        } else if input.is_file() {
            resolved.files.push(input.clone());
        } else {
            resolved.issues.push(InputIssue::NotFound(input.clone()));
        }
    }

    resolved
}

/// Canonicalize, de-duplicate and sort a list of prompt files.
pub fn normalize_prompt_files(files: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();

    for path in files {
        let path = path.canonicalize().unwrap_or(path);
        let key = path.to_string_lossy().into_owned();
        if seen.insert(key) {
            normalized.push(path);
        }
    }

    normalized.sort();
    normalized
}

/// Parse a `--quality` style tier string into a [`Quality`].
pub fn parse_quality_override(raw: Option<&str>) -> color_eyre::Result<Option<Quality>> {
    match raw {
        Some(q) => match q.parse::<Quality>() {
            Ok(v) => Ok(Some(v)),
            Err(e) => color_eyre::eyre::bail!("--quality: {}", e),
        },
        None => Ok(None),
    }
}

/// Parse every prompt file, invoking `on_loaded` after each successful parse.
///
/// The callback is how a front-end reports progress; this function never prints.
pub fn load_prompts(
    paths: &[PathBuf],
    on_loaded: &mut dyn FnMut(&Path, &ParsedPrompt),
) -> color_eyre::Result<Vec<ParsedPrompt>> {
    let mut prompts = Vec::with_capacity(paths.len());
    for path in paths {
        let parsed = parse_prompt_file(path)?;
        on_loaded(path, &parsed);
        prompts.push(parsed);
    }
    Ok(prompts)
}

/// Run the generation pipeline over already-parsed prompts.
pub async fn run_generation(
    prompts: Vec<ParsedPrompt>,
    config: &PipelineConfig,
) -> color_eyre::Result<()> {
    pipeline::run_generation(prompts, config).await
}

/// Split prompt files into at most `batch_count` batches, keeping dependency-linked prompts
/// together in the same batch and balancing batch sizes.
pub fn build_batches(
    prompt_files: &[PathBuf],
    batch_count: usize,
) -> color_eyre::Result<Vec<Vec<PathBuf>>> {
    if batch_count == 0 || prompt_files.is_empty() {
        return Ok(Vec::new());
    }

    let prompts = prompt_files
        .iter()
        .map(|path| parse_prompt_file(path))
        .collect::<color_eyre::Result<Vec<_>>>()?;

    let groups = dependency_groups(&prompts)?;
    let mut batches = vec![Vec::new(); batch_count];
    let mut batch_sizes = vec![0usize; batch_count];

    for group in groups {
        let batch_index = batch_sizes
            .iter()
            .enumerate()
            .min_by_key(|(_, size)| **size)
            .map(|(index, _)| index)
            .unwrap_or(0);

        for prompt_index in group {
            batches[batch_index].push(prompt_files[prompt_index].clone());
            batch_sizes[batch_index] += 1;
        }
    }

    batches.retain(|batch| !batch.is_empty());
    Ok(batches)
}

/// Group prompt indices into connected components of the `depends_on` graph.
pub fn dependency_groups(prompts: &[ParsedPrompt]) -> color_eyre::Result<Vec<Vec<usize>>> {
    let mut by_id: HashMap<String, usize> = HashMap::new();
    let mut by_path: HashMap<String, usize> = HashMap::new();

    for (index, prompt) in prompts.iter().enumerate() {
        if by_id.insert(prompt.meta.id.clone(), index).is_some() {
            color_eyre::eyre::bail!("Duplicate prompt ID: {}", prompt.meta.id);
        }
        by_path.insert(path_key(&prompt.meta.path), index);
    }

    let mut dsu = DisjointSet::new(prompts.len());

    for (index, prompt) in prompts.iter().enumerate() {
        for dep in &prompt.payload.depends_on {
            let ref_id = dep.ref_id();
            let dependency_index = if let Some(found) = by_id.get(ref_id) {
                *found
            } else {
                let dep_path = prompt.meta.output_dir.join(ref_id);
                let dep_key = path_key(&dep_path);
                *by_path.get(&dep_key).ok_or_else(|| {
                    color_eyre::eyre::eyre!(
                        "Selected prompt {} depends on '{}' but that prompt was not selected",
                        prompt.meta.path.display(),
                        ref_id
                    )
                })?
            };

            dsu.union(index, dependency_index);
        }
    }

    let mut by_root: HashMap<usize, Vec<usize>> = HashMap::new();
    for index in 0..prompts.len() {
        by_root.entry(dsu.find(index)).or_default().push(index);
    }

    let mut groups: Vec<Vec<usize>> = by_root.into_values().collect();
    groups.sort_by_key(|group| group.iter().copied().min().unwrap_or(usize::MAX));
    Ok(groups)
}

fn path_key(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

struct DisjointSet {
    parent: Vec<usize>,
    rank: Vec<usize>,
}

impl DisjointSet {
    fn new(len: usize) -> Self {
        Self {
            parent: (0..len).collect(),
            rank: vec![0; len],
        }
    }

    fn find(&mut self, index: usize) -> usize {
        if self.parent[index] != index {
            self.parent[index] = self.find(self.parent[index]);
        }
        self.parent[index]
    }

    fn union(&mut self, left: usize, right: usize) {
        let left_root = self.find(left);
        let right_root = self.find(right);

        if left_root == right_root {
            return;
        }

        match self.rank[left_root].cmp(&self.rank[right_root]) {
            std::cmp::Ordering::Less => self.parent[left_root] = right_root,
            std::cmp::Ordering::Greater => self.parent[right_root] = left_root,
            std::cmp::Ordering::Equal => {
                self.parent[right_root] = left_root;
                self.rank[left_root] += 1;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// `explain` — render the preference cascade for one prompt (design §4.3)
// ---------------------------------------------------------------------------

/// Everything `explain` needs about one prompt: the facts selectors matched
/// against, the loaded rule set, and the full provenance of each field.
///
/// Returned rather than printed so the GUI can render the same trace as a
/// provenance popover; [`explain_prompt`] is the terminal rendering of it.
#[derive(Debug, Clone)]
pub struct Explanation {
    pub path: PathBuf,
    pub facts: preferences::PromptFacts,
    pub resolution: preferences::Resolution,
    /// Snippet names that resolved but are not defined under `snippets:`.
    pub missing_snippets: Vec<String>,
    /// Config selectors that failed to parse and were skipped.
    pub errors: Vec<preferences::SelectorError>,
}

/// Resolve the cascade for a single prompt file without generating anything.
///
/// Pure with respect to the prompt: nothing is mutated and no API is called.
// ⟦𓉦𓈬𓅷𓏃⟧ explain :: resolve the preference cascade for one prompt file
pub fn explain(path: &Path, config: &PipelineConfig) -> color_eyre::Result<Explanation> {
    let prompt = parse_prompt_file(path)?;
    let prefs = preferences::PreferenceSet::from_config(crate::provider_config::loaded());
    let facts = preferences::PromptFacts::from_prompt(&prompt, config.quality_override);
    let file = preferences::file_overrides(&prompt);
    let cli = pipeline::cli_overrides(config);
    let resolution = preferences::resolve(&prefs, &facts, &file, &cli);
    let missing_snippets = resolution
        .snippets()
        .map(|names| prefs.compose(names).1)
        .unwrap_or_default();
    Ok(Explanation {
        path: prompt.meta.path.clone(),
        facts,
        resolution,
        missing_snippets,
        errors: prefs.errors.clone(),
    })
}

/// Print an [`Explanation`] through the telemetry facade.
///
/// Library code never touches the terminal directly, so every line here is a
/// `media_tool::ui` event: the preamble reuses the existing `plan_detail` kind
/// and the provenance block rides `raw` (pre-formatted, printed verbatim), the
/// same channel `provider_config` already uses for its warnings. A GUI ignores
/// all of it and renders the returned [`Explanation`] instead.
// ⟦𓆊𓐁𓀉𓍒⟧ explain_prompt :: resolve and print the cascade for one prompt file
pub fn explain_prompt(path: &Path, config: &PipelineConfig) -> color_eyre::Result<()> {
    let ex = explain(path, config)?;

    tel::step(&format!("Preference cascade for {}", ex.path.display()));
    tel::plan_detail("id", &ex.facts.id);
    tel::plan_detail("type", &ex.facts.type_name);
    let tags = if ex.facts.tags.is_empty() {
        "\u{2014}".to_string()
    } else {
        ex.facts.tags.join(", ")
    };
    tel::plan_detail("tags", &tags);
    tel::plan_detail("formats", &ex.facts.formats.join(", "));
    tel::plan_detail("quality", &format!("{} (pre-cascade)", ex.facts.quality));

    for err in &ex.errors {
        tel::warn_msg(&format!(
            "ignoring preference `{}` \u{2014} {}",
            err.selector, err.message
        ));
    }

    for field in preferences::Field::ALL {
        let Some(trace) = ex.resolution.trace(field) else {
            continue;
        };
        tel::blank();
        match trace.winning() {
            Some(c) => tel::raw(&format!("  {}: {}", field.as_str(), c.value.display())),
            None => tel::raw(&format!("  {}: (unset)", field.as_str())),
        }
        if trace.candidates.is_empty() {
            tel::raw("      (no rule, file field or flag set this)");
            continue;
        }
        for (i, c) in trace.candidates.iter().enumerate() {
            let marker = if Some(i) == trace.winner {
                "\u{2190} winner"
            } else {
                "(overridden)"
            };
            tel::raw(&format!(
                "      [{}] {:<34} {:<26} {}",
                c.rank,
                c.origin.label(),
                c.value.display(),
                marker
            ));
        }
    }

    for name in &ex.missing_snippets {
        tel::warn_msg(&format!(
            "unknown snippet `{name}` \u{2014} not defined under `snippets:`"
        ));
    }

    Ok(())
}
