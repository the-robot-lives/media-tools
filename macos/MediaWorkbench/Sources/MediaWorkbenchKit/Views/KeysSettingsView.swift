import SwiftUI
import ToborKitUI

/// Settings → Keys. Lists media-tool's 16 providers with a configured /
/// unconfigured indicator, and hands the selected one to tobor-kit's
/// `LLMInferenceSettingsView`.
///
/// The kit view never persists — it only mutates the binding — so this host
/// owns load-on-appear and save-on-change against media-tool.yaml.
public struct KeysSettingsView: View {

    private let store: MediaToolConfigStore
    /// Injectable so previews and tests resolve env-var keys without reading
    /// the real process environment.
    private let processEnvironment: [String: String]

    @State private var selectedProviderID: String?
    @State private var entries: [String: ProviderKeyEntry] = [:]
    @State private var draft = LLMInferenceConfig()
    @State private var loadError: String?

    public init(
        store: MediaToolConfigStore = MediaToolConfigStore(),
        processEnvironment: [String: String] = ProcessInfo.processInfo.environment
    ) {
        self.store = store
        self.processEnvironment = processEnvironment
    }

    public var body: some View {
        HSplitView {
            providerList
                .frame(minWidth: 220, idealWidth: 260, maxWidth: 340)
            detail
                .frame(minWidth: 420, maxWidth: .infinity, maxHeight: .infinity)
        }
        .task { reload() }
        .onChange(of: selectedProviderID) { _, newValue in
            guard let newValue else { return }
            loadDraft(for: newValue)
        }
    }

    // MARK: Provider list

    private var providerList: some View {
        List(selection: $selectedProviderID) {
            if let loadError {
                Text(loadError)
                    .font(.footnote)
                    .foregroundStyle(.red)
            }
            ForEach(LLMProvider.modalities(in: MediaProviderCatalog.all), id: \.self) { modality in
                Section(modality.label) {
                    ForEach(MediaProviderCatalog.providers(in: modality)) { provider in
                        row(for: provider).tag(provider.id)
                    }
                }
            }
        }
        .listStyle(.sidebar)
    }

    private func row(for provider: LLMProvider) -> some View {
        HStack(spacing: 8) {
            Circle()
                .fill(status(for: provider).tint)
                .frame(width: 8, height: 8)
            VStack(alignment: .leading, spacing: 1) {
                Text(provider.label)
                Text(status(for: provider).label)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
        }
        .accessibilityElement(children: .combine)
        .accessibilityLabel("\(provider.label), \(status(for: provider).label)")
    }

    private enum KeyStatus {
        case unconfigured
        case environmentSet(String)
        case environmentMissing(String)
        case literal

        var label: String {
            switch self {
            case .unconfigured: return "Not configured"
            case .environmentSet(let name): return "\(name) set"
            case .environmentMissing(let name): return "\(name) not in environment"
            case .literal: return "Key stored in config"
            }
        }

        var tint: Color {
            switch self {
            case .unconfigured: return .secondary
            case .environmentSet: return .green
            case .environmentMissing: return .orange
            case .literal: return .yellow
            }
        }
    }

    private func status(for provider: LLMProvider) -> KeyStatus {
        guard let spec = entries[provider.id]?.apiKey else { return .unconfigured }
        switch spec {
        case .literal:
            return .literal
        case .environment(let name):
            return spec.resolve(environment: processEnvironment) == nil
                ? .environmentMissing(name)
                : .environmentSet(name)
        }
    }

    // MARK: Detail

    @ViewBuilder
    private var detail: some View {
        if let id = selectedProviderID, let provider = MediaProviderCatalog.provider(id: id) {
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    Text(provider.label)
                        .font(.title3.weight(.semibold))
                    Text("media-tool service `\(provider.serviceID)`")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .textSelection(.enabled)

                    LLMInferenceSettingsView(
                        config: $draft,
                        catalog: MediaProviderCatalog.all,
                        environment: processEnvironment,
                        sectionBy: .modality
                    )
                }
                .padding(20)
            }
            .id(id)
            .onChange(of: draft) { _, newValue in persist(newValue, providerID: id) }
        } else {
            PlaceholderView(
                systemImage: "key",
                title: "API Keys",
                summary: """
                Pick a provider to point it at an environment variable. Keys \
                are written to ~/.config/media-tool/media-tool.yaml under \
                `keys:`; the rest of that file is left untouched.
                """,
                blockedOn: "Nothing — select a provider on the left."
            )
        }
    }

    // MARK: Load / save

    private func reload() {
        do {
            entries = try store.readAll()
            loadError = nil
        } catch {
            entries = [:]
            loadError = String(describing: error)
        }
        if let id = selectedProviderID { loadDraft(for: id) }
    }

    private func loadDraft(for id: String) {
        guard let provider = MediaProviderCatalog.provider(id: id) else { return }
        // Default a brand-new provider to `.environment`, never `.literal`:
        // no key at rest in a plaintext YAML file we do not control the
        // permissions of.
        let entry = entries[id] ?? ProviderKeyEntry.defaultEntry(for: provider)
        draft = entry.inferenceConfig(providerID: id)
    }

    private func persist(_ config: LLMInferenceConfig, providerID: String) {
        // The kit view rewrites `providerID` when its own picker changes; this
        // host scopes each block to the row the user selected, so pin it back.
        let entry = ProviderKeyEntry(inferenceConfig: config)
        guard entry != entries[providerID] else { return }
        do {
            try store.write(entry: entry, for: providerID)
            entries[providerID] = entry
            loadError = nil
        } catch {
            loadError = String(describing: error)
        }
    }
}
