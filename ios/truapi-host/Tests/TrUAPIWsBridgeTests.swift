import Foundation
import Testing
import TrUAPIHost

struct TrUAPIWsBridgeTests {
    @Test(.timeLimit(.minutes(1)))
    func testFeatureSupportedRoundTripOverWsBridge() async throws {
        let core = try TrUAPIHostCore(
            callbacks: StubHostCallbacks(),
            runtimeConfig: Self.makeRuntimeConfig()
        )

        let endpoint = try core.startWsBridge(bindPort: 0)
        defer { core.stopWsBridge() }

        let url = try #require(URL(string: "ws://127.0.0.1:\(endpoint.port)/?t=\(endpoint.token)"))
        let task = URLSession.shared.webSocketTask(with: url)
        task.resume()
        defer { task.cancel(with: .normalClosure, reason: nil) }

        try await task.send(.data(Self.featureSupportedRequestFrame()))
        let message = try await task.receive()

        guard case let .data(response) = message else {
            Issue.record("expected binary frame, got \(message)")
            return
        }
        // Frame tail is the SCALE Result payload: Ok(0x00), V1(0x00), supported(0x01).
        #expect(response.suffix(3) == Data([0x00, 0x00, 0x01]))
    }
}

private extension TrUAPIWsBridgeTests {
    static func makeRuntimeConfig() -> RuntimeConfig {
        RuntimeConfig(
            productId: "test.dot",
            hostName: "truapi-host-tests",
            peopleChainGenesisHash: Data(repeating: 0, count: 32),
            bulletinChainGenesisHash: Data(repeating: 0, count: 32)
        )
    }

    // wire_table.rs: SYSTEM_FEATURE_SUPPORTED.request_id = 2
    static let featureSupportedRequestDiscriminant = Data([0x02])

    static func featureSupportedRequestFrame() -> Data {
        var frame = Data()
        frame.append(contentsOf: [0x0C]) // compact length 3
        frame.append("p:1".data(using: .utf8)!)
        frame.append(featureSupportedRequestDiscriminant) // from wire_table.rs
        frame.append(contentsOf: [0x00, 0x00, 0x80]) // V1, Chain, compact(32)
        frame.append(Data(repeating: 0, count: 32))
        return frame
    }
}

final class StubHostCallbacks: HostCallbacks, @unchecked Sendable {
    private var localStore: [String: Data] = [:]
    private var coreStore: [Data: Data] = [:]

    func onCoreLog(marker _: String, detail _: String) {}
    func navigateTo(url _: String) async throws {}
    func pushNotification(request _: HostPushNotificationRequest) async throws -> UInt32 { 0 }
    func cancelNotification(id _: UInt32) throws {}
    func devicePermission(request _: HostDevicePermissionRequest) async throws -> Bool { false }
    func remotePermission(request _: RemotePermission) async throws -> Bool { false }
    func authStateChanged(state _: AuthState) {}
    func pairingPeerDisconnected(peer _: NativePairingPeer) {}
    func coreStorageRead(key: Data) throws -> Data? { coreStore[key] }
    func coreStorageWrite(key: Data, value: Data) throws { coreStore[key] = value }
    func coreStorageClear(key: Data) throws { coreStore[key] = nil }
    func chainConnect(genesisHash _: Data) throws -> UInt32? { nil }
    func chainSend(connectionId _: UInt32, request _: String) throws {}
    func chainClose(connectionId _: UInt32) throws {}
    func confirmUserAction(review _: UserConfirmationReview) async throws -> Bool { false }
    func lookupPreimage(key _: Data) async throws -> Data? { nil }
    func currentTheme() throws -> ThemeVariant { .dark }
    func featureSupported(request _: HostFeatureSupportedRequest) async throws -> Bool { true }
    func localStorageRead(key: String) throws -> Data? { localStore[key] }
    func localStorageWrite(key: String, value: Data) throws { localStore[key] = value }
    func localStorageClear(key: String) throws { localStore[key] = nil }
}
