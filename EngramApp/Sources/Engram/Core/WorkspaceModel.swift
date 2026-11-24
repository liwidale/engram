import AppKit
import Observation
import SwiftUI

enum Module: String, CaseIterable, Identifiable, Hashable {
    case memory, architecture, decisions, prompts, context, health, handoff
    var id: String { rawValue }

    var title: String {
        switch self {
        case .memory: String(localized: "Project memory")
        case .architecture: String(localized: "Architecture map")
        case .decisions: String(localized: "Decision log")
        case .prompts: String(localized: "Prompt vault")
        case .context: String(localized: "Context builder")
        case .health: String(localized: "Context health")
        case .handoff: String(localized: "Agent handoff")
        }
    }

    var symbol: String {
        switch self {
        case .memory: "brain"
        case .architecture: "point.3.connected.trianglepath.dotted"
        case .decisions: "signpost.right.and.left"
        case .prompts: "text.quote"
        case .context: "square.stack.3d.down.right"
        case .health: "stethoscope"
        case .handoff: "arrow.triangle.turn.up.right.diamond"
        }
    }

    var shortcut: KeyEquivalent { KeyEquivalent(Character(String(Module.allCases.firstIndex(of: self)! + 1))) }
}

enum Intent: Equatable {
    case newMemory, newDecision, newPrompt, newNode, focusTask, bootstrap
}

struct Banner: Identifiable {
    enum Kind { case info, success, error }
    let id = UUID()
    var kind: Kind
    var message: String
    var action: BannerAction?
}

struct BannerAction {
    var title: String
    var run: @MainActor () -> Void
}

struct ActivityEntry: Identifiable {
    let id = UUID()
    var date: Date
    var author: String
    var external: Bool
    var diff: DocumentDiff
}

enum SyncStatus: Equatable {
    case synced
    case saving
    case updated(by: String, at: Date)
}

struct ContextBuilderState: Equatable {
    var task = ""
    var maxTokens = UserDefaults.standard.integer(forKey: Pref.contextBudget)
    var autoRank = true
    var includeHandoff = UserDefaults.standard.bool(forKey: Pref.contextHandoff)
    var markdown = UserDefaults.standard.string(forKey: Pref.contextFormat) == "markdown"
    var forced: Set<String> = []
    var excluded: Set<String> = []
    var prompt: String?
    var promptVars: [String: String] = [:]
}

@Observable
@MainActor
final class WorkspaceModel {
    let url: URL
    private(set) var engine: EngramEngine?
    private(set) var document: EngramDocument?
    private(set) var layout: GraphLayout = .empty
    var loadError: String?

    var module: Module = .memory
    var selectedMemory: String?
    var selectedDecision: String?
    var selectedPrompt: String?
    var selectedNode: String?
    var showPalette = false
    var showProjectSettings = false
    var showConnectAgent = false
    var showShortcuts = false
    var intent: Intent?
    var banner: Banner?

    private(set) var history: HistoryState = .empty
    private(set) var activity: [ActivityEntry] = []
    private(set) var sync: SyncStatus = .synced
    private(set) var pendingWrites = 0
    private(set) var externalRevision = 0

    var healthReport: HealthReport?
    var isAuditing = false
    var contextState = ContextBuilderState()

    @ObservationIgnored private let queue = DispatchQueue(label: "com.liwidale.engram.engine", qos: .userInitiated)
    @ObservationIgnored private var watcher: FileWatcher?
    @ObservationIgnored private var commitHandlers: [String: () -> Bool] = [:]
    @ObservationIgnored private var bannerTask: Task<Void, Never>?
    @ObservationIgnored private var auditAgain = false

    init(url: URL) {
        self.url = url
        open()
    }

    func open() {
        do {
            let engine = try EngramEngine.open(url)
            self.engine = engine
            loadError = nil
            let snapshot = try engine.document().document
            document = snapshot
            layout = (try? engine.layout()) ?? .empty
            NSDocumentController.shared.noteNewRecentDocumentURL(url)
            let watcher = FileWatcher(url: url) { [weak self] in self?.fileChangedOnDisk() }
            watcher.start()
            self.watcher = watcher
            runAudit()
        } catch {
            loadError = error.localizedDescription
        }
    }

    func close() {
        watcher?.stop()
        watcher = nil
    }

    var projectName: String { document?.project.name ?? url.deletingPathExtension().lastPathComponent }
    var revision: UInt64 { document?.revision ?? 0 }
    var isSaving: Bool { pendingWrites > 0 }

