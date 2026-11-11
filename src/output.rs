use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::schema::ParsedPrompt;

/// Returns (output_path, per_output_description) pairs.
/// When a format entry has a `description`, it becomes the generation prompt
/// for that specific output, enabling multi-output prompts (e.g. SFX collections).
// <REMOVED UUID HERE> resolve_output_paths :: Returns (output_path, per_output_description) pairs.
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

// <REMOVED UUID HERE> genai_dir_for :: auto-generated pointer for public function genai_dir_for
pub fn genai_dir_for(output_path: &Path) -> PathBuf {
    let name = output_path.file_name().unwrap().to_str().unwrap();
    output_path
        .parent()
        .unwrap()
        .join(format!(".genai.{}", name))
}

// <REMOVED UUID HERE> genai_candidate_path :: auto-generated pointer for public function genai_candidate_path
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

// <REMOVED UUID HERE> link_active :: auto-generated pointer for public function link_active
pub fn link_active(genai_path: &Path, output_path: &Path) -> color_eyre::Result<()> {
    if output_path.exists() || output_path.is_symlink() {
        std::fs::remove_file(output_path)?;
    }
    std::fs::hard_link(genai_path, output_path)?;
    Ok(())
}

/// Eval provenance for a sidecar: which attempt produced the artifact, whether
/// it passed the rubric, and the full score (per-criterion values, judge
/// identity/model/request id, observer model, probability distributions).
pub struct EvalMeta<'a> {
    pub attempt: Option<usize>,
    pub verdict: Option<bool>,
    pub score: Option<&'a crate::eval::EvalScore>,
}

impl<'a> EvalMeta<'a> {
    pub fn from_score(attempt: usize, verdict: bool, score: &'a crate::eval::EvalScore) -> Self {
        Self {
            attempt: Some(attempt),
            verdict: Some(verdict),
            score: Some(score),
        }
    }
}

