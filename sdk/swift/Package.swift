// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "OLGP",
    platforms: [.macOS(.v12), .iOS(.v15)],
    products: [
        .library(name: "OLGP", targets: ["OLGP"])
    ],
    targets: [
        .target(name: "OLGP"),
        .testTarget(name: "OLGPTests", dependencies: ["OLGP"])
    ]
)
