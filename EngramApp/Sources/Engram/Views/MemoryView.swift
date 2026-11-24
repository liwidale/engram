import SwiftUI

struct MemoryView: View {
    @Bindable var model: WorkspaceModel
    @State private var search = ""
    @State private var showSuperseded = true
    @State private var categoryFilter: MemoryCategory?
    @State private var composer: MemoryComposer.Seed?

    var body: some View {
        HSplitView {
            list.frame(minWidth: 260, idealWidth: 300, maxWidth: 420)
            Group {
                if let key = model.selectedMemory, let entry = model.document?.memoryEntry(key) {
                    MemoryEditor(model: model, entry: entry) { composer = .supersede(entry) }
                        .id(entry.key)
                } else if model.document?.isEmpty == true {
                    OnboardingCard(model: model) { composer = .new }
                } else {
                    EmptyState(
                        symbol: "brain",
                        title: "Project memory",
                        message: "The rules, stack and conventions an agent should never forget. Select an entry, or add one.",
                        note: "Agents read this through engram://memory.",
                        action: ("New entry", { composer = .new })
                    )
                }
            }
            .frame(minWidth: 380, maxWidth: .infinity, maxHeight: .infinity)
        }
        .sheet(item: $composer) { MemoryComposer(model: model, seed: $0) }
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
                && (q.isEmpty || e.key.lowercased().contains(q) || e.content.lowercased().contains(q)
                    || e.tags.contains { $0.lowercased().contains(q) })
        }
    }

    private var list: some View {
        VStack(spacing: 0) {
            HStack(spacing: 6) {
                SearchField(text: $search, prompt: "Filter by key, text or tag")
                Menu {
                    Picker("Category", selection: $categoryFilter) {
                        Text("All categories").tag(MemoryCategory?.none)
                        ForEach(MemoryCategory.allCases) { Label($0.title, systemImage: $0.symbol).tag(Optional($0)) }
                    }
                    .pickerStyle(.inline)
                    Toggle("Show superseded", isOn: $showSuperseded)
                } label: {
                    Image(systemName: categoryFilter == nil && showSuperseded ? "line.3.horizontal.decrease.circle" : "line.3.horizontal.decrease.circle.fill")
                }
                .menuStyle(.borderlessButton)
                .fixedSize()
                .help("Filter")
                Button { composer = .new } label: { Image(systemName: "plus") }
                    .buttonStyle(.borderless)
                    .help("New memory entry (⇧⌘N)")
            }
            .padding(10)
            Divider()
            List(selection: $model.selectedMemory) {
                let entries = filtered
                ForEach(MemoryCategory.allCases) { cat in
                    let group = entries.filter { $0.category == cat }
                        .sorted { ($0.isActive ? 0 : 1, $0.pinned ? 0 : 1, $0.key) < ($1.isActive ? 0 : 1, $1.pinned ? 0 : 1, $1.key) }
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
                if filtered.isEmpty, model.document?.memory.isEmpty == false {
                    Text("No entries match").foregroundStyle(.tertiary)
                }
            }
        }
    }

    @ViewBuilder
    private func menu(for e: MemoryEntry) -> some View {
        Button("Copy as Markdown") { model.copy("### \(e.key)\n\n\(e.content)", message: "Copied \(e.key)") }
        Button(e.pinned ? "Unpin" : "Pin to every context") {
            model.perform(.upsertMemory(key: e.key, category: nil, content: e.content, pinned: !e.pinned))
        }
        Divider()
        if e.isActive {
            Button("Supersede with new entry…") { composer = .supersede(e) }
            Button("Retire as superseded") { model.perform(.setMemoryStatus(key: e.key, .superseded)) }
        } else {
            Button("Restore as active") { model.perform(.setMemoryStatus(key: e.key, .active)) }
        }
        Divider()
        Button("Delete…", role: .destructive) {
            model.delete(.deleteMemory(key: e.key), title: e.key) {
                if model.selectedMemory == e.key { model.selectedMemory = nil }
            }
        }
    }
}

