import SwiftUI

@main
struct PergamentApp: App {
    private let core = Core()
    @Environment(\.scenePhase) private var scenePhase

    var body: some Scene {
        WindowGroup {
            WebShell(core: core)
                .ignoresSafeArea()
        }
        // iOS may end a backgrounded app without warning, so every trip there is a backup.
        .onChange(of: scenePhase) { _, phase in
            if phase == .background { core.backup() }
        }
    }
}
