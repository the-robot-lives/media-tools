//! CSS-style preference cascade (design §4.3) and the house-style snippet
//! library (design §4.2).
//!
//! # The cascade
//!
//! Config declares selector rules; each matching rule *patches* the resolved
//! settings field-by-field rather than replacing them wholesale. Layers, from
//! weakest to strongest:
//!
//! | rank | layer                                   | example                      |
//! |------|-----------------------------------------|------------------------------|
//! | 0    | `"*"` universal                          | every prompt                 |
//! | 1    | `type:<t>`                               | `type:image`                 |
//! | 2    | `type:<t>[attr=v]` and `tag:<tag>`       | `type:image[quality=high]`   |
//! | 3    | `id:<id>`                                | `id:hero-shot`               |
//! | 4    | explicit field in the `.media.prompt`    | `service: grok`              |
//! | 5    | CLI flag / GUI override for this run     | `--service grok`             |
//!
//! **Tie rule:** contributions at equal rank are ordered by declaration index
//! and the *last* one wins — as in CSS. Rule declaration order is taken from
//! the YAML mapping order of `preferences:`, which `serde_yaml::Mapping`
//! preserves.
//!
//! [`resolve`] is a pure function over (rules, prompt facts, file overrides,
//! CLI overrides). It never reads the filesystem or environment, and it always
//! returns full provenance: every contribution that was considered for a field,
//! in precedence order, with the winner marked. `explain` and the GUI
//! provenance popover both render that trace.
//!
//! # Deliberate non-circularity
//!
//! Attribute selectors such as `type:image[quality=high]` match against the
//! *pre-cascade* facts (CLI quality override, else the prompt file's `quality:`,
//! else `medium`). A `quality` value produced *by* the cascade does not feed
//! back into selector matching, so resolution is a single pass with no fixpoint
//! iteration. §4.3 does not address this; see the PR notes.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::provider_config::ProviderConfig;
use crate::schema::{ParsedPrompt, Quality};

// ---------------------------------------------------------------------------
// Fields
// ---------------------------------------------------------------------------

/// The settings the cascade can resolve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Field {
    Service,
    Model,
    Quality,
    Snippets,
}

impl Field {
    /// Every resolvable field, in display order.
    pub const ALL: [Field; 4] = [
        Field::Service,
        Field::Model,
        Field::Quality,
        Field::Snippets,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Field::Service => "service",
            Field::Model => "model",
            Field::Quality => "quality",
            Field::Snippets => "snippets",
        }
    }
}

// ---------------------------------------------------------------------------
// Overrides — what one layer may contribute
// ---------------------------------------------------------------------------

/// The patch a single layer contributes. Absent fields inherit.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct Overrides {
    #[serde(default)]
    pub service: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub quality: Option<String>,
    /// Snippet names. Patching is per-field, so a layer that sets `snippets`
    /// *replaces* the inherited list rather than appending to it.
    #[serde(default)]
    pub snippets: Option<Vec<String>>,
}

impl Overrides {
    pub fn is_empty(&self) -> bool {
        self == &Overrides::default()
    }

    fn get(&self, field: Field) -> Option<Value> {
        match field {
            Field::Service => self.service.clone().map(Value::Text),
            Field::Model => self.model.clone().map(Value::Text),
            Field::Quality => self.quality.clone().map(Value::Text),
            Field::Snippets => self.snippets.clone().map(Value::List),
        }
    }
}

// ---------------------------------------------------------------------------
// Selectors
// ---------------------------------------------------------------------------

/// A parsed selector. Specificity mirrors the §4.3 table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selector {
    /// `*`
    Universal,
    /// `type:<name>` optionally with `[attr=value]` qualifiers.
    Type {
        name: String,
        attrs: Vec<(String, String)>,
    },
    /// `tag:<tag>`
    Tag(String),
    /// `id:<id>`
    Id(String),
}

/// A selector that could not be parsed. Kept rather than discarded so the
/// loader can warn and `explain` can show what was ignored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectorError {
    pub selector: String,
    pub message: String,
}

/// Attribute names an attribute selector may test.
pub const SUPPORTED_ATTRS: [&str; 7] = [
    "quality",
    "format",
    "service",
    "model",
    "text_format",
    "tag",
    "id",
];

