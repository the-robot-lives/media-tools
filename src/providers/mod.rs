pub mod anthropic;
pub mod dashscope;
pub mod elevenlabs;
pub mod gemini;
pub mod gemini_chat;
pub mod grok_video;
pub mod groq_chat;
pub mod openai_chat;
pub mod openai_tts;
pub mod openrouter;
pub mod qwen_image;
pub mod qwen_tts;
pub mod suno;
pub mod veo;
pub mod wan_video;
pub mod zai;

use std::collections::HashMap;
use std::path::Path;

use crate::attachments::LoadedAttachment;
use crate::schema::{AssetType, AudioKind, Quality};

/// Default Groq chat model for FIM / component / diagram text generation.
/// Exact id from Groq `GET /openai/v1/models` — not the retired llama-4-scout id.
pub const DEFAULT_CHAT_MODEL: &str = "openai/gpt-oss-120b";

// ---------------------------------------------------------------------------
// GenerationOptions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct GenerationOptions {
    pub model: String,
    pub aspect_ratio: Option<String>,
    pub negative_prompt: Option<String>,
    pub provider_options: HashMap<String, serde_yaml::Value>,
    pub verbose: bool,
    pub duration_seconds: Option<f64>,
}

// ---------------------------------------------------------------------------
// Provider traits
// ---------------------------------------------------------------------------

#[async_trait::async_trait]
pub trait MediaProvider: Send + Sync {
    async fn generate(
        &self,
        prompt_text: &str,
        output_path: &Path,
        api_key: &str,
        options: &GenerationOptions,
        attachments: &[LoadedAttachment],
    ) -> color_eyre::Result<bool>;

    fn name(&self) -> &str;
}

#[async_trait::async_trait]
pub trait ChatProvider: Send + Sync {
    async fn generate(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        output_path: &Path,
        api_key: &str,
        options: &GenerationOptions,
        attachments: &[LoadedAttachment],
    ) -> color_eyre::Result<bool>;

    fn name(&self) -> &str;
}

// ---------------------------------------------------------------------------
// Candidate selection
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Candidate {
    pub service: &'static str,
    pub model: &'static str,
}

