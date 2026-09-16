import Foundation
import ToborKitUI

/// media-tool's provider catalog, replacing tobor-kit's 9-entry chat default.
///
/// Ids match the `--service` strings media-tool's Rust core accepts, with
/// hyphens normalised to underscores so they are safe YAML mapping keys under
/// `keys:` (`media_tool_service` gives the hyphenated form back).
///
/// `envVarName` is taken verbatim from `src/providers/mod.rs::api_key_env` —
/// that table is authoritative, so the Settings screen's env hint names the
/// same variable the CLI actually reads. Where the Rust table has no entry the
/// value stays nil rather than inventing one.
///
/// `LLMProvider.Group` only has three cases (cloud / proxyLocal / advanced),
/// inherited from llm-toolkit's chat-oriented picker. Media-tool's meaningful
/// axis is modality, so ``MediaModality`` carries that grouping alongside the
/// kit entry and drives the Settings sidebar sections.
public enum MediaModality: String, Sendable, Hashable, CaseIterable {
    case textChat
    case image
    case audio
    case video

    public var label: String {
        switch self {
        case .textChat: return "Chat & Text"
        case .image: return "Image"
        case .audio: return "Audio & Voice"
        case .video: return "Video"
        }
    }
}

/// A media-tool provider: the kit's `LLMProvider` plus the modality grouping
/// the kit's three-case `Group` cannot express.
public struct MediaProvider: Identifiable, Sendable, Hashable {
    public let entry: LLMProvider
    public let modality: MediaModality

    public var id: String { entry.id }
    public var label: String { entry.label }

    /// The hyphenated service id media-tool's CLI expects.
    public var serviceID: String { entry.id.replacingOccurrences(of: "_", with: "-") }

    public init(entry: LLMProvider, modality: MediaModality) {
        self.entry = entry
        self.modality = modality
    }
}

public enum MediaProviderCatalog {

    /// The kit-shaped catalog handed to `LLMInferenceSettingsView(catalog:)`.
    public static let all: [LLMProvider] = providers.map(\.entry)

    public static func provider(id: String) -> MediaProvider? {
        providers.first { $0.id == id }
    }

    public static func providers(in modality: MediaModality) -> [MediaProvider] {
        providers.filter { $0.modality == modality }
    }

