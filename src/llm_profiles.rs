//! Named LLM connections. Credentials remain in environment variables, never in this file.
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use color_eyre::eyre::{bail, eyre, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::attachments::LoadedAttachment;
use crate::providers::{ChatProvider, GenerationOptions};

pub const PROFILE_SERVICE: &str = "llm-profile";
pub const PROVIDERS: &[&str] = &[
    "openai",
    "anthropic",
    "groq",
    "cerebras",
    "deepseek",
    "zai",
    "alibaba",
    "openrouter",
    "ollama",
    "lmstudio",
    "llamacpp",
    "custom",
];

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub name: String,
    pub provider: String,
    pub base_url: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key_env: Option<String>,
    #[serde(default)]
    pub local: bool,
}

impl Profile {
    pub fn validate(&self) -> Result<()> {
        if !PROVIDERS.contains(&self.provider.as_str()) {
            bail!("Unsupported LLM provider: {}", self.provider);
        }
        let url = reqwest::Url::parse(&self.base_url)
            .map_err(|_| eyre!("LLM base URL must be an absolute HTTP(S) URL"))?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            bail!("LLM base URL must use HTTP(S), without credentials, query, or fragment");
        }
        if self.name.trim().is_empty() || self.model.trim().is_empty() {
            bail!("LLM profile name and model must not be empty");
        }
        if let Some(key) = &self.api_key_env {
            if !valid_env_name(key) {
                bail!("api_key_env must be an environment variable name, never a key value");
            }
        }
        Ok(())
    }

    pub fn api_key(&self) -> Result<Option<String>> {
        match &self.api_key_env {
            Some(name) => Ok(Some(
                std::env::var(name)
                    .ok()
                    .filter(|v| !v.is_empty())
                    .ok_or_else(|| eyre!("Set {name} for this LLM profile"))?,
            )),
            None if self.local => Ok(None),
            None => {
                bail!("Remote LLM profile needs api_key_env; configure an environment variable reference");
            }
        }
    }

    fn root(&self) -> &str {
        self.base_url.trim_end_matches('/')
    }
    fn endpoint(&self, suffix: &str) -> String {
        format!("{}/{}", self.root(), suffix)
    }
}