private struct MemoryRow: View {
    var entry: MemoryEntry

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            HStack(spacing: 6) {
                Text(verbatim: entry.key)
                    .font(.system(.callout, design: .monospaced).weight(.medium))
                    .strikethrough(!entry.isActive)
                    .foregroundStyle(entry.isActive ? .primary : .secondary)
                    .lineLimit(1)
                if entry.pinned {
                    Image(systemName: "pin.fill").font(.caption2).foregroundStyle(Theme.accent).help("Pinned to every context")
                }
                Spacer(minLength: 0)
                if !entry.isActive { Chip(text: "Superseded", color: Theme.superseded) }
            }
            Text(verbatim: entry.summary)
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(2)
            if !entry.tags.isEmpty {
                Text(verbatim: entry.tags.map { "#\($0)" }.joined(separator: " "))
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
                    .lineLimit(1)
            }
        }
        .padding(.vertical, 3)
        .opacity(entry.isActive ? 1 : 0.7)
    }
}

struct MemoryDraft: Equatable {
    var content: String
    var tags: String
    var files: [String]

    init(_ e: MemoryEntry) {
        content = e.content
        tags = e.tags.joined(separator: ", ")
        files = e.files
    }

    var tagList: [String] {
        tags.split(separator: ",").map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }
    }
}

private struct MemoryEditor: View {
    enum Mode: String, CaseIterable { case edit, split, preview }

    var model: WorkspaceModel
    var entry: MemoryEntry
    var onSupersede: () -> Void

    @AppStorage(Pref.memoryMode) private var modeRaw = "edit"
    @State private var draft: EditorDraft<MemoryDraft>
    @State private var keyDraft: String
    @State private var tokens = 0
    @State private var tokenDebounce = Debouncer()
    @State private var showDetails: Bool

    init(model: WorkspaceModel, entry: MemoryEntry, onSupersede: @escaping () -> Void) {
        self.model = model
        self.entry = entry
        self.onSupersede = onSupersede
        _draft = State(initialValue: EditorDraft(MemoryDraft(entry)))
        _keyDraft = State(initialValue: entry.key)
        _showDetails = State(initialValue: !entry.tags.isEmpty || !entry.files.isEmpty)
    }

    private var mode: Mode { Mode(rawValue: modeRaw) ?? .edit }

