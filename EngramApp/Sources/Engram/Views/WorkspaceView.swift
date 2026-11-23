import SwiftUI

struct WorkspaceWindow: View {
    let url: URL
    @State private var model: WorkspaceModel?
    @Environment(\.openWindow) private var openWindow
    @Environment(\.dismissWindow) private var dismissWindow

    var body: some View {
        Group {
            if let model {
                if let error = model.loadError {
                    EmptyState(
                        symbol: "exclamationmark.triangle",
                        title: "Couldn't open \(url.lastPathComponent)",
                        message: "\(error)",
                        action: ("Try again", { model.open() })
                    )
                    .frame(minWidth: 520, minHeight: 360)
                } else {
                    WorkspaceView(model: model)
                }
            } else {
                ProgressView().frame(minWidth: 520, minHeight: 360)
            }
        }
        .onAppear {
            OpenRouter.shared.register(open: openWindow, dismiss: dismissWindow)
            if model == nil { model = WorkspaceModel(url: url) }
        }
        .onDisappear { model?.close() }
        .hostingWindow { window in
            window.tabbingIdentifier = "com.liwidale.engram.workspace"
            window.tabbingMode = .preferred
            window.representedURL = url
        }
    }
}

struct WorkspaceView: View {
    @Bindable var model: WorkspaceModel

    var body: some View {
        NavigationSplitView {
            Sidebar(model: model)
                .navigationSplitViewColumnWidth(min: 200, ideal: 228, max: 300)
        } detail: {
            ZStack(alignment: .bottom) {
                detail
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .background(Theme.canvas)
                if let banner = model.banner {
                    BannerView(banner: banner)
                        .padding(.bottom, 16)
                        .transition(.move(edge: .bottom).combined(with: .opacity))
                        .onTapGesture { model.banner = nil }
                }
            }
            .animation(.spring(duration: 0.3), value: model.banner)
        }
        .navigationTitle(model.projectName)
        .navigationSubtitle(model.module.title)
        .toolbar { toolbar }
        .overlay {
            if model.showPalette {
                CommandPalette(model: model)
            }
        }
        .focusedSceneValue(\.workspace, model)
        .frame(minWidth: 960, minHeight: 600)
    }

    @ViewBuilder
    private var detail: some View {
        switch model.module {
        case .memory: MemoryView(model: model)
        case .architecture: ArchitectureView(model: model)
        case .decisions: DecisionsView(model: model)
        case .prompts: PromptVaultView(model: model)
        case .context: ContextBuilderView(model: model)
        case .health: HealthView(model: model)
        case .handoff: HandoffView(model: model)
        }
    }

    @ToolbarContentBuilder
    private var toolbar: some ToolbarContent {
        ToolbarItemGroup(placement: .primaryAction) {
            RevisionIndicator(model: model)
            Button {
                model.showPalette = true
            } label: {
                Label("Command palette", systemImage: "command")
            }
            .help("Command palette (⌘K)")
            Menu {
                Button("Copy JSON for claude_desktop_config.json / .mcp.json") { model.copyMCPConfig() }
                Button("Copy `claude mcp add` command") {
                    if let cfg = model.mcpConfig() { model.copy(cfg.claudeCodeCommand, message: "Claude Code command copied") }
                }
                Divider()
                Text("Server: \(model.mcpBinaryPath)")
            } label: {
                Label("Copy MCP config", systemImage: "point.3.filled.connected.trianglepath.dotted")
            } primaryAction: {
                model.copyMCPConfig()
            }
            .help("Copy the MCP server config for this file (⇧⌘M)")
        }
    }
}

private struct RevisionIndicator: View {
    var model: WorkspaceModel
    @State private var pulse = false

