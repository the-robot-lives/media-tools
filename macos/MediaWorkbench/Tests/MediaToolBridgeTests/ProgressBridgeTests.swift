import XCTest
@testable import MediaToolBridge

/// Proves the UniFFI progress path arrives in Swift with its fields still typed.
///
/// Dry-run only — no provider is contacted and nothing is written outside a temp directory.
final class ProgressBridgeTests: XCTestCase {

    /// The foreign half of the callback interface. `dryRun` calls this synchronously on the
    /// calling thread, but the lock is cheap insurance against that changing.
    final class Recorder: ProgressObserver, @unchecked Sendable {
        private let lock = NSLock()
        private var storage: [ProgressEvent] = []

        var events: [ProgressEvent] {
            lock.lock()
            defer { lock.unlock() }
            return storage
        }

        func onEvent(event: ProgressEvent) {
            lock.lock()
            storage.append(event)
            lock.unlock()
        }
    }

    private static let promptYAML = """
    schema: "0.4"
    id: swift-bridge-001
    type: image
    service: gemini
    model: imagen-4

    prompt:
      text: |
        A single flat-shaded teal hexagon on a white background.

    output:
      formats:
        - format: png
    """

    private var fixtureDir: URL!

    override func setUpWithError() throws {
        fixtureDir = FileManager.default.temporaryDirectory
            .appendingPathComponent("media-tool-swift-bridge-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: fixtureDir, withIntermediateDirectories: true)
        try Self.promptYAML.write(
            to: fixtureDir.appendingPathComponent("hexagon.media.prompt"),
            atomically: true,
            encoding: .utf8
        )
    }

    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(at: fixtureDir)
    }

    func testParsePromptReturnsTypedSummary() throws {
        let path = fixtureDir.appendingPathComponent("hexagon.media.prompt").path
        let summary = try parsePrompt(path: path)

        XCTAssertEqual(summary.id, "swift-bridge-001")
        XCTAssertEqual(summary.assetType, "Image")
        XCTAssertEqual(summary.service, "gemini")
        XCTAssertEqual(summary.model, "imagen-4")
        XCTAssertEqual(summary.schemaVersion, "0.4")
        XCTAssertEqual(summary.outputPaths.count, 1)
        XCTAssertTrue(summary.outputPaths[0].hasSuffix(".png"))
    }

    func testListPromptsFindsTheFixture() throws {
        let summaries = try listPrompts(dir: fixtureDir.path, recursive: true)

        XCTAssertEqual(summaries.count, 1)
        XCTAssertEqual(summaries.first?.id, "swift-bridge-001")
    }

    func testListPromptsRejectsANonDirectory() {
        let path = fixtureDir.appendingPathComponent("hexagon.media.prompt").path
        XCTAssertThrowsError(try listPrompts(dir: path, recursive: false))
    }

    /// The point of the slice: events cross the boundary as an enum with readable fields,
    /// not as a formatted string a consumer would have to parse.
    func testDryRunStreamsTypedProgressEvents() throws {
        let path = fixtureDir.appendingPathComponent("hexagon.media.prompt").path
        let recorder = Recorder()

        let summary = try dryRun(paths: [path], observer: recorder)

        XCTAssertEqual(summary.totalPrompts, 1)
        XCTAssertEqual(summary.totalOutputs, 1)
        XCTAssertTrue(summary.dryRun)

        let events = recorder.events
        XCTAssertFalse(events.isEmpty, "the observer must have been called")

        var sawRunStarted = false
        var sawPlanItem = false
        var sawRunCompleted = false

        for event in events {
            switch event {
            case let .runStarted(totalOutputs, totalPrompts, _, dryRunFlag):
                sawRunStarted = true
                XCTAssertEqual(totalOutputs, 1)
                XCTAssertEqual(totalPrompts, 1)
                XCTAssertTrue(dryRunFlag)
            case let .planItem(promptId, assetType, service, model, outputPath):
                sawPlanItem = true
                XCTAssertEqual(promptId, "swift-bridge-001")
                XCTAssertEqual(assetType, "Image")
                XCTAssertEqual(service, "gemini")
                XCTAssertEqual(model, "imagen-4")
                XCTAssertTrue(outputPath.hasSuffix(".png"))
            case let .runCompleted(_, _, dryRunFlag):
                sawRunCompleted = true
                XCTAssertTrue(dryRunFlag)
            default:
                continue
            }
        }

        XCTAssertTrue(sawRunStarted, "run.started must reach Swift")
        XCTAssertTrue(sawPlanItem, "plan.item must reach Swift")
        XCTAssertTrue(sawRunCompleted, "run.completed must reach Swift")
    }

    /// A second run must not reach the first run's observer: the Rust side scopes its
    /// subscriber to the call, never installing one globally.
    func testAnObserverGoesDeafOnceItsCallReturns() throws {
        let path = fixtureDir.appendingPathComponent("hexagon.media.prompt").path

        let first = Recorder()
        _ = try dryRun(paths: [path], observer: first)
        let afterFirst = first.events.count
        XCTAssertGreaterThan(afterFirst, 0)

        let second = Recorder()
        _ = try dryRun(paths: [path], observer: second)

        XCTAssertEqual(first.events.count, afterFirst)
        XCTAssertGreaterThan(second.events.count, 0)
    }
}
