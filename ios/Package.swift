// swift-tools-version: 6.0

import PackageDescription

// The iOS shell, built with xtool (https://xtool.sh). It has no Rust core yet:
// it only shows the web UI from dist/, copied in by sync-web.sh.
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
        .target(
            name: "Pergament",
            resources: [
                .copy("Resources/tauri-shim.js"),
                .copy("web"),
            ]
        )
    ]
)
