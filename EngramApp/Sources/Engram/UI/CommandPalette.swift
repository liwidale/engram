import SwiftUI

struct CommandPalette: View {
    var model: WorkspaceModel
    @State private var query = ""
    @State private var cursor = 0
    @FocusState private var focused: Bool

    struct Item: Identifiable {
        let id: String
        let title: String
        let subtitle: String
        let symbol: String
        let group: String
        let run: @MainActor () -> Void
    }

    var body: some View {
        ZStack(alignment: .top) {
            Color.black.opacity(0.18)
                .ignoresSafeArea()
                .onTapGesture { close() }
            VStack(spacing: 0) {
                HStack(spacing: 10) {
                    Image(systemName: "magnifyingglass").foregroundStyle(.secondary)
                    TextField("Search memory, decisions, prompts, commands…", text: $query)
                        .textFieldStyle(.plain)
                        .font(.title3)
                        .focused($focused)
                        .onSubmit { execute() }
                }
                .padding(14)
                Divider()
                ScrollViewReader { proxy in
                    ScrollView {
                        LazyVStack(spacing: 1) {
                            let items = results
                            if items.isEmpty {
                                Text("No matches").foregroundStyle(.tertiary).padding(20)
                            }
                            ForEach(Array(items.enumerated()), id: \.element.id) { i, item in
                                row(item, selected: i == cursor)
                                    .id(i)
                                    .onTapGesture { cursor = i; execute() }
                            }
                        }
                        .padding(6)
                    }
                    .frame(maxHeight: 380)
                    .onChange(of: cursor) { _, c in proxy.scrollTo(c) }
                }
                HStack(spacing: 14) {
                    hint("↑↓", "navigate"); hint("↩", "open"); hint("esc", "close")
                    Spacer()
                }
                .padding(.horizontal, 14)
                .padding(.vertical, 7)
                .background(Theme.subtleFill)
            }
            .frame(width: 620)
            .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 14))
            .overlay(RoundedRectangle(cornerRadius: 14).strokeBorder(Theme.hairline))
            .shadow(color: .black.opacity(0.25), radius: 30, y: 12)
            .padding(.top, 70)
        }
        .onAppear { focused = true }
        .onChange(of: query) { cursor = 0 }
        .onKeyPress(.downArrow) { cursor = min(cursor + 1, max(results.count - 1, 0)); return .handled }
        .onKeyPress(.upArrow) { cursor = max(cursor - 1, 0); return .handled }
        .onKeyPress(.escape) { close(); return .handled }
    }

    private func row(_ item: Item, selected: Bool) -> some View {
        HStack(spacing: 10) {
            Image(systemName: item.symbol)
                .frame(width: 22)
                .foregroundStyle(selected ? .white : Theme.accent)
            VStack(alignment: .leading, spacing: 1) {
                Text(item.title).lineLimit(1)
                if !item.subtitle.isEmpty {
                    Text(item.subtitle)
                        .font(.caption)
                        .lineLimit(1)
                        .foregroundStyle(selected ? Color.white.opacity(0.8) : .secondary)
                }
            }
            Spacer()
            Text(item.group).font(.caption2).foregroundStyle(selected ? Color.white.opacity(0.7) : Color.secondary.opacity(0.7))
        }
        .foregroundStyle(selected ? .white : .primary)
        .padding(.horizontal, 10)
        .padding(.vertical, 7)
        .background(selected ? Theme.accent : .clear, in: RoundedRectangle(cornerRadius: 8))
        .contentShape(Rectangle())
    }

    private func hint(_ key: String, _ label: String) -> some View {
        HStack(spacing: 4) {
            Text(key).font(.caption.monospaced()).padding(.horizontal, 4).background(Theme.hoverFill, in: RoundedRectangle(cornerRadius: 3))
            Text(label).font(.caption)
        }
        .foregroundStyle(.secondary)
    }

    private func close() { model.showPalette = false }

    private func execute() {
        let items = results
        guard items.indices.contains(cursor) else { return }
        close()
        items[cursor].run()
    }

    private var catalog: [Item] {
        let m = model
        var items: [Item] = [
            Item(id: "cmd.new-memory", title: String(localized: "New memory entry"), subtitle: String(localized: "Add a rule, invariant, convention…"), symbol: "plus.circle", group: String(localized: "Command")) { m.request(.newMemory, in: .memory) },
            Item(id: "cmd.new-adr", title: String(localized: "Log decision"), subtitle: String(localized: "Record an ADR with rejected alternatives"), symbol: "signpost.right", group: String(localized: "Command")) { m.request(.newDecision, in: .decisions) },
            Item(id: "cmd.new-prompt", title: String(localized: "New prompt"), subtitle: String(localized: "Add a template to the prompt vault"), symbol: "text.badge.plus", group: String(localized: "Command")) { m.request(.newPrompt, in: .prompts) },
            Item(id: "cmd.new-node", title: String(localized: "Add architecture node"), subtitle: String(localized: "Module, service, database…"), symbol: "plus.square.on.square", group: String(localized: "Command")) { m.request(.newNode, in: .architecture) },
            Item(id: "cmd.context", title: String(localized: "Build context for a task…"), subtitle: String(localized: "Rank memory against a task description"), symbol: "square.stack.3d.down.right", group: String(localized: "Command")) { m.request(.focusTask, in: .context) },
            Item(id: "cmd.audit", title: String(localized: "Run health check"), subtitle: String(localized: "Find broken paths, stale references and contradictions"), symbol: "stethoscope", group: String(localized: "Command")) { m.module = .health; m.runAudit() },
            Item(id: "cmd.connect", title: String(localized: "Connect agent…"), subtitle: String(localized: "Claude Code, Cursor, Windsurf, VS Code and more"), symbol: "point.3.filled.connected.trianglepath.dotted", group: String(localized: "Command")) { m.showConnectAgent = true },
            Item(id: "cmd.settings", title: String(localized: "Project settings…"), subtitle: String(localized: "Name, codebase folder, health rules, import and export"), symbol: "gearshape", group: String(localized: "Command")) { m.showProjectSettings = true },
            Item(id: "cmd.scan", title: String(localized: "Scan repository…"), subtitle: String(localized: "Draft memory from build files and agent instructions"), symbol: "wand.and.stars", group: String(localized: "Command")) { m.request(.bootstrap, in: .memory) },
            Item(id: "cmd.undo", title: m.history.undoLabel.map { String(localized: "Undo \($0)") } ?? String(localized: "Undo"), subtitle: "", symbol: "arrow.uturn.backward", group: String(localized: "Command")) { m.undo() },
            Item(id: "cmd.redo", title: m.history.redoLabel.map { String(localized: "Redo \($0)") } ?? String(localized: "Redo"), subtitle: "", symbol: "arrow.uturn.forward", group: String(localized: "Command")) { m.redo() },
            Item(id: "cmd.shortcuts", title: String(localized: "Keyboard shortcuts"), subtitle: "⌘/", symbol: "keyboard", group: String(localized: "Command")) { m.showShortcuts = true },
            Item(id: "cmd.copy-memory", title: String(localized: "Copy project memory as Markdown"), subtitle: "", symbol: "doc.on.doc", group: String(localized: "Command")) {
                if let s = try? m.engine?.render(.memory) { m.copy(s, message: "Project memory copied") }
            },
            Item(id: "cmd.copy-handoff", title: String(localized: "Copy agent handoff as XML"), subtitle: "", symbol: "arrow.triangle.turn.up.right.diamond", group: String(localized: "Command")) {
                if let s = try? m.engine?.render(.handoffXML) { m.copy(s, message: "Handoff copied") }
            },
            Item(id: "cmd.save", title: String(localized: "Save"), subtitle: String(localized: "Flush pending edits"), symbol: "square.and.arrow.down", group: String(localized: "Command")) { m.saveAll() },
            Item(id: "cmd.reload", title: String(localized: "Reload from disk"), subtitle: "", symbol: "arrow.clockwise", group: String(localized: "Command")) { m.reloadFromDisk() },
            Item(id: "cmd.reveal", title: String(localized: "Reveal .engram in Finder"), subtitle: m.url.path, symbol: "folder", group: String(localized: "Command")) {
                NSWorkspace.shared.activateFileViewerSelecting([m.url])
            },
        ]
        items += Module.allCases.map { mod in
            Item(id: "go.\(mod.rawValue)", title: String(localized: "Go to \(mod.title)"), subtitle: "", symbol: mod.symbol, group: "⌘" + String(mod.shortcut.character)) { m.module = mod }
        }
        guard let doc = m.document else { return items }
        items += doc.memory.map { e in
            Item(id: "mem.\(e.key)", title: e.key, subtitle: (e.isActive ? "" : "[SUPERSEDED] ") + e.summary, symbol: e.category.symbol, group: String(localized: "Memory")) {
                m.module = .memory; m.selectedMemory = e.key
            }
        }
        items += doc.decisions.map { d in
            Item(id: "adr.\(d.id)", title: String(localized: "\(d.id) · \(d.title)"), subtitle: d.status.title, symbol: "signpost.right.and.left", group: String(localized: "Decision")) {
                m.module = .decisions; m.selectedDecision = d.id
            }
        }
        items += doc.prompts.map { p in
            Item(id: "prompt.\(p.name)", title: p.name, subtitle: p.category, symbol: "text.quote", group: String(localized: "Prompt")) {
                m.module = .prompts; m.selectedPrompt = p.name
            }
        }
        items += doc.architecture.nodes.map { n in
            Item(id: "node.\(n.id)", title: n.displayName, subtitle: n.kind.title + (n.path.map { " · \($0)" } ?? ""), symbol: n.kind.symbol, group: String(localized: "Node")) {
                m.module = .architecture; m.selectedNode = n.id
            }
        }
        return items
    }

    private var results: [Item] {
        let q = query.trimmingCharacters(in: .whitespaces)
        let all = catalog
        guard !q.isEmpty else { return Array(all.prefix(14)) }
        return all
            .compactMap { item -> (Item, Int)? in
                let s = max(FuzzyMatch.score(q, item.title), FuzzyMatch.score(q, item.subtitle) - 20)
                return s > 0 ? (item, s) : nil
            }
            .sorted { $0.1 > $1.1 }
            .prefix(40)
            .map(\.0)
    }
}

enum FuzzyMatch {
    static func score(_ query: String, _ candidate: String) -> Int {
        let q = Array(query.lowercased()), c = Array(candidate.lowercased())
        guard !q.isEmpty, !c.isEmpty else { return 0 }
        if let r = candidate.lowercased().range(of: query.lowercased()) {
            let start = candidate.distance(from: candidate.startIndex, to: r.lowerBound)
            return 1000 - start * 2 - (c.count - q.count)
        }
        var score = 0, qi = 0, streak = 0
        for (i, ch) in c.enumerated() where qi < q.count {
            if ch == q[qi] {
                let boundary = i == 0 || " -_./·".contains(c[i - 1])
                streak += 1
                score += 10 + streak * 5 + (boundary ? 25 : 0)
                qi += 1
            } else {
                streak = 0
            }
        }
        return qi == q.count ? score : 0
    }
}
