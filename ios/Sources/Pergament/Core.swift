import CPergament
import Foundation
import UIKit
import WebKit

/// The Rust core for the app's lifetime. Receives `{id, name, args}` from the
/// shim through the `pergament` script message handler and answers through
/// `window.__pergament_reply`; core events go to `window.__pergament_event`.
/// Commands that need UIKit go to `Native` instead.
@MainActor
final class Core: NSObject, WKScriptMessageHandler {
    static let messageHandler = "pergament"
    /// Hands commands to the core in the order they arrive, which the core keeps for the commands
    /// desktop runs synchronously. Serial is cheap here: `pergament_invoke` only queues the work.
    private static let commands = DispatchQueue(label: "pergament.commands", qos: .userInitiated)

    nonisolated let version = String(cString: pergament_ios_version())
    private let bridge: JSBridge
    private let opened: Result<CoreHandle, OpenError>
    private lazy var native = Native { [unowned self] name, args in await self.call(name, args: args) }
    /// Ids of the core calls Native makes itself; they start above every id the shim uses (< 2^53).
    private var nextNativeCall: UInt64 = 1 << 53

    override init() {
        let bridge = JSBridge()
        self.bridge = bridge
        // The core keeps calling back through ctx until the process exits, so this
        // reference is retained once and never released.
        let ctx = Unmanaged.passRetained(bridge).toOpaque()
        opened = Self.open(ctx: ctx)
        super.init()
        if case .failure(let error) = opened { NSLog("[pergament] core failed to open: %@", error.message) }
    }

