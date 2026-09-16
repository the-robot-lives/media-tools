import Foundation
import ToborKitUI

/// media-tool's provider catalog, replacing tobor-kit's 9-entry chat default.
///
/// Ids match the `--service` strings media-tool's Rust core accepts, with
/// hyphens normalised to underscores so they are safe YAML mapping keys under
/// `keys:` (`media_tool_service` gives the hyphenated form back — see
/// ``LLMProvider/serviceID``).
///
/// `envVarName` is taken verbatim from `src/providers/mod.rs::api_key_env` —
/// that table is authoritative, so the Settings screen's env hint names the
/// same variable the CLI actually reads. Where the Rust table has no entry the
/// value stays nil rather than inventing one.
///
/// Modality comes straight from tobor-kit's ``LLMModality`` axis (PR #52):
/// each entry's `modalities:` says what it produces, and the Settings
/// sidebar / kit picker section on that via `sectionBy: .modality`.
public enum MediaProviderCatalog {

    /// The kit-shaped catalog handed to `LLMInferenceSettingsView(catalog:)`.
    public static let all: [LLMProvider] = providers

    public static func provider(id: String) -> LLMProvider? {
        LLMProvider.provider(id: id, in: providers)
    }

    public static func providers(in modality: LLMModality) -> [LLMProvider] {
        LLMProvider.providers(in: providers, matching: modality)
    }