    var body: some View {
        HStack(spacing: 5) {
            Circle()
                .fill(pulse ? Theme.accent : Color.green.opacity(0.8))
                .frame(width: 7, height: 7)
                .scaleEffect(pulse ? 1.5 : 1)
            Text("rev \(model.revision)").font(.caption.monospacedDigit()).foregroundStyle(.secondary)
        }
        .padding(.horizontal, 6)
        .help("Live: external edits from engram-mcp reload automatically")
        .onChange(of: model.lastExternalRevision) {
            withAnimation(.easeOut(duration: 0.25)) { pulse = true }
            withAnimation(.easeIn(duration: 0.6).delay(0.6)) { pulse = false }
        }
    }
}

private struct BannerView: View {
    var banner: Banner

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: symbol).foregroundStyle(color)
            Text(banner.message).font(.callout).lineLimit(3)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 9)
        .background(.regularMaterial, in: Capsule())
        .overlay(Capsule().strokeBorder(Theme.hairline))
        .shadow(color: .black.opacity(0.12), radius: 12, y: 4)
        .frame(maxWidth: 640)
    }

    private var symbol: String {
        switch banner.kind {
        case .info: "arrow.triangle.2.circlepath"
        case .success: "checkmark.circle.fill"
        case .error: "exclamationmark.octagon.fill"
        }
    }

    private var color: Color {
        switch banner.kind {
        case .info: Theme.accent
        case .success: .green
        case .error: Theme.color(for: .error)
        }
    }
}

private struct Sidebar: View {
    @Bindable var model: WorkspaceModel

    var body: some View {
        List(selection: Binding(get: { model.module }, set: { if let m = $0 { model.module = m } })) {
            Section("Memory") {
                row(.memory)
                row(.architecture)
                row(.decisions)
                row(.prompts)
            }
            Section("Agents") {
                row(.context)
                row(.health)
                row(.handoff)
            }
        }
        .listStyle(.sidebar)
        .safeAreaInset(edge: .bottom) { footer }
    }

    private func row(_ m: Module) -> some View {
        Label {
            HStack {
                Text(m.title)
                Spacer()
                badge(m)
            }
        } icon: {
            Image(systemName: m.symbol)
        }
        .tag(m)
    }

    @ViewBuilder
    private func badge(_ m: Module) -> some View {
        let doc = model.document
        switch m {
        case .memory: count(doc?.memory.filter(\.isActive).count)
        case .architecture: count(doc?.architecture.nodes.count)
        case .decisions: count(doc?.decisions.filter(\.isActive).count)
        case .prompts: count(doc?.prompts.count)
        case .context: EmptyView()
        case .health:
            if let r = model.healthReport {
                Text("\(r.score)")
                    .font(.caption.monospacedDigit().weight(.semibold))
                    .foregroundStyle(Theme.scoreColor(r.score))
            }
        case .handoff:
            if let at = doc?.handoff.updatedAt {
                Text(EngramDate.relativeString(at)).font(.caption2).foregroundStyle(.tertiary)
            }
        }
    }

    private func count(_ n: Int?) -> some View {
        Text("\(n ?? 0)").font(.caption.monospacedDigit()).foregroundStyle(.tertiary)
    }

    private var footer: some View {
        VStack(alignment: .leading, spacing: 4) {
            Divider().padding(.bottom, 4)
            HStack(spacing: 6) {
                Image(systemName: "doc.text").foregroundStyle(.secondary)
                Text(model.url.lastPathComponent).lineLimit(1).truncationMode(.middle)
            }
            .font(.caption)
            Text(model.resolvedRepoRoot.path.replacingOccurrences(of: NSHomeDirectory(), with: "~"))
                .font(.caption2)
                .foregroundStyle(.tertiary)
                .lineLimit(1)
                .truncationMode(.middle)
        }
        .padding(.horizontal, 14)
        .padding(.bottom, 10)
        .contextMenu {
            Button("Reveal in Finder") { NSWorkspace.shared.activateFileViewerSelecting([model.url]) }
            Button("Copy path") { model.copy(model.url.path, message: "Path copied") }
        }
    }
}
