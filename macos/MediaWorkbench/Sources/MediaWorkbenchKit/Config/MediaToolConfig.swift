import Foundation
import ToborKitUI
import Yams

/// Where the media-tool YAML config lives. Always injectable so tests never
/// touch the developer's real `~/.config/media-tool/media-tool.yaml`.
public struct MediaToolConfigLocation: Sendable, Hashable {
    public let fileURL: URL

    public init(fileURL: URL) {
        self.fileURL = fileURL
    }

    /// `~/.config/media-tool/media-tool.yaml` — the path the Rust CLI reads
    /// last in its lookup chain, and the one the app owns.
    public static var userDefault: MediaToolConfigLocation {
        MediaToolConfigLocation(
            fileURL: FileManager.default.homeDirectoryForCurrentUser
                .appendingPathComponent(".config/media-tool/media-tool.yaml")
        )
    }
}

public enum MediaToolConfigError: Error, CustomStringConvertible {
    /// The file parsed, but its root is not a mapping — we refuse to
    /// overwrite something we do not understand.
    case rootIsNotAMapping
    case unreadable(underlying: any Error)

    public var description: String {
        switch self {
        case .rootIsNotAMapping:
            return "media-tool.yaml root is not a YAML mapping; refusing to write."
        case .unreadable(let underlying):
            return "media-tool.yaml could not be read: \(underlying)"
        }
    }
}

/// Per-provider key settings, as stored under the `keys:` section.
///
/// Serialised shape (one block per provider id):
/// ```yaml
/// keys:
///   openai_chat:
///     env: OPENAI_API_KEY     # or `key:` for a literal (discouraged)
///     base_url: https://…
///     model: gpt-4o
///     api_shape: openai
/// ```
public struct ProviderKeyEntry: Sendable, Hashable {
    public var apiKey: LLMKeySpec?
    public var baseURL: URL?
    public var model: String?
    public var apiShape: LLMApiShape?

    public init(
        apiKey: LLMKeySpec? = nil,
        baseURL: URL? = nil,
        model: String? = nil,
        apiShape: LLMApiShape? = nil
    ) {
        self.apiKey = apiKey
        self.baseURL = baseURL
        self.model = model
        self.apiShape = apiShape
    }

    /// A provider counts as configured once it has *some* key spec. An
    /// `.environment` spec pointing at an unset variable still counts as
    /// configured here — resolution is a runtime concern, not a settings one.
    public var isConfigured: Bool { apiKey != nil }

    // MARK: Bridging to the tobor-kit config the settings view binds to.

    public func inferenceConfig(providerID: String) -> LLMInferenceConfig {
        LLMInferenceConfig(
            providerID: providerID,
            baseURL: baseURL,
            apiKey: apiKey,
            model: model,
            apiShape: apiShape
        )
    }

    public init(inferenceConfig: LLMInferenceConfig) {
        self.apiKey = inferenceConfig.apiKey
        self.baseURL = inferenceConfig.baseURL
        self.model = inferenceConfig.model
        self.apiShape = inferenceConfig.apiShape
    }

    /// The security default: a new entry names an environment variable, never
    /// a literal key at rest in the config file.
    public static func defaultEntry(for provider: LLMProvider) -> ProviderKeyEntry {
        ProviderKeyEntry(
            apiKey: provider.envVarName.map { LLMKeySpec.environment(name: $0) },
            baseURL: nil,
            model: nil,
            apiShape: nil
        )
    }
}

// MARK: - YAML node <-> entry

extension ProviderKeyEntry {
    static let keysSectionName = "keys"

    init?(node: Node) {
        guard let mapping = node.mapping else { return nil }

        var spec: LLMKeySpec?
        if let env = mapping["env"]?.string, !env.isEmpty {
            spec = .environment(name: env)
        } else if let literal = mapping["key"]?.string, !literal.isEmpty {
            spec = .literal(literal)
        }

        self.init(
            apiKey: spec,
            baseURL: mapping["base_url"]?.string.flatMap(URL.init(string:)),
            model: mapping["model"]?.string,
            apiShape: mapping["api_shape"]?.string.flatMap(LLMApiShape.init(rawValue:))
        )
    }

    /// Merges this entry into an existing node, preserving any unknown keys the
    /// user (or a future media-tool version) put in the provider's block.
    func merged(into existing: Node?) -> Node {
        var mapping = existing?.mapping ?? Node.Mapping([])

        // The key spec is a one-of; writing one form clears the other.
        mapping["env"] = nil
        mapping["key"] = nil
        switch apiKey {
        case .environment(let name): mapping["env"] = Node(name)
        case .literal(let value): mapping["key"] = Node(value)
        case nil: break
        }

        mapping["base_url"] = baseURL.map { Node($0.absoluteString) }
        mapping["model"] = model.flatMap { $0.isEmpty ? nil : Node($0) }
        mapping["api_shape"] = apiShape.map { Node($0.rawValue) }

        return Node.mapping(mapping)
    }
}