    /// All 16 media-tool providers, grouped by modality then alphabetical.
    public static let providers: [LLMProvider] = [
        // ── Chat & Text ─────────────────────────────────────────────
        LLMProvider(
            id: "anthropic", label: "Anthropic (Claude)", group: .cloud,
            apiShape: .anthropic,
            defaultBaseURL: URL(string: "https://api.anthropic.com/v1"),
            baseURLPlaceholder: nil,
            envVarName: "ANTHROPIC_API_KEY", requiresKey: true,
            defaultModel: nil,
            modalities: [.text]
        ),
        LLMProvider(
            id: "gemini_chat", label: "Gemini (Chat)", group: .cloud,
            apiShape: .openAI,
            defaultBaseURL: URL(string: "https://generativelanguage.googleapis.com/v1beta"),
            baseURLPlaceholder: nil,
            envVarName: "GEMINI_API_KEY", requiresKey: true,
            defaultModel: nil,
            modalities: [.text]
        ),
        LLMProvider(
            id: "groq_chat", label: "Groq (Chat)", group: .cloud,
            apiShape: .openAI,
            defaultBaseURL: URL(string: "https://api.groq.com/openai/v1"),
            baseURLPlaceholder: nil,
            envVarName: "GROQ_API_KEY", requiresKey: true,
            defaultModel: nil,
            modalities: [.text]
        ),
        LLMProvider(
            id: "openai_chat", label: "OpenAI (Chat)", group: .cloud,
            apiShape: .openAI,
            defaultBaseURL: URL(string: "https://api.openai.com/v1"),
            baseURLPlaceholder: nil,
            envVarName: "OPENAI_API_KEY", requiresKey: true,
            defaultModel: nil,
            modalities: [.text]
        ),
        LLMProvider(
            id: "openrouter", label: "OpenRouter", group: .proxyLocal,
            apiShape: .openAI,
            defaultBaseURL: URL(string: "https://openrouter.ai/api/v1"),
            baseURLPlaceholder: "https://openrouter.ai/api/v1",
            envVarName: "OPENROUTER_API_KEY", requiresKey: true,
            defaultModel: nil,
            modalities: [.text]
        ),
        // media-tool's `zai` service reads XAI_API_KEY (src/providers/mod.rs),
        // not ZAI_API_KEY — the CLI table wins over the name's appearance.
        LLMProvider(
            id: "zai", label: "Z.ai (GLM)", group: .cloud,
            apiShape: .openAI,
            defaultBaseURL: URL(string: "https://api.z.ai/api/coding/paas/v4"),
            baseURLPlaceholder: "https://api.z.ai/api/coding/paas/v4",
            envVarName: "XAI_API_KEY", requiresKey: true,
            defaultModel: nil,
            modalities: [.text]
        ),

        // ── Image ───────────────────────────────────────────────────
        LLMProvider(
            id: "gemini", label: "Gemini (Image)", group: .cloud,
            apiShape: .openAI,
            defaultBaseURL: URL(string: "https://generativelanguage.googleapis.com/v1beta"),
            baseURLPlaceholder: nil,
            envVarName: "GEMINI_API_KEY", requiresKey: true,
            defaultModel: nil,
            modalities: [.image]
        ),
        LLMProvider(
            id: "qwen_image", label: "Qwen Image (DashScope)", group: .cloud,
            apiShape: .openAI,
            defaultBaseURL: URL(string: "https://dashscope-intl.aliyuncs.com/api/v1"),
            baseURLPlaceholder: nil,
            envVarName: "DASHSCOPE_API_KEY", requiresKey: true,
            defaultModel: nil,
            modalities: [.image]
        ),

        // ── Audio & Voice ───────────────────────────────────────────
        // Not a dispatchable --service on its own (no get_provider /
        // get_chat_provider arm in src/providers/mod.rs) — it's the shared
        // DashScope family credential (qwen-tts / qwen-image / wan-video /
        // happyhorse all resolve DASHSCOPE_API_KEY through dashscope.rs), so
        // it has no single generation modality to inherit. Filed under
        // `.speech` (DashScope's TTS surface is its most direct analog and
        // keeps this a single-section sidebar entry like every other
        // provider) rather than fanned across every modality the family
        // touches — a judgment call, flagged for review.
        LLMProvider(
            id: "dashscope", label: "DashScope (Alibaba)", group: .cloud,
            apiShape: .openAI,
            defaultBaseURL: URL(string: "https://dashscope-intl.aliyuncs.com/api/v1"),
            baseURLPlaceholder: nil,
            envVarName: "DASHSCOPE_API_KEY", requiresKey: true,
            defaultModel: nil,
            modalities: [.speech]
        ),
        LLMProvider(
            id: "elevenlabs", label: "ElevenLabs", group: .cloud,
            apiShape: .openAI,
            defaultBaseURL: URL(string: "https://api.elevenlabs.io/v1"),
            baseURLPlaceholder: nil,
            envVarName: "ELEVENLABS_API_KEY", requiresKey: true,
            defaultModel: nil,
            modalities: [.audio]
        ),
        LLMProvider(
            id: "openai_tts", label: "OpenAI TTS", group: .cloud,
            apiShape: .openAI,
            defaultBaseURL: URL(string: "https://api.openai.com/v1"),
            baseURLPlaceholder: nil,
            envVarName: "OPENAI_API_KEY", requiresKey: true,
            defaultModel: nil,
            modalities: [.speech]
        ),
        LLMProvider(
            id: "qwen_tts", label: "Qwen TTS (DashScope)", group: .cloud,
            apiShape: .openAI,
            defaultBaseURL: URL(string: "https://dashscope-intl.aliyuncs.com/api/v1"),
            baseURLPlaceholder: nil,
            envVarName: "DASHSCOPE_API_KEY", requiresKey: true,
            defaultModel: nil,
            modalities: [.speech]
        ),
        LLMProvider(
            id: "suno", label: "Suno", group: .cloud,
            apiShape: .openAI,
            defaultBaseURL: nil,
            baseURLPlaceholder: "https://api.sunoapi.org/api/v1",
            envVarName: "SUNO_API_KEY", requiresKey: true,
            defaultModel: nil,
            modalities: [.audio]
        ),

        // ── Video ───────────────────────────────────────────────────
        LLMProvider(
            id: "grok_video", label: "Grok Video (xAI)", group: .cloud,
            apiShape: .openAI,
            defaultBaseURL: URL(string: "https://api.x.ai/v1"),
            baseURLPlaceholder: nil,
            envVarName: "XAI_API_KEY", requiresKey: true,
            defaultModel: nil,
            modalities: [.video]
        ),
        LLMProvider(
            id: "veo", label: "Veo (Google)", group: .cloud,
            apiShape: .openAI,
            defaultBaseURL: URL(string: "https://generativelanguage.googleapis.com/v1beta"),
            baseURLPlaceholder: nil,
            envVarName: "GEMINI_API_KEY", requiresKey: true,
            defaultModel: nil,
            modalities: [.video]
        ),
        LLMProvider(
            id: "wan_video", label: "Wan Video (DashScope)", group: .cloud,
            apiShape: .openAI,
            defaultBaseURL: URL(string: "https://dashscope-intl.aliyuncs.com/api/v1"),
            baseURLPlaceholder: nil,
            envVarName: "DASHSCOPE_API_KEY", requiresKey: true,
            defaultModel: nil,
            modalities: [.video]
        ),
    ]
}

extension LLMProvider {
    /// The hyphenated service id media-tool's CLI expects.
    public var serviceID: String { id.replacingOccurrences(of: "_", with: "-") }
}
