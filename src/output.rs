use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::schema::ParsedPrompt;

/// Returns (output_path, per_output_description) pairs.
/// When a format entry has a `description`, it becomes the generation prompt
/// for that specific output, enabling multi-output prompts (e.g. SFX collections).
// ⟦𓊷𓋔𓁳𓃧⟧ resolve_output_paths :: Returns (output_path, per_output_description) pairs.
pub fn resolve_output_paths(prompt: &ParsedPrompt) -> Vec<(PathBuf, Option<String>)> {
    let meta = &prompt.meta;
    meta.output_formats
        .iter()
        .map(|fmt| {
            let stem = fmt.filename.as_deref().unwrap_or(&meta.name_stem);
            let path = meta.output_dir.join(format!("{}.{}", stem, fmt.format));
            (path, fmt.description.clone())
        })
        .collect()
}

// ⟦𓉞𓊧𓏃𓅂⟧ genai_dir_for :: auto-generated pointer for public function genai_dir_for
pub fn genai_dir_for(output_path: &Path) -> PathBuf {
    let name = output_path.file_name().unwrap().to_str().unwrap();
    output_path
        .parent()
        .unwrap()
        .join(format!(".genai.{}", name))
}

// ⟦𓌿𓊏𓏻𓐠⟧ genai_candidate_path :: auto-generated pointer for public function genai_candidate_path
pub fn genai_candidate_path(output_path: &Path) -> PathBuf {
    let gdir = genai_dir_for(output_path);
    std::fs::create_dir_all(&gdir).ok();
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let hex: String = (0..4).map(|_| format!("{:02x}", rand_byte())).collect();
    let ext = output_path
        .extension()
        .unwrap_or_default()
        .to_str()
        .unwrap();
    gdir.join(format!("{}_{}.{}", ts, hex, ext))
}

// ⟦𓌗𓈿𓏭𓅈⟧ link_active :: auto-generated pointer for public function link_active
pub fn link_active(genai_path: &Path, output_path: &Path) -> color_eyre::Result<()> {
    if output_path.exists() || output_path.is_symlink() {
        std::fs::remove_file(output_path)?;
    }
    std::fs::hard_link(genai_path, output_path)?;
    Ok(())
}

/// Write a metadata sidecar file next to a generated candidate.
/// Path: same as candidate but with `.metadata.yaml` replacing the extension.
// ⟦𓎝𓂷𓌣𓅎⟧ write_metadata :: Write a metadata sidecar file next to a generated candidate.
pub fn write_metadata(
    genai_path: &Path,
    service: &str,
    model: &str,
    prompt_text: &str,
    negative: Option<&str>,
    eval_score: Option<f64>,
    eval_notes: Option<&str>,
    options: &std::collections::HashMap<String, serde_yaml::Value>,
) {
    let meta_path = genai_path.with_extension("metadata.yaml");
    let mut content = String::new();
    content.push_str(&format!("service: {}\n", service));
    content.push_str(&format!("model: {}\n", model));
    content.push_str(&format!(
        "timestamp: {}\n",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    ));
    if let Some(score) = eval_score {
        content.push_str(&format!("eval_score: {:.3}\n", score));
    }
    if let Some(notes) = eval_notes {
        let escaped = notes.replace('\\', "\\\\").replace('"', "\\\"");
        content.push_str(&format!(
            "eval_notes: \"{}\"\n",
            crate::text::truncate(&escaped, 500)
        ));
    }
    if !options.is_empty() {
        content.push_str("provider_options:\n");
        // Deterministic key order. `options` is a `HashMap`, whose iteration order
        // varies from process to process. This block is *persisted* provenance
        // written next to the generated asset, so unstable ordering makes two
        // sidecars for identical inputs diff spuriously — defeating exactly the
        // reproducibility the sidecar exists to provide. Sorting here changes the
        // order only; each line keeps its original `  key: Debug(value)` shape.
        let ordered: BTreeMap<&String, &serde_yaml::Value> = options.iter().collect();
        for (k, v) in ordered {
            content.push_str(&format!("  {}: {:?}\n", k, v));
        }
    }
    content.push_str("prompt: |\n");
    for line in prompt_text.lines() {
        content.push_str(&format!("  {}\n", line));
    }
    if let Some(neg) = negative {
        if !neg.is_empty() {
            content.push_str("negative: |\n");
            for line in neg.lines() {
                content.push_str(&format!("  {}\n", line));
            }
        }
    }
    let _ = std::fs::write(&meta_path, content);
}

