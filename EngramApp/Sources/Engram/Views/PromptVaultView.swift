import SwiftUI

struct PromptVaultView: View {
    @Bindable var model: WorkspaceModel
    @State private var search = ""

    private var prompts: [Prompt] { model.document?.prompts ?? [] }

    private var categories: [String] {
        Array(Set(prompts.map { $0.category.isEmpty ? "Uncategorized" : $0.category })).sorted()
    }

    var body: some View {
        HSplitView {
            list.frame(minWidth: 230, idealWidth: 270, maxWidth: 380)
            Group {
                if let name = model.selectedPrompt, let prompt = model.document?.prompt(name) {
                    PromptEditor(model: model, prompt: prompt, categories: categories)
                        .id(prompt.name)
                } else {
                    EmptyState(
                        symbol: "text.quote",
                        title: "Prompt Vault",
                        message: "Project-specific prompts and templates with {{variable}} placeholders. Agents fetch them with get_prompt.",
                        action: ("New Prompt", newPrompt)
                    )
                }
            }
            .frame(minWidth: 420, maxWidth: .infinity, maxHeight: .infinity)
        }
        .onChange(of: model.intent) { _, intent in
            if intent == .newPrompt { newPrompt(); model.intent = nil }
        }
        .onAppear {
            if model.selectedPrompt == nil { model.selectedPrompt = prompts.first?.name }
        }
    }

    private var list: some View {
        VStack(spacing: 0) {
            HStack(spacing: 6) {
                SearchField(text: $search, prompt: "Filter prompts")
                Button(action: newPrompt) { Image(systemName: "plus") }
                    .buttonStyle(.borderless)
                    .help("New prompt")
            }
            .padding(10)
            Divider()
            List(selection: $model.selectedPrompt) {
                let q = search.lowercased()
                ForEach(categories, id: \.self) { cat in
                    let items = prompts.filter {
                        ($0.category.isEmpty ? "Uncategorized" : $0.category) == cat
                            && (q.isEmpty || $0.name.lowercased().contains(q) || $0.body.lowercased().contains(q))
                    }
                    if !items.isEmpty {
                        Section(cat) {
                            ForEach(items) { p in
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(p.name).font(.callout.weight(.medium)).lineLimit(1)
                                    Text(p.description.isEmpty ? p.body.replacingOccurrences(of: "\n", with: " ") : p.description)
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                        .lineLimit(1)
                                }
                                .padding(.vertical, 2)
                                .tag(p.name)
                                .contextMenu {
                                    Button("Copy Raw Template") { model.copy(p.body, message: "Copied '\(p.name)'") }
                                    Button("Duplicate") { duplicate(p) }
                                    Divider()
                                    Button("Delete", role: .destructive) { delete(p) }
                                }
                            }
                        }
                    }
                }
            }
            .listStyle(.inset)
            .overlay {
                if prompts.isEmpty { Text("No prompts yet").foregroundStyle(.tertiary) }
            }
        }
    }

    private func uniqueName(_ base: String) -> String {
        let names = Set(prompts.map(\.name))
        var name = base, n = 2
        while names.contains(name) { name = "\(base)-\(n)"; n += 1 }
        return name
    }

    private func newPrompt() {
        let name = uniqueName("untitled-prompt")
        if model.perform(.upsertPrompt(name: name, body: "", category: nil, description: nil)) != nil {
            model.selectedPrompt = name
        }
    }

    private func duplicate(_ p: Prompt) {
        let name = uniqueName(p.name + "-copy")
        if model.perform(.upsertPrompt(name: name, body: p.body, category: p.category, description: p.description)) != nil {
            model.selectedPrompt = name
        }
    }

    private func delete(_ p: Prompt) {
        if model.selectedPrompt == p.name { model.selectedPrompt = nil }
        model.perform(.deletePrompt(name: p.name), success: "Deleted '\(p.name)'")
    }
}

private struct PromptEditor: View {
    var model: WorkspaceModel
    var prompt: Prompt
    var categories: [String]

    @State private var name = ""
    @State private var category = ""
    @State private var description = ""
    @State private var body_ = ""
    @State private var baseline = ["", "", "", ""]
    @State private var saveDebounce = Debouncer()
    @State private var tokens = 0
    @State private var showFill = false

