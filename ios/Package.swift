// swift-tools-version: 6.0

import PackageDescription

// The iOS shell, built with xtool (https://xtool.sh). It shows the web UI from
// dist/, copied in by sync-web.sh, and runs the Rust core (ios/core) linked in
// as the static library build-core.sh puts in lib/.
let package = Package(
    name: "Pergament",
    platforms: [
        .iOS(.v17)
    ],
    products: [
        // An xtool project contains exactly one library product, the app.
        .library(
            name: "Pergament",
            targets: ["Pergament"]
        )
    ],
    targets: [
        // The C ABI from core/include/pergament_ios.h; the module map links libpergament_ios.
        .systemLibrary(
            name: "CPergament",
            path: "core/include"
        ),
        .target(
            name: "Pergament",
            dependencies: ["CPergament"],
            resources: [
                .copy("Resources/tauri-shim.js"),
                .copy("web"),
            ],
            linkerSettings: [
                // The linker runs outside the package directory, so the search path must be absolute.
                .unsafeFlags(["-L\(Context.packageDirectory)/lib"]),
                .linkedLibrary("sqlite3"),
            ]
        ),
    ]
)
