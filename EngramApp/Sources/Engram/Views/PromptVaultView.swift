import SwiftUI

struct PromptVaultView: View {
    static let uncategorized = String(localized: "Uncategorized")

    @Bindable var model: WorkspaceModel
    @State private var search = ""
    @State private var composer: PromptComposer.Seed?

    private var prompts: [Prompt] { model.document?.prompts ?? [] }

    private var categories: [String] {
        Array(Set(prompts.map { $0.category.isEmpty ? Self.uncategorized : $0.category })).sorted()
    }

    var body: some View {
        HSplitView {
            list.frame(minWidth: 230, idealWidth: 270, maxWidth: 380)
            Group {
                if let name = model.selectedPrompt, let prompt = model.document?.prompt(name) {
                    PromptEditor(model: model, prompt: prompt, categories: categories).id(prompt.name)
                } else {
                    EmptyState(
                        symbol: "text.quote",
                        title: "Prompt vault",
                        message: "Reusable prompts for this project. Use {{variable}} placeholders and fill them in when you copy.",
                        note: "Agents fetch prompts with get_prompt.",
                        action: ("New prompt", { composer = .new })
                    )
                }
            }
            .frame(minWidth: 420, maxWidth: .infinity, maxHeight: .infinity)
        }
        .sheet(item: $composer) { PromptComposer(model: model, seed: $0, categories: categories) }
        .onChange(of: model.intent) { _, intent in
            if intent == .newPrompt { composer = .new; model.intent = nil }
        }
        .onAppear {
            if model.selectedPrompt == nil { model.selectedPrompt = prompts.first?.name }
        }
    }

