import SwiftUI
import MediaWorkbenchKit

@main
struct MediaWorkbenchApp: App {
    var body: some Scene {
        WindowGroup("Media Workbench") {
            RootView()
        }
        .defaultSize(width: 1080, height: 700)
        .commands {
            SidebarCommands()
        }
    }
}
