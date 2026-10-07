import XCTest
import ToborKitUI
@testable import MediaWorkbenchKit

final class CatalogTests: XCTestCase {

    /// The 16 services media-tool's Rust core dispatches on.
    private let expectedIDs: Set<String> = [
        "anthropic", "dashscope", "elevenlabs", "gemini", "gemini_chat",
        "grok_video", "groq_chat", "openai_chat", "openai_tts", "openrouter",
        "qwen_image", "qwen_tts", "suno", "veo", "wan_video", "zai"
    ]

    /// Expected tobor-kit `LLMModality` per producer (PR #52's modality
    /// axis, replacing the app's local `MediaModality` wrapper). `dashscope`
    /// is a shared credential, not a producer — it declares all three
    /// modalities it backs, so it is checked separately below rather than
    /// forced into this one-modality-per-id table.
    private let expectedModalities: [String: LLMModality] = [
        "anthropic": .text,
        "gemini_chat": .text,
        "groq_chat": .text,
        "openai_chat": .text,
        "openrouter": .text,
        "zai": .text,
        "gemini": .image,
        "qwen_image": .image,
        "elevenlabs": .audio,
        "suno": .audio,
        "openai_tts": .speech,
        "qwen_tts": .speech,
        "grok_video": .video,
        "veo": .video,
        "wan_video": .video
    ]

    func testCatalogCoversExactlyTheSixteenProviders() {
        XCTAssertEqual(MediaProviderCatalog.providers.count, 16)
        XCTAssertEqual(Set(MediaProviderCatalog.providers.map(\.id)), expectedIDs)
    }

    func testKitFacingCatalogMirrorsTheProviderList() {
        XCTAssertEqual(
            MediaProviderCatalog.all.map(\.id),
            MediaProviderCatalog.providers.map(\.id)
        )
    }

    /// Producers (single declared modality) must each be reachable from
    /// exactly one modality section — the Keys sidebar's per-modality
    /// sections and the "each provider = one product" mental model both
    /// depend on this. Shared credentials are explicitly out of scope: they
    /// are allowed, by design, to declare more than one modality, and are
    /// checked separately (`testSharedCredentialsDeclareEveryModalityTheyBack`).
    func testEveryProducerIsReachableFromExactlyOneModalitySection() {
        let producers = MediaProviderCatalog.producerProviders
        let grouped = LLMModality.allCases.flatMap { LLMProvider.providers(in: producers, matching: $0) }
        let expectedProducerIDs = expectedIDs.subtracting(["dashscope"])
        XCTAssertEqual(Set(grouped.map(\.id)), expectedProducerIDs)
        XCTAssertEqual(grouped.count, expectedProducerIDs.count, "a producer appears in more than one modality section")
    }

    /// Every producer declares a modality via the kit's `LLMModality` axis,
    /// and it matches the mapping this catalog chose.
    func testModalitiesMatchExpectedMapping() {
        for (id, modality) in expectedModalities {
            let provider = MediaProviderCatalog.provider(id: id)
            XCTAssertEqual(provider?.effectiveModalities, [modality], "modality drift for \(id)")
        }
    }

    /// `dashscope` is not a dispatchable `--service` (no get_provider /
    /// get_chat_provider arm in src/providers/mod.rs) — it's the shared
    /// DashScope-family credential backing qwen-image, qwen-tts, and
    /// wan-video, so it honestly declares all three modalities instead of
    /// picking one as a stand-in, and the sidebar routes it to a distinct
    /// "Shared Credentials" section rather than three producer sections.
    func testSharedCredentialsDeclareEveryModalityTheyBack() {
        let shared = MediaProviderCatalog.sharedCredentialProviders
        XCTAssertEqual(shared.map(\.id), ["dashscope"])
        let dashscope = MediaProviderCatalog.provider(id: "dashscope")
        XCTAssertEqual(
            Set(dashscope?.effectiveModalities ?? []),
            [.image, .speech, .video]
        )
        XCTAssertEqual(
            MediaProviderCatalog.backedServiceIDs(for: dashscope!),
            ["qwen-image", "qwen-tts", "wan-video"]
        )
    }

    /// Env var names come from `src/providers/mod.rs::api_key_env`; drift here
    /// means the Settings hint names a variable the CLI does not read.
    func testEnvVarNamesMatchTheRustTable() {
        let expected: [String: String] = [
            "gemini": "GEMINI_API_KEY",
            "veo": "GEMINI_API_KEY",
            "gemini_chat": "GEMINI_API_KEY",
            "suno": "SUNO_API_KEY",
            "openai_tts": "OPENAI_API_KEY",
            "openai_chat": "OPENAI_API_KEY",
            "elevenlabs": "ELEVENLABS_API_KEY",
            "qwen_tts": "DASHSCOPE_API_KEY",
            "qwen_image": "DASHSCOPE_API_KEY",
            "wan_video": "DASHSCOPE_API_KEY",
            "dashscope": "DASHSCOPE_API_KEY",
            "grok_video": "XAI_API_KEY",
            "anthropic": "ANTHROPIC_API_KEY",
            "groq_chat": "GROQ_API_KEY",
            "openrouter": "OPENROUTER_API_KEY",
            "zai": "ZAI_API_KEY"
        ]
        for (id, envVar) in expected {
            XCTAssertEqual(
                MediaProviderCatalog.provider(id: id)?.envVarName, envVar,
                "env var drift for \(id)"
            )
        }
    }

    func testServiceIDRestoresTheHyphenatedCLIForm() {
        XCTAssertEqual(MediaProviderCatalog.provider(id: "qwen_image")?.serviceID, "qwen-image")
        XCTAssertEqual(MediaProviderCatalog.provider(id: "suno")?.serviceID, "suno")
    }

    func testCatalogReplacesRatherThanExtendsTheKitDefault() {
        // The kit's 9-entry chat catalog must not leak into media-tool's picker.
        XCTAssertFalse(MediaProviderCatalog.all.contains { $0.id == "ollama" })
        XCTAssertFalse(MediaProviderCatalog.all.contains { $0.id == "custom" })
    }
}