    private var list: some View {
        VStack(spacing: 0) {
            HStack(spacing: 6) {
                SearchField(text: $search, prompt: "Filter prompts")
                Button { composer = .new } label: { Image(systemName: "plus") }
                    .buttonStyle(.borderless)
                    .help("New prompt")
            }
            .padding(10)
            Divider()
            List(selection: $model.selectedPrompt) {
                let q = search.lowercased()
                ForEach(categories, id: \.self) { cat in
                    let items = prompts.filter {
                        ($0.category.isEmpty ? Self.uncategorized : $0.category) == cat
                            && (q.isEmpty || $0.name.lowercased().contains(q) || $0.body.lowercased().contains(q))
                    }
                    if !items.isEmpty {
                        Section(cat) {
                            ForEach(items) { p in
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(verbatim: p.name).font(.callout.weight(.medium)).lineLimit(1)
                                    Text(verbatim: p.description.isEmpty ? p.body.replacingOccurrences(of: "\n", with: " ") : p.description)
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                        .lineLimit(1)
                                }
                                .padding(.vertical, 2)
                                .tag(p.name)
                                .contextMenu {
                                    Button("Copy template") { model.copy(p.body, message: "Copied \(p.name)") }
                                    Button("Duplicate…") { composer = .duplicate(p) }
                                    Divider()
                                    Button("Delete…", role: .destructive) {
                                        model.delete(.deletePrompt(name: p.name), title: p.name) {
                                            if model.selectedPrompt == p.name { model.selectedPrompt = nil }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            .listStyle(.inset)
        }
    }
}

struct PromptDraft: Equatable {
    var name: String
    var category: String
    var description: String
    var body: String

    init(_ p: Prompt) {
        name = p.name
        category = p.category
        description = p.description
        body = p.body
    }
}

private struct PromptEditor: View {
    var model: WorkspaceModel
    var prompt: Prompt
    var categories: [String]

    @State private var draft: EditorDraft<PromptDraft>
    @State private var tokens = 0
    @State private var debounce = Debouncer()
    @State private var showFill = false

    init(model: WorkspaceModel, prompt: Prompt, categories: [String]) {
        self.model = model
        self.prompt = prompt
        self.categories = categories
        _draft = State(initialValue: EditorDraft(PromptDraft(prompt)))
    }

    private var variables: [String] { Prompt.variables(in: draft.value.body) }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 10) {
                Image(systemName: "text.quote").foregroundStyle(Theme.accent).font(.title3)
                TextField("name", text: $draft.value.name)
                    .textFieldStyle(.plain)
                    .font(.system(.title3, design: .monospaced).weight(.semibold))
                    .onSubmit { _ = commit() }
                Spacer()
                Button {
                    if variables.isEmpty { copyRendered([:]) } else { showFill = true }
                } label: {
                    Label("Copy", systemImage: "doc.on.doc")
                }
                .keyboardShortcut("c", modifiers: [.command, .shift])
                .help("Copy the prompt and fill its variables (⇧⌘C)")
                .popover(isPresented: $showFill, arrowEdge: .bottom) {
                    VariableForm(variables: variables, prompt: prompt.name) { values in
                        showFill = false
                        copyRendered(values)
                    }
                }
                Menu {
                    Button("Copy template") { model.copy(draft.value.body, message: "Copied the template") }
                    Divider()
                    Button("Delete…", role: .destructive) {
                        model.delete(.deletePrompt(name: prompt.name), title: prompt.name) { model.selectedPrompt = nil }
                    }
                } label: { Image(systemName: "ellipsis.circle") }
                    .menuStyle(.borderlessButton)
                    .fixedSize()
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 10)

            HStack(spacing: 10) {
                HStack(spacing: 4) {
                    Image(systemName: "folder").foregroundStyle(.tertiary)
                    TextField("Category", text: $draft.value.category).textFieldStyle(.plain).frame(width: 140)
                    if !categories.isEmpty {
                        Menu {
                            ForEach(categories.filter { $0 != PromptVaultView.uncategorized }, id: \.self) { c in
                                Button(c) { draft.value.category = c }
                            }
                        } label: { Image(systemName: "chevron.down") }
                            .menuStyle(.borderlessButton)
                            .fixedSize()
                    }
                }
                Divider().frame(height: 14)
                TextField("Short description", text: $draft.value.description).textFieldStyle(.plain)
            }
            .font(.callout)
            .padding(.horizontal, 16)
            .padding(.bottom, 10)
            if draft.conflict != nil {
                ConflictBar(onLoad: { draft.loadExternal() }, onKeep: {
                    draft.keepMine()
                    _ = commit()
                })
            }
            Divider()
            CodeEditor(text: $draft.value.body, placeholder: "Write the prompt. Use {{variable}} for placeholders.", minHeight: 240)
                .padding(14)
                .background(Theme.surface)
            Divider()
            HStack(spacing: 8) {
                if variables.isEmpty {
                    Text("No variables").font(.caption).foregroundStyle(.tertiary)
                } else {
                    Image(systemName: "curlybraces").font(.caption).foregroundStyle(.secondary)
                    ForEach(variables, id: \.self) { Chip(text: "\($0)", color: Theme.accent) }
                }
                Spacer()
                TokenBadge(tokens: tokens)
                SaveStatus(model: model, dirty: draft.dirty)
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 8)
        }
        .onAppear {
            tokens = EngramEngine.countTokens(prompt.body)
            model.registerCommit("prompt") { commit() }
        }
        .onDisappear {
            _ = commit()
            model.unregisterCommit("prompt")
        }
        .onChange(of: prompt) { _, new in draft.sync(external: PromptDraft(new)) }
        .onChange(of: draft.value) { _, value in
            debounce.run(after: .milliseconds(300)) { tokens = EngramEngine.countTokens(value.body) }
            draft.scheduleSave { _ = commit() }
        }
    }

    @discardableResult
    private func commit() -> Bool {
        guard draft.dirty, draft.conflict == nil else { return false }
        let v = draft.value
        let name = v.name.trimmingCharacters(in: .whitespaces)
        guard !name.isEmpty else {
            draft.value.name = prompt.name
            return false
        }
        draft.markSaved(v)
        let op = EngramOp.upsertPrompt(
            name: name, body: v.body, category: v.category, description: v.description,
            renameFrom: name == prompt.name ? nil : prompt.name
        )
        model.perform(op) { result in
            if result != nil {
                if name != prompt.name { model.selectedPrompt = name }
            } else {
                draft.reset(PromptDraft(prompt))
            }
        }
        return true
    }

    private func copyRendered(_ values: [String: String]) {
        _ = commit()
        do {
            guard let engine = model.engine else { return }
            let r = try engine.renderPrompt(draft.value.name.trimmingCharacters(in: .whitespaces), variables: values)
            model.copy(r.text, message: r.missing.isEmpty ? "Prompt copied" : "Copied with unfilled variables: \(r.missing.joined(separator: ", "))")
        } catch {
            model.show(.error, verbatim: error.localizedDescription)
        }
    }
}

struct PromptComposer: View {
    enum Seed: Identifiable {
        case new
        case duplicate(Prompt)
        var id: String {
            switch self {
            case .new: "new"
            case .duplicate(let p): "duplicate.\(p.name)"
            }
        }
    }

    var model: WorkspaceModel
    var seed: Seed
    var categories: [String]
    @Environment(\.dismiss) private var dismiss
    @State private var name = ""
    @State private var category = ""
    @State private var description = ""
    @State private var body_ = ""
    @State private var tokens = 0
    @State private var debounce = Debouncer()

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("New prompt").font(.title2.weight(.semibold))
            Grid(alignment: .leading, horizontalSpacing: 12, verticalSpacing: 10) {
                GridRow {
                    Text("Name").foregroundStyle(.secondary)
                    TextField("review-diff", text: $name).textFieldStyle(.roundedBorder).font(.body.monospaced())
                }
                GridRow {
                    Text("Category").foregroundStyle(.secondary)
                    HStack {
                        TextField("Review", text: $category).textFieldStyle(.roundedBorder)
                        if !categories.isEmpty {
                            Menu {
                                ForEach(categories.filter { $0 != PromptVaultView.uncategorized }, id: \.self) { c in Button(c) { category = c } }
                            } label: { Image(systemName: "chevron.down") }
                                .menuStyle(.borderlessButton)
                                .fixedSize()
                        }
                    }
                }
                GridRow {
                    Text("Description").foregroundStyle(.secondary)
                    TextField("What it is for", text: $description).textFieldStyle(.roundedBorder)
                }
            }
            CodeEditor(text: $body_, placeholder: "Write the prompt. Use {{variable}} for placeholders.", minHeight: 200)
                .padding(6)
                .background(Theme.surface, in: RoundedRectangle(cornerRadius: 8))
                .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(Theme.hairline))
            HStack {
                TokenBadge(tokens: tokens)
                if !Prompt.variables(in: body_).isEmpty {
                    Text("Variables: \(Prompt.variables(in: body_).joined(separator: ", "))").font(.caption).foregroundStyle(.secondary)
                }
                Spacer()
                Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction)
                Button("Create", action: create)
                    .keyboardShortcut(.defaultAction)
                    .disabled(name.trimmingCharacters(in: .whitespaces).isEmpty)
            }
        }
        .padding(22)
        .frame(width: 580)
        .onChange(of: body_) { _, value in
            debounce.run(after: .milliseconds(300)) { tokens = EngramEngine.countTokens(value) }
        }
        .onAppear {
            if case .duplicate(let p) = seed {
                name = unique("\(p.name)-copy")
                category = p.category
                description = p.description
                body_ = p.body
            }
        }
    }

