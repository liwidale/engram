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
            OpenRouter.shared.didOpen(url)
        }
        .onDisappear {
            model?.close()
            OpenRouter.shared.didClose(url)
        }
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
                    BannerView(banner: banner) { model.banner = nil }
                        .padding(.bottom, 16)
                        .transition(.move(edge: .bottom).combined(with: .opacity))
                }
            }
            .animation(.spring(duration: 0.3), value: model.banner?.id)
        }
        .navigationTitle(model.projectName)
        .navigationSubtitle(model.module.title)
        .toolbar { toolbar }
        .overlay {
            if model.showPalette { CommandPalette(model: model) }
        }
        .sheet(isPresented: $model.showProjectSettings) { ProjectSettingsView(model: model) }
        .sheet(isPresented: $model.showConnectAgent) { ConnectAgentView(model: model) }
        .sheet(isPresented: $model.showShortcuts) { ShortcutsView() }
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
            SyncPill(model: model)
            Button { model.showPalette = true } label: { Label("Command palette", systemImage: "command") }
                .help("Command palette (⌘K)")
            Button { model.showConnectAgent = true } label: {
                Label("Connect agent", systemImage: "point.3.filled.connected.trianglepath.dotted")
            }
            .help("Connect an AI agent to this file (⇧⌘M)")
        }
    }
}

private struct SyncPill: View {
    var model: WorkspaceModel
    @State private var showActivity = false

    var body: some View {
        Button { showActivity.toggle() } label: {
            HStack(spacing: 6) {
                Circle().fill(color).frame(width: 7, height: 7)
                Text(verbatim: label).font(.caption).lineLimit(1)
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 3)
            .background(Theme.subtleFill, in: Capsule())
        }
        .buttonStyle(.plain)
        .help("Recent changes to this file")
        .popover(isPresented: $showActivity, arrowEdge: .bottom) { ActivityPopover(model: model) }
    }

    private var label: String {
        switch model.sync {
        case .synced: String(localized: "Synced")
        case .saving: String(localized: "Saving…")
        case .updated(let by, let at):
            abs(at.timeIntervalSinceNow) < 120 ? String(localized: "Updated by \(by)") : String(localized: "Synced")
        }
    }

    private var color: Color {
        switch model.sync {
        case .synced: Theme.success.opacity(0.8)
        case .saving: Theme.accent
        case .updated(_, let at): abs(at.timeIntervalSinceNow) < 120 ? Theme.accent : Theme.success.opacity(0.8)
        }
    }
}

private struct ActivityPopover: View {
    var model: WorkspaceModel

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text("Activity").font(.headline)
                Spacer()
                Text("Revision \(model.revision)").font(.caption.monospacedDigit()).foregroundStyle(.secondary)
            }
            Text(verbatim: model.url.path.replacingOccurrences(of: NSHomeDirectory(), with: "~"))
                .font(.caption.monospaced())
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .truncationMode(.middle)
            Divider()
            if model.activity.isEmpty {
                Text("Changes made here or by agents appear in this list.").font(.callout).foregroundStyle(.tertiary)
            } else {
                ScrollView {
                    VStack(alignment: .leading, spacing: 12) {
                        ForEach(model.activity) { entry in
                            VStack(alignment: .leading, spacing: 4) {
                                HStack {
                                    Text(verbatim: entry.author).font(.callout.weight(.semibold))
                                    if entry.external { Chip(text: "external", color: Theme.accent) }
                                    Spacer()
                                    Text(entry.date, style: .relative).font(.caption).foregroundStyle(.tertiary)
                                }
                                Text(verbatim: entry.diff.summary).font(.caption).foregroundStyle(.secondary)
                                ForEach(entry.diff.changes.prefix(6)) { change in
                                    Button {
                                        if let ref = change.ref { model.reveal(ref) }
                                    } label: {
                                        HStack(spacing: 6) {
                                            Image(systemName: icon(change.kind)).foregroundStyle(.secondary).frame(width: 14)
                                            Text(verbatim: change.entityId).font(.caption.monospaced())
                                            Text(verbatim: change.title).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                                        }
                                    }
                                    .buttonStyle(.plain)
                                    .disabled(change.ref == nil)
                                }
                            }
                        }
                    }
                }
                .frame(maxHeight: 320)
            }
            Divider()
            HStack {
                Text(verbatim: "engram-mcp").font(.caption.monospaced()).foregroundStyle(.secondary)
                Spacer()
                Button("Connect agent…") { model.showConnectAgent = true }.buttonStyle(.link).font(.caption)
            }
        }
        .padding(14)
        .frame(width: 380)
    }

    private func icon(_ kind: DocumentChange.Kind) -> String {
        switch kind {
        case .added: "plus.circle"
        case .updated: "pencil.circle"
        case .deleted: "minus.circle"
        case .superseded: "arrow.uturn.right.circle"
        }
    }
}

private struct BannerView: View {
    var banner: Banner
    var dismiss: () -> Void

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: symbol).foregroundStyle(color)
            Text(verbatim: banner.message).font(.callout).lineLimit(3)
            if let action = banner.action {
                Button(action.title) {
                    action.run()
                    dismiss()
                }
                .buttonStyle(.link)
                .font(.callout.weight(.semibold))
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 9)
        .background(.regularMaterial, in: Capsule())
        .overlay(Capsule().strokeBorder(Theme.hairline))
        .shadow(color: .black.opacity(0.12), radius: 12, y: 4)
        .frame(maxWidth: 640)
        .onTapGesture(perform: dismiss)
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
        case .success: Theme.success
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
                    .foregroundStyle(.white)
                    .padding(.horizontal, 6)
                    .padding(.vertical, 1)
                    .background(Theme.scoreColor(r.score), in: Capsule())
            } else if model.isAuditing {
                ProgressView().controlSize(.mini)
            }
        case .handoff:
            if let at = doc?.handoff.updatedAt {
                Text(verbatim: EngramDate.relativeString(at)).font(.caption2).foregroundStyle(.tertiary)
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
                Text(verbatim: model.url.lastPathComponent).lineLimit(1).truncationMode(.middle)
                Spacer()
                Button { model.showProjectSettings = true } label: { Image(systemName: "gearshape") }
                    .buttonStyle(.borderless)
                    .help("Project settings")
            }
            .font(.caption)
            Text(verbatim: model.resolvedRepoRoot.path.replacingOccurrences(of: NSHomeDirectory(), with: "~"))
                .font(.caption2)
                .foregroundStyle(.tertiary)
                .lineLimit(1)
                .truncationMode(.middle)
        }
        .padding(.horizontal, 14)
        .padding(.bottom, 10)
        .contextMenu {
            Button("Project settings…") { model.showProjectSettings = true }
            Button("Reveal in Finder") { NSWorkspace.shared.activateFileViewerSelecting([model.url]) }
            Button("Copy path") { model.copy(model.url.path, message: "Path copied") }
        }
    }
}
