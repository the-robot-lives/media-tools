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

    func testEveryProviderIsReachableFromExactlyOneModalitySection() {
        let grouped = MediaModality.allCases.flatMap { MediaProviderCatalog.providers(in: $0) }
        XCTAssertEqual(Set(grouped.map(\.id)), expectedIDs)
        XCTAssertEqual(grouped.count, 16, "a provider appears in more than one modality section")
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
            "zai": "XAI_API_KEY"
        ]
        for (id, envVar) in expected {
            XCTAssertEqual(
                MediaProviderCatalog.provider(id: id)?.entry.envVarName, envVar,
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