    private static func open(ctx: UnsafeMutableRawPointer) -> Result<CoreHandle, OpenError> {
        let files = FileManager.default
        do {
            let support = try files.url(for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
            let caches = try files.url(for: .cachesDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
            let library = support.appendingPathComponent("Pergament/library", isDirectory: true)
            let cache = caches.appendingPathComponent("Pergament", isDirectory: true)
            try files.createDirectory(at: library, withIntermediateDirectories: true)
            try files.createDirectory(at: cache, withIntermediateDirectories: true)

            var errorOut: UnsafePointer<CChar>?
            let handle = pergament_open(library.path, cache.path, ctx, onReply, onEvent, &errorOut)
            if let handle { return .success(CoreHandle(raw: handle)) }
            let message = errorOut.map { String(cString: $0) } ?? "the core failed to open"
            pergament_string_free(errorOut)
            return .failure(OpenError(message: message))
        } catch {
            return .failure(OpenError(message: error.localizedDescription))
        }
    }

    /// The open core, or nil if it failed to open.
    var handle: CoreHandle? {
        if case .success(let handle) = opened { handle } else { nil }
    }

    /// Where replies and events go. Held weakly; the web view owns this handler through its configuration.
    func attach(_ webView: WKWebView) {
        bridge.webView = webView
        native.webView = webView
    }

    /// Calls the core and waits for its reply, which goes here instead of to the page.
    func call(_ name: String, args: String) async -> NativeReply {
        guard case .success(let handle) = opened else {
            if case .failure(let error) = opened { return .error(error.message) }
            return .error("the core failed to open")
        }
        let id = nextNativeCall
        nextNativeCall += 1
        return await withCheckedContinuation { continuation in
            bridge.waiting[id] = continuation
            Self.commands.async { pergament_invoke(handle.raw, id, name, args) }
        }
    }

    /// Writes a rolling backup off the main thread, asking iOS for the time to finish it
    /// before the app is suspended.
    func backup() {
        guard let handle else { return }
        let time = BackgroundTime(name: "pergament backup")
        DispatchQueue.global(qos: .utility).async {
            pergament_backup(handle.raw)
            DispatchQueue.main.async { MainActor.assumeIsolated { time.end() } }
        }
    }

    func userContentController(_ controller: WKUserContentController, didReceive message: WKScriptMessage) {
        guard let body = message.body as? [String: Any], let id = (body["id"] as? NSNumber)?.uint64Value else {
            NSLog("[pergament] dropped a message without an id")
            return
        }
        guard let name = body["name"] as? String, let args = body["args"] as? String else {
            bridge.reply(id: id, ok: 0, json: jsString("malformed command message"))
            return
        }
        if Native.commands.contains(name) {
            Task {
                let reply = await native.run(name, args: args)
                bridge.reply(id: id, ok: reply.ok ? 1 : 0, json: reply.json)
            }
            return
        }
        switch opened {
        case .failure(let error):
            bridge.reply(id: id, ok: 0, json: jsString(error.message))
        case .success(let handle):
            Self.commands.async { pergament_invoke(handle.raw, id, name, args) }
        }
    }
}

private struct OpenError: Error {
    let message: String
}

// The header makes the handle usable from any thread; the raw pointer type just can't say so.
struct CoreHandle: @unchecked Sendable {
    let raw: UnsafeMutableRawPointer
}

/// Ends the background task when the work is done or iOS runs out of patience, whichever is first.
@MainActor
private final class BackgroundTime {
    private var id = UIBackgroundTaskIdentifier.invalid

    init(name: String) {
        id = UIApplication.shared.beginBackgroundTask(withName: name) { [weak self] in
            MainActor.assumeIsolated { self?.end() }
        }
    }

    func end() {
        guard id != .invalid else { return }
        UIApplication.shared.endBackgroundTask(id)
        id = .invalid
    }
}

/// Calls into the page. Callbacks arrive on any thread and hop to the main queue,
/// which keeps replies and events in the order the core sent them.
@MainActor
private final class JSBridge {
    weak var webView: WKWebView?
    /// Replies to Native's own core calls, by id; these never reach the page.
    var waiting: [UInt64: CheckedContinuation<NativeReply, Never>] = [:]

    nonisolated func reply(id: UInt64, ok: Int32, json: String) {
        DispatchQueue.main.async { MainActor.assumeIsolated { self.deliver(id: id, ok: ok, json: json) } }
    }

    private func deliver(id: UInt64, ok: Int32, json: String) {
        if let waiter = waiting.removeValue(forKey: id) {
            waiter.resume(returning: NativeReply(ok: ok != 0, json: json))
            return
        }
        run("window.__pergament_reply(\(id), \(ok), \(jsString(json)))")
    }

    nonisolated func event(_ name: String, json: String) {
        let script = "window.__pergament_event(\(jsString(name)), \(jsString(json)))"
        DispatchQueue.main.async { MainActor.assumeIsolated { self.run(script) } }
    }

    private func run(_ script: String) {
        webView?.evaluateJavaScript(script, completionHandler: nil)
    }
}

/// `text` as a JS string literal, encoded as JSON (which is valid JS since ES2019,
/// U+2028 included) so quotes, backslashes and non-ASCII text survive. Encoding a
/// String cannot fail.
func jsString(_ text: String) -> String {
    let data = try! JSONSerialization.data(withJSONObject: text, options: .fragmentsAllowed)
    return String(decoding: data, as: UTF8.self)
}

private func bridge(_ ctx: UnsafeMutableRawPointer?) -> JSBridge? {
    ctx.map { Unmanaged<JSBridge>.fromOpaque($0).takeUnretainedValue() }
}

private func onReply(ctx: UnsafeMutableRawPointer?, id: UInt64, ok: Int32, json: UnsafePointer<CChar>?) {
    let json = json.map { String(cString: $0) } ?? "null"
    bridge(ctx)?.reply(id: id, ok: ok, json: json)
}

private func onEvent(ctx: UnsafeMutableRawPointer?, name: UnsafePointer<CChar>?, json: UnsafePointer<CChar>?) {
    guard let name else { return }
    let event = String(cString: name)
    let json = json.map { String(cString: $0) } ?? "null"
    bridge(ctx)?.event(event, json: json)
}
