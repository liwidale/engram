import AppKit
import Observation
import SwiftUI

enum Module: String, CaseIterable, Identifiable, Hashable {
    case memory, architecture, decisions, prompts, context, health, handoff
    var id: String { rawValue }

    var title: String {
        switch self {
        case .memory: "Project Memory"
        case .architecture: "Architecture Map"
        case .decisions: "Decision Log"
        case .prompts: "Prompt Vault"
        case .context: "Context Builder"
        case .health: "Context Health"
        case .handoff: "Agent Handoff"
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
    case newMemory, newDecision, newPrompt, newNode, focusTask
}

struct Banner: Identifiable, Equatable {
    enum Kind { case info, success, error }
    let id = UUID()
    var kind: Kind
    var message: String
}

struct ContextBuilderState: Equatable {
    var task = ""
    var maxTokens = 4000
    var autoRank = true
    var includeHandoff = true
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
    var intent: Intent?
    var banner: Banner?
    var lastExternalRevision: UInt64?

    var healthReport: HealthReport?
    var isAuditing = false
    var contextState = ContextBuilderState()

    @ObservationIgnored private var watcher: FileWatcher?
    @ObservationIgnored private var commitHandlers: [String: () -> Void] = [:]
    @ObservationIgnored private var bannerTask: Task<Void, Never>?

    init(url: URL) {
        self.url = url
        open()
    }

    func open() {
        do {
            let engine = try EngramEngine.open(url)
            self.engine = engine
            loadError = nil
            refresh()
            NSDocumentController.shared.noteNewRecentDocumentURL(url)
            let watcher = FileWatcher(url: url) { [weak self] in self?.fileChangedOnDisk() }
            watcher.start()
            self.watcher = watcher
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

    func refresh() {
        guard let engine else { return }
        do {
            document = try engine.document().document
            layout = (try? engine.layout()) ?? .empty
        } catch {
            show(.error, error.localizedDescription)
        }
    }

    private func fileChangedOnDisk() {
        guard let engine else { return }
        do {
            let r = try engine.reload()
            guard r.changed else { return }
            refresh()
            lastExternalRevision = r.revision
            show(.info, "Reloaded external changes · revision \(r.revision)")
        } catch {
            show(.error, "Could not reload: \(error.localizedDescription)")
        }
    }

    func reloadFromDisk() {
        guard let engine else { return open() }
        do {
            let r = try engine.reload()
            refresh()
            show(.info, r.changed ? "Reloaded revision \(r.revision)" : "Already up to date")
        } catch {
            show(.error, error.localizedDescription)
        }
    }

    @discardableResult
    func perform(_ op: EngramOp, success: String? = nil) -> JSONValue? {
        guard let engine else { return nil }
        do {
            let r = try engine.apply(op)
            refresh()
            if let warnings = r.result["warnings"]?.strings, !warnings.isEmpty {
                show(.info, warnings.joined(separator: " · "))
            } else if let success {
                show(.success, success)
            }
            return r.result
        } catch {
            show(.error, error.localizedDescription)
            return nil
        }
    }

    func registerCommit(_ id: String, _ handler: @escaping () -> Void) { commitHandlers[id] = handler }
    func unregisterCommit(_ id: String) { commitHandlers.removeValue(forKey: id) }

    func saveAll() {
        commitHandlers.values.forEach { $0() }
        show(.success, "Saved · revision \(revision)")
    }

    func show(_ kind: Banner.Kind, _ message: String) {
        banner = Banner(kind: kind, message: message)
        bannerTask?.cancel()
        bannerTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(kind == .error ? 6 : 3))
            guard !Task.isCancelled else { return }
            withAnimation(.easeOut(duration: 0.2)) { self?.banner = nil }
        }
    }

    func copy(_ text: String, message: String) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
        show(.success, message)
    }

    var mcpBinaryPath: String {
        let installed = "/Applications/Engram.app/Contents/MacOS/engram-mcp"
        let bundled = Bundle.main.bundleURL.appendingPathComponent("Contents/MacOS/engram-mcp").path
        let fm = FileManager.default
        if Bundle.main.bundlePath.hasPrefix("/Applications/"), fm.isExecutableFile(atPath: bundled) { return bundled }
        if fm.isExecutableFile(atPath: installed) { return installed }
        if fm.isExecutableFile(atPath: bundled) { return bundled }
        return installed
    }

    func mcpConfig() -> MCPConfig? {
        do { return try engine?.mcpConfig(serverBinary: mcpBinaryPath) } catch {
            show(.error, error.localizedDescription)
            return nil
        }
    }

    func copyMCPConfig() {
        guard let cfg = mcpConfig() else { return }
        copy(cfg.json, message: "MCP config copied · paste into claude_desktop_config.json or .mcp.json")
    }

    var repoRoot: String? { document?.project.repoRoot }

    func runAudit() {
        guard let engine, !isAuditing else { return }
        isAuditing = true
        let root = repoRoot
        Task {
            let result = await Task.detached { Result { try engine.audit(repoRoot: root) } }.value
            isAuditing = false
            switch result {
            case .success(let r): healthReport = r
            case .failure(let e): show(.error, e.localizedDescription)
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
}
