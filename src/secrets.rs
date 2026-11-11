//! Secret resolution for judge/inference credentials.
//!
//! A [`SecretSpec`] is an ordered list of sources: environment variables and
//! `dc` store lookups. The first source that yields a usable value wins; a
//! source is absent when the env var is unset/empty, when the `dc` binary is
//! missing or exits non-zero, or when its output is empty or the literal
//! string `null`. Results are cached per process.
//!
//! Values never appear in logs, `Debug` impls or error strings: they live in
//! the redacting [`Secret`] newtype, whose only reader is
//! [`Secret::expose`]. Credentials must flow to endpoints exclusively through
//! the HTTP `Authorization` header.

use std::collections::HashMap;
use std::fmt;
use std::io;
use std::process::Command;
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Source types
// ---------------------------------------------------------------------------

/// One resolvable source in a [`SecretSpec`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SecretSource {
    /// Environment variable name. Names are safe to log; values are not.
    Env {
        env: String,
    },
    /// `dc` store lookup: shells out to `dc get <subject> <key> --reveal --raw`.
    Dc {
        dc: DcKey,
    },
}

/// A `dc` store address (subject first, then key).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DcKey {
    pub subject: String,
    pub key: String,
}

/// Ordered source list; first usable hit wins.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct SecretSpec(pub Vec<SecretSource>);

impl SecretSpec {
    pub fn new(sources: Vec<SecretSource>) -> Self {
        Self(sources)
    }