impl Selector {
    /// Parse a selector string. Rejects anything outside the §4.3 grammar so a
    /// typo surfaces as a warning instead of a silently inert rule.
    pub fn parse(raw: &str) -> Result<Selector, SelectorError> {
        let err = |msg: &str| SelectorError {
            selector: raw.to_string(),
            message: msg.to_string(),
        };
        let s = raw.trim();
        if s.is_empty() {
            return Err(err("empty selector"));
        }
        if s == "*" {
            return Ok(Selector::Universal);
        }
        let (kind, rest) = s
            .split_once(':')
            .ok_or_else(|| err("expected `*`, `type:<t>`, `tag:<t>` or `id:<id>`"))?;
        let rest = rest.trim();
        if rest.is_empty() {
            return Err(err("selector value is empty"));
        }
        match kind.trim() {
            "type" => {
                let (name, attrs) = parse_attrs(rest).map_err(|m| err(&m))?;
                if name.is_empty() {
                    return Err(err("type name is empty"));
                }
                Ok(Selector::Type { name, attrs })
            }
            "tag" => {
                if rest.contains('[') {
                    return Err(err(
                        "attribute qualifiers are only valid on `type:` selectors",
                    ));
                }
                Ok(Selector::Tag(rest.to_string()))
            }
            "id" => {
                if rest.contains('[') {
                    return Err(err(
                        "attribute qualifiers are only valid on `type:` selectors",
                    ));
                }
                Ok(Selector::Id(rest.to_string()))
            }
            other => Err(err(&format!(
                "unknown selector kind `{other}`; expected type|tag|id"
            ))),
        }
    }

    /// Rank in the §4.3 precedence table (0..=3).
    pub fn specificity(&self) -> u8 {
        match self {
            Selector::Universal => 0,
            Selector::Type { attrs, .. } if attrs.is_empty() => 1,
            Selector::Type { .. } => 2,
            Selector::Tag(_) => 2,
            Selector::Id(_) => 3,
        }
    }

    /// Does this selector match the prompt?
    pub fn matches(&self, facts: &PromptFacts) -> bool {
        match self {
            Selector::Universal => true,
            Selector::Tag(t) => facts.tags.iter().any(|x| x == t),
            Selector::Id(id) => facts.id == *id,
            Selector::Type { name, attrs } => {
                facts.type_name == *name && attrs.iter().all(|(k, v)| facts.attr_matches(k, v))
            }
        }
    }
}

/// Split `name[a=b][c=d]` / `name[a=b,c=d]` into its parts.
fn parse_attrs(input: &str) -> Result<(String, Vec<(String, String)>), String> {
    let Some(open) = input.find('[') else {
        if input.contains(']') {
            return Err("unbalanced `]` in selector".into());
        }
        return Ok((input.trim().to_string(), Vec::new()));
    };
    let name = input[..open].trim().to_string();
    let mut attrs = Vec::new();
    let mut rest = &input[open..];
    while !rest.is_empty() {
        if !rest.starts_with('[') {
            return Err(format!("unexpected `{rest}` after attribute group"));
        }
        let close = rest.find(']').ok_or("unbalanced `[` in selector")?;
        let group = &rest[1..close];
        if group.trim().is_empty() {
            return Err("empty attribute group `[]`".into());
        }
        for part in group.split(',') {
            let part = part.trim();
            let (k, v) = part
                .split_once('=')
                .ok_or_else(|| format!("attribute `{part}` is missing `=`"))?;
            let k = k.trim();
            if k.is_empty() {
                return Err("attribute name is empty".into());
            }
            let v = unquote(v.trim());
            if !SUPPORTED_ATTRS.contains(&k) {
                return Err(format!(
                    "unknown attribute `{k}`; supported: {}",
                    SUPPORTED_ATTRS.join(", ")
                ));
            }
            attrs.push((k.to_string(), v));
        }
        rest = rest[close + 1..].trim_start();
    }
    Ok((name, attrs))
}

fn unquote(s: &str) -> String {
    let b = s.as_bytes();
    if b.len() >= 2
        && ((b[0] == b'"' && b[b.len() - 1] == b'"') || (b[0] == b'\'' && b[b.len() - 1] == b'\''))
    {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

// ---------------------------------------------------------------------------
// Prompt facts — the match subject
// ---------------------------------------------------------------------------

/// Everything a selector may test, snapshotted *before* the cascade runs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PromptFacts {
    pub id: String,
    pub type_name: String,
    pub tags: Vec<String>,
    /// Pre-cascade effective quality: CLI override, else the file's `quality:`,
    /// else `medium`. See the module note on non-circularity.
    pub quality: String,
    pub formats: Vec<String>,
    pub service: Option<String>,
    pub model: Option<String>,
    pub text_format: Option<String>,
}

impl PromptFacts {
    /// Snapshot a parsed prompt. `cli_quality` is the `--quality` override, if any.
    pub fn from_prompt(prompt: &ParsedPrompt, cli_quality: Option<Quality>) -> Self {
        let quality = cli_quality.unwrap_or(prompt.meta.quality);
        PromptFacts {
            id: prompt.meta.id.clone(),
            type_name: prompt.payload.r#type.clone(),
            tags: prompt.payload.tags.clone(),
            quality: quality.as_str().to_string(),
            formats: prompt
                .meta
                .output_formats
                .iter()
                .map(|f| f.format.clone())
                .collect(),
            service: prompt.meta.service.clone(),
            model: prompt.meta.model.clone(),
            text_format: prompt.payload.output.text_format.clone(),
        }
    }

    fn attr_matches(&self, key: &str, want: &str) -> bool {
        match key {
            "quality" => self.quality == want,
            "format" => self.formats.iter().any(|f| f == want),
            "service" => self.service.as_deref() == Some(want),
            "model" => self.model.as_deref() == Some(want),
            "text_format" => self.text_format.as_deref() == Some(want),
            "tag" => self.tags.iter().any(|t| t == want),
            "id" => self.id == want,
            // Unreachable: `parse_attrs` rejects unknown names at load time.
            _ => false,
        }
    }
}

// ---------------------------------------------------------------------------
// Rules and the loaded preference set
// ---------------------------------------------------------------------------

/// One `preferences:` entry, with its declaration index for tie-breaking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub index: usize,
    pub selector_text: String,
    pub selector: Selector,
    pub overrides: Overrides,
}

