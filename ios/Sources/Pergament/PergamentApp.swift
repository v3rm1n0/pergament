import SwiftUI

@main
struct PergamentApp: App {
    var body: some Scene {
        WindowGroup {
            WebShell()
                .ignoresSafeArea()
        }
    }
}