/// Returns candidates ordered best-first for the given asset type / audio kind / quality tier.
/// The list is NOT filtered by API-key availability — call `available()` to filter.
// ⟦𓄃𓉞𓅍𓍺⟧ candidates_for :: Returns candidates ordered best-first for the given asset type / audio kind / quality tier.
pub fn candidates_for(
    asset_type: AssetType,
    audio_kind: AudioKind,
    quality: Quality,
) -> Vec<Candidate> {
    match asset_type {
        AssetType::Image => match quality {
            // YAML override (media-tool.yaml image_tiers) wins over the built-in ladder
            Quality::Low
            | Quality::Medium
            | Quality::High
                if crate::provider_config::loaded()
                    .and_then(|c| c.image_tiers.get(crate::provider_config::tier_key(quality)))
                    .is_some() =>
            {
                let cfg = crate::provider_config::loaded().expect("checked above");
                let key = crate::provider_config::tier_key(quality);
                crate::provider_config::parse_candidates(
                    cfg.image_tiers.get(key).expect("checked above"),
                )
            }
            Quality::Low => vec![
                Candidate {
                    service: "gemini",
                    model: "gemini-3.1-flash-lite-image",
                },
                Candidate {
                    service: "gemini",
                    model: "gemini-3.1-flash-image",
                },
                Candidate {
                    service: "qwen-image",
                    model: "qwen-image-3.0",
                },
            ],
            Quality::Medium => vec![
                Candidate {
                    service: "gemini",
                    model: "gemini-3.1-flash-image",
                },
                Candidate {
                    service: "gemini",
                    model: "gemini-3.1-flash-lite-image",
                },
                Candidate {
                    service: "qwen-image",
                    model: "qwen-image-3.0",
                },
            ],
            Quality::High => vec![
                Candidate {
                    service: "gemini",
                    model: "gemini-3-pro-image",
                },
                Candidate {
                    service: "gemini",
                    model: "gemini-3.1-flash-image",
                },
                Candidate {
                    service: "qwen-image",
                    model: "qwen-image-3.0-pro",
                },
            ],
        },

        AssetType::Video => match quality {
            Quality::Low => vec![
                Candidate {
                    service: "grok-video",
                    model: "grok-imagine-video",
                },
                Candidate {
                    service: "veo",
                    model: "veo-3.0-fast-generate-001",
                },
                Candidate {
                    service: "wan-video",
                    model: "wan2.7-t2v",
                },
            ],
            Quality::Medium => vec![
                Candidate {
                    service: "veo",
                    model: "veo-3.0-fast-generate-001",
                },
                Candidate {
                    service: "grok-video",
                    model: "grok-imagine-video",
                },
                Candidate {
                    service: "wan-video",
                    model: "wan2.7-t2v",
                },
            ],
            Quality::High => vec![
                Candidate {
                    service: "veo",
                    model: "veo-3.0-generate-001",
                },
                Candidate {
                    service: "grok-video",
                    model: "grok-imagine-video",
                },
                Candidate {
                    service: "wan-video",
                    model: "wan2.7-t2v",
                },
            ],
        },

        AssetType::Audio => match audio_kind {
            AudioKind::Music => vec![Candidate {
                service: "suno",
                model: "V5_5",
            }],
            AudioKind::Sfx => vec![Candidate {
                service: "suno",
                model: "V5_SOUND",
            }],
            AudioKind::Voice => match quality {
                Quality::Low => vec![
                    Candidate {
                        service: "qwen-tts",
                        model: "qwen3-tts-flash",
                    },
                    Candidate {
                        service: "openai-tts",
                        model: "gpt-4o-mini-tts",
                    },
                ],
                Quality::Medium => vec![
                    Candidate {
                        service: "openai-tts",
                        model: "gpt-4o-mini-tts",
                    },
                    Candidate {
                        service: "elevenlabs",
                        model: "eleven_multilingual_v2",
                    },
                ],
                Quality::High => vec![
                    Candidate {
                        service: "elevenlabs",
                        model: "eleven_multilingual_v2",
                    },
                    Candidate {
                        service: "openai-tts",
                        model: "gpt-4o-mini-tts",
                    },
                ],
            },
        },

        // Chat / code generation types — use a Groq model id that is currently listed.
        AssetType::Component
        | AssetType::ReactPage
        | AssetType::Html
        | AssetType::StyleGuide
        | AssetType::Diagram
        | AssetType::Document => match quality {
            Quality::Low | Quality::Medium | Quality::High => vec![Candidate {
                service: "groq-chat",
                model: DEFAULT_CHAT_MODEL,
            }],
        },

        AssetType::Unknown => vec![Candidate {
            service: "gemini",
            model: "gemini-3.1-flash-image",
        }],
    }
}

/// Returns true if the candidate's required API key env var is set and non-empty.
// ⟦𓀖𓂿𓎇𓃟⟧ available :: Returns true if the candidate's required API key env var is set and non-empty.
pub fn available(c: &Candidate) -> bool {
    resolve_api_key(c.service).is_some()
}

/// Resolve the runtime API key for a service (DashScope family accepts several env names).
pub fn resolve_api_key(service: &str) -> Option<String> {
    match service {
        "qwen-tts" | "qwen-image" | "wan-video" | "happyhorse" => dashscope::resolve_key(),
        other => {
            let env = api_key_env(other);
            std::env::var(env).ok().filter(|v| !v.is_empty())
        }
    }
}

// ---------------------------------------------------------------------------
// Provider factory helpers
// ---------------------------------------------------------------------------

// ⟦𓉢𓀡𓂟𓅼⟧ get_provider :: auto-generated pointer for public function get_provider
pub fn get_provider(service: &str) -> Option<Box<dyn MediaProvider>> {
    match service {
        "gemini" => Some(Box::new(gemini::GeminiProvider)),
        "suno" => Some(Box::new(suno::SunoProvider)),
        "openai-tts" => Some(Box::new(openai_tts::OpenAiTtsProvider)),
        "elevenlabs" => Some(Box::new(elevenlabs::ElevenLabsProvider)),
        "qwen-tts" => Some(Box::new(qwen_tts::QwenTtsProvider)),
        "qwen-image" => Some(Box::new(qwen_image::QwenImageProvider)),
        "wan-video" | "happyhorse" => Some(Box::new(wan_video::WanVideoProvider)),
        "grok-video" => Some(Box::new(grok_video::GrokVideoProvider)),
        "veo" => Some(Box::new(veo::VeoProvider)),
        _ => None,
    }
}