/// The parsed `preferences:` + `snippets:` sections of the config.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PreferenceSet {
    pub rules: Vec<Rule>,
    pub snippets: BTreeMap<String, String>,
    /// Selectors (or rule bodies) that failed to parse and were skipped.
    pub errors: Vec<SelectorError>,
}

impl PreferenceSet {
    /// True when nothing was declared — resolution is then a no-op over the
    /// file/CLI layers only, which is exactly today's behaviour.
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty() && self.snippets.is_empty()
    }

    /// Build from a loaded config. `None` (no config file) yields an empty set.
    ///
    /// Malformed selectors and malformed rule bodies are collected into
    /// [`PreferenceSet::errors`] and skipped; one bad rule never invalidates
    /// the rest of the file.
    pub fn from_config(cfg: Option<&ProviderConfig>) -> Self {
        let Some(cfg) = cfg else {
            return PreferenceSet::default();
        };
        let mut set = PreferenceSet {
            snippets: cfg.snippets.clone(),
            ..Default::default()
        };
        for (index, (key, value)) in cfg.preferences.iter().enumerate() {
            let Some(selector_text) = key.as_str() else {
                set.errors.push(SelectorError {
                    selector: format!("{key:?}"),
                    message: "selector key is not a string".into(),
                });
                continue;
            };
            let selector = match Selector::parse(selector_text) {
                Ok(s) => s,
                Err(e) => {
                    set.errors.push(e);
                    continue;
                }
            };
            let overrides: Overrides = match serde_yaml::from_value(value.clone()) {
                Ok(o) => o,
                Err(e) => {
                    set.errors.push(SelectorError {
                        selector: selector_text.to_string(),
                        message: format!("invalid rule body: {e}"),
                    });
                    continue;
                }
            };
            set.rules.push(Rule {
                index,
                selector_text: selector_text.to_string(),
                selector,
                overrides,
            });
        }
        set
    }

    /// Compose named snippets in declared order. Returns the composed text and
    /// any names that were not defined in `snippets:`.
    pub fn compose(&self, names: &[String]) -> (String, Vec<String>) {
        let mut parts = Vec::new();
        let mut missing = Vec::new();
        for name in names {
            match self.snippets.get(name) {
                Some(body) => {
                    let body = body.trim();
                    if !body.is_empty() {
                        parts.push(body.to_string());
                    }
                }
                None => missing.push(name.clone()),
            }
        }
        (parts.join("\n\n"), missing)
    }
}

// ---------------------------------------------------------------------------
// Resolution + provenance
// ---------------------------------------------------------------------------

/// Where a contribution came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// A config `preferences:` rule, by its selector text.
    Selector(String),
    /// An explicit field in the `.media.prompt` file.
    PromptFile,
    /// A CLI flag / GUI per-run override.
    CliFlag,
}

impl Origin {
    pub fn label(&self) -> String {
        match self {
            Origin::Selector(s) => format!("\"{s}\""),
            Origin::PromptFile => "file".into(),
            Origin::CliFlag => "cli".into(),
        }
    }
}

/// Rank of the `.media.prompt` inline-field layer.
pub const RANK_PROMPT_FILE: u8 = 4;
/// Rank of the CLI / per-run override layer (CSS `!important`).
pub const RANK_CLI: u8 = 5;

/// A resolved value, or a list value for `snippets`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Text(String),
    List(Vec<String>),
}

impl Value {
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Value::Text(s) => Some(s),
            Value::List(_) => None,
        }
    }
    pub fn as_list(&self) -> Option<&[String]> {
        match self {
            Value::List(v) => Some(v),
            Value::Text(_) => None,
        }
    }
    pub fn display(&self) -> String {
        match self {
            Value::Text(s) => s.clone(),
            Value::List(v) => format!("[{}]", v.join(", ")),
        }
    }
}

