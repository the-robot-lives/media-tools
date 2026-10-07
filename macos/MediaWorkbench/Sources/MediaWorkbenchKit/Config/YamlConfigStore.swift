import Foundation
import ToborKitUI
import Yams

/// Read/write access to the `keys:` section of media-tool.yaml.
///
/// ## Round-trip contract
///
/// media-tool.yaml is a *user-owned* file: `defaults`, `image_tiers`,
/// `max_prompt_chars`, `refine_model`, `prompt_guidance` (and anything a
/// future media-tool release adds) are none of this app's business. Losing a
/// hand-tuned `image_tiers` ladder because Settings saved an API key would be
/// a serious bug.
///
/// So writes never re-encode a Swift model of the whole file. They:
/// 1. parse the existing bytes into a generic Yams `Node` tree,
/// 2. mutate **only** the `keys` subtree (and, within it, only the fields this
///    app owns — unknown fields inside a provider block survive too),
/// 3. re-emit the same tree.
///
/// Yams' `Node.Mapping` is insertion-ordered, so untouched keys come back in
/// their original order with their original values. Comments are the one thing
/// the YAML parser cannot preserve; see ``MediaToolConfigStore/write(entry:for:)``.
public struct MediaToolConfigStore: Sendable {
    public let location: MediaToolConfigLocation

    public init(location: MediaToolConfigLocation = .userDefault) {
        self.location = location
    }

    // MARK: Reading

    /// The parsed root mapping, or an empty mapping when the file is absent
    /// or blank. Throws only when the file exists but is unusable.
    func rootMapping() throws -> Node.Mapping {
        guard FileManager.default.fileExists(atPath: location.fileURL.path) else {
            return Node.Mapping([])
        }
        let text: String
        do {
            text = try String(contentsOf: location.fileURL, encoding: .utf8)
        } catch {
            throw MediaToolConfigError.unreadable(underlying: error)
        }
        guard let node = try Yams.compose(yaml: text) else {
            return Node.Mapping([])
        }
        guard let mapping = node.mapping else {
            throw MediaToolConfigError.rootIsNotAMapping
        }
        return mapping
    }

    /// Every provider block currently under `keys:`.
    public func readAll() throws -> [String: ProviderKeyEntry] {
        guard let keys = try rootMapping()[ProviderKeyEntry.keysSectionName]?.mapping else {
            return [:]
        }
        var result: [String: ProviderKeyEntry] = [:]
        for (key, value) in keys {
            guard let id = key.string, let entry = ProviderKeyEntry(node: value) else { continue }
            result[id] = entry
        }
        return result
    }

    public func read(_ providerID: String) throws -> ProviderKeyEntry? {
        try readAll()[providerID]
    }

    // MARK: Writing

    /// Writes one provider block, leaving every other top-level key — and
    /// every other provider block — byte-for-byte equivalent.
    ///
    /// Caveat worth stating out loud: YAML comments are not part of the node
    /// tree, so a rewrite drops them. Values, key order, and structure all
    /// survive; `# comments` do not. That is a property of round-tripping
    /// through a YAML parser, not of this mutation.
    public func write(entry: ProviderKeyEntry, for providerID: String) throws {
        var root = try rootMapping()

        var keys = root[ProviderKeyEntry.keysSectionName]?.mapping ?? Node.Mapping([])
        keys[providerID] = entry.merged(into: keys[providerID])
        root[ProviderKeyEntry.keysSectionName] = Node.mapping(keys)

        try emit(root)
    }

    /// Removes a provider block entirely (the "unset this key" path).
    public func remove(_ providerID: String) throws {
        var root = try rootMapping()
        guard var keys = root[ProviderKeyEntry.keysSectionName]?.mapping else { return }
        keys[providerID] = nil
        root[ProviderKeyEntry.keysSectionName] = Node.mapping(keys)
        try emit(root)
    }

    private func emit(_ root: Node.Mapping) throws {
        let text = try Yams.serialize(
            node: Node.mapping(root),
            width: -1,
            sortKeys: false
        )
        let directory = location.fileURL.deletingLastPathComponent()
        try FileManager.default.createDirectory(
            at: directory, withIntermediateDirectories: true
        )
        try text.write(to: location.fileURL, atomically: true, encoding: .utf8)
    }
}

// MARK: - Per-provider tobor-kit store

/// Adapts the file-backed store to tobor-kit's persistence seam, scoped to one
/// provider so `LLMInferenceSettingsView` can bind against it directly.
///
/// The view never persists on its own — the host observes the binding and calls
/// ``save(_:)``.
public struct ProviderConfigStore: LLMInferenceConfigStoring {
    public let providerID: String
    private let store: MediaToolConfigStore

    public init(providerID: String, store: MediaToolConfigStore) {
        self.providerID = providerID
        self.store = store
    }

    public func load() -> LLMInferenceConfig? {
        guard let entry = try? store.read(providerID) else { return nil }
        return entry.inferenceConfig(providerID: providerID)
    }

    public func save(_ config: LLMInferenceConfig) throws {
        try store.write(entry: ProviderKeyEntry(inferenceConfig: config), for: providerID)
    }
}