// ⟦𓐥𓄗𓈥𓉨⟧ get_chat_provider :: auto-generated pointer for public function get_chat_provider
pub fn get_chat_provider(service: &str) -> Option<Box<dyn ChatProvider>> {
    match service {
        "anthropic" => Some(Box::new(anthropic::AnthropicProvider)),
        "gemini-chat" => Some(Box::new(gemini_chat::GeminiChatProvider)),
        "groq" | "groq-chat" => Some(Box::new(groq_chat::GroqChatProvider)),
        "openai-chat" => Some(Box::new(openai_chat::OpenAiChatProvider)),
        "openrouter" | "openrouter-chat" => {
            Some(Box::new(openrouter::OpenRouterChatProvider))
        }
        "zai" | "z.ai" => Some(Box::new(zai::ZaiProvider)),
        _ => None,
    }
}

// ⟦𓃢𓅓𓂩𓇩⟧ is_stub_provider :: auto-generated pointer for public function is_stub_provider
pub fn is_stub_provider(service: &str) -> bool {
    !matches!(
        service,
        "gemini"
            | "suno"
            | "openai-tts"
            | "elevenlabs"
            | "qwen-tts"
            | "qwen-image"
            | "wan-video"
            | "happyhorse"
            | "grok-video"
            | "veo"
            | "anthropic"
            | "gemini-chat"
            | "groq"
            | "groq-chat"
            | "openai-chat"
            | "openrouter"
            | "openrouter-chat"
            | "zai"
            | "z.ai"
    )
}

// ⟦𓆌𓉭𓈳𓐮⟧ api_key_env :: auto-generated pointer for public function api_key_env
pub fn api_key_env(service: &str) -> &'static str {
    match service {
        "gemini" | "veo" => "GEMINI_API_KEY",
        "suno" => "SUNO_API_KEY",
        "openai-tts" => "OPENAI_API_KEY",
        "elevenlabs" => "ELEVENLABS_API_KEY",
        "qwen-tts" | "qwen-image" | "wan-video" | "happyhorse" => "DASHSCOPE_API_KEY",
        "grok-video" => "XAI_API_KEY",
        "anthropic" => "ANTHROPIC_API_KEY",
        "gemini-chat" => "GEMINI_API_KEY",
        "groq" | "groq-chat" => "GROQ_API_KEY",
        "openai-chat" => "OPENAI_API_KEY",
        "openrouter" | "openrouter-chat" => "OPENROUTER_API_KEY",
        "zai" | "z.ai" => "XAI_API_KEY",
        _ => "GEMINI_API_KEY",
    }
}

// ⟦𓄫𓁗𓆙𓐧⟧ sanitize_chat_output :: auto-generated pointer for public function sanitize_chat_output
pub fn sanitize_chat_output(raw: &str, output_path: &Path) -> String {
    let mut text = raw.trim().to_string();

    // Strip markdown code fences: ```lang\n...\n```
    if text.starts_with("```") {
        if let Some(first_newline) = text.find('\n') {
            text = text[first_newline + 1..].to_string();
        }
        if text.ends_with("```") {
            text = text[..text.len() - 3].trim_end().to_string();
        }
    }

    // Format-specific validation
    let ext = output_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    match ext {
        "svg" => {
            if !text.starts_with('<') {
                if let Some(svg_start) = text.find("<svg") {
                    text = text[svg_start..].to_string();
                }
            }
            if !text.contains("</svg>") {
                if text.contains("<svg") {
                    if let Some(last_open) = text.rfind('<') {
                        let after = &text[last_open..];
                        if !after.contains('>') {
                            text.truncate(last_open);
                        }
                    }
                    text = text.trim_end().to_string();
                    text.push_str("\n</svg>");
                }
            }
        }
        "mmd" => {
            text = text
                .trim_start_matches("```mermaid")
                .trim_start_matches("```")
                .trim_end_matches("```")
                .trim()
                .to_string();
        }
        "puml" => {
            if !text.contains("@startuml") {
                text = format!("@startuml\n{}", text);
            }
            if !text.contains("@enduml") {
                text.push_str("\n@enduml");
            }
        }
        _ => {}
    }

    text
}

/// Provider constraints that affect prompt preparation.
pub struct ProviderConstraints {
    pub max_prompt_chars: Option<usize>,
}

