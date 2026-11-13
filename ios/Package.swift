// swift-tools-version: 6.0

import Foundation
import PackageDescription

// On Linux swiftc links through clang with --sysroot, and clang reads the SDK's version only
// from -isysroot. Without it the binary is stamped as built with the deployment target's SDK
// (17.0), and iOS, which reads that stamp, gives the app the compatibility mode of an old SDK:
// black bars on screens that came later. The same variable and SDK as in build-core.sh.
let sdk =
    Context.environment["PERGAMENT_IOS_SDK"]
    ?? "\(NSHomeDirectory())/.config/swiftpm/swift-sdks/darwin.artifactbundle/Developer/Platforms/iPhoneOS.platform/Developer/SDKs/iPhoneOS.sdk"
// Xcode stamps the SDK itself, and has no SDK at this path.
let sdkStamp =
    FileManager.default.fileExists(atPath: "\(sdk)/SDKSettings.json")
    ? ["-Xclang-linker", "-isysroot", "-Xclang-linker", sdk] : []

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
                .unsafeFlags(["-L\(Context.packageDirectory)/lib"] + sdkStamp),
                .linkedLibrary("sqlite3"),
            ]
        ),
    ]
)
