import SwiftUI

struct MemoryView: View {
    @Bindable var model: WorkspaceModel
    @State private var search = ""
    @State private var showSuperseded = true
    @State private var categoryFilter: MemoryCategory?
    @State private var composer: MemoryComposer.Seed?

    var body: some View {
        HSplitView {
            list
                .frame(minWidth: 260, idealWidth: 300, maxWidth: 420)
            Group {
                if let key = model.selectedMemory, let entry = model.document?.memoryEntry(key) {
                    MemoryEditor(model: model, entry: entry) { composer = .supersede(entry) }
                        .id(entry.key)
                } else {
                    EmptyState(
                        symbol: "brain",
                        title: "Project Memory",
                        message: "Canonical truth for this codebase: rules, tech stack, architecture invariants and conventions. Agents read it through engram://memory.",
                        action: ("New Entry", { composer = .new })
                    )
                }
            }
            .frame(minWidth: 420, maxWidth: .infinity, maxHeight: .infinity)
        }
        .sheet(item: $composer) { seed in
            MemoryComposer(model: model, seed: seed)
        }
        .onChange(of: model.intent) { _, intent in
            if intent == .newMemory { composer = .new; model.intent = nil }
        }
        .onAppear {
            if model.selectedMemory == nil { model.selectedMemory = model.document?.memory.first(where: \.isActive)?.key }
        }
    }

    private var filtered: [MemoryEntry] {
        let q = search.lowercased()
        return (model.document?.memory ?? []).filter { e in
            (showSuperseded || e.isActive)
                && (categoryFilter == nil || e.category == categoryFilter)
                && (q.isEmpty || e.key.lowercased().contains(q) || e.content.lowercased().contains(q) || e.tags.contains { $0.lowercased().contains(q) })
        }
    }

    private var list: some View {
        VStack(spacing: 0) {
            HStack(spacing: 6) {
                SearchField(text: $search, prompt: "Filter memory")
                Menu {
                    Picker("Category", selection: $categoryFilter) {
                        Text("All Categories").tag(MemoryCategory?.none)
                        ForEach(MemoryCategory.allCases) { Label($0.title, systemImage: $0.symbol).tag(Optional($0)) }
                    }
                    .pickerStyle(.inline)
                    Toggle("Show Superseded", isOn: $showSuperseded)
                } label: {
                    Image(systemName: categoryFilter == nil && showSuperseded ? "line.3.horizontal.decrease.circle" : "line.3.horizontal.decrease.circle.fill")
                }
                .menuStyle(.borderlessButton)
                .fixedSize()
                Button { composer = .new } label: { Image(systemName: "plus") }
                    .buttonStyle(.borderless)
                    .help("New memory entry (⇧⌘N)")
            }
            .padding(10)
            Divider()
            List(selection: $model.selectedMemory) {
                let entries = filtered
                ForEach(MemoryCategory.allCases) { cat in
                    let group = entries.filter { $0.category == cat }.sorted { ($0.isActive ? 0 : 1, $0.key) < ($1.isActive ? 0 : 1, $1.key) }
                    if !group.isEmpty {
                        Section {
                            ForEach(group) { e in
                                MemoryRow(entry: e).tag(e.key).contextMenu { menu(for: e) }
                            }
                        } header: {
                            Label(cat.title, systemImage: cat.symbol).foregroundStyle(Theme.color(for: cat))
                        }
                    }
                }
            }
            .listStyle(.inset)
            .overlay {
                if filtered.isEmpty {
                    Text(model.document?.memory.isEmpty == true ? "No entries yet" : "No matches")
                        .foregroundStyle(.tertiary)
                }
            }
        }
    }

    @ViewBuilder
    private func menu(for e: MemoryEntry) -> some View {
        Button("Copy as Markdown") { model.copy("### \(e.key)\n\n\(e.content)", message: "Copied '\(e.key)'") }
        Button(e.pinned ? "Unpin" : "Pin to Every Context") {
            model.perform(.upsertMemory(key: e.key, category: nil, content: e.content, pinned: !e.pinned))
        }
        Divider()
        if e.isActive {
            Button("Supersede with New Entry…") { composer = .supersede(e) }
            Button("Mark Superseded") { model.perform(.setMemoryStatus(key: e.key, .superseded)) }
        } else {
            Button("Restore as Active") { model.perform(.setMemoryStatus(key: e.key, .active)) }
        }
        Divider()
        Button("Delete", role: .destructive) {
            if model.selectedMemory == e.key { model.selectedMemory = nil }
            model.perform(.deleteMemory(key: e.key), success: "Deleted '\(e.key)'")
        }
    }
}

private struct MemoryRow: View {
    var entry: MemoryEntry

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            HStack(spacing: 6) {
                Text(entry.key)
                    .font(.system(.callout, design: .monospaced).weight(.medium))
                    .strikethrough(!entry.isActive)
                    .foregroundStyle(entry.isActive ? .primary : .secondary)
                    .lineLimit(1)
                if entry.pinned { Image(systemName: "pin.fill").font(.caption2).foregroundStyle(Theme.accent) }
                Spacer(minLength: 0)
                if !entry.isActive { Chip(text: "SUPERSEDED", color: .orange) }
            }
            Text(entry.summary)
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(2)
        }
        .padding(.vertical, 3)
        .opacity(entry.isActive ? 1 : 0.7)
    }
}