    private var current: [String] { [name, category, description, body_] }
    private var dirty: Bool { current != baseline }
    private var variables: [String] { Prompt.variables(in: body_) }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 10) {
                Image(systemName: "text.quote").foregroundStyle(Theme.accent).font(.title3)
                TextField("name", text: $name)
                    .textFieldStyle(.plain)
                    .font(.system(.title3, design: .monospaced).weight(.semibold))
                    .onSubmit(commit)
                Spacer()
                Button {
                    if variables.isEmpty { copyRendered([:]) } else { showFill = true }
                } label: {
                    Label("Copy", systemImage: "doc.on.doc")
                }
                .keyboardShortcut("c", modifiers: [.command, .shift])
                .help("Copy prompt, filling {{variables}} (⇧⌘C)")
                .popover(isPresented: $showFill, arrowEdge: .bottom) {
                    VariableForm(variables: variables, prompt: prompt.name) { values in
                        showFill = false
                        copyRendered(values)
                    }
                }
                Menu {
                    Button("Copy Raw Template") { model.copy(body_, message: "Copied template") }
                    Divider()
                    Button("Delete", role: .destructive) {
                        model.selectedPrompt = nil
                        model.perform(.deletePrompt(name: prompt.name), success: "Deleted '\(prompt.name)'")
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
                    TextField("Category", text: $category).textFieldStyle(.plain).frame(width: 140)
                    if !categories.isEmpty {
                        Menu {
                            ForEach(categories.filter { $0 != "Uncategorized" }, id: \.self) { c in Button(c) { category = c } }
                        } label: { Image(systemName: "chevron.down") }
                            .menuStyle(.borderlessButton)
                            .fixedSize()
                    }
                }
                Divider().frame(height: 14)
                TextField("Short description", text: $description).textFieldStyle(.plain)
            }
            .font(.callout)
            .padding(.horizontal, 16)
            .padding(.bottom, 10)
            Divider()

            CodeEditor(text: $body_, placeholder: "Write the prompt. Use {{variable}} for placeholders.", minHeight: 240)
                .padding(14)
                .background(Theme.surface)

            Divider()
            HStack(spacing: 8) {
                if variables.isEmpty {
                    Text("No variables").font(.caption).foregroundStyle(.tertiary)
                } else {
                    Image(systemName: "curlybraces").font(.caption).foregroundStyle(.secondary)
                    ForEach(variables, id: \.self) { Chip(text: $0, color: Theme.accent) }
                }
                Spacer()
                TokenBadge(tokens: tokens)
                Text(dirty ? "Edited" : "Saved").font(.caption).foregroundStyle(dirty ? Theme.accent : .secondary)
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 8)
        }
        .onAppear {
            load()
            model.registerCommit("prompt") { commit() }
        }
        .onDisappear {
            commit()
            model.unregisterCommit("prompt")
        }
        .onChange(of: prompt) { if !dirty { load() } }
        .onChange(of: current) {
            tokens = EngramEngine.countTokens(body_)
            if dirty { saveDebounce.run(after: .milliseconds(900)) { commit() } }
        }
    }

    private func load() {
        name = prompt.name
        category = prompt.category
        description = prompt.description
        body_ = prompt.body
        baseline = current
        tokens = EngramEngine.countTokens(body_)
    }

    private func commit() {
        guard dirty else { return }
        let newName = name.trimmingCharacters(in: .whitespaces)
        guard !newName.isEmpty else { name = prompt.name; return }
        let snapshot = current
        let op = EngramOp.upsertPrompt(
            name: newName, body: body_, category: category, description: description,
            renameFrom: newName == prompt.name ? nil : prompt.name
        )
        if model.perform(op) != nil {
            baseline = snapshot
            if newName != prompt.name { model.selectedPrompt = newName }
        } else {
            name = prompt.name
        }
    }

    private func copyRendered(_ values: [String: String]) {
        commit()
        do {
            guard let engine = model.engine else { return }
            let r = try engine.renderPrompt(name.trimmingCharacters(in: .whitespaces), variables: values)
            model.copy(r.text, message: r.missing.isEmpty ? "Prompt copied" : "Copied with unfilled: \(r.missing.joined(separator: ", "))")
        } catch {
            model.show(.error, error.localizedDescription)
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
                    Text(v).font(.caption.monospaced()).foregroundStyle(.secondary)
                    TextField(v, text: Binding(get: { values[v, default: ""] }, set: { values[v] = $0 }), axis: .vertical)
                        .textFieldStyle(.roundedBorder)
                        .lineLimit(1...4)
                }
            }
            HStack {
                Spacer()
                Button("Copy Prompt") {
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