/// One candidate value for a field, with the layer it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contribution {
    pub origin: Origin,
    pub value: Value,
    /// Layer rank, 0..=5. Higher wins.
    pub rank: u8,
    /// Declaration index within the config; ties at equal rank break on this,
    /// later wins.
    pub order: usize,
}

/// Every candidate for one field, weakest → strongest, with the winner marked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldResolution {
    pub field: Field,
    pub candidates: Vec<Contribution>,
    /// Index into `candidates`; `None` when nothing set the field.
    pub winner: Option<usize>,
}

impl FieldResolution {
    pub fn winning(&self) -> Option<&Contribution> {
        self.winner.and_then(|i| self.candidates.get(i))
    }
}

/// The full result of a cascade resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolution {
    pub fields: Vec<FieldResolution>,
}

impl Resolution {
    pub fn trace(&self, field: Field) -> Option<&FieldResolution> {
        self.fields.iter().find(|f| f.field == field)
    }

    fn winner_value(&self, field: Field) -> Option<&Value> {
        self.trace(field)
            .and_then(|f| f.winning())
            .map(|c| &c.value)
    }

    /// The origin that won `field`, if any.
    pub fn winner_origin(&self, field: Field) -> Option<&Origin> {
        self.trace(field)
            .and_then(|f| f.winning())
            .map(|c| &c.origin)
    }

    pub fn service(&self) -> Option<&str> {
        self.winner_value(Field::Service).and_then(Value::as_text)
    }
    pub fn model(&self) -> Option<&str> {
        self.winner_value(Field::Model).and_then(Value::as_text)
    }
    pub fn quality(&self) -> Option<&str> {
        self.winner_value(Field::Quality).and_then(Value::as_text)
    }
    pub fn snippets(&self) -> Option<&[String]> {
        self.winner_value(Field::Snippets).and_then(Value::as_list)
    }
}

/// Resolve the cascade.
///
/// Pure: no I/O, no globals. Given the same rules, facts and overrides it
/// always produces the same [`Resolution`], provenance included.
///
/// * `prefs` — config rules and snippet library
/// * `facts` — pre-cascade snapshot of the prompt (see [`PromptFacts`])
/// * `file`  — explicit fields from the `.media.prompt` (rank 4)
/// * `cli`   — per-run CLI/GUI overrides (rank 5)
pub fn resolve(
    prefs: &PreferenceSet,
    facts: &PromptFacts,
    file: &Overrides,
    cli: &Overrides,
) -> Resolution {
    let matched: Vec<&Rule> = prefs
        .rules
        .iter()
        .filter(|r| r.selector.matches(facts))
        .collect();

    let mut fields = Vec::with_capacity(Field::ALL.len());
    for field in Field::ALL {
        let mut candidates: Vec<Contribution> = Vec::new();
        for rule in &matched {
            if let Some(value) = rule.overrides.get(field) {
                candidates.push(Contribution {
                    origin: Origin::Selector(rule.selector_text.clone()),
                    value,
                    rank: rule.selector.specificity(),
                    order: rule.index,
                });
            }
        }
        if let Some(value) = file.get(field) {
            candidates.push(Contribution {
                origin: Origin::PromptFile,
                value,
                rank: RANK_PROMPT_FILE,
                order: usize::MAX,
            });
        }
        if let Some(value) = cli.get(field) {
            candidates.push(Contribution {
                origin: Origin::CliFlag,
                value,
                rank: RANK_CLI,
                order: usize::MAX,
            });
        }
        // Stable sort on (rank, order) leaves equal-rank rules in declaration
        // order, so the last element is the winner — CSS's "later wins".
        candidates.sort_by_key(|c| (c.rank, c.order));
        let winner = candidates.len().checked_sub(1);
        fields.push(FieldResolution {
            field,
            candidates,
            winner,
        });
    }
    Resolution { fields }
}

// ---------------------------------------------------------------------------
// Applying a resolution to a prompt
// ---------------------------------------------------------------------------

/// What [`apply`] actually changed, for verbose reporting.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Applied {
    pub service: Option<String>,
    /// Selector that supplied `service`, for failure messages and `--verbose`.
    pub service_rule: Option<String>,
    pub model: Option<String>,
    pub quality: Option<Quality>,
    pub snippets: Vec<String>,
    pub missing_snippets: Vec<String>,
}

impl Applied {
    pub fn is_empty(&self) -> bool {
        self == &Applied::default()
    }
}

