pub mod anthropic;
pub mod elevenlabs;
pub mod gemini;
pub mod gemini_chat;
pub mod grok_video;
pub mod openai_chat;
pub mod openai_tts;
pub mod qwen_tts;
pub mod suno;
pub mod veo;
pub mod zai;

use std::collections::HashMap;
use std::path::Path;

use crate::attachments::LoadedAttachment;

#[derive(Debug, Clone)]
pub struct GenerationOptions {
    pub model: String,
    pub aspect_ratio: Option<String>,
    pub negative_prompt: Option<String>,
    pub provider_options: HashMap<String, serde_yaml::Value>,
    pub verbose: bool,
}

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

pub fn get_provider(service: &str) -> Option<Box<dyn MediaProvider>> {
    match service {
        "gemini" => Some(Box::new(gemini::GeminiProvider)),
        "suno" => Some(Box::new(suno::SunoProvider)),
        "openai-tts" => Some(Box::new(openai_tts::OpenAiTtsProvider)),
        "elevenlabs" => Some(Box::new(elevenlabs::ElevenLabsProvider)),
        "qwen-tts" => Some(Box::new(qwen_tts::QwenTtsProvider)),
        "grok-video" => Some(Box::new(grok_video::GrokVideoProvider)),
        "veo" => Some(Box::new(veo::VeoProvider)),
        _ => None,
    }
}

pub fn get_chat_provider(service: &str) -> Option<Box<dyn ChatProvider>> {
    match service {
        "anthropic" => Some(Box::new(anthropic::AnthropicProvider)),
        "gemini-chat" => Some(Box::new(gemini_chat::GeminiChatProvider)),
        "openai-chat" => Some(Box::new(openai_chat::OpenAiChatProvider)),
        "zai" | "z.ai" => Some(Box::new(zai::ZaiProvider)),
        _ => None,
    }
}

pub fn is_stub_provider(service: &str) -> bool {
    !matches!(service, "gemini" | "suno" | "openai-tts" | "elevenlabs" | "qwen-tts" | "grok-video" | "veo" | "anthropic" | "gemini-chat" | "openai-chat" | "zai" | "z.ai")
}

pub fn api_key_env(service: &str) -> &'static str {
    match service {
        "gemini" | "veo" => "GEMINI_API_KEY",
        "suno" => "SUNO_API_KEY",
        "openai-tts" => "OPENAI_API_KEY",
        "elevenlabs" => "ELEVENLABS_API_KEY",
        "qwen-tts" => "DASHSCOPE_API_KEY",
        "grok-video" => "XAI_API_KEY",
        "anthropic" => "ANTHROPIC_API_KEY",
        "gemini-chat" => "GEMINI_API_KEY",
        "openai-chat" => "OPENAI_API_KEY",
        "zai" | "z.ai" => "XAI_API_KEY",
        _ => "GEMINI_API_KEY",
    }
}

pub fn default_model(service: &str) -> &'static str {
    match service {
        "gemini" => "imagen-4.0-generate-001",
        "suno" => "V4_5ALL",
        "openai-tts" => "gpt-4o-mini-tts",
        "elevenlabs" => "eleven_multilingual_v2",
        "qwen-tts" => "qwen3-tts-flash",
        "grok-video" => "grok-imagine-video",
        "veo" => "veo-3.0-generate-001",
        "anthropic" => "claude-sonnet-4-6",
        "gemini-chat" => "gemini-2.5-flash",
        "openai-chat" => "gpt-4.1",
        "zai" | "z.ai" => "grok-4.3",
        _ => "default",
    }
}
