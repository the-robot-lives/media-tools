import SwiftUI

/// The honest empty state: what this surface will hold and what has to exist
/// before it can hold it. Not a "TODO" — a reader should learn something.
public struct PlaceholderView: View {
    public let systemImage: String
    public let title: String
    public let summary: String
    public let blockedOn: String

    public init(systemImage: String, title: String, summary: String, blockedOn: String) {
        self.systemImage = systemImage
        self.title = title
        self.summary = summary
        self.blockedOn = blockedOn
    }

    public var body: some View {
        VStack(spacing: 12) {
            Image(systemName: systemImage)
                .font(.system(size: 40, weight: .light))
                .foregroundStyle(.secondary)
            Text(title)
                .font(.title2.weight(.semibold))
            Text(summary)
                .font(.body)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 420)
            Label(blockedOn, systemImage: "clock.badge.questionmark")
                .font(.footnote)
                .foregroundStyle(.tertiary)
                .padding(.top, 4)
        }
        .padding(40)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

public struct LibraryView: View {
    public init() {}
    public var body: some View {
        PlaceholderView(
            systemImage: "photo.on.rectangle.angled",
            title: "Library",
            summary: """
            Generated images, audio, and video land here alongside the \
            .media.prompt that produced them, so a result can always be traced \
            back to its prompt and re-run.
            """,
            blockedOn: "Waiting on the Rust FFI bridge to media-tool's asset index."
        )
    }
}

public struct RunsView: View {
    public init() {}
    public var body: some View {
        PlaceholderView(
            systemImage: "list.bullet.rectangle",
            title: "Runs",
            summary: """
            One row per generation: provider, model, tier, cost, elapsed time, \
            and the failure or retry chain when a tier falls back.
            """,
            blockedOn: "Waiting on the Rust FFI bridge to stream run events."
        )
    }
}
