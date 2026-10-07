// swift-tools-version: 6.0

import PackageDescription
import Foundation

// tobor-kit is a sibling portfolio package, consumed by local path rather
// than by tag: the published v0.1.0 tag predates the Swift surface entirely.
// The default is the depth from a canonical monorepo checkout
// (Portfolio/Utilities/source/media-tool/macos/MediaWorkbench → Portfolio/Libs/tobor-kit);
// git worktrees sit at a different depth, so TOBOR_KIT_PATH overrides it.
let toborKitPath = ProcessInfo.processInfo.environment["TOBOR_KIT_PATH"]
    ?? "../../../../../Libs/tobor-kit"

let swiftV6: [SwiftSetting] = [.swiftLanguageMode(.v6)]

let package = Package(
    name: "MediaWorkbench",
    platforms: [
        .macOS(.v14)
    ],
    products: [
        .executable(name: "MediaWorkbench", targets: ["MediaWorkbench"]),
        .library(name: "MediaWorkbenchKit", targets: ["MediaWorkbenchKit"])
    ],
    dependencies: [
        .package(path: toborKitPath),
        .package(url: "https://github.com/jpsim/Yams", from: "5.0.0")
    ],
    targets: [
        .target(
            name: "MediaWorkbenchKit",
            dependencies: [
                .product(name: "ToborKitUI", package: "tobor-kit"),
                .product(name: "ToborKitCore", package: "tobor-kit"),
                .product(name: "Yams", package: "Yams")
            ],
            swiftSettings: swiftV6
        ),
        .executableTarget(
            name: "MediaWorkbench",
            dependencies: ["MediaWorkbenchKit"],
            swiftSettings: swiftV6
        ),
        .testTarget(
            name: "MediaWorkbenchKitTests",
            dependencies: ["MediaWorkbenchKit"],
            swiftSettings: swiftV6
        )
    ]
)