fn rand_byte() -> u8 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    SystemTime::now().hash(&mut hasher);
    std::thread::current().id().hash(&mut hasher);
    (hasher.finish() & 0xFF) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn scratch_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "media-tool-output-{}-{}",
            tag,
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sidecar_for(options: &HashMap<String, serde_yaml::Value>, dir: &Path, n: usize) -> String {
        let asset = dir.join(format!("asset-{n}.png"));
        write_metadata(
            &asset,
            "gemini",
            "some-model",
            "prompt text",
            None,
            None,
            None,
            options,
        );
        std::fs::read_to_string(asset.with_extension("metadata.yaml")).unwrap()
    }

    /// The sidecar's `timestamp:` line is wall-clock and legitimately varies; every
    /// other line must be byte-stable for identical inputs.
    fn without_timestamp(sidecar: &str) -> String {
        sidecar
            .lines()
            .filter(|l| !l.starts_with("timestamp:"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn many_keys() -> HashMap<String, serde_yaml::Value> {
        [
            "zulu", "alpha", "mike", "bravo", "yankee", "charlie", "tango", "delta", "sierra",
            "echo", "romeo", "foxtrot",
        ]
        .iter()
        .map(|k| (k.to_string(), serde_yaml::Value::String(format!("v-{k}"))))
        .collect()
    }

    /// Regression: the persisted `provider_options:` block must be byte-stable
    /// across writes of the same inputs, whatever order the HashMap iterates in.
    #[test]
    fn sidecar_provider_options_are_stable_across_writes() {
        let dir = scratch_dir("stable");
        let mut first: Option<String> = None;
        for n in 0..32 {
            let rendered = without_timestamp(&sidecar_for(&many_keys(), &dir, n));
            match &first {
                None => first = Some(rendered),
                Some(prev) => assert_eq!(
                    prev, &rendered,
                    "sidecar provider_options must be byte-stable across writes"
                ),
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sidecar_provider_options_are_sorted() {
        let dir = scratch_dir("sorted");
        let sidecar = sidecar_for(&many_keys(), &dir, 0);
        let emitted: Vec<&str> = sidecar
            .lines()
            .skip_while(|l| *l != "provider_options:")
            .skip(1)
            .take_while(|l| l.starts_with("  "))
            .map(|l| l.trim_start().split(':').next().unwrap())
            .collect();
        let mut sorted = emitted.clone();
        sorted.sort_unstable();
        assert_eq!(emitted, sorted, "provider_options keys must be sorted");
        assert_eq!(emitted.len(), 12);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Shape guard: sorting must not alter how a line is rendered, only its order.
    /// Each entry keeps the original `  key: Debug(value)` form.
    #[test]
    fn sidecar_provider_options_line_shape_unchanged() {
        let dir = scratch_dir("shape");
        let mut options: HashMap<String, serde_yaml::Value> = HashMap::new();
        options.insert(
            "temperature".to_string(),
            serde_yaml::Value::Number(0.3.into()),
        );
        options.insert(
            "voice".to_string(),
            serde_yaml::Value::String("nova".to_string()),
        );
        options.insert("instrumental".to_string(), serde_yaml::Value::Bool(true));

        let sidecar = sidecar_for(&options, &dir, 0);
        assert!(
            sidecar.contains(
                "provider_options:\n  instrumental: Bool(true)\n  temperature: Number(0.3)\n  voice: String(\"nova\")\n"
            ),
            "unexpected provider_options block: {sidecar}"
        );

        // Every emitted line must still match the pre-sort rendering for its pair.
        for (k, v) in &options {
            assert!(sidecar.contains(&format!("  {k}: {v:?}\n")));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An empty map must still emit no `provider_options:` header at all.
    #[test]
    fn sidecar_omits_empty_provider_options() {
        let dir = scratch_dir("empty");
        let sidecar = sidecar_for(&HashMap::new(), &dir, 0);
        assert!(!sidecar.contains("provider_options:"), "{sidecar}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
