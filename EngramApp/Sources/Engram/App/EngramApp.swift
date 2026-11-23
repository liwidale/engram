import AppKit
import SwiftUI
import UniformTypeIdentifiers

extension UTType {
    static let engram = UTType(exportedAs: "com.liwidale.engram.project", conformingTo: .json)
}

@main
struct EngramApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate

    var body: some Scene {
        Window("Welcome to Engram", id: WindowID.welcome) {
            WelcomeView()
        }
        .windowResizability(.contentSize)
        .windowStyle(.hiddenTitleBar)

        WindowGroup("Engram", id: WindowID.workspace, for: URL.self) { $url in
            if let url {
                WorkspaceWindow(url: url)
            } else {
                WelcomeView()
            }
        }
        .defaultSize(width: 1240, height: 800)
        .windowToolbarStyle(.unified(showsTitle: true))
        .commands { EngramCommands() }
    }
}

enum WindowID {
    static let welcome = "welcome"
    static let workspace = "workspace"
}

final class AppDelegate: NSObject, NSApplicationDelegate {
    func applicationWillFinishLaunching(_ notification: Notification) {
        NSWindow.allowsAutomaticWindowTabbing = true
    }

    func application(_ application: NSApplication, open urls: [URL]) {
        MainActor.assumeIsolated {
            for url in urls where url.pathExtension.lowercased() == "engram" {
                OpenRouter.shared.open(url)
            }
        }
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { false }
}

@MainActor
final class OpenRouter {
    static let shared = OpenRouter()

    private var openWindow: OpenWindowAction?
    private var dismissWindow: DismissWindowAction?
    private var pending: [URL] = []

    func register(open: OpenWindowAction, dismiss: DismissWindowAction) {
        openWindow = open
        dismissWindow = dismiss
        let queued = pending
        pending.removeAll()
        queued.forEach(self.open)
    }

    func open(_ url: URL) {
        let url = url.standardizedFileURL
        guard let openWindow else {
            pending.append(url)
            return
        }
        openWindow(id: WindowID.workspace, value: url)
        dismissWindow?(id: WindowID.welcome)
    }

    func presentOpenPanel() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.engram]
        panel.allowsMultipleSelection = true
        panel.message = "Choose a project memory file"
        if panel.runModal() == .OK { panel.urls.forEach(open) }
    }

    func presentNewPanel() {
        let panel = NSSavePanel()
        panel.allowedContentTypes = [.engram]
        panel.nameFieldStringValue = "project.engram"
        panel.message = "Create a project memory file. Keep it next to your code so it is versioned with it."
        panel.prompt = "Create"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        let name = url.deletingLastPathComponent().lastPathComponent
        do {
            if FileManager.default.fileExists(atPath: url.path) {
                try FileManager.default.trashItem(at: url, resultingItemURL: nil)
            }
            _ = try EngramEngine.create(url, projectName: name.isEmpty ? "Project" : name)
            open(url)
        } catch {
            NSAlert(error: error).runModal()
        }
    }
}

private struct WorkspaceKey: FocusedValueKey { typealias Value = WorkspaceModel }

extension FocusedValues {
    var workspace: WorkspaceModel? {
        get { self[WorkspaceKey.self] }
        set { self[WorkspaceKey.self] = newValue }
    }
}

struct EngramCommands: Commands {
    @FocusedValue(\.workspace) private var workspace

    var body: some Commands {
        CommandGroup(replacing: .newItem) {
            Button("New Engram…") { OpenRouter.shared.presentNewPanel() }
                .keyboardShortcut("n")
            Button("Open…") { OpenRouter.shared.presentOpenPanel() }
                .keyboardShortcut("o")
            Menu("Open Recent") {
                ForEach(NSDocumentController.shared.recentDocumentURLs, id: \.self) { url in
                    Button(url.lastPathComponent) { OpenRouter.shared.open(url) }
                }
                Divider()
                Button("Clear Menu") { NSDocumentController.shared.clearRecentDocuments(nil) }
            }
        }
        CommandGroup(replacing: .saveItem) {
            Button("Save") { workspace?.saveAll() }
                .keyboardShortcut("s")
                .disabled(workspace == nil)
            Button("Reload from Disk") { workspace?.reloadFromDisk() }
                .keyboardShortcut("r")
                .disabled(workspace == nil)
        }
        CommandMenu("Workspace") {
            Button("Command Palette…") { workspace?.showPalette.toggle() }
                .keyboardShortcut("k")
                .disabled(workspace == nil)
            Divider()
            ForEach(Module.allCases) { m in
                Button(m.title) { workspace?.module = m }
                    .keyboardShortcut(m.shortcut)
                    .disabled(workspace == nil)
            }
            Divider()
            Button("New Memory Entry") { workspace?.request(.newMemory, in: .memory) }
                .keyboardShortcut("n", modifiers: [.command, .shift])
                .disabled(workspace == nil)
            Button("Log Decision") { workspace?.request(.newDecision, in: .decisions) }
                .keyboardShortcut("d", modifiers: [.command, .shift])
                .disabled(workspace == nil)
            Divider()
            Button("Copy MCP Config") { workspace?.copyMCPConfig() }
                .keyboardShortcut("m", modifiers: [.command, .shift])
                .disabled(workspace == nil)
            Button("Run Health Audit") {
                workspace?.module = .health
                workspace?.runAudit()
            }
            .keyboardShortcut("h", modifiers: [.command, .shift])
            .disabled(workspace == nil)
        }
    }
}