/// Write a metadata sidecar file next to a generated candidate.
/// Path: same as candidate but with `.metadata.yaml` replacing the extension.
// <REMOVED UUID HERE> write_metadata :: Write a metadata sidecar file next to a generated candidate.
#[allow(clippy::too_many_arguments)]
pub fn write_metadata(
    genai_path: &Path,
    service: &str,
    model: &str,
    prompt_text: &str,
    negative: Option<&str>,
    eval_score: Option<f64>,
    eval_notes: Option<&str>,
    options: &std::collections::HashMap<String, serde_yaml::Value>,
    eval_meta: Option<&EvalMeta<'_>>,
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
    if let Some(meta) = eval_meta {
        content.push_str(&render_eval_block(meta));
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

/// Render the structured `eval:` block: attempt, verdict, per-criterion scores,
/// judge identity (backend/model/request id), observer model when the judge saw
/// an observe pass, and per-question probability distributions. Keys are sorted
/// for byte-stable sidecars.
fn render_eval_block(meta: &EvalMeta<'_>) -> String {
    let mut out = String::from("eval:\n");
    if let Some(a) = meta.attempt {
        out.push_str(&format!("  attempt: {}\n", a));
    }
    if let Some(v) = meta.verdict {
        out.push_str(&format!("  verdict: {}\n", if v { "pass" } else { "fail" }));
    }
    if let Some(s) = meta.score {
        out.push_str(&format!("  weighted: {:.4}\n", s.weighted));
        if !s.per_criterion.is_empty() {
            out.push_str("  per_criterion:\n");
            let ordered: BTreeMap<&String, &f64> = s.per_criterion.iter().collect();
            for (k, v) in ordered {
                out.push_str(&format!("    {}: {:.4}\n", k, v));
            }
        }
        if !s.reject_hits.is_empty() {
            out.push_str("  reject_hits:\n");
            for h in &s.reject_hits {
                out.push_str(&format!("    - {}\n", h));
            }
        }
        if let Some(j) = &s.judge {
            out.push_str("  judge:\n");
            out.push_str(&format!("    backend: {}\n", j.backend));
            out.push_str(&format!("    model: {}\n", j.model));
            if let Some(rid) = &j.request_id {
                out.push_str(&format!("    request_id: {}\n", rid));
            }
            if let Some(om) = &j.observer_model {
                out.push_str(&format!("    observer_model: {}\n", om));
            }
            if !j.probabilities.is_empty() {
                out.push_str("    probabilities:\n");
                for (k, probs) in &j.probabilities {
                    let rendered = probs
                        .iter()
                        .map(|p| format!("{:.4}", p))
                        .collect::<Vec<_>>()
                        .join(", ");
                    out.push_str(&format!("      {}: [{}]\n", k, rendered));
                }
            }
        }
    }
    out
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
            None,
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

    /// The `eval:` block must carry judge provenance — per-criterion scores,
    /// weighted + verdict, backend/model/request id, observer model when the
    /// judge saw an observe pass, and the probability distributions.
    #[test]
    fn sidecar_records_judge_metadata() {
        let dir = scratch_dir("judge-meta");

        let mut per = HashMap::new();
        per.insert("relevance".to_string(), 8.0 / 9.0);
        per.insert("technical".to_string(), 7.0 / 9.0);
        let detail = crate::judge::JudgeRunDetail {
            backend: "alibaba".to_string(),
            model: "decision-model-preview".to_string(),
            request_id: Some("req-123".to_string()),
            observer_model: Some("qwen3.8-flash".to_string()),
            probabilities: [
                ("criterion:relevance".to_string(), vec![0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0]),
                ("reject:00".to_string(), vec![0.95, 0.05]),
            ]
            .into_iter()
            .collect(),
        };
        let score = crate::eval::EvalScore {
            weighted: 0.9167,
            per_criterion: per,
            reject_hits: vec![],
            notes: "decision judge".to_string(),
            judge: Some(detail),
        };
        let meta = EvalMeta::from_score(2, true, &score);

        let options: HashMap<String, serde_yaml::Value> = HashMap::new();
        let asset = dir.join("asset.png");
        write_metadata(
            &asset,
            "qwen-image",
            "qwen-image-plus",
            "prompt text",
            None,
            Some(score.weighted),
            Some(&score.notes),
            &options,
            Some(&meta),
        );
        let sidecar = std::fs::read_to_string(asset.with_extension("metadata.yaml")).unwrap();

        for expected in [
            "eval:",
            "  attempt: 2",
            "  verdict: pass",
            "  weighted: 0.9167",
            "  per_criterion:",
            "    relevance: 0.8889",
            "    technical: 0.7778",
            "  judge:",
            "    backend: alibaba",
            "    model: decision-model-preview",
            "    request_id: req-123",
            "    observer_model: qwen3.8-flash",
            "    probabilities:",
            "      criterion:relevance: [0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 0.0000, 1.0000, 0.0000]",
            "      reject:00: [0.9500, 0.0500]",
        ] {
            assert!(
                sidecar.contains(expected),
                "sidecar missing {expected:?}\n---\n{sidecar}\n---"
            );
        }
        assert!(sidecar.contains("eval_notes: \"decision judge\""));

        // A score without judge detail must not invent judge fields.
        let bare = crate::eval::EvalScore {
            weighted: 0.5,
            per_criterion: HashMap::new(),
            reject_hits: vec![],
            notes: "legacy".to_string(),
            judge: None,
        };
        let meta2 = EvalMeta::from_score(1, false, &bare);
        let asset2 = dir.join("asset2.png");
        write_metadata(
            &asset2,
            "qwen-image",
            "m",
            "p",
            None,
            Some(0.5),
            Some("legacy"),
            &options,
            Some(&meta2),
        );
        let sidecar2 = std::fs::read_to_string(asset2.with_extension("metadata.yaml")).unwrap();
        assert!(sidecar2.contains("  verdict: fail"));
        assert!(!sidecar2.contains("judge:"), "no judge block when absent: {sidecar2}");

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
