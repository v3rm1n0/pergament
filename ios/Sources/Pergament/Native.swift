import UIKit
import UniformTypeIdentifiers
import WebKit

/// What a command answers with: `json` is the value, or the error string when `ok` is false.
struct NativeReply: Sendable {
    let ok: Bool
    let json: String

    static let null = NativeReply(ok: true, json: "null")

    static func value(_ value: some Encodable) -> NativeReply {
        // Encoding strings and arrays of strings cannot fail.
        let data = try! JSONEncoder().encode(value)
        return NativeReply(ok: true, json: String(decoding: data, as: UTF8.self))
    }

    static func error(_ message: String) -> NativeReply {
        NativeReply(ok: false, json: jsString(message))
    }
}

private struct NativeError: Error {
    let message: String
}

/// The commands that need UIKit, answered here before the core sees them: links,
/// the dialog plugin's pickers and alerts, and import, export and restore.
///
/// Pickers and the save dialog hand the UI paths under `Caches/Pergament/`,
/// which the core can read and write; the UI then passes them back to
/// `import_files`, `export_backup` or `restore_backup` as on desktop.
@MainActor
final class Native {
    static let commands: Set<String> = [
        "open_external", "import_files", "export_backup", "restore_backup",
        "plugin:dialog|open", "plugin:dialog|save", "plugin:dialog|message",
    ]

    weak var webView: WKWebView?
    private let core: (String, String) async -> NativeReply
    private let importDir: URL
    private let exportDir: URL
    /// The delegate of the picker or alert on screen, which UIKit holds only weakly.
    private var presenting: AnyObject?

    init(core: @escaping (String, String) async -> NativeReply) {
        self.core = core
        let caches = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("Pergament", isDirectory: true)
        importDir = caches.appendingPathComponent("import", isDirectory: true)
        exportDir = caches.appendingPathComponent("export", isDirectory: true)
    }

    func run(_ name: String, args: String) async -> NativeReply {
        do {
            switch name {
            case "open_external": return try await openExternal(decode(UrlArgs.self, args).url)
            case "plugin:dialog|open":
                let options = try decode(OpenArgs.self, args)
                let multiple = options.multiple ?? false
                guard let paths = try await pick(options.extensions ?? [], multiple: multiple) else { return .null }
                return multiple ? .value(paths) : .value(paths[0])
            case "plugin:dialog|save":
                let options = try decode(SaveArgs.self, args)
                return .value(try exportPath(options.defaultPath, extensions: options.extensions ?? []).path)
            case "plugin:dialog|message": return try await message(decode(MessageArgs.self, args))
            case "import_files":
                if (try decodeIfPresent(PathsArgs.self, args))?.paths != nil { return await core(name, args) }
                let paths = try await pick(["jwpub"], multiple: true) ?? []
                if paths.isEmpty { return .value([String]()) }
                return await core(name, encoded(PathsArgs(paths: paths)))
            case "export_backup": return try await exportBackup(decodeIfPresent(PathArg.self, args)?.path)
            case "restore_backup":
                if (try decodeIfPresent(PathArg.self, args))?.path != nil { return await core(name, args) }
                guard let path = try await pick(["jwlibrary"], multiple: false)?.first else {
                    return .error("no backup was chosen")
                }
                return await core(name, encoded(PathArg(path: path)))
            default: return .error("unknown command: \(name)")
            }
        } catch let error as NativeError {
            return .error(error.message)
        } catch {
            return .error(error.localizedDescription)
        }
    }

    // Desktop opens only http(s) links (Link::External); anything else is refused the same way.
    private func openExternal(_ url: String) async throws -> NativeReply {
        let trimmed = url.trimmingCharacters(in: .whitespacesAndNewlines)
        guard trimmed.hasPrefix("https://") || trimmed.hasPrefix("http://"), let target = URL(string: trimmed) else {
            throw NativeError(message: "refusing to open \(jsString(url))")
        }
        guard await UIApplication.shared.open(target) else { throw NativeError(message: "cannot open \(trimmed)") }
        return .null
    }