    var body: some View {
        VStack(spacing: 0) {
            header
            if draft.conflict != nil {
                ConflictBar(onLoad: { draft.loadExternal() }, onKeep: {
                    draft.keepMine()
                    _ = commit()
                })
            }
            Divider()
            GeometryReader { geo in editorBody(width: geo.size.width) }
            Divider()
            footer
        }
        .onAppear {
            tokens = EngramEngine.countTokens(entry.content)
            model.registerCommit("memory") { commit() }
        }
        .onDisappear {
            _ = commit()
            model.unregisterCommit("memory")
        }
        .onChange(of: entry) { _, new in draft.sync(external: MemoryDraft(new)) }
        .onChange(of: draft.value) { _, value in
            tokenDebounce.run(after: .milliseconds(300)) { tokens = EngramEngine.countTokens(value.content) }
            draft.scheduleSave { _ = commit() }
        }
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
                _ = commit()
                model.perform(.upsertMemory(key: entry.key, category: c, content: draft.value.content))
            })) {
                ForEach(MemoryCategory.allCases) { Label($0.singular, systemImage: $0.symbol).tag($0) }
            }
            .labelsHidden()
            .fixedSize()
            Toggle(isOn: Binding(get: { entry.pinned }, set: { p in
                _ = commit()
                model.perform(.upsertMemory(key: entry.key, category: nil, content: draft.value.content, pinned: p))
            })) {
                Image(systemName: entry.pinned ? "pin.fill" : "pin")
            }
            .toggleStyle(.button)
            .help("Pinned entries are included in every built context")
            Picker("Mode", selection: $modeRaw) {
                Text("Edit").tag(Mode.edit.rawValue)
                Text("Split").tag(Mode.split.rawValue)
                Text("Preview").tag(Mode.preview.rawValue)
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .fixedSize()
            Menu {
                Button("Copy as Markdown") { model.copy("### \(entry.key)\n\n\(draft.value.content)", message: "Copied \(entry.key)") }
                if entry.isActive {
                    Button("Supersede with new entry…", action: onSupersede)
                    Button("Retire as superseded") {
                        _ = commit()
                        model.perform(.setMemoryStatus(key: entry.key, .superseded))
                    }
                } else {
                    Button("Restore as active") { model.perform(.setMemoryStatus(key: entry.key, .active)) }
                }
                Divider()
                Button("Delete…", role: .destructive) {
                    model.delete(.deleteMemory(key: entry.key), title: entry.key) { model.selectedMemory = nil }
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
    private func editorBody(width: CGFloat) -> some View {
        let editor = CodeEditor(text: $draft.value.content, placeholder: "Write in Markdown", minHeight: 160)
            .padding(12)
            .background(Theme.surface)
        let preview = ScrollView { MarkdownView(text: draft.value.content).padding(18) }
            .background(Theme.surface)
        switch mode {
        case .edit: editor
        case .preview: preview
        case .split:
            if width < 520 {
                editor
            } else {
                HSplitView {
                    editor.frame(minWidth: 240)
                    preview.frame(minWidth: 240)
                }
            }
        }
    }

    private var footer: some View {
        VStack(alignment: .leading, spacing: 8) {
            DisclosureGroup(isExpanded: $showDetails) {
                HStack(alignment: .top, spacing: 16) {
                    VStack(alignment: .leading, spacing: 4) {
                        SectionLabel("Tags")
                        TextField("comma, separated", text: $draft.value.tags).textFieldStyle(.roundedBorder)
                    }
                    .frame(maxWidth: 240)
                    VStack(alignment: .leading, spacing: 4) {
                        SectionLabel("Linked files", trailing: String(localized: "checked by Context health"))
                        FileListEditor(model: model, files: $draft.value.files)
                    }
                }
                .padding(.top, 6)
            } label: {
                Text(detailsSummary).font(.caption).foregroundStyle(.secondary)
            }
            HStack(spacing: 12) {
                if let s = entry.supersedes {
                    Button { model.selectedMemory = s } label: { Label("Supersedes \(s)", systemImage: "arrow.uturn.left") }
                        .buttonStyle(.link)
                }
                if let by = entry.supersededBy {
                    Button { model.selectedMemory = by } label: { Label("Replaced by \(by)", systemImage: "arrow.uturn.right") }
                        .buttonStyle(.link)
                }
                Spacer()
                TokenBadge(tokens: tokens)
                SaveStatus(model: model, dirty: draft.dirty)
            }
            .font(.caption)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
    }

    private var detailsSummary: String {
        let tags = draft.value.tagList.count, files = draft.value.files.count
        if tags == 0, files == 0 { return String(localized: "Tags and linked files") }
        return String(localized: "\(tags) tags · \(files) linked files")
    }

    @discardableResult
    private func commit() -> Bool {
        guard draft.dirty, draft.conflict == nil else { return false }
        let saved = draft.value
        draft.markSaved(saved)
        model.perform(.upsertMemory(key: entry.key, category: nil, content: saved.content, tags: saved.tagList, files: saved.files))
        return true
    }

    private func rename() {
        let new = keyDraft.trimmingCharacters(in: .whitespaces)
        guard !new.isEmpty, new != entry.key else { keyDraft = entry.key; return }
        _ = commit()
        model.perform(.renameMemory(key: entry.key, to: new), success: "Renamed to \(new)") { result in
            if result != nil { model.selectedMemory = new } else { keyDraft = entry.key }
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
    @State private var tokens = 0
    @State private var debounce = Debouncer()

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text(supersedes.map { String(localized: "Supersede \($0)") } ?? String(localized: "New memory entry"))
                .font(.title2.weight(.semibold))
            if supersedes != nil {
                Text("The old entry is marked superseded, linked to this one and left out of future agent context.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
            Grid(alignment: .leading, horizontalSpacing: 12, verticalSpacing: 10) {
                GridRow {
                    Text("Key").foregroundStyle(.secondary)
                    TextField("db-engine", text: $key).textFieldStyle(.roundedBorder).font(.body.monospaced())
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
            CodeEditor(text: $content, placeholder: "Write in Markdown", minHeight: 180)
                .padding(6)
                .background(Theme.surface, in: RoundedRectangle(cornerRadius: 8))
                .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(Theme.hairline))
            HStack {
                TokenBadge(tokens: tokens)
                Spacer()
                Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction)
                Button(supersedes == nil ? "Create" : "Supersede", action: create)
                    .keyboardShortcut(.defaultAction)
                    .disabled(key.trimmingCharacters(in: .whitespaces).isEmpty || content.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
        }
        .padding(22)
        .frame(width: 560)
        .onChange(of: content) { _, value in
            debounce.run(after: .milliseconds(300)) { tokens = EngramEngine.countTokens(value) }
        }
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
            model.show(.error, "An entry named \(k) already exists. Choose another key.")
            return
        }
        let op = EngramOp.upsertMemory(key: k, category: category, content: content, supersedesKey: supersedes, pinned: pinned)
        let message: String.LocalizationValue = supersedes.map { "\(k) supersedes \($0)" } ?? "Created \(k)"
        model.perform(op, success: message) { result in
            guard result != nil else { return }
            model.selectedMemory = k
            dismiss()
        }
    }
}

struct OnboardingCard: View {
    var model: WorkspaceModel
    var startBlank: () -> Void
    @State private var review: ImportReview?
    @State private var scanning = false

    var body: some View {
        VStack(spacing: 18) {
            VStack(spacing: 6) {
                Text("Start this project's memory").font(.title2.weight(.semibold))
                Text("Engram can read your build files and agent instructions and draft the first entries. You review everything before it is added.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: 440)
            }
            HStack(spacing: 14) {
                choice(symbol: "wand.and.stars", title: "Scan repository", detail: "Draft the stack, commands, rules and starter prompts.", primary: true, action: scan)
                choice(symbol: "square.and.pencil", title: "Start blank", detail: "Write the first entry yourself.", primary: false, action: startBlank)
            }
            if scanning { ProgressView().controlSize(.small) }
            Text(verbatim: model.resolvedRepoRoot.path.replacingOccurrences(of: NSHomeDirectory(), with: "~"))
                .font(.caption.monospaced())
                .foregroundStyle(.tertiary)
        }
        .padding(40)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .sheet(item: $review) { ImportReviewSheet(model: model, review: $0) }
        .onChange(of: model.intent) { _, intent in
            if intent == .bootstrap { model.intent = nil; scan() }
        }
    }

    private func choice(symbol: String, title: LocalizedStringKey, detail: LocalizedStringKey, primary: Bool, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            VStack(alignment: .leading, spacing: 8) {
                Image(systemName: symbol).font(.title2).foregroundStyle(primary ? Theme.accent : .secondary)
                Text(title).font(.headline)
                Text(detail).font(.callout).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
            }
            .padding(16)
            .frame(width: 220, height: 130, alignment: .topLeading)
            .background(Theme.surface, in: RoundedRectangle(cornerRadius: Theme.cardRadius))
            .overlay(RoundedRectangle(cornerRadius: Theme.cardRadius).strokeBorder(primary ? Theme.accent.opacity(0.5) : Theme.hairline))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    private func scan() {
        guard let engine = model.engine, !scanning else { return }
        scanning = true
        Task {
            let result = await Task.detached { Result { try engine.bootstrap() } }.value
            scanning = false
            switch result {
            case .success(let b) where b.memory.isEmpty && b.prompts.isEmpty:
                model.show(.info, "No build files or agent instructions found in this folder.")
            case .success(let b):
                review = ImportReview(title: String(localized: "Review the draft"), memory: b.memory, prompts: b.prompts)
            case .failure(let e):
                model.show(.error, verbatim: e.localizedDescription)
            }
        }
    }
}