private struct MemoryEditor: View {
    enum Mode: String, CaseIterable { case edit = "Edit", split = "Split", preview = "Preview" }

    var model: WorkspaceModel
    var entry: MemoryEntry
    var onSupersede: () -> Void

    @AppStorage("memory.editorMode") private var mode: Mode = .split
    @State private var content = ""
    @State private var tags = ""
    @State private var files: [String] = []
    @State private var keyDraft = ""
    @State private var baseline: (content: String, tags: [String], files: [String]) = ("", [], [])
    @State private var tokens = 0
    @State private var debounce = Debouncer()
    @State private var saveDebounce = Debouncer()

    private var parsedTags: [String] {
        tags.split(separator: ",").map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }
    }
    private var dirty: Bool { content != baseline.content || parsedTags != baseline.tags || files != baseline.files }

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            editorBody
            Divider()
            footer
        }
        .onAppear {
            load(entry)
            model.registerCommit("memory") { commit() }
        }
        .onDisappear {
            commit()
            model.unregisterCommit("memory")
        }
        .onChange(of: entry) { _, new in
            if !dirty { load(new) }
        }
        .onChange(of: content) { _, _ in scheduleSave() }
        .onChange(of: tags) { _, _ in scheduleSave() }
        .onChange(of: files) { _, _ in scheduleSave() }
    }

    private var header: some View {
        HStack(spacing: 10) {
            Image(systemName: entry.category.symbol)
                .foregroundStyle(Theme.color(for: entry.category))
                .font(.title3)
            TextField("key", text: $keyDraft)
                .textFieldStyle(.plain)
                .font(.system(.title3, design: .monospaced).weight(.semibold))
                .onSubmit(rename)
                .frame(maxWidth: 320)
            if !entry.isActive { SupersededBadge(by: entry.supersededBy) }
            Spacer()
            Picker("Category", selection: Binding(get: { entry.category }, set: { c in
                commit()
                model.perform(.upsertMemory(key: entry.key, category: c, content: content))
            })) {
                ForEach(MemoryCategory.allCases) { Label($0.singular, systemImage: $0.symbol).tag($0) }
            }
            .labelsHidden()
            .fixedSize()
            Toggle(isOn: Binding(get: { entry.pinned }, set: { p in
                commit()
                model.perform(.upsertMemory(key: entry.key, category: nil, content: content, pinned: p))
            })) {
                Image(systemName: entry.pinned ? "pin.fill" : "pin")
            }
            .toggleStyle(.button)
            .help("Pinned entries are included in every built context")
            Picker("Mode", selection: $mode) {
                ForEach(Mode.allCases, id: \.self) { Text($0.rawValue) }
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .fixedSize()
            Menu {
                Button("Copy as Markdown") { model.copy("### \(entry.key)\n\n\(content)", message: "Copied '\(entry.key)'") }
                if entry.isActive {
                    Button("Supersede with New Entry…", action: onSupersede)
                    Button("Mark Superseded") { commit(); model.perform(.setMemoryStatus(key: entry.key, .superseded)) }
                } else {
                    Button("Restore as Active") { model.perform(.setMemoryStatus(key: entry.key, .active)) }
                }
                Divider()
                Button("Delete", role: .destructive) {
                    model.selectedMemory = nil
                    model.perform(.deleteMemory(key: entry.key), success: "Deleted '\(entry.key)'")
                }
            } label: {
                Image(systemName: "ellipsis.circle")
            }
            .menuStyle(.borderlessButton)
            .fixedSize()
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
    }

    @ViewBuilder
    private var editorBody: some View {
        let editor = CodeEditor(text: $content, placeholder: "Markdown. Prefix with [SUPERSEDED] to retire this entry.", minHeight: 200)
            .padding(12)
        let preview = ScrollView {
            MarkdownView(text: content).padding(18)
        }
        .background(Theme.surface)
        switch mode {
        case .edit: editor.background(Theme.surface)
        case .preview: preview
        case .split:
            HSplitView {
                editor.background(Theme.surface).frame(minWidth: 240)
                preview.frame(minWidth: 240)
            }
        }
    }

    private var footer: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .top, spacing: 16) {
                VStack(alignment: .leading, spacing: 4) {
                    SectionLabel("Tags")
                    TextField("comma, separated", text: $tags)
                        .textFieldStyle(.roundedBorder)
                }
                .frame(maxWidth: 260)
                VStack(alignment: .leading, spacing: 4) {
                    SectionLabel("Linked files", trailing: "one path per line, relative to repo")
                    LineListEditor(items: $files, placeholder: "src/payments/charge.ts")
                        .padding(4)
                        .background(Theme.surface, in: RoundedRectangle(cornerRadius: 6))
                        .overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(Theme.hairline))
                        .frame(height: 58)
                }
            }
            HStack(spacing: 12) {
                if let s = entry.supersedes {
                    Button { model.selectedMemory = s } label: { Label("supersedes \(s)", systemImage: "arrow.uturn.left") }
                        .buttonStyle(.link)
                }
                if let by = entry.supersededBy {
                    Button { model.selectedMemory = by } label: { Label("replaced by \(by)", systemImage: "arrow.uturn.right") }
                        .buttonStyle(.link)
                }
                Spacer()
                TokenBadge(tokens: tokens)
                Text(dirty ? "Edited" : "Saved \(EngramDate.relativeString(entry.updatedAt))")
                    .font(.caption)
                    .foregroundStyle(dirty ? Theme.accent : Color.secondary)
            }
            .font(.caption)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
    }

    private func load(_ e: MemoryEntry) {
        content = e.content
        tags = e.tags.joined(separator: ", ")
        files = e.files
        keyDraft = e.key
        baseline = (e.content, e.tags, e.files)
        tokens = EngramEngine.countTokens(e.content)
    }

    private func scheduleSave() {
        debounce.run(after: .milliseconds(200)) { tokens = EngramEngine.countTokens(content) }
        guard dirty else { return }
        saveDebounce.run(after: .milliseconds(900)) { commit() }
    }

    private func commit() {
        guard dirty else { return }
        let snapshot = (content, parsedTags, files)
        if model.perform(.upsertMemory(key: entry.key, category: nil, content: snapshot.0, tags: snapshot.1, files: snapshot.2)) != nil {
            baseline = snapshot
        }
    }

    private func rename() {
        let new = keyDraft.trimmingCharacters(in: .whitespaces)
        guard !new.isEmpty, new != entry.key else { keyDraft = entry.key; return }
        commit()
        if model.perform(.renameMemory(key: entry.key, to: new), success: "Renamed to '\(new)'") != nil {
            model.selectedMemory = new
        } else {
            keyDraft = entry.key
        }
    }
}