    private struct Fresh {
        var document: EngramDocument
        var layout: GraphLayout?
    }

    nonisolated private static func fetch(_ engine: EngramEngine, previous: Architecture?) throws -> Fresh {
        let doc = try engine.document().document
        let layout = doc.architecture == previous ? nil : try engine.layout()
        return Fresh(document: doc, layout: layout)
    }

    private func adopt(_ fresh: Fresh) {
        document = fresh.document
        if let layout = fresh.layout { self.layout = layout }
    }

    private func background<T>(_ work: @escaping @Sendable (EngramEngine, Architecture?) throws -> (T, Fresh), done: @escaping @MainActor (Result<T, Error>) -> Void) {
        guard let engine else { return }
        let architecture = document?.architecture
        pendingWrites += 1
        sync = .saving
        queue.async {
            let outcome = Result { try work(engine, architecture) }
            DispatchQueue.main.async {
                MainActor.assumeIsolated {
                    self.pendingWrites -= 1
                    if self.pendingWrites == 0, self.sync == .saving { self.sync = .synced }
                    switch outcome {
                    case .success(let (value, fresh)):
                        self.adopt(fresh)
                        done(.success(value))
                    case .failure(let e):
                        done(.failure(e))
                    }
                }
            }
        }
    }

    private func fileChangedOnDisk() {
        guard let engine else { return }
        let architecture = document?.architecture
        queue.async {
            let outcome = Result { () -> (ReloadResult, Fresh?) in
                let r = try engine.reload()
                return (r, r.changed ? try Self.fetch(engine, previous: architecture) : nil)
            }
            DispatchQueue.main.async {
                MainActor.assumeIsolated {
                    switch outcome {
                    case .success(let (r, fresh)):
                        guard let fresh else { return }
                        self.adopt(fresh)
                        self.externalRevision += 1
                        let author = r.diff?.author ?? String(localized: "another app")
                        if let diff = r.diff, !diff.changes.isEmpty {
                            self.log(diff, author: author, external: true)
                        }
                        self.sync = .updated(by: author, at: .now)
                        if Pref.notifiesExternalEdits {
                            self.show(.info, "Updated by \(author) · \(r.diff?.summary ?? String(localized: "revision \(r.revision)"))")
                        }
                        self.runAudit()
                    case .failure(let e):
                        self.show(.error, "Could not reload the file: \(e.localizedDescription)")
                    }
                }
            }
        }
    }

    func reloadFromDisk() {
        guard engine != nil else { return open() }
        fileChangedOnDisk()
    }

    private func log(_ diff: DocumentDiff, author: String, external: Bool) {
        activity.insert(ActivityEntry(date: .now, author: author, external: external, diff: diff), at: 0)
        if activity.count > 30 { activity.removeLast(activity.count - 30) }
    }

    func perform(
        _ op: EngramOp,
        success: String.LocalizationValue? = nil,
        completion: (@MainActor (JSONValue?) -> Void)? = nil
    ) {
        background({ engine, architecture in
            let r = try engine.apply(op)
            return (r, try Self.fetch(engine, previous: architecture))
        }) { outcome in
            switch outcome {
            case .success(let r):
                if let h = r.history { self.history = h }
                if let diff = r.diff, !diff.changes.isEmpty { self.log(diff, author: String(localized: "You"), external: false) }
                if let warnings = r.result["warnings"]?.strings, !warnings.isEmpty {
                    self.show(.info, verbatim: warnings.joined(separator: " · "))
                } else if let success {
                    self.show(.success, success)
                }
                completion?(r.result)
            case .failure(let e):
                self.show(.error, verbatim: e.localizedDescription)
                completion?(nil)
            }
        }
    }

    func delete(_ op: EngramOp, title: String, confirm: Bool = true, then: (@MainActor () -> Void)? = nil) {
        if confirm, Pref.confirmsDeletion {
            let alert = NSAlert()
            alert.messageText = String(localized: "Delete \(title)?")
            alert.informativeText = String(localized: "You can undo this with Edit > Undo.")
            alert.addButton(withTitle: String(localized: "Delete"))
            alert.addButton(withTitle: String(localized: "Cancel"))
            alert.buttons.first?.hasDestructiveAction = true
            guard alert.runModal() == .alertFirstButtonReturn else { return }
        }
        perform(op) { result in
            guard result != nil else { return }
            then?()
            self.show(.success, "Deleted \(title)", action: BannerAction(title: String(localized: "Undo")) { self.undo() })
        }
    }

    func undo() { step(undo: true) }
    func redo() { step(undo: false) }

