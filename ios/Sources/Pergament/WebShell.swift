import CPergament
import SwiftUI
import WebKit

/// The scheme the UI is served on. A custom scheme, rather than file://, gives
/// the page one origin so ES modules and absolute `/assets/...` paths work.
let appScheme = "pergament"

/// A full-screen web view showing the frontend from the bundled `web/` folder.
struct WebShell: UIViewRepresentable {
    let core: Core

    func makeUIView(context: Context) -> WKWebView {
        let config = WKWebViewConfiguration()
        config.setURLSchemeHandler(BundleSchemeHandler(), forURLScheme: appScheme)
        let media = MediaSchemeHandler(core: core.handle)
        config.setURLSchemeHandler(media, forURLScheme: String(cString: pergament_media_scheme()))
        config.setURLSchemeHandler(media, forURLScheme: MediaSchemeHandler.assetScheme)
        config.userContentController.add(core, name: Core.messageHandler)
        // Runs before the shim, which reads these globals.
        config.userContentController.addUserScript(
            WKUserScript(source: nativeGlobals(), injectionTime: .atDocumentStart, forMainFrameOnly: true))
        if let shim = Bundle.module.url(forResource: "tauri-shim", withExtension: "js"),
            let source = try? String(contentsOf: shim, encoding: .utf8)
        {
            config.userContentController.addUserScript(
                WKUserScript(source: source, injectionTime: .atDocumentStart, forMainFrameOnly: true))
        }

        let web = WKWebView(frame: .zero, configuration: config)
        web.scrollView.contentInsetAdjustmentBehavior = .never
        web.scrollView.bounces = false
        web.isInspectable = true
        core.attach(web)
        web.load(URLRequest(url: URL(string: "\(appScheme)://localhost/index.html")!))
        return web
    }

    private func nativeGlobals() -> String {
        let device = UIDevice.current.userInterfaceIdiom == .pad ? "ipad" : "iphone"
        return """
            window.__PERGAMENT_DEVICE__ = \(jsString(device));
            window.__PERGAMENT_VERSION__ = \(jsString(core.version));
            """
    }

    func updateUIView(_ uiView: WKWebView, context: Context) {}
}

/// Serves `pergament://localhost/<path>` from the `web/` resource folder.
final class BundleSchemeHandler: NSObject, WKURLSchemeHandler {
    private let root = Bundle.module.resourceURL!.appendingPathComponent("web", isDirectory: true)

    func webView(_ webView: WKWebView, start task: WKURLSchemeTask) {
        guard let url = task.request.url else {
            // WebKit waits on a task that never finishes; fail it so the page sees an error instead.
            task.didFailWithError(URLError(.badURL))
            return
        }
        var path = url.path
        if path.isEmpty || path == "/" { path = "/index.html" }
        let file = root.appendingPathComponent(path).standardizedFileURL
        guard file.path.hasPrefix(root.standardizedFileURL.path), let data = try? Data(contentsOf: file) else {
            task.didFailWithError(URLError(.fileDoesNotExist))
            return
        }
        let headers = [
            "Content-Type": mimeType(for: file.pathExtension),
            "Content-Length": String(data.count),
        ]
        let response = HTTPURLResponse(url: url, statusCode: 200, httpVersion: "HTTP/1.1", headerFields: headers)!
        task.didReceive(response)
        task.didReceive(data)
        task.didFinish()
    }

    func webView(_ webView: WKWebView, stop task: WKURLSchemeTask) {}

    private func mimeType(for ext: String) -> String {
        switch ext.lowercased() {
        case "html": return "text/html; charset=utf-8"
        case "js", "mjs": return "text/javascript; charset=utf-8"
        case "css": return "text/css; charset=utf-8"
        case "json": return "application/json"
        case "svg": return "image/svg+xml"
        case "png": return "image/png"
        case "jpg", "jpeg": return "image/jpeg"
        case "webp": return "image/webp"
        case "ico": return "image/x-icon"
        case "woff": return "font/woff"
        case "woff2": return "font/woff2"
        case "ttf": return "font/ttf"
        default: return "application/octet-stream"
        }
    }
}
