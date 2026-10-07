import SwiftUI

public enum WorkbenchSection: String, Hashable, CaseIterable, Identifiable, Sendable {
    case library
    case runs
    case settings

    public var id: String { rawValue }

    public var label: String {
        switch self {
        case .library: return "Library"
        case .runs: return "Runs"
        case .settings: return "Settings"
        }
    }

    public var systemImage: String {
        switch self {
        case .library: return "photo.on.rectangle.angled"
        case .runs: return "list.bullet.rectangle"
        case .settings: return "gearshape"
        }
    }
}

public struct RootView: View {
    private let store: MediaToolConfigStore
    @State private var selection: WorkbenchSection = .settings

    public init(store: MediaToolConfigStore = MediaToolConfigStore()) {
        self.store = store
    }

    public var body: some View {
        NavigationSplitView {
            List(WorkbenchSection.allCases, selection: $selection) { section in
                Label(section.label, systemImage: section.systemImage)
                    .tag(section)
            }
            .navigationSplitViewColumnWidth(min: 160, ideal: 180, max: 220)
        } detail: {
            switch selection {
            case .library: LibraryView()
            case .runs: RunsView()
            case .settings: KeysSettingsView(store: store)
            }
        }
        .navigationTitle("Media Workbench")
        .frame(minWidth: 900, minHeight: 560)
    }
}
