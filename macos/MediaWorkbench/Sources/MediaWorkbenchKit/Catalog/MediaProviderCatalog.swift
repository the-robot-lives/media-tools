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
/// each entry's `modalities:` says what it produces (or, for a shared
/// credential like `dashscope`, every surface it backs), and the Settings
/// sidebar / kit picker section on that via `sectionBy: .modality`. The kit's
/// axis assumes one producer maps to one modality; `isSharedCredential` /
/// `producerProviders` / `sharedCredentialProviders` below are media-tool's
/// own layer on top for entries that don't fit that assumption — see
/// `KeysSettingsView` for how the sidebar uses them.
public enum MediaProviderCatalog {

    /// The kit-shaped catalog handed to `LLMInferenceSettingsView(catalog:)`.
    public static let all: [LLMProvider] = providers

    public static func provider(id: String) -> LLMProvider? {
        LLMProvider.provider(id: id, in: providers)
    }

    /// Every entry (producer or shared credential) that supports `modality`.
    /// The Keys sidebar does not use this directly for sectioning — see
    /// ``producerProviders`` — but it is the honest "what can do X" answer.
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

        // ── Shared credentials ─────────────────────────────────────
        // Not a dispatchable --service on its own — no get_provider /
        // get_chat_provider arm in src/providers/mod.rs. It's the shared
        // DashScope family credential: qwen-tts, qwen-image, wan-video (and
        // its happyhorse alias) all resolve DASHSCOPE_API_KEY through
        // dashscope::resolve_key(). Its modalities list every surface that
        // key actually backs (image + speech + video) rather than picking
        // one as a stand-in; ``LLMProvider/isSharedCredential`` below is
        // what routes it to the Keys sidebar's "Shared Credentials" section
        // instead of duplicating it across three producer sections.
        LLMProvider(
            id: "dashscope", label: "DashScope (Alibaba)", group: .cloud,
            apiShape: .openAI,
            defaultBaseURL: URL(string: "https://dashscope-intl.aliyuncs.com/api/v1"),
            baseURLPlaceholder: nil,
            envVarName: "DASHSCOPE_API_KEY", requiresKey: true,
            defaultModel: nil,
            modalities: [.image, .speech, .video]
        ),
    ]
}

extension LLMProvider {
    /// The hyphenated service id media-tool's CLI expects.
    public var serviceID: String { id.replacingOccurrences(of: "_", with: "-") }

    /// App-level grouping over the kit's one-producer-one-modality data:
    /// tobor-kit's `LLMProvider` has no notion of "credential vs. producer"
    /// (its modality axis only says what a provider generates), so this is
    /// pure media-tool convention, not a kit invariant. A catalog entry that
    /// declares more than one modality isn't itself dispatchable — it's a
    /// credential shared by several single-modality producers (see
    /// `dashscope` above) — and the Keys sidebar sections it separately
    /// instead of fanning it across every modality section it touches.
    public var isSharedCredential: Bool { (modalities?.count ?? 0) > 1 }
}

extension MediaProviderCatalog {
    /// Producer entries only (single declared modality) — what the Keys
    /// sidebar sections `by .modality`, excluding shared credentials.
    public static var producerProviders: [LLMProvider] {
        providers.filter { !$0.isSharedCredential }
    }

    /// Shared-credential entries (e.g. `dashscope`) — catalog rows that back
    /// more than one producer rather than being one themselves.
    public static var sharedCredentialProviders: [LLMProvider] {
        providers.filter(\.isSharedCredential)
    }

    /// The producer service ids a shared credential backs, in catalog order
    /// (e.g. "qwen-image, qwen-tts, wan-video" for `dashscope`) — derived
    /// from matching `envVarName` rather than hand-maintained, so it can't
    /// drift from the entries it actually describes.
    public static func backedServiceIDs(for credential: LLMProvider) -> [String] {
        guard let envVarName = credential.envVarName else { return [] }
        return producerProviders
            .filter { $0.envVarName == envVarName }
            .map(\.serviceID)
    }
}