    private func unique(_ base: String) -> String {
        let names = Set(model.document?.prompts.map(\.name) ?? [])
        var candidate = base, n = 2
        while names.contains(candidate) { candidate = "\(base)-\(n)"; n += 1 }
        return candidate
    }

    private func create() {
        let n = name.trimmingCharacters(in: .whitespaces)
        if model.document?.prompt(n) != nil {
            model.show(.error, "A prompt named \(n) already exists. Choose another name.")
            return
        }
        model.perform(.upsertPrompt(name: n, body: body_, category: category, description: description), success: "Created \(n)") { result in
            guard result != nil else { return }
            model.selectedPrompt = n
            dismiss()
        }
    }
}

private struct VariableForm: View {
    var variables: [String]
    var prompt: String
    var onCopy: ([String: String]) -> Void
    @AppStorage("prompt.lastValues") private var stored = "{}"
    @State private var values: [String: String] = [:]

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Fill variables").font(.headline)
            ForEach(variables, id: \.self) { v in
                VStack(alignment: .leading, spacing: 3) {
                    Text(verbatim: v).font(.caption.monospaced()).foregroundStyle(.secondary)
                    TextField(v, text: Binding(get: { values[v, default: ""] }, set: { values[v] = $0 }), axis: .vertical)
                        .textFieldStyle(.roundedBorder)
                        .lineLimit(1...4)
                }
            }
            HStack {
                Spacer()
                Button("Copy prompt") {
                    persist()
                    onCopy(values.filter { !$0.value.isEmpty })
                }
                .keyboardShortcut(.defaultAction)
            }
        }
        .padding(16)
        .frame(width: 340)
        .onAppear {
            let all = (try? JSONDecoder().decode([String: [String: String]].self, from: Data(stored.utf8))) ?? [:]
            values = all[prompt] ?? [:]
        }
    }

    private func persist() {
        var all = (try? JSONDecoder().decode([String: [String: String]].self, from: Data(stored.utf8))) ?? [:]
        all[prompt] = values
        if let data = try? JSONEncoder().encode(all) { stored = String(decoding: data, as: UTF8.self) }
    }
}