// ⟦𓋠𓃽𓁒𓉵⟧ constraints :: auto-generated pointer for public function constraints
pub fn constraints(service: &str) -> ProviderConstraints {
    // YAML override (media-tool.yaml max_prompt_chars) wins over the built-in table
    if let Some(cfg) = crate::provider_config::loaded() {
        if let Some(max) = cfg.max_prompt_chars.get(service) {
            return ProviderConstraints {
                max_prompt_chars: Some(*max),
            };
        }
    }
    match service {
        // Suno music: 3000 in custom mode (auto-enabled). Sounds endpoint: 500.
        // Use 3000 here; SFX constraint enforced via suno-sfx key below.
        "suno" => ProviderConstraints {
            max_prompt_chars: Some(3000),
        },
        "suno-sfx" => ProviderConstraints {
            max_prompt_chars: Some(500),
        },
        "gemini" => ProviderConstraints {
            max_prompt_chars: Some(4000),
        },
        "veo" => ProviderConstraints {
            max_prompt_chars: Some(1000),
        },
        "grok-video" => ProviderConstraints {
            max_prompt_chars: Some(1000),
        },
        _ => ProviderConstraints {
            max_prompt_chars: None,
        },
    }
}

// ⟦𓐧𓉱𓍻𓉋⟧ default_model :: auto-generated pointer for public function default_model
pub fn default_model(service: &str) -> &'static str {
    // YAML override (media-tool.yaml defaults) wins over the built-in table
    if let Some(cfg) = crate::provider_config::loaded() {
        if let Some(model) = cfg.defaults.get(service) {
            return model.as_str();
        }
    }
    match service {
        "gemini" => "gemini-3.1-flash-image",
        "suno" => "V5_5",
        "openai-tts" => "gpt-4o-mini-tts",
        "elevenlabs" => "eleven_multilingual_v2",
        "qwen-tts" => "qwen3-tts-flash",
        "qwen-image" => "qwen-image-3.0",
        "wan-video" => "wan2.7-t2v",
        "happyhorse" => "happyhorse-1.1-t2v",
        "grok-video" => "grok-imagine-video",
        "veo" => "veo-3.0-generate-001",
        "anthropic" => "claude-sonnet-4-6",
        "gemini-chat" => "gemini-2.5-flash",
        "groq" | "groq-chat" => DEFAULT_CHAT_MODEL,
        "openai-chat" => "gpt-4.1",
        "openrouter" | "openrouter-chat" => "openai/gpt-4o-mini",
        "zai" | "z.ai" => "grok-4.3",
        _ => "default",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{AssetType, AudioKind, Quality};

    #[test]
    fn image_candidates_by_quality() {
        let low = candidates_for(AssetType::Image, AudioKind::Voice, Quality::Low);
        assert_eq!(low[0].model, "gemini-3.1-flash-lite-image");
        let high = candidates_for(AssetType::Image, AudioKind::Voice, Quality::High);
        assert!(high.iter().any(|c| c.model == "gemini-3-pro-image"));
    }

    #[test]
    fn video_has_fallback_chain() {
        let med = candidates_for(AssetType::Video, AudioKind::Voice, Quality::Medium);
        assert!(med.len() >= 2);
        assert_eq!(med[0].service, "veo");
    }

    #[test]
    fn voice_and_music_routes() {
        let music = candidates_for(AssetType::Audio, AudioKind::Music, Quality::Medium);
        assert_eq!(music[0].service, "suno");
        let voice = candidates_for(AssetType::Audio, AudioKind::Voice, Quality::Medium);
        assert!(voice.iter().any(|c| c.service == "openai-tts"));
    }

    #[test]
    fn openrouter_chat_is_wired() {
        assert_eq!(api_key_env("openrouter"), "OPENROUTER_API_KEY");
        assert_eq!(default_model("openrouter"), "openai/gpt-4o-mini");
        assert!(get_chat_provider("openrouter").is_some());
        assert!(get_chat_provider("openrouter-chat").is_some());
        assert!(!is_stub_provider("openrouter"));
    }

    #[test]
    fn chat_auto_select_is_groq() {
        for at in [
            AssetType::Html,
            AssetType::Diagram,
            AssetType::ReactPage,
            AssetType::Document,
        ] {
            let c = candidates_for(at, AudioKind::Voice, Quality::High);
            assert_eq!(c[0].service, "groq-chat");
        }
    }
}