struct MemoryComposer: View {
    enum Seed: Identifiable {
        case new
        case supersede(MemoryEntry)
        var id: String {
            switch self {
            case .new: "new"
            case .supersede(let e): "supersede.\(e.key)"
            }
        }
    }

    var model: WorkspaceModel
    var seed: Seed
    @Environment(\.dismiss) private var dismiss
    @State private var key = ""
    @State private var category: MemoryCategory = .rule
    @State private var content = ""
    @State private var supersedes: String?
    @State private var pinned = false

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text(supersedes == nil ? "New Memory Entry" : "Supersede '\(supersedes!)'")
                .font(.title2.weight(.semibold))
            if supersedes != nil {
                Text("The old entry is tagged [SUPERSEDED], linked to this one and excluded from all future agent context.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
            Grid(alignment: .leading, horizontalSpacing: 12, verticalSpacing: 10) {
                GridRow {
                    Text("Key").foregroundStyle(.secondary)
                    TextField("e.g. db-engine", text: $key).textFieldStyle(.roundedBorder).font(Theme.mono)
                }
                GridRow {
                    Text("Category").foregroundStyle(.secondary)
                    Picker("", selection: $category) {
                        ForEach(MemoryCategory.allCases) { Label($0.singular, systemImage: $0.symbol).tag($0) }
                    }
                    .labelsHidden()
                    .fixedSize()
                }
                GridRow {
                    Text("Pinned").foregroundStyle(.secondary)
                    Toggle("Include in every built context", isOn: $pinned)
                }
            }
            CodeEditor(text: $content, placeholder: "Markdown content", minHeight: 180)
                .padding(6)
                .background(Theme.surface, in: RoundedRectangle(cornerRadius: 8))
                .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(Theme.hairline))
            HStack {
                TokenBadge(tokens: EngramEngine.countTokens(content))
                Spacer()
                Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction)
                Button(supersedes == nil ? "Create" : "Supersede") { create() }
                    .keyboardShortcut(.defaultAction)
                    .disabled(key.trimmingCharacters(in: .whitespaces).isEmpty || content.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
        }
        .padding(22)
        .frame(width: 560)
        .onAppear {
            if case .supersede(let e) = seed {
                supersedes = e.key
                key = nextKey(for: e.key)
                category = e.category
                content = e.content
                pinned = e.pinned
            }
        }
    }

    private func nextKey(for key: String) -> String {
        let existing = Set(model.document?.memory.map(\.key) ?? [])
        var n = 2
        let stem = key.replacingOccurrences(of: #"-v\d+$"#, with: "", options: .regularExpression)
        while existing.contains("\(stem)-v\(n)") { n += 1 }
        return "\(stem)-v\(n)"
    }

    private func create() {
        let k = key.trimmingCharacters(in: .whitespaces)
        if model.document?.memoryEntry(k) != nil, supersedes == nil {
            model.show(.error, "An entry named '\(k)' already exists")
            return
        }
        let op = EngramOp.upsertMemory(key: k, category: category, content: content, supersedesKey: supersedes, pinned: pinned)
        if model.perform(op, success: supersedes.map { "'\(k)' supersedes '\($0)'" } ?? "Created '\(k)'") != nil {
            model.selectedMemory = k
            dismiss()
        }
    }
}
