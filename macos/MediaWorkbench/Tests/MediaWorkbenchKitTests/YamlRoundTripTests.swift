import XCTest
import Yams
import ToborKitUI
@testable import MediaWorkbenchKit

/// The load-bearing test: writing `keys:` must not cost the user any of the
/// five sections media-tool already owns. Every case here runs against a
/// temp-dir config — never `~/.config/media-tool/media-tool.yaml`.
final class YamlRoundTripTests: XCTestCase {

    private var tempDir: URL!

    override func setUpWithError() throws {
        tempDir = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("media-workbench-tests-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: tempDir, withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(at: tempDir)
    }

    private func makeStore(seed: String? = nil) throws -> MediaToolConfigStore {
        let url = tempDir.appendingPathComponent("media-tool.yaml")
        if let seed { try seed.write(to: url, atomically: true, encoding: .utf8) }
        return MediaToolConfigStore(location: MediaToolConfigLocation(fileURL: url))
    }

    /// A file exercising all five existing top-level keys, with nested
    /// sequences and mappings, plus an unknown key a future release might add.
    private let fullConfig = """
        version: 1
        defaults:
          gemini: gemini-3.1-flash-image
          suno: V6
        image_tiers:
          low:
            - gemini:gemini-3.1-flash-lite-image
            - qwen-image:qwen-image-3.0
          high:
            - gemini:gemini-3-pro-image
        max_prompt_chars:
          gemini: 4000
          suno: 400
        refine_model: anthropic:claude-sonnet-4-6
        prompt_guidance:
          image: Be specific about lighting.
          video: Describe camera motion.
        some_future_key:
          nested: value
        """

    func testKeysWritePreservesEveryOtherTopLevelKey() throws {
        let store = try makeStore(seed: fullConfig)

        try store.write(
            entry: ProviderKeyEntry(apiKey: .environment(name: "OPENAI_API_KEY")),
            for: "openai_chat"
        )

        let before = try compose(fullConfig)
        let after = try compose(contentsOfStore: store)

        // Values: every original key survives untouched.
        for (key, value) in before {
            XCTAssertEqual(
                after[key], value,
                "top-level key \(key.string ?? "?") changed or was dropped by a keys: write"
            )
        }

        // Order: originals keep their relative order, `keys` is appended.
        let originalOrder = before.compactMap { $0.key.string }
        let newOrder = after.compactMap { $0.key.string }
        XCTAssertEqual(newOrder.filter { originalOrder.contains($0) }, originalOrder)
        XCTAssertTrue(newOrder.contains("keys"))
        XCTAssertEqual(newOrder.count, originalOrder.count + 1)
    }

    func testRepeatedWritesRemainStable() throws {
        let store = try makeStore(seed: fullConfig)
        try store.write(entry: ProviderKeyEntry(apiKey: .environment(name: "SUNO_API_KEY")), for: "suno")
        let firstPass = try String(contentsOf: store.location.fileURL, encoding: .utf8)
        try store.write(entry: ProviderKeyEntry(apiKey: .environment(name: "SUNO_API_KEY")), for: "suno")
        let secondPass = try String(contentsOf: store.location.fileURL, encoding: .utf8)
        XCTAssertEqual(firstPass, secondPass, "a no-op write must be idempotent on disk")
    }

    func testWritingOneProviderLeavesOtherProviderBlocksIntact() throws {
        let store = try makeStore(seed: fullConfig)
        try store.write(entry: ProviderKeyEntry(apiKey: .environment(name: "SUNO_API_KEY")), for: "suno")
        try store.write(
            entry: ProviderKeyEntry(
                apiKey: .environment(name: "GEMINI_API_KEY"),
                model: "gemini-3-pro-image"
            ),
            for: "gemini"
        )

        let all = try store.readAll()
        XCTAssertEqual(all["suno"]?.apiKey, .environment(name: "SUNO_API_KEY"))
        XCTAssertEqual(all["gemini"]?.model, "gemini-3-pro-image")
        XCTAssertEqual(all.count, 2)
    }

    func testUnknownFieldsInsideAProviderBlockSurvive() throws {
        let seed = """
            defaults:
              gemini: gemini-3.1-flash-image
            keys:
              suno:
                env: SUNO_API_KEY
                unknown_future_field: keep-me
            """
        let store = try makeStore(seed: seed)
        try store.write(
            entry: ProviderKeyEntry(apiKey: .environment(name: "SUNO_API_KEY"), model: "V6"),
            for: "suno"
        )

        let after = try compose(contentsOfStore: store)
        let block = after["keys"]?.mapping?["suno"]?.mapping
        XCTAssertEqual(block?["unknown_future_field"]?.string, "keep-me")
        XCTAssertEqual(block?["model"]?.string, "V6")
    }

    func testSwitchingFromLiteralToEnvironmentClearsTheLiteral() throws {
        let store = try makeStore(seed: "defaults: {}\n")
        try store.write(entry: ProviderKeyEntry(apiKey: .literal("sk-secret")), for: "openai_chat")
        try store.write(
            entry: ProviderKeyEntry(apiKey: .environment(name: "OPENAI_API_KEY")),
            for: "openai_chat"
        )

        let text = try String(contentsOf: store.location.fileURL, encoding: .utf8)
        XCTAssertFalse(text.contains("sk-secret"), "a stale literal key must not linger in the file")
        XCTAssertEqual(try store.read("openai_chat")?.apiKey, .environment(name: "OPENAI_API_KEY"))
    }

    func testMissingFileIsCreatedWithOnlyTheKeysSection() throws {
        let store = try makeStore()
        XCTAssertNil(try store.read("suno"))
        try store.write(entry: ProviderKeyEntry(apiKey: .environment(name: "SUNO_API_KEY")), for: "suno")

        let root = try compose(contentsOfStore: store)
        XCTAssertEqual(root.compactMap { $0.key.string }, ["keys"])
    }

    func testNonMappingRootIsRefusedRatherThanOverwritten() throws {
        let store = try makeStore(seed: "- just\n- a\n- list\n")
        XCTAssertThrowsError(
            try store.write(entry: ProviderKeyEntry(apiKey: .literal("x")), for: "suno")
        )
        // The original bytes are still there.
        let text = try String(contentsOf: store.location.fileURL, encoding: .utf8)
        XCTAssertTrue(text.contains("- just"))
    }

    func testRemoveDropsOnlyThatProvider() throws {
        let store = try makeStore(seed: fullConfig)
        try store.write(entry: ProviderKeyEntry(apiKey: .environment(name: "SUNO_API_KEY")), for: "suno")
        try store.write(entry: ProviderKeyEntry(apiKey: .environment(name: "GEMINI_API_KEY")), for: "gemini")
        try store.remove("suno")

        let all = try store.readAll()
        XCTAssertNil(all["suno"])
        XCTAssertNotNil(all["gemini"])
        let after = try compose(contentsOfStore: store)
        XCTAssertNotNil(after["image_tiers"])
    }

    // MARK: Helpers

    private func compose(_ yaml: String) throws -> Node.Mapping {
        guard let mapping = try Yams.compose(yaml: yaml)?.mapping else {
            throw XCTSkip("seed YAML did not compose to a mapping")
        }
        return mapping
    }

    private func compose(contentsOfStore store: MediaToolConfigStore) throws -> Node.Mapping {
        try compose(try String(contentsOf: store.location.fileURL, encoding: .utf8))
    }
}