    /// Source descriptions for diagnostics — names only, never values.
    pub fn describe(&self) -> Vec<String> {
        self.0
            .iter()
            .map(|source| match source {
                SecretSource::Env { env } => format!("env:{}", env),
                SecretSource::Dc { dc } => format!("dc:{}/{}", dc.subject, dc.key),
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Redacting newtype
// ---------------------------------------------------------------------------

/// A resolved secret value. `Debug`/`Display` print a redacted placeholder so a
/// value can only reach a log through an explicit [`Secret::expose`] call.
#[derive(Clone)]
pub struct Secret(String);

impl Secret {
    /// The single controlled read. Only for constructing HTTP auth headers.
    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Secret(***)")
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Secret(***)")
    }
}

// ---------------------------------------------------------------------------
// Resolution
// ---------------------------------------------------------------------------

/// Raw output of one `dc` invocation, before value sanitization.
pub struct DcOutput {
    pub success: bool,
    pub stdout: Vec<u8>,
}

/// Resolve the spec against the process environment and the real `dc` binary,
/// memoizing per process so repeated lookups don't re-shell out.
pub fn resolve_secret(spec: &SecretSpec) -> Option<Secret> {
    static CACHE: OnceLock<Mutex<HashMap<SecretSpec, Option<Secret>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = match cache.lock() {
        Ok(guard) => guard,
        // A poisoned lock only means a holder panicked mid-insert; the map is
        // still usable and every entry is an `Option`, so recover instead of
        // failing a run that could otherwise resolve the secret.
        Err(poisoned) => poisoned.into_inner(),
    };
    if let Some(hit) = guard.get(spec) {
        return hit.clone();
    }
    let resolved = resolve_with(spec, &|name| std::env::var(name).ok(), &dc_command);
    guard.insert(spec.clone(), resolved.clone());
    resolved
}

/// The real `dc` boundary: `dc get <subject> <key> --reveal --raw`.
fn dc_command(subject: &str, key: &str) -> io::Result<DcOutput> {
    let output = Command::new("dc")
        .args(["get", subject, key, "--reveal", "--raw"])
        .output()?;
    Ok(DcOutput {
        success: output.status.success(),
        stdout: output.stdout,
    })
}

/// Uncached resolution core with injectable env and `dc` runners (tests).
pub(crate) fn resolve_with(
    spec: &SecretSpec,
    env: &dyn Fn(&str) -> Option<String>,
    dc: &dyn Fn(&str, &str) -> io::Result<DcOutput>,
) -> Option<Secret> {
    for source in &spec.0 {
        match source {
            SecretSource::Env { env: name } => {
                if let Some(value) = env(name) {
                    if let Some(secret) = sanitize(&value) {
                        return Some(secret);
                    }
                }
            }
            SecretSource::Dc { dc: dc_key } => {
                // Missing binary, non-zero exit, empty output and literal
                // `null` are all "absent": fall through to the next source.
                match dc(&dc_key.subject, &dc_key.key) {
                    Ok(output) if output.success => {
                        let value = String::from_utf8_lossy(&output.stdout);
                        if let Some(secret) = sanitize(&value) {
                            return Some(secret);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    None
}

/// Trim shell/env artifacts; treat empty and literal `null` as absent.
fn sanitize(value: &str) -> Option<Secret> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "null" {
        None
    } else {
        Some(Secret(trimmed.to_string()))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn env_lookup(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let pairs: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |name: &str| pairs.get(name).cloned()
    }

    fn dc_ok(value: &str) -> impl Fn(&str, &str) -> io::Result<DcOutput> + '_ {
        let value = value.to_string();
        move |_subject, _key| {
            Ok(DcOutput {
                success: true,
                stdout: value.clone().into_bytes(),
            })
        }
    }

    fn spec(sources: Vec<SecretSource>) -> SecretSpec {
        SecretSpec(sources)
    }

    fn env(name: &str) -> SecretSource {
        SecretSource::Env {
            env: name.to_string(),
        }
    }

    fn dc(subject: &str, key: &str) -> SecretSource {
        SecretSource::Dc {
            dc: DcKey {
                subject: subject.to_string(),
                key: key.to_string(),
            },
        }
    }

    #[test]
    fn env_source_wins_before_dc() {
        let s = spec(vec![env("A_KEY"), dc("services", "a")]);
        let resolved = resolve_with(&s, &env_lookup(&[("A_KEY", "from-env")]), &dc_ok("from-dc"))
            .expect("env hit");
        assert_eq!(resolved.expose(), "from-env");
    }

    #[test]
    fn dc_fallback_when_env_unset() {
        let s = spec(vec![env("MISSING_A"), dc("services", "b")]);
        let resolved =
            resolve_with(&s, &env_lookup(&[]), &dc_ok("fixture-dc-value")).expect("dc hit");
        assert_eq!(resolved.expose(), "fixture-dc-value");
    }

    #[test]
    fn empty_env_value_falls_through_to_dc() {
        let s = spec(vec![env("EMPTY_KEY"), dc("defaults", "c")]);
        let resolved = resolve_with(&s, &env_lookup(&[("EMPTY_KEY", "  ")],), &dc_ok("second"))
            .expect("dc fallback");
        assert_eq!(resolved.expose(), "second");
    }

    #[test]
    fn literal_null_is_absent() {
        // Env literal "null" is absent: resolution falls through to dc.
        let s = spec(vec![env("NULL_KEY"), dc("defaults", "d")]);
        let resolved = resolve_with(&s, &env_lookup(&[("NULL_KEY", "null")]), &dc_ok("dc-null"));
        assert_eq!(resolved.expect("dc hit").expose(), "dc-null");
        // With no further source, a "null" env value resolves to None.
        let env_only = spec(vec![env("NULL_KEY")]);
        assert!(
            resolve_with(&env_only, &env_lookup(&[("NULL_KEY", "null")]), &dc_ok("x")).is_none()
        );
    }

    #[test]
    fn dc_failure_modes_are_absent() {
        let s = spec(vec![dc("defaults", "e")]);
        // Non-zero exit
        let failing = |_subject: &str, _key: &str| {
            Ok(DcOutput {
                success: false,
                stdout: b"secret".to_vec(),
            })
        };
        assert!(resolve_with(&s, &env_lookup(&[]), &failing).is_none());
        // Missing binary
        let missing =
            |_subject: &str, _key: &str| Err(io::Error::new(io::ErrorKind::NotFound, "no dc"));
        assert!(resolve_with(&s, &env_lookup(&[]), &missing).is_none());
        // Empty stdout
        let resolved = resolve_with(&s, &env_lookup(&[]), &dc_ok("")).is_none();
        assert!(resolved);
    }

    #[test]
    fn first_usable_source_wins_over_later_ones() {
        let s = spec(vec![dc("defaults", "one"), dc("defaults", "two")]);
        let seen = std::cell::RefCell::new(Vec::new());
        let routing = |_subject: &str, key: &str| {
            seen.borrow_mut().push(key.to_string());
            if key == "one" {
                Ok(DcOutput {
                    success: true,
                    stdout: b"first".to_vec(),
                })
            } else {
                Ok(DcOutput {
                    success: true,
                    stdout: b"second".to_vec(),
                })
            }
        };
        let resolved = resolve_with(&s, &env_lookup(&[]), &routing).expect("hit");
        assert_eq!(resolved.expose(), "first");
        assert_eq!(*seen.borrow(), vec!["one".to_string()]);
    }

    #[test]
    fn redacted_debug_and_display() {
        let secret = sanitize("fixture-value").unwrap();
        assert_eq!(format!("{:?}", secret), "Secret(***)");
        assert_eq!(format!("{}", secret), "Secret(***)");
        assert!(format!("{:?}", secret).contains("***"));
    }

    #[test]
    fn describe_lists_names_not_values() {
        let s = spec(vec![env("NAMED_KEY"), dc("defaults", "some.key")]);
        let described = s.describe();
        assert_eq!(described, vec!["env:NAMED_KEY", "dc:defaults/some.key"]);
    }

    #[test]
    fn parses_config_yaml_shapes() {
        let yaml = r#"
- env: QWEN_API_KEY
- dc:
    subject: services
    key: apps.npl_mcp_qwen_api_key
"#;
        let parsed: SecretSpec = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(
            parsed,
            spec(vec![
                env("QWEN_API_KEY"),
                dc("services", "apps.npl_mcp_qwen_api_key")
            ])
        );
    }

    #[test]
    fn real_dc_binary_via_path_override_and_cache() {
        // Exercises the actual Command::new("dc") wiring with a fake binary
        // found through a PATH override, then proves the process cache answers
        // the second identical spec without another invocation.
        static SERIAL: Mutex<()> = Mutex::new(());
        let _guard = SERIAL.lock().unwrap_or_else(|p| p.into_inner());

        let dir = std::env::temp_dir().join(format!(
            "media_secrets_fake_dc_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let counter = dir.join("invocations");
        let script = dir.join("dc");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\necho fixture-real-dc >> \"{}\"\necho fixture-real-dc-value\n",
                counter.display()
            ),
        )
        .unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let original_path = std::env::var("PATH").unwrap_or_default();
        std::env::set_var("PATH", format!("{}:{}", dir.display(), original_path));

        // Unique spec so concurrent/previous tests can't share this cache entry.
        let s = spec(vec![dc("fixture-subject", "fixture.path.key")]);
        let first = resolve_secret(&s).expect("fake dc resolved");
        let second = resolve_secret(&s).expect("cached");
        std::env::set_var("PATH", original_path);

        assert_eq!(first.expose(), "fixture-real-dc-value");
        assert_eq!(second.expose(), "fixture-real-dc-value");
        let invocations = std::fs::read_to_string(&counter).unwrap().lines().count();
        assert_eq!(invocations, 1, "second lookup must be served from cache");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