/// Fold cascade-supplied settings into a parsed prompt.
///
/// Only contributions from *config rules* are written for `service` / `model` /
/// `quality`: the prompt-file and CLI layers already win downstream in
/// `pipeline::resolve_candidates`, so writing them here would be redundant and
/// would risk changing today's output. `snippets` is written for both the rule
/// and prompt-file layers because nothing else consumes it.
///
/// A rule-supplied `service` becomes a *pin*, exactly like an inline
/// `service:` field — so it skips the availability-ordered ladder, as §4.3's
/// worked example implies.
pub fn apply(prompt: &mut ParsedPrompt, res: &Resolution, prefs: &PreferenceSet) -> Applied {
    let mut applied = Applied::default();

    // Returns (value, selector-that-supplied-it) for rule-sourced fields only.
    let from_rule = |field: Field| -> Option<(&str, &str)> {
        let t = res.trace(field)?;
        let c = t.winning()?;
        match c.origin {
            Origin::Selector(ref sel) => c.value.as_text().map(|v| (v, sel.as_str())),
            _ => None,
        }
    };

    if prompt.meta.service.is_none() {
        if let Some((svc, selector)) = from_rule(Field::Service) {
            prompt.meta.service = Some(svc.to_string());
            prompt.payload.service = Some(svc.to_string());
            // Kept so a missing-key failure can name the rule responsible.
            prompt.meta.service_provenance = Some(selector.to_string());
            applied.service = Some(svc.to_string());
            applied.service_rule = Some(selector.to_string());
        }
    }
    if prompt.meta.model.is_none() {
        if let Some((model, _)) = from_rule(Field::Model) {
            prompt.meta.model = Some(model.to_string());
            prompt.payload.model = Some(model.to_string());
            applied.model = Some(model.to_string());
        }
    }
    if prompt.payload.quality.is_none() {
        if let Some(q) = from_rule(Field::Quality).and_then(|(q, _)| q.parse::<Quality>().ok()) {
            prompt.meta.quality = q;
            prompt.payload.quality = Some(q.as_str().to_string());
            applied.quality = Some(q);
        }
    }

    // Snippets: rule layer or prompt-file layer, composed into `prompt.system`.
    let names: Vec<String> = res
        .trace(Field::Snippets)
        .and_then(|t| t.winning())
        .and_then(|c| match c.origin {
            Origin::Selector(_) | Origin::PromptFile => c.value.as_list(),
            Origin::CliFlag => None,
        })
        .map(|v| v.to_vec())
        .unwrap_or_default();

    if !names.is_empty() {
        let (text, missing) = prefs.compose(&names);
        applied.missing_snippets = missing;
        if !text.is_empty() {
            applied.snippets = names;
            prompt.payload.prompt.system = Some(match prompt.payload.prompt.system.take() {
                Some(existing) if !existing.trim().is_empty() => {
                    format!("{}\n\n{}", text, existing.trim())
                }
                _ => text,
            });
        }
    }

    applied
}

