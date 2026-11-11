import SwiftUI

@main
struct PergamentApp: App {
    private let core = Core()

    var body: some Scene {
        WindowGroup {
            WebShell(core: core)
                .ignoresSafeArea()
        }
    }
}