    /// All 16 media-tool providers, grouped by modality then alphabetical.
    public static let providers: [MediaProvider] = [
        // ── Chat & Text ─────────────────────────────────────────────
        MediaProvider(
            entry: LLMProvider(
                id: "anthropic", label: "Anthropic (Claude)", group: .cloud,
                apiShape: .anthropic,
                defaultBaseURL: URL(string: "https://api.anthropic.com/v1"),
                baseURLPlaceholder: nil,
                envVarName: "ANTHROPIC_API_KEY", requiresKey: true,
                defaultModel: nil
            ),
            modality: .textChat
        ),
        MediaProvider(
            entry: LLMProvider(
                id: "gemini_chat", label: "Gemini (Chat)", group: .cloud,
                apiShape: .openAI,
                defaultBaseURL: URL(string: "https://generativelanguage.googleapis.com/v1beta"),
                baseURLPlaceholder: nil,
                envVarName: "GEMINI_API_KEY", requiresKey: true,
                defaultModel: nil
            ),
            modality: .textChat
        ),
        MediaProvider(
            entry: LLMProvider(
                id: "groq_chat", label: "Groq (Chat)", group: .cloud,
                apiShape: .openAI,
                defaultBaseURL: URL(string: "https://api.groq.com/openai/v1"),
                baseURLPlaceholder: nil,
                envVarName: "GROQ_API_KEY", requiresKey: true,
                defaultModel: nil
            ),
            modality: .textChat
        ),
        MediaProvider(
            entry: LLMProvider(
                id: "openai_chat", label: "OpenAI (Chat)", group: .cloud,
                apiShape: .openAI,
                defaultBaseURL: URL(string: "https://api.openai.com/v1"),
                baseURLPlaceholder: nil,
                envVarName: "OPENAI_API_KEY", requiresKey: true,
                defaultModel: nil
            ),
            modality: .textChat
        ),
        MediaProvider(
            entry: LLMProvider(
                id: "openrouter", label: "OpenRouter", group: .proxyLocal,
                apiShape: .openAI,
                defaultBaseURL: URL(string: "https://openrouter.ai/api/v1"),
                baseURLPlaceholder: "https://openrouter.ai/api/v1",
                envVarName: "OPENROUTER_API_KEY", requiresKey: true,
                defaultModel: nil
            ),
            modality: .textChat
        ),
        // media-tool's `zai` service reads XAI_API_KEY (src/providers/mod.rs),
        // not ZAI_API_KEY — the CLI table wins over the name's appearance.
        MediaProvider(
            entry: LLMProvider(
                id: "zai", label: "Z.ai (GLM)", group: .cloud,
                apiShape: .openAI,
                defaultBaseURL: URL(string: "https://api.z.ai/api/coding/paas/v4"),
                baseURLPlaceholder: "https://api.z.ai/api/coding/paas/v4",
                envVarName: "XAI_API_KEY", requiresKey: true,
                defaultModel: nil
            ),
            modality: .textChat
        ),

        // ── Image ───────────────────────────────────────────────────
        MediaProvider(
            entry: LLMProvider(
                id: "gemini", label: "Gemini (Image)", group: .cloud,
                apiShape: .openAI,
                defaultBaseURL: URL(string: "https://generativelanguage.googleapis.com/v1beta"),
                baseURLPlaceholder: nil,
                envVarName: "GEMINI_API_KEY", requiresKey: true,
                defaultModel: nil
            ),
            modality: .image
        ),
        MediaProvider(
            entry: LLMProvider(
                id: "qwen_image", label: "Qwen Image (DashScope)", group: .cloud,
                apiShape: .openAI,
                defaultBaseURL: URL(string: "https://dashscope-intl.aliyuncs.com/api/v1"),
                baseURLPlaceholder: nil,
                envVarName: "DASHSCOPE_API_KEY", requiresKey: true,
                defaultModel: nil
            ),
            modality: .image
        ),

        // ── Audio & Voice ───────────────────────────────────────────
        MediaProvider(
            entry: LLMProvider(
                id: "dashscope", label: "DashScope (Alibaba)", group: .cloud,
                apiShape: .openAI,
                defaultBaseURL: URL(string: "https://dashscope-intl.aliyuncs.com/api/v1"),
                baseURLPlaceholder: nil,
                envVarName: "DASHSCOPE_API_KEY", requiresKey: true,
                defaultModel: nil
            ),
            modality: .audio
        ),
        MediaProvider(
            entry: LLMProvider(
                id: "elevenlabs", label: "ElevenLabs", group: .cloud,
                apiShape: .openAI,
                defaultBaseURL: URL(string: "https://api.elevenlabs.io/v1"),
                baseURLPlaceholder: nil,
                envVarName: "ELEVENLABS_API_KEY", requiresKey: true,
                defaultModel: nil
            ),
            modality: .audio
        ),
        MediaProvider(
            entry: LLMProvider(
                id: "openai_tts", label: "OpenAI TTS", group: .cloud,
                apiShape: .openAI,
                defaultBaseURL: URL(string: "https://api.openai.com/v1"),
                baseURLPlaceholder: nil,
                envVarName: "OPENAI_API_KEY", requiresKey: true,
                defaultModel: nil
            ),
            modality: .audio
        ),
        MediaProvider(
            entry: LLMProvider(
                id: "qwen_tts", label: "Qwen TTS (DashScope)", group: .cloud,
                apiShape: .openAI,
                defaultBaseURL: URL(string: "https://dashscope-intl.aliyuncs.com/api/v1"),
                baseURLPlaceholder: nil,
                envVarName: "DASHSCOPE_API_KEY", requiresKey: true,
                defaultModel: nil
            ),
            modality: .audio
        ),
        MediaProvider(
            entry: LLMProvider(
                id: "suno", label: "Suno", group: .cloud,
                apiShape: .openAI,
                defaultBaseURL: nil,
                baseURLPlaceholder: "https://api.sunoapi.org/api/v1",
                envVarName: "SUNO_API_KEY", requiresKey: true,
                defaultModel: nil
            ),
            modality: .audio
        ),

        // ── Video ───────────────────────────────────────────────────
        MediaProvider(
            entry: LLMProvider(
                id: "grok_video", label: "Grok Video (xAI)", group: .cloud,
                apiShape: .openAI,
                defaultBaseURL: URL(string: "https://api.x.ai/v1"),
                baseURLPlaceholder: nil,
                envVarName: "XAI_API_KEY", requiresKey: true,
                defaultModel: nil
            ),
            modality: .video
        ),
        MediaProvider(
            entry: LLMProvider(
                id: "veo", label: "Veo (Google)", group: .cloud,
                apiShape: .openAI,
                defaultBaseURL: URL(string: "https://generativelanguage.googleapis.com/v1beta"),
                baseURLPlaceholder: nil,
                envVarName: "GEMINI_API_KEY", requiresKey: true,
                defaultModel: nil
            ),
            modality: .video
        ),
        MediaProvider(
            entry: LLMProvider(
                id: "wan_video", label: "Wan Video (DashScope)", group: .cloud,
                apiShape: .openAI,
                defaultBaseURL: URL(string: "https://dashscope-intl.aliyuncs.com/api/v1"),
                baseURLPlaceholder: nil,
                envVarName: "DASHSCOPE_API_KEY", requiresKey: true,
                defaultModel: nil
            ),
            modality: .video
        ),
    ]
}