/// Build the rank-4 override layer from a prompt file's explicit fields.
pub fn file_overrides(prompt: &ParsedPrompt) -> Overrides {
    Overrides {
        service: prompt.payload.service.clone(),
        model: prompt.payload.model.clone(),
        quality: prompt.payload.quality.clone(),
        snippets: if prompt.payload.snippets.is_empty() {
            None
        } else {
            Some(prompt.payload.snippets.clone())
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> PromptFacts {
        PromptFacts {
            id: "hero-shot".into(),
            type_name: "image".into(),
            tags: vec!["brand".into()],
            quality: "high".into(),
            formats: vec!["png".into()],
            ..Default::default()
        }
    }

    fn set(yaml: &str) -> PreferenceSet {
        let cfg: ProviderConfig = serde_yaml::from_str(yaml).unwrap();
        PreferenceSet::from_config(Some(&cfg))
    }

    // -- selector parsing ---------------------------------------------------

    #[test]
    fn parses_every_selector_shape() {
        assert_eq!(Selector::parse("*").unwrap(), Selector::Universal);
        assert_eq!(
            Selector::parse("type:image").unwrap(),
            Selector::Type {
                name: "image".into(),
                attrs: vec![]
            }
        );
        assert_eq!(
            Selector::parse("type:image[quality=high]").unwrap(),
            Selector::Type {
                name: "image".into(),
                attrs: vec![("quality".into(), "high".into())]
            }
        );
        assert_eq!(
            Selector::parse("tag:brand").unwrap(),
            Selector::Tag("brand".into())
        );
        assert_eq!(
            Selector::parse("id:hero").unwrap(),
            Selector::Id("hero".into())
        );
    }

    #[test]
    fn specificity_matches_the_design_table() {
        assert_eq!(Selector::parse("*").unwrap().specificity(), 0);
        assert_eq!(Selector::parse("type:image").unwrap().specificity(), 1);
        assert_eq!(
            Selector::parse("type:image[quality=high]")
                .unwrap()
                .specificity(),
            2
        );
        assert_eq!(Selector::parse("tag:brand").unwrap().specificity(), 2);
        assert_eq!(Selector::parse("id:hero").unwrap().specificity(), 3);
    }

    #[test]
    fn malformed_selectors_are_rejected_with_a_reason() {
        for bad in [
            "",
            "image",
            "kind:image",
            "type:",
            "type:image[quality]",
            "type:image[quality=high",
            "type:image[bogus=1]",
            "tag:brand[quality=high]",
        ] {
            assert!(Selector::parse(bad).is_err(), "expected `{bad}` to fail");
        }
    }

    #[test]
    fn multiple_and_quoted_attributes_parse() {
        let s = Selector::parse("type:image[quality=high,format=\"png\"]").unwrap();
        assert_eq!(
            s,
            Selector::Type {
                name: "image".into(),
                attrs: vec![
                    ("quality".into(), "high".into()),
                    ("format".into(), "png".into())
                ]
            }
        );
        let s2 = Selector::parse("type:image[quality=high][format=png]").unwrap();
        assert_eq!(s2.specificity(), 2);
        assert!(s2.matches(&facts()));
    }

    // -- precedence ---------------------------------------------------------

    #[test]
    fn higher_specificity_wins() {
        let prefs = set(r#"
preferences:
  "*": {service: openai}
  "type:image": {service: grok}
  "type:image[quality=high]": {service: gemini}
  "id:hero-shot": {service: anthropic}
"#);
        let r = resolve(
            &prefs,
            &facts(),
            &Overrides::default(),
            &Overrides::default(),
        );
        assert_eq!(r.service(), Some("anthropic"));
        let trace = r.trace(Field::Service).unwrap();
        assert_eq!(trace.candidates.len(), 4);
        // weakest → strongest
        let ranks: Vec<u8> = trace.candidates.iter().map(|c| c.rank).collect();
        assert_eq!(ranks, vec![0, 1, 2, 3]);
    }

    #[test]
    fn equal_specificity_ties_go_to_the_later_rule() {
        let prefs = set(r#"
preferences:
  "tag:brand": {service: first}
  "type:image[quality=high]": {service: second}
"#);
        let r = resolve(
            &prefs,
            &facts(),
            &Overrides::default(),
            &Overrides::default(),
        );
        assert_eq!(
            r.service(),
            Some("second"),
            "later rule wins at equal specificity"
        );

        let prefs = set(r#"
preferences:
  "type:image[quality=high]": {service: second}
  "tag:brand": {service: first}
"#);
        let r = resolve(
            &prefs,
            &facts(),
            &Overrides::default(),
            &Overrides::default(),
        );
        assert_eq!(
            r.service(),
            Some("first"),
            "declaration order decides, not selector kind"
        );
    }

    #[test]
    fn patching_is_per_field() {
        let prefs = set(r#"
preferences:
  "*": {service: openai, quality: med}
  "type:image": {model: grok-2-image}
"#);
        let r = resolve(
            &prefs,
            &facts(),
            &Overrides::default(),
            &Overrides::default(),
        );
        assert_eq!(r.service(), Some("openai"));
        assert_eq!(r.quality(), Some("med"));
        assert_eq!(r.model(), Some("grok-2-image"));
    }

    #[test]
    fn prompt_file_beats_rules_and_cli_beats_the_file() {
        let prefs = set(r#"
preferences:
  "id:hero-shot": {service: anthropic}
"#);
        let file = Overrides {
            service: Some("grok".into()),
            ..Default::default()
        };
        let cli = Overrides {
            service: Some("gemini".into()),
            ..Default::default()
        };
        let r = resolve(&prefs, &facts(), &file, &Overrides::default());
        assert_eq!(r.service(), Some("grok"));
        assert_eq!(r.winner_origin(Field::Service), Some(&Origin::PromptFile));

        let r = resolve(&prefs, &facts(), &file, &cli);
        assert_eq!(r.service(), Some("gemini"));
        assert_eq!(r.winner_origin(Field::Service), Some(&Origin::CliFlag));
    }

    #[test]
    fn non_matching_rules_contribute_nothing() {
        let prefs = set(r#"
preferences:
  "type:music": {service: suno}
  "tag:internal": {service: openai}
  "id:other": {service: veo}
"#);
        let r = resolve(
            &prefs,
            &facts(),
            &Overrides::default(),
            &Overrides::default(),
        );
        assert_eq!(r.service(), None);
        assert!(r.trace(Field::Service).unwrap().candidates.is_empty());
    }

    // -- degenerate configs -------------------------------------------------

    #[test]
    fn absent_config_resolves_to_nothing() {
        let prefs = PreferenceSet::from_config(None);
        assert!(prefs.is_empty());
        let r = resolve(
            &prefs,
            &facts(),
            &Overrides::default(),
            &Overrides::default(),
        );
        for field in Field::ALL {
            assert!(r.trace(field).unwrap().winning().is_none());
        }
    }

    #[test]
    fn empty_and_missing_sections_are_harmless() {
        for yaml in ["version: 1\n", "preferences: {}\nsnippets: {}\n"] {
            let prefs = set(yaml);
            assert!(prefs.rules.is_empty());
            assert!(prefs.errors.is_empty());
            let r = resolve(
                &prefs,
                &facts(),
                &Overrides::default(),
                &Overrides::default(),
            );
            assert_eq!(r.service(), None);
        }
    }

    #[test]
    fn duplicate_selector_keys_are_a_hard_yaml_error() {
        // Two rules with the same selector cannot express "later wins" \u{2014} serde_yaml
        // rejects the document outright, and `provider_config::read_file` then warns
        // and falls back to compiled-in defaults for the WHOLE config, not just this
        // section. Distinguish equal-specificity ties with distinct selectors.
        let err = serde_yaml::from_str::<ProviderConfig>(
            "preferences:\n  \"type:image\": {service: a}\n  \"type:image\": {service: b}\n",
        )
        .unwrap_err();
        assert!(err.to_string().contains("duplicate entry"), "{err}");
    }

    #[test]
    fn a_malformed_selector_is_skipped_not_fatal() {
        let prefs = set(r#"
preferences:
  "bogus-selector": {service: openai}
  "type:image": {service: grok}
"#);
        assert_eq!(prefs.errors.len(), 1);
        assert_eq!(prefs.errors[0].selector, "bogus-selector");
        assert_eq!(prefs.rules.len(), 1);
        let r = resolve(
            &prefs,
            &facts(),
            &Overrides::default(),
            &Overrides::default(),
        );
        assert_eq!(r.service(), Some("grok"));
    }

    #[test]
    fn a_malformed_rule_body_is_skipped_not_fatal() {
        let prefs = set(r#"
preferences:
  "type:image": "not-a-mapping"
  "tag:brand": {service: grok}
"#);
        assert_eq!(prefs.errors.len(), 1);
        assert_eq!(prefs.rules.len(), 1);
        let r = resolve(
            &prefs,
            &facts(),
            &Overrides::default(),
            &Overrides::default(),
        );
        assert_eq!(r.service(), Some("grok"));
    }

    #[test]
    fn declaration_order_survives_yaml_parsing() {
        let prefs = set(r#"
preferences:
  "zzz-last": {}
  "*": {service: a}
  "id:hero-shot": {service: b}
  "tag:brand": {service: c}
"#);
        // "zzz-last" is malformed and skipped; the rest keep their file indices.
        let order: Vec<&str> = prefs
            .rules
            .iter()
            .map(|r| r.selector_text.as_str())
            .collect();
        assert_eq!(order, vec!["*", "id:hero-shot", "tag:brand"]);
        assert_eq!(prefs.rules[0].index, 1);
    }

    // -- snippets -----------------------------------------------------------

    #[test]
    fn snippets_replace_rather_than_append() {
        let prefs = set(r#"
snippets:
  house-style: "Clean and understated."
  no-gemini: "Do not use Gemini."
preferences:
  "*": {snippets: [house-style]}
  "type:image": {snippets: [house-style, no-gemini]}
"#);
        let r = resolve(
            &prefs,
            &facts(),
            &Overrides::default(),
            &Overrides::default(),
        );
        assert_eq!(
            r.snippets(),
            Some(["house-style".to_string(), "no-gemini".to_string()].as_slice())
        );
        let (text, missing) = prefs.compose(r.snippets().unwrap());
        assert_eq!(text, "Clean and understated.\n\nDo not use Gemini.");
        assert!(missing.is_empty());
    }

    // -- apply --------------------------------------------------------------

    fn prompt(yaml: &str) -> ParsedPrompt {
        use crate::schema::{AssetType, PromptMeta, PromptPayload};
        let payload: PromptPayload = serde_yaml::from_str(yaml).unwrap();
        let (asset_type, audio_kind) = AssetType::from_type_str(&payload.r#type);
        let quality = payload
            .quality
            .as_deref()
            .and_then(|q| q.parse().ok())
            .unwrap_or(Quality::Medium);
        let meta = PromptMeta {
            path: "hero.media.prompt".into(),
            asset_type,
            audio_kind,
            output_formats: payload.output.formats.clone(),
            name_stem: "hero".into(),
            output_dir: ".".into(),
            id: payload.id.clone().unwrap_or_else(|| "hero".into()),
            service: payload.service.clone(),
            service_provenance: None,
            model: payload.model.clone(),
            schema_version: payload.schema.clone(),
            quality,
            duration: None,
        };
        ParsedPrompt { payload, meta }
    }

    fn cascade(prefs: &PreferenceSet, p: &mut ParsedPrompt, cli: Overrides) -> Applied {
        let facts = PromptFacts::from_prompt(p, None);
        let file = file_overrides(p);
        let res = resolve(prefs, &facts, &file, &cli);
        apply(p, &res, prefs)
    }

    #[test]
    fn apply_is_a_no_op_without_a_config() {
        let prefs = PreferenceSet::from_config(None);
        let mut p = prompt("id: hero-shot\ntype: image\n");
        let before = format!("{:?}", p);
        let applied = cascade(&prefs, &mut p, Overrides::default());
        assert!(applied.is_empty());
        assert_eq!(before, format!("{:?}", p), "prompt must be untouched");
        assert_eq!(p.meta.service, None);
        assert_eq!(p.meta.quality, Quality::Medium);
        assert_eq!(p.payload.prompt.system, None);
    }

    #[test]
    fn apply_pins_a_rule_supplied_service_and_model() {
        let prefs = set(r#"
preferences:
  "*": {service: openai}
  "type:image": {service: grok, model: grok-2-image}
"#);
        let mut p = prompt("id: hero-shot\ntype: image\n");
        let applied = cascade(&prefs, &mut p, Overrides::default());
        assert_eq!(applied.service.as_deref(), Some("grok"));
        assert_eq!(p.meta.service.as_deref(), Some("grok"));
        assert_eq!(p.meta.model.as_deref(), Some("grok-2-image"));
    }

    #[test]
    fn a_cascade_pinned_service_records_the_rule_that_chose_it() {
        // The failure path needs this: a cascade-pinned service does not fall
        // back to the ladder, so a missing key is fatal and the message must be
        // able to name the rule the user never typed.
        let prefs = set("preferences:\n  \"type:image\": {service: grok}\n");
        let mut p = prompt("id: hero-shot\ntype: image\n");
        let applied = cascade(&prefs, &mut p, Overrides::default());
        assert_eq!(applied.service_rule.as_deref(), Some("type:image"));
        assert_eq!(p.meta.service_provenance.as_deref(), Some("type:image"));
    }

    #[test]
    fn a_file_pinned_service_records_no_rule_provenance() {
        // Guards the byte-identical promise: without cascade provenance the
        // failure messages must render exactly as they do today.
        let prefs = set("preferences:\n  \"type:image\": {service: grok}\n");
        let mut p = prompt("id: hero-shot\ntype: image\nservice: gemini\n");
        let applied = cascade(&prefs, &mut p, Overrides::default());
        assert_eq!(applied.service_rule, None);
        assert_eq!(p.meta.service_provenance, None);
    }

    #[test]
    fn apply_never_overwrites_an_explicit_file_field() {
        let prefs = set("preferences:\n  \"type:image\": {service: grok, quality: high}\n");
        let mut p = prompt("id: hero-shot\ntype: image\nservice: gemini\nquality: low\n");
        let applied = cascade(&prefs, &mut p, Overrides::default());
        assert_eq!(applied.service, None);
        assert_eq!(applied.quality, None);
        assert_eq!(p.meta.service.as_deref(), Some("gemini"));
        assert_eq!(p.meta.quality, Quality::Low);
    }

    #[test]
    fn apply_ignores_the_cli_layer_pipeline_already_honours_it() {
        let prefs = set("preferences:\n  \"type:image\": {service: grok}\n");
        let mut p = prompt("id: hero-shot\ntype: image\n");
        let cli = Overrides {
            service: Some("gemini".into()),
            ..Default::default()
        };
        let applied = cascade(&prefs, &mut p, cli);
        assert_eq!(applied.service, None, "CLI wins downstream, not here");
        assert_eq!(p.meta.service, None);
    }

    #[test]
    fn apply_prepends_snippets_to_an_existing_system_prompt() {
        let prefs = set(r#"
snippets:
  house-style: "Understated."
preferences:
  "*": {snippets: [house-style]}
"#);
        let mut p = prompt("id: hero-shot\ntype: image\nprompt:\n  system: \"Be terse.\"\n");
        let applied = cascade(&prefs, &mut p, Overrides::default());
        assert_eq!(applied.snippets, vec!["house-style".to_string()]);
        assert_eq!(
            p.payload.prompt.system.as_deref(),
            Some("Understated.\n\nBe terse.")
        );
    }

    #[test]
    fn a_prompt_file_snippets_list_beats_the_rule_layer() {
        let prefs = set(r#"
snippets:
  a: "A"
  b: "B"
preferences:
  "*": {snippets: [a]}
"#);
        let mut p = prompt("id: hero-shot\ntype: image\nsnippets: [b]\n");
        let applied = cascade(&prefs, &mut p, Overrides::default());
        assert_eq!(applied.snippets, vec!["b".to_string()]);
        assert_eq!(p.payload.prompt.system.as_deref(), Some("B"));
    }

    #[test]
    fn unknown_snippet_names_are_reported_not_fatal() {
        let prefs = set("snippets:\n  a: \"A\"\n");
        let (text, missing) = prefs.compose(&["a".into(), "nope".into()]);
        assert_eq!(text, "A");
        assert_eq!(missing, vec!["nope".to_string()]);
    }
}
