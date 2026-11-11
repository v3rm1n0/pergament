import CPergament
import Foundation
import WebKit

/// Serves pictures (`jwmedia://localhost/<path>`) and media folder files
/// (`asset://localhost/<encoded absolute path>`) from the core, with byte ranges.
///
/// WebKit raises an Objective-C exception when a task is used after it called
/// `stop` for it, so every task call happens on the main actor, after checking
/// that the task is still in `live`.
@MainActor
final class MediaSchemeHandler: NSObject, WKURLSchemeHandler {
    static let assetScheme = "asset"
    nonisolated private static let chunk = 256 * 1024

    private let core: CoreHandle?
    private var live = Set<ObjectIdentifier>()

    init(core: CoreHandle?) {
        self.core = core
    }

    func webView(_ webView: WKWebView, start task: WKURLSchemeTask) {
        guard let url = task.request.url, let scheme = url.scheme, let core else {
            task.didFailWithError(URLError(.cannotConnectToHost))
            return
        }
        // The core decodes the path itself, so it gets the URL's own spelling.
        let path = URLComponents(url: url, resolvingAgainstBaseURL: false)?.percentEncodedPath ?? url.path
        let range = task.request.value(forHTTPHeaderField: "Range")
        live.insert(ObjectIdentifier(task))
        let job = Job(task: task, url: url, scheme: scheme, path: path, range: range)
        DispatchQueue.global(qos: .userInitiated).async {
            self.serve(job, core: core)
        }
    }

    func webView(_ webView: WKWebView, stop task: WKURLSchemeTask) {
        live.remove(ObjectIdentifier(task))
    }

    /// Runs on a worker queue. The task stays retained by `job` until this returns, so its
    /// identifier cannot be reused by a new task while it is still in `live`.
    nonisolated private func serve(_ job: Job, core: CoreHandle) {
        guard let media = pergament_media_open(core.raw, job.scheme, job.path, job.range) else {
            onMain(job) { $0.didFailWithError(URLError(.unsupportedURL)); return false }
            return
        }
        defer { pergament_media_free(media) }

        let status = Int(pergament_media_status(media))
        var headers = Self.headers(pergament_media_headers(media))
        // Pictures are fetched from the pergament:// page and captions load through <track>.
        headers["Access-Control-Allow-Origin"] = "*"
        let sent = onMain(job) { task in
            guard
                let response = HTTPURLResponse(
                    url: job.url, statusCode: status, httpVersion: "HTTP/1.1", headerFields: headers)
            else {
                task.didFailWithError(URLError(.badServerResponse))
                return false
            }
            task.didReceive(response)
            return true
        }
        guard sent else { return }

        var buffer = [UInt8](repeating: 0, count: Self.chunk)
        while true {
            let n = buffer.withUnsafeMutableBufferPointer { pergament_media_read(media, $0.baseAddress, $0.count) }
            if n < 0 {
                onMain(job) { $0.didFailWithError(URLError(.cannotDecodeContentData)); return false }
                return
            }
            if n == 0 {
                onMain(job) { $0.didFinish(); return false }
                return
            }
            let data = Data(buffer[0..<n])
            guard onMain(job, { $0.didReceive(data); return true }) else { return }
        }
    }

    /// Runs `body` on the main thread if the task was not stopped, and waits for it. `body`
    /// returns whether the task stays open; when it does not, or the task was stopped, it is
    /// dropped from `live` and this returns false.
    @discardableResult
    nonisolated private func onMain(_ job: Job, _ body: @MainActor (WKURLSchemeTask) -> Bool) -> Bool {
        DispatchQueue.main.sync {
            MainActor.assumeIsolated {
                let id = ObjectIdentifier(job.task)
                guard live.contains(id) else { return false }
                if body(job.task) { return true }
                live.remove(id)
                return false
            }
        }
    }

    nonisolated private static func headers(_ json: UnsafePointer<CChar>?) -> [String: String] {
        guard let json,
            let object = try? JSONSerialization.jsonObject(with: Data(String(cString: json).utf8)),
            let headers = object as? [String: String]
        else { return [:] }
        return headers
    }
}

/// One request, carried to the worker. The task is only touched on the main thread.
private struct Job: @unchecked Sendable {
    let task: WKURLSchemeTask
    let url: URL
    let scheme: String
    let path: String
    let range: String?
}
