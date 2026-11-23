// swift-tools-version:5.9
import PackageDescription
import Foundation

// libengram_core.a and engram_core.h are produced by ../build.sh into Vendor/.
let packageRoot = URL(fileURLWithPath: #filePath).deletingLastPathComponent().path
let vendorLib = "\(packageRoot)/Vendor/lib"

let package = Package(
    name: "Engram",
    platforms: [.macOS(.v14)],
    products: [
        .executable(name: "Engram", targets: ["Engram"])
    ],
    targets: [
        .systemLibrary(
            name: "CEngramCore",
            path: "Sources/CEngramCore"
        ),
        .executableTarget(
            name: "Engram",
            dependencies: ["CEngramCore"],
            path: "Sources/Engram",
            linkerSettings: [
                .unsafeFlags(["-L", vendorLib])
            ]
        ),
        .testTarget(
            name: "EngramTests",
            dependencies: ["Engram"],
            path: "Tests/EngramTests",
            linkerSettings: [
                .unsafeFlags(["-L", vendorLib])
            ]
        ),
    ]
)