    private func step(undo: Bool) {
        flushEditors()
        background({ engine, architecture in
            let r = undo ? try engine.undo() : try engine.redo()
            return (r, try Self.fetch(engine, previous: architecture))
        }) { outcome in
            switch outcome {
            case .success(let r):
                self.history = r.history
                if let diff = r.diff, !diff.changes.isEmpty { self.log(diff, author: String(localized: "You"), external: false) }
                let label = r.label ?? String(localized: "change")
                self.show(.info, undo ? "Undid \(label)" : "Redid \(label)")
            case .failure(let e):
                self.show(.error, verbatim: e.localizedDescription)
            }
        }
    }

    func registerCommit(_ id: String, _ handler: @escaping () -> Bool) { commitHandlers[id] = handler }
    func unregisterCommit(_ id: String) { commitHandlers.removeValue(forKey: id) }

    @discardableResult
    func flushEditors() -> Bool {
        commitHandlers.values.reduce(false) { wrote, commit in commit() || wrote }
    }

    func saveAll() {
        if flushEditors() { show(.success, "Saved") }
    }

    func show(_ kind: Banner.Kind, _ message: String.LocalizationValue, action: BannerAction? = nil) {
        banner = Banner(kind: kind, message: String(localized: message), action: action)
        bannerTask?.cancel()
        let seconds = action != nil ? 8 : kind == .error ? 6 : 3
        bannerTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(seconds))
            guard !Task.isCancelled else { return }
            withAnimation(.easeOut(duration: 0.2)) { self?.banner = nil }
        }
    }

    func show(_ kind: Banner.Kind, verbatim message: String) {
        show(kind, "\(message)")
    }

    func copy(_ text: String, message: String.LocalizationValue) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
        show(.success, message)
    }

    var mcpBinaryPath: String { MCPServer.path }

    func clientConfigs() -> [ClientConfig] {
        do { return try engine?.clientConfigs(serverBinary: mcpBinaryPath) ?? [] } catch {
            show(.error, verbatim: error.localizedDescription)
            return []
        }
    }

    func writeClientConfig(_ client: MCPClient) -> WriteOutcome? {
        do { return try engine?.writeClientConfig(client, serverBinary: mcpBinaryPath) } catch {
            show(.error, verbatim: error.localizedDescription)
            return nil
        }
    }

    var repoRoot: String? { document?.project.repoRoot }

    func runAudit() {
        guard let engine else { return }
        guard !isAuditing else {
            auditAgain = true
            return
        }
        isAuditing = true
        Task {
            let result = await Task.detached { Result { try engine.audit(repoRoot: nil) } }.value
            isAuditing = false
            switch result {
            case .success(let r): healthReport = r
            case .failure(let e): show(.error, verbatim: e.localizedDescription)
            }
            if auditAgain {
                auditAgain = false
                runAudit()
            }
        }
    }

    func request(_ intent: Intent, in module: Module) {
        self.module = module
        DispatchQueue.main.async { self.intent = intent }
    }

    func reveal(_ ref: EntityRef) {
        switch ref.kind {
        case "memory": module = .memory; selectedMemory = ref.id
        case "decision": module = .decisions; selectedDecision = document?.decision(ref.id)?.id ?? ref.id
        case "node": module = .architecture; selectedNode = ref.id
        case "handoff": module = .handoff
        case "prompt": module = .prompts; selectedPrompt = ref.id
        default: module = .health
        }
    }

    var resolvedRepoRoot: URL {
        let base = url.deletingLastPathComponent()
        guard let root = repoRoot, !root.isEmpty else { return base }
        let expanded = (root as NSString).expandingTildeInPath
        return expanded.hasPrefix("/") ? URL(fileURLWithPath: expanded) : base.appendingPathComponent(expanded).standardizedFileURL
    }

    func relativeRepoRoot(for chosen: URL) -> String {
        let base = url.deletingLastPathComponent().standardizedFileURL.path
        let target = chosen.standardizedFileURL.path
        if target == base { return "" }
        if target.hasPrefix(base + "/") { return String(target.dropFirst(base.count + 1)) }
        let b = base.split(separator: "/"), t = target.split(separator: "/")
        var i = 0
        while i < min(b.count, t.count), b[i] == t[i] { i += 1 }
        guard i > 0, b.count - i <= 3 else { return target }
        return (Array(repeating: "..", count: b.count - i) + t[i...].map(String.init)).joined(separator: "/")
    }

    func repoFiles(matching query: String) -> [String] {
        (try? engine?.repoFiles(matching: query)) ?? []
    }
}
