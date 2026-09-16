import XCTest
import ToborKitUI
@testable import MediaWorkbenchKit

final class ProviderConfigStoreTests: XCTestCase {

    private var tempDir: URL!

    override func setUpWithError() throws {
        tempDir = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("media-workbench-store-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: tempDir, withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(at: tempDir)
    }

    private func makeStore() -> MediaToolConfigStore {
        MediaToolConfigStore(
            location: MediaToolConfigLocation(
                fileURL: tempDir.appendingPathComponent("media-tool.yaml")
            )
        )
    }

    func testSaveThenLoadRoundTripsEveryField() throws {
        let store = ProviderConfigStore(providerID: "gemini", store: makeStore())
        XCTAssertNil(store.load())

        let config = LLMInferenceConfig(
            providerID: "gemini",
            baseURL: URL(string: "https://proxy.internal/v1"),
            apiKey: .environment(name: "GEMINI_API_KEY"),
            model: "gemini-3-pro-image",
            apiShape: .openAI
        )
        try store.save(config)

        let loaded = try XCTUnwrap(store.load())
        XCTAssertEqual(loaded, config)
    }

    func testLoadedConfigIsScopedToItsOwnProvider() throws {
        let file = makeStore()
        try ProviderConfigStore(providerID: "suno", store: file)
            .save(LLMInferenceConfig(providerID: "suno", apiKey: .environment(name: "SUNO_API_KEY")))

        XCTAssertNil(ProviderConfigStore(providerID: "veo", store: file).load())
        XCTAssertEqual(
            ProviderConfigStore(providerID: "suno", store: file).load()?.providerID,
            "suno"
        )
    }

    /// The security default the design calls for: a fresh provider entry names
    /// an env var, and never produces a literal key at rest.
    func testDefaultEntryUsesEnvironmentNotLiteral() throws {
        for provider in MediaProviderCatalog.providers {
            let entry = ProviderKeyEntry.defaultEntry(for: provider.entry)
            switch entry.apiKey {
            case .environment(let name):
                XCTAssertEqual(name, provider.entry.envVarName)
            case .literal:
                XCTFail("\(provider.id) defaulted to a literal key")
            case nil:
                XCTFail("\(provider.id) has no env var to default to")
            }
        }
    }

    func testDefaultEntryCarriesNoBaseURLModelOrShapeOverride() {
        let entry = ProviderKeyEntry.defaultEntry(
            for: XCTUnwrapOrFail(MediaProviderCatalog.provider(id: "suno")?.entry)
        )
        XCTAssertNil(entry.baseURL)
        XCTAssertNil(entry.model)
        XCTAssertNil(entry.apiShape)
    }

    func testIsConfiguredTracksPresenceOfAKeySpec() {
        XCTAssertFalse(ProviderKeyEntry().isConfigured)
        XCTAssertTrue(ProviderKeyEntry(apiKey: .environment(name: "X")).isConfigured)
    }

    func testEnvironmentSpecResolvesAgainstAnInjectedEnvironment() {
        let spec = LLMKeySpec.environment(name: "MEDIA_TEST_KEY")
        XCTAssertEqual(spec.resolve(environment: ["MEDIA_TEST_KEY": "abc"]), "abc")
        XCTAssertNil(spec.resolve(environment: [:]))
    }

    private func XCTUnwrapOrFail(_ value: LLMProvider?) -> LLMProvider {
        guard let value else {
            fatalError("expected provider in catalog")
        }
        return value
    }
}