fn valid_env_name(value: &str) -> bool {
    let mut chars = value.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

pub fn slug(value: &str) -> String {
    value
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileStore {
    #[serde(default)]
    pub default_profile: Option<String>,
    #[serde(default)]
    pub profiles: BTreeMap<String, Profile>,
}

pub fn config_path() -> Result<PathBuf> {
    if let Some(p) = std::env::var_os("MEDIA_TOOL_LLM_CONFIG") {
        return Ok(PathBuf::from(p));
    }
    let root = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .ok_or_else(|| eyre!("Set HOME, XDG_CONFIG_HOME or MEDIA_TOOL_LLM_CONFIG"))?;
    Ok(root.join("media-tool/llm-profiles.yaml"))
}

impl ProfileStore {
    pub fn load() -> Result<Self> {
        Self::load_path(&config_path()?)
    }
    pub fn load_path(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => {
                // Do not include serde's input excerpt: malformed YAML could contain a pasted secret.
                let store: Self = serde_yaml::from_str(&text).map_err(|_| {
                    eyre!("Invalid LLM profile configuration at {}", path.display())
                })?;
                store.validate()?;
                Ok(store)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).wrap_err("Cannot read LLM profile configuration"),
        }
    }
    pub fn validate(&self) -> Result<()> {
        for (id, profile) in &self.profiles {
            if !valid_id(id) {
                bail!("Profile IDs may contain only letters, numbers, '.', '-' and '_'");
            }
            profile.validate()?;
        }
        if let Some(id) = &self.default_profile {
            if !self.profiles.contains_key(id) {
                bail!("Default LLM profile does not exist: {id}");
            }
        }
        Ok(())
    }
    pub fn upsert(&mut self, id: String, profile: Profile) -> Result<()> {
        if !valid_id(&id) {
            bail!("Invalid profile ID; use letters, numbers, '.', '-' and '_'");
        }
        profile.validate()?;
        self.profiles.insert(id, profile);
        Ok(())
    }
    pub fn save(&self) -> Result<()> {
        self.save_path(&config_path()?)
    }
    pub fn save_path(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        std::fs::create_dir_all(parent)?;
        let tmp = parent.join(format!(".llm-profiles-{}.tmp", uuid::Uuid::new_v4()));
        let text = serde_yaml::to_string(self)?;
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        use std::io::Write;
        let result = (|| -> Result<()> {
            let mut file = opts.open(&tmp)?;
            file.write_all(text.as_bytes())?;
            file.sync_all()?;
            std::fs::rename(&tmp, path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        result
    }
    /// Explicit CLI > environment > stored default. Unknown choices always fail closed.
    pub fn resolve(
        &self,
        flag: Option<&str>,
        env: Option<&str>,
    ) -> Result<Option<(String, Profile)>> {
        let id = flag.or(env).or(self.default_profile.as_deref());
        match id {
            Some(id) => Ok(Some((
                id.to_string(),
                self.profiles
                    .get(id)
                    .ok_or_else(|| {
                        eyre!("Unknown LLM profile '{id}'; run media-tool llm list or configure")
                    })?
                    .clone(),
            ))),
            None => Ok(None),
        }
    }
}

pub fn preset(provider: &str) -> Profile {
    let (base, key, local) = match provider {
        "anthropic" => (
            "https://api.anthropic.com/v1",
            Some("ANTHROPIC_API_KEY"),
            false,
        ),
        "groq" => (
            "https://api.groq.com/openai/v1",
            Some("GROQ_API_KEY"),
            false,
        ),
        "cerebras" => (
            "https://api.cerebras.ai/v1",
            Some("CEREBRAS_API_KEY"),
            false,
        ),
        "deepseek" => (
            "https://api.deepseek.com/v1",
            Some("DEEPSEEK_API_KEY"),
            false,
        ),
        "zai" => ("https://api.z.ai/api/paas/v4", Some("ZAI_API_KEY"), false),
        "alibaba" => (
            "https://token-plan.ap-southeast-1.maas.aliyuncs.com/compatible-mode/v1",
            Some("ALIBABA_API_KEY"),
            false,
        ),
        "openrouter" => (
            "https://openrouter.ai/api/v1",
            Some("OPENROUTER_API_KEY"),
            false,
        ),
        "ollama" => ("http://localhost:11434", None, true),
        "lmstudio" => ("http://localhost:1234/v1", None, true),
        "llamacpp" => ("http://localhost:8080/v1", None, true),
        "custom" => ("http://localhost:1234/v1", None, true),
        _ => ("https://api.openai.com/v1", Some("OPENAI_API_KEY"), false),
    };
    Profile {
        name: provider.into(),
        provider: provider.into(),
        base_url: base.into(),
        model: String::new(),
        api_key_env: key.map(String::from),
        local,
    }
}

static ACTIVE: OnceLock<Option<(String, Profile)>> = OnceLock::new();
pub fn selected() -> Option<&'static Profile> {
    ACTIVE
        .get()
        .and_then(|p| p.as_ref().map(|(_, profile)| profile))
}

pub fn init(flag: Option<&str>) -> Result<()> {
    let env = std::env::var("MEDIA_TOOL_LLM_PROFILE").ok();
    let active = ProfileStore::load()?.resolve(flag, env.as_deref())?;
    ACTIVE
        .set(active)
        .map_err(|_| eyre!("LLM profile already initialized"))?;
    Ok(())
}

pub fn selected_candidate() -> Option<crate::providers::Candidate> {
    selected().map(|p| crate::providers::Candidate {
        service: PROFILE_SERVICE,
        model: Box::leak(p.model.clone().into_boxed_str()),
    })
}

pub struct ProfileProvider(pub Profile);
#[async_trait::async_trait]
impl ChatProvider for ProfileProvider {
    async fn generate(
        &self,
        system: &str,
        user: &str,
        output: &Path,
        _key: &str,
        options: &GenerationOptions,
        attachments: &[LoadedAttachment],
    ) -> Result<bool> {
        let content = complete(
            &self.0,
            system,
            user,
            &options.model,
            &options.provider_options,
            attachments,
        )
        .await?;
        let content = crate::providers::sanitize_chat_output(&content, output);
        if content.trim().is_empty() {
            bail!("LLM returned empty output");
        }
        if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(output, content)?;
        Ok(true)
    }
    fn name(&self) -> &str {
        PROFILE_SERVICE
    }
}

fn timeout_seconds(value: Option<&str>) -> Result<u64> {
    let seconds = value
        .unwrap_or("180")
        .parse::<u64>()
        .map_err(|_| eyre!("MEDIA_TOOL_LLM_TIMEOUT_SECONDS must be an integer from1 to900"))?;
    if !(1..=900).contains(&seconds) {
        bail!("MEDIA_TOOL_LLM_TIMEOUT_SECONDS must be from1 to900");
    }
    Ok(seconds)
}

fn client() -> Result<reqwest::Client> {
    // Cross-host redirects must never forward API credentials to another endpoint.
    Ok(reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(timeout_seconds(
            std::env::var("MEDIA_TOOL_LLM_TIMEOUT_SECONDS")
                .ok()
                .as_deref(),
        )?))
        .build()?)
}

fn auth(request: reqwest::RequestBuilder, profile: &Profile) -> Result<reqwest::RequestBuilder> {
    let key = profile.api_key()?;
    Ok(match (profile.provider.as_str(), key) {
        ("anthropic", Some(key)) => request
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01"),
        (_, Some(key)) => request.bearer_auth(key),
        _ => request,
    })
}

async fn response_json(request: reqwest::RequestBuilder) -> Result<Value> {
    let response = request.send().await.map_err(|_| {
        eyre!("LLM endpoint connection failed or timed out; check URL and server availability")
    })?;
    if !response.status().is_success() {
        bail!(
            "LLM endpoint returned HTTP {}; check profile, credentials and model",
            response.status().as_u16()
        );
    }
    response
        .json()
        .await
        .map_err(|_| eyre!("LLM endpoint returned invalid JSON"))
}

pub async fn discover_models(profile: &Profile) -> Result<Vec<String>> {
    // Model can be blank during setup; remaining validation still applies.
    let mut check = profile.clone();
    check.model = "discovery".into();
    check.validate()?;
    let endpoint = if profile.provider == "ollama" {
        profile.endpoint("api/tags")
    } else {
        profile.endpoint("models")
    };
    let data = response_json(auth(
        client()?.get(endpoint).timeout(Duration::from_secs(10)),
        profile,
    )?)
    .await?;
    let mut models: Vec<String> = if profile.provider == "ollama" {
        data["models"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v["name"].as_str().map(String::from))
            .collect()
    } else {
        data["data"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v["id"].as_str().map(String::from))
            .collect()
    };
    models.sort();
    models.dedup();
    if models.is_empty() {
        bail!("Endpoint returned no models; enter an exact model ID manually");
    }
    Ok(models)
}

pub async fn complete(
    profile: &Profile,
    system: &str,
    user: &str,
    model: &str,
    options: &HashMap<String, serde_yaml::Value>,
    attachments: &[LoadedAttachment],
) -> Result<String> {
    profile.validate()?;
    let max_tokens = options
        .get("max_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(8192);
    let temperature = options.get("temperature").and_then(|v| v.as_f64());
    let (suffix, mut body) = match profile.provider.as_str() {
        "anthropic" => {
            let mut content = Vec::new();
            for att in attachments
                .iter()
                .filter(|a| a.mime_type.starts_with("image/"))
            {
                content.push(json!({"type":"image","source":{"type":"base64","media_type":att.mime_type,"data":att.data_b64}}));
            }
            content.push(json!({"type":"text","text":user}));
            (
                "messages",
                json!({"model":model,"max_tokens":max_tokens,"system":system,"messages":[{"role":"user","content":content}]}),
            )
        }
        "ollama" => {
            let images: Vec<_> = attachments
                .iter()
                .filter(|a| a.mime_type.starts_with("image/"))
                .map(|a| a.data_b64.clone())
                .collect();
            (
                "api/chat",
                json!({"model":model,"stream":false,"messages":[{"role":"system","content":system},{"role":"user","content":user,"images":images}],"options":{"num_predict":max_tokens}}),
            )
        }
        _ => {
            let images: Vec<_> = attachments
                .iter()
                .filter(|a| a.mime_type.starts_with("image/"))
                .collect();
            let content = if images.is_empty() {
                json!(user)
            } else {
                let mut parts: Vec<Value> = images.iter().map(|a| json!({"type":"image_url","image_url":{"url":format!("data:{};base64,{}",a.mime_type,a.data_b64)}})).collect();
                parts.push(json!({"type":"text","text":user}));
                json!(parts)
            };
            (
                "chat/completions",
                json!({"model":model,"max_tokens":max_tokens,"messages":[{"role":"system","content":system},{"role":"user","content":content}]}),
            )
        }
    };
    if let Some(t) = temperature {
        if profile.provider == "ollama" {
            body["options"]["temperature"] = json!(t);
        } else {
            body["temperature"] = json!(t);
        }
    }
    let data = response_json(auth(
        client()?.post(profile.endpoint(suffix)).json(&body),
        profile,
    )?)
    .await?;
    let truncated = if profile.provider == "anthropic" {
        data["stop_reason"] == "max_tokens"
    } else if profile.provider == "ollama" {
        data["done_reason"] == "length"
    } else {
        data["choices"][0]["finish_reason"] == "length"
    };
    if truncated {
        bail!("LLM output was truncated; increase max_tokens or shorten the request");
    }
    let text = if profile.provider == "anthropic" {
        data["content"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v["text"].as_str())
            .collect::<Vec<_>>()
            .join("")
    } else if profile.provider == "ollama" {
        data["message"]["content"].as_str().unwrap_or("").into()
    } else {
        data["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .into()
    };
    if text.trim().is_empty() {
        bail!("LLM response contains no generated text");
    }
    Ok(text)
}

/// Safe import: never copies plaintext/encrypted API keys. Keep only env references.
pub fn import_profiles(value: &Value) -> Vec<(String, Profile)> {
    let profiles: Vec<&Value> = if let Some(p) = value["llmProfiles"].as_array() {
        p.iter().collect()
    } else if value["llm"].is_object() {
        vec![&value["llm"]]
    } else {
        vec![]
    };
    profiles
        .into_iter()
        .filter_map(|v| {
            let provider = v["provider"].as_str()?;
            let mapped = if v["apiType"].as_str() == Some("anthropic") {
                "anthropic"
            } else if matches!(provider, "openai-compatible" | "litellm") {
                "custom"
            } else {
                provider
            };
            let mut p = preset(mapped);
            p.name = v["name"].as_str().unwrap_or(provider).to_string();
            p.model = v["model"].as_str()?.into();
            if let Some(url) = v["baseUrl"].as_str().filter(|s| !s.is_empty()) {
                p.base_url = url.into();
            }
            p.local = v["local"].as_bool().unwrap_or(p.local);
            // custom/litellm providers aren't local simply because their preset is local.
            if matches!(provider, "custom" | "openai-compatible" | "litellm") {
                p.local = v["local"].as_bool().unwrap_or_else(|| {
                    reqwest::Url::parse(&p.base_url)
                        .ok()
                        .and_then(|u| {
                            u.host_str().map(|h| {
                                matches!(h, "localhost" | "127.0.0.1" | "::1")
                                    || h.starts_with("192.168.")
                                    || h.starts_with("10.")
                            })
                        })
                        .unwrap_or(false)
                });
            }
            if let Some(key) = v["apiKey"].as_str().and_then(|s| s.strip_prefix("env:")) {
                p.api_key_env = Some(key.trim().into());
            }
            if p.validate().is_err() {
                return None;
            }
            Some((slug(&p.name), p))
        })
        .collect()
}

pub fn import_candidates() -> Result<Vec<(String, Profile)>> {
    let home = PathBuf::from(std::env::var_os("HOME").ok_or_else(|| eyre!("HOME missing"))?);
    let mut candidates = Vec::new();
    let toolkit = std::env::var_os("LLM_TOOLKIT_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".llm-toolkit"));
    for path in [
        toolkit.join("config.json"),
        home.join(".config/queue-populator/config.json"),
    ] {
        if path.exists() {
            let v: Value = serde_json::from_slice(&std::fs::read(path)?)
                .map_err(|_| eyre!("Invalid utility JSON configuration"))?;
            candidates.extend(import_profiles(&v));
        }
    }
    let db = toolkit.join("llm-toolkit.db");
    if db.exists() {
        // sqlite3 reads only one config row. No full DB scan, no credentials in arguments/output.
        let out = std::process::Command::new("sqlite3")
            .args(["-readonly"])
            .arg(&db)
            .arg("SELECT value FROM settings WHERE key='app_config';")
            .output()
            .wrap_err("Install sqlite3 to import LLM Toolkit database profiles")?;
        if !out.status.success() {
            bail!("Cannot read LLM Toolkit profiles database");
        }
        if !out.stdout.is_empty() {
            let v: Value = serde_json::from_slice(&out.stdout)
                .map_err(|_| eyre!("Invalid LLM Toolkit configuration"))?;
            candidates.extend(import_profiles(&v));
        }
    }
    // Distinct IDs for similarly named imports, without silently overwriting an entry.
    let mut found = BTreeMap::new();
    for (id, p) in candidates {
        let mut unique = id.clone();
        let mut i = 2;
        while found.contains_key(&unique) {
            unique = format!("{id}-{i}");
            i += 1;
        }
        found.insert(unique, p);
    }
    Ok(found.into_iter().collect())
}

#[cfg(test)]
mod tests {
    #[test]
    fn timeout_override_is_bounded_and_actionable() {
        assert_eq!(super::timeout_seconds(None).unwrap(), 180);
        assert_eq!(super::timeout_seconds(Some("300")).unwrap(), 300);
        for value in ["", "zero", "0", "901"] {
            assert!(super::timeout_seconds(Some(value)).is_err());
        }
    }

    use super::*;
    fn store() -> ProfileStore {
        let mut store = ProfileStore::default();
        for id in ["default", "env", "flag"] {
            let mut p = preset("lmstudio");
            p.model = "qwen/exact".into();
            store.upsert(id.into(), p).unwrap();
        }
        store.default_profile = Some("default".into());
        store
    }
    #[test]
    fn selection_precedence_and_fail_closed() {
        let s = store();
        assert_eq!(
            s.resolve(Some("flag"), Some("env")).unwrap().unwrap().0,
            "flag"
        );
        assert_eq!(s.resolve(None, Some("env")).unwrap().unwrap().0, "env");
        assert_eq!(s.resolve(None, None).unwrap().unwrap().0, "default");
        assert!(s.resolve(None, Some("missing")).is_err());
        assert!(ProfileStore::default()
            .resolve(None, None)
            .unwrap()
            .is_none());
    }
    #[test]
    fn validation_rejects_credentials_urls_and_literal_keys() {
        let mut p = preset("lmstudio");
        p.model = "qwen".into();
        p.validate().unwrap();
        for url in [
            "file:///tmp/x",
            "http://user:secret@localhost/v1",
            "http://localhost/v1?key=secret",
        ] {
            p.base_url = url.into();
            assert!(p.validate().is_err());
        }
        p.base_url = "http://localhost:1234/v1".into();
        p.api_key_env = Some("sk-secret-value".into());
        assert!(p.validate().is_err());
    }
    #[test]
    fn imports_never_copy_secrets() {
        let rows = import_profiles(
            &json!({"llmProfiles":[{"name":"Local Qwen","provider":"lmstudio","baseUrl":"http://localhost:3717/v1","model":"qwen/qwen3.8-27b","apiKey":"literal-secret","local":true},{"name":"Cloud","provider":"openai-compatible","baseUrl":"https://example.com/v1","model":"m","apiKey":"env:TEST_KEY","local":false}]}),
        );
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0, "local-qwen");
        assert!(rows[0].1.api_key_env.is_none());
        assert_eq!(rows[1].1.api_key_env.as_deref(), Some("TEST_KEY"));
        assert!(!serde_yaml::to_string(&rows)
            .unwrap()
            .contains("literal-secret"));
    }
    #[test]
    fn atomic_config_roundtrip() {
        let dir =
            std::env::temp_dir().join(format!("media-tool-profiles-{}", uuid::Uuid::new_v4()));
        let path = dir.join("profiles.yaml");
        let s = store();
        s.save_path(&path).unwrap();
        assert_eq!(ProfileStore::load_path(&path).unwrap().profiles.len(), 3);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