    private func exportBackup(_ path: String?) async throws -> NativeReply {
        let file = try path.map { URL(fileURLWithPath: $0) } ?? exportPath(nil, extensions: ["jwlibrary"])
        let written = await core("export_backup", encoded(PathArg(path: file.path)))
        guard written.ok else { return written }
        try await share(file)
        return .null
    }

    /// A fresh file name in the export folder, which is emptied first: nothing in it outlives one export.
    private func exportPath(_ defaultPath: String?, extensions: [String]) throws -> URL {
        let files = FileManager.default
        try? files.removeItem(at: exportDir)
        try files.createDirectory(at: exportDir, withIntermediateDirectories: true)
        var name = defaultPath.map { URL(fileURLWithPath: $0).lastPathComponent } ?? ""
        if name.isEmpty || name == "/" { name = "Pergament" + (extensions.first.map { ".\($0)" } ?? "") }
        return exportDir.appendingPathComponent(name)
    }

    /// The paths of the picked files, copied into the import folder, or nil when the picker was cancelled.
    private func pick(_ extensions: [String], multiple: Bool) async throws -> [String]? {
        let types = extensions.compactMap { UTType(filenameExtension: $0) }
        // asCopy has the system download a file that is still in iCloud and copy it into tmp/.
        let picker = UIDocumentPickerViewController(forOpeningContentTypes: types.isEmpty ? [.item] : types, asCopy: true)
        picker.allowsMultipleSelection = multiple
        let urls: [URL]? = try await present(picker) { finish in
            let delegate = PickerDelegate(finish)
            picker.delegate = delegate
            picker.presentationController?.delegate = delegate
            return delegate
        }
        guard let urls, !urls.isEmpty else { return nil }
        return try copyIn(urls)
    }

    private func copyIn(_ urls: [URL]) throws -> [String] {
        let files = FileManager.default
        try? files.removeItem(at: importDir)
        try files.createDirectory(at: importDir, withIntermediateDirectories: true)
        return try urls.map { url in
            let scoped = url.startAccessingSecurityScopedResource()
            defer { if scoped { url.stopAccessingSecurityScopedResource() } }
            let target = importDir.appendingPathComponent(url.lastPathComponent)
            try? files.removeItem(at: target)
            try files.copyItem(at: url, to: target)
            return target.path
        }
    }

    private func message(_ args: MessageArgs) async throws -> NativeReply {
        let alert = UIAlertController(title: args.title, message: args.message, preferredStyle: .alert)
        let buttons = args.buttons.flatMap { $0.isEmpty ? nil : $0 } ?? ["Ok"]
        let cancels = args.cancel ?? false
        let label: String = try await present(alert) { finish in
            let delegate = AlertAnswer(finish)
            for (i, title) in buttons.enumerated() {
                let style: UIAlertAction.Style = cancels && i == buttons.count - 1 ? .cancel : .default
                alert.addAction(UIAlertAction(title: title, style: style) { _ in delegate.finish(title) })
            }
            return delegate
        }
        return .value(label)
    }

    private func share(_ file: URL) async throws {
        let host = try await settledPresenter()
        let sheet = UIActivityViewController(activityItems: [file], applicationActivities: nil)
        // iPad shows the sheet as a popover, which UIKit refuses to present without an anchor.
        if let popover = sheet.popoverPresentationController {
            popover.sourceView = host.view
            popover.sourceRect = CGRect(x: host.view.bounds.midX, y: host.view.bounds.midY, width: 0, height: 0)
            popover.permittedArrowDirections = []
        }
        // Only a file this class made is removed; a path the page passed in stays where it is.
        let ours = file.standardizedFileURL.path.hasPrefix(exportDir.standardizedFileURL.path + "/")
        sheet.completionWithItemsHandler = { _, _, _, _ in
            if ours { try? FileManager.default.removeItem(at: file) }
        }
        host.present(sheet, animated: true)
        if sheet.presentingViewController == nil { throw NativeError(message: "could not show the share sheet") }
    }

