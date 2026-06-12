// swift-tools-version: 5.9
//
// TrUAPI iOS host package.
//
// The `truapi_serverFFI` target wraps the UniFFI-generated C header + module
// map so the generated Swift bindings can `import truapi_serverFFI`. The
// `TrUAPIHost` target contains both the generated Swift bindings and the
// thin host shell defined in `TrUAPIHost.swift`.
//
// Consumers must link a prebuilt `libtruapi_server` static or dynamic
// library when integrating into their app target. This package does not
// vendor the binary itself; see README.md for build instructions.

import PackageDescription

let package = Package(
    name: "TrUAPIHost",
    platforms: [.iOS(.v16), .macOS(.v13)],
    products: [
        .library(name: "TrUAPIHost", targets: ["TrUAPIHost"]),
    ],
    targets: [
        .systemLibrary(
            name: "truapi_serverFFI",
            path: "Sources/truapi_serverFFI",
            pkgConfig: nil,
            providers: []
        ),
        .target(
            name: "TrUAPIHost",
            dependencies: ["truapi_serverFFI"],
            path: "Sources/TrUAPIHost"
        ),
    ]
)