    /// Presents `controller` and waits for the answer its delegate (made by `wire`) gives.
    private func present<T: Sendable>(
        _ controller: UIViewController, wire: (@escaping (T) -> Void) -> AnyObject
    ) async throws -> T {
        guard presenting == nil else { throw NativeError(message: "another dialog is still open") }
        // Held while waiting below, so a second dialog cannot slip in.
        presenting = controller
        defer { presenting = nil }
        let host = try await settledPresenter()
        return try await withCheckedThrowingContinuation { continuation in
            var resumed = false
            func resume(_ result: Result<T, NativeError>) {
                guard !resumed else { return }
                resumed = true
                continuation.resume(with: result)
            }
            presenting = wire { resume(.success($0)) }
            host.present(controller, animated: true)
            // UIKit refuses a presentation with only a log line; without this the answer would never come.
            if controller.presentingViewController == nil {
                resume(.failure(NativeError(message: "could not show the dialog")))
            }
        }
    }

    /// The presenter once no presentation or dismissal is running on it. The UI opens an alert
    /// right after a picker answers, while the picker is still animating away, and UIKit refuses
    /// to present during that. Waits at most about a second, then tries anyway.
    private func settledPresenter() async throws -> UIViewController {
        for _ in 0..<20 {
            let host = try presenter()
            if host.transitionCoordinator == nil, host.presentedViewController?.isBeingDismissed != true {
                return host
            }
            try? await Task.sleep(for: .milliseconds(50))
        }
        return try presenter()
    }

    private func presenter() throws -> UIViewController {
        guard var top = webView?.window?.rootViewController else {
            throw NativeError(message: "no window to show the dialog in")
        }
        while let next = top.presentedViewController, !next.isBeingDismissed { top = next }
        return top
    }

    private func decode<T: Decodable>(_ type: T.Type, _ args: String) throws -> T {
        guard let value = try decodeIfPresent(type, args) else { throw NativeError(message: "missing arguments") }
        return value
    }

    private func decodeIfPresent<T: Decodable>(_ type: T.Type, _ args: String) throws -> T? {
        do {
            return try JSONDecoder().decode(T?.self, from: Data(args.utf8))
        } catch {
            throw NativeError(message: "invalid arguments: \(error.localizedDescription)")
        }
    }

    private func encoded(_ value: some Encodable) -> String {
        String(decoding: try! JSONEncoder().encode(value), as: UTF8.self)
    }
}

private struct UrlArgs: Decodable {
    let url: String
}

// The shim sends these shapes (tauri-shim.js); synthesized Decodable ignores default values, hence the optionals.
private struct OpenArgs: Decodable {
    let multiple: Bool?
    let extensions: [String]?
}

private struct SaveArgs: Decodable {
    let defaultPath: String?
    let extensions: [String]?
}

private struct MessageArgs: Decodable {
    let message: String
    let title: String?
    let buttons: [String]?
    let cancel: Bool?
}

private struct PathsArgs: Codable {
    let paths: [String]?
}

private struct PathArg: Codable {
    let path: String?
}

@MainActor
private final class PickerDelegate: NSObject, UIDocumentPickerDelegate, UIAdaptivePresentationControllerDelegate {
    private let finish: ([URL]?) -> Void

    init(_ finish: @escaping ([URL]?) -> Void) {
        self.finish = finish
    }

    func documentPicker(_ controller: UIDocumentPickerViewController, didPickDocumentsAt urls: [URL]) {
        finish(urls)
    }

    func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) {
        finish(nil)
    }

    // A swipe down on the sheet dismisses it without calling documentPickerWasCancelled.
    func presentationControllerDidDismiss(_ presentationController: UIPresentationController) {
        finish(nil)
    }
}

@MainActor
private final class AlertAnswer {
    let finish: (String) -> Void

    init(_ finish: @escaping (String) -> Void) {
        self.finish = finish
    }
}
