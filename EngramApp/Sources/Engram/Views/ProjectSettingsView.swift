import AppKit
import SwiftUI
import UniformTypeIdentifiers

struct ProjectSettingsView: View {
    var model: WorkspaceModel
    @Environment(\.dismiss) private var dismiss
    @State private var name = ""
    @State private var description = ""
    @State private var repoRoot = ""
    @State private var customDays = false
    @State private var days = ProjectSettings.defaultStaleHandoffDays
    @State private var ignoredDirs: [String] = []
    @State private var newDir = ""
    @State private var ignoredIssues: [String] = []
    @State private var customScore = false
    @State private var minScore = ProjectSettings.defaultContextMinScore
    @State private var serverName = ""
    @State private var review: ImportReview?

    var body: some View {
        VStack(spacing: 0) {
            Form {
                Section("Project") {
                    TextField("Name", text: $name)
                    VStack(alignment: .leading, spacing: 4) {
                        Text("Description").font(.callout).foregroundStyle(.secondary)
                        CodeEditor(text: $description, placeholder: "What this project is, in a sentence or two", minHeight: 54, font: .body)
                    }
                }
                Section("Codebase") {
                    LabeledContent("Folder") {
                        Text(verbatim: displayRoot).font(.callout.monospaced()).lineLimit(1).truncationMode(.middle)
                    }
                    HStack {
                        Button("Choose folder…", action: chooseRoot)
                        if !repoRoot.isEmpty { Button("Use .engram folder") { repoRoot = "" } }
                    }
                }
                Section("Context health") {
                    Toggle("Custom stale handoff threshold", isOn: $customDays)
                    if customDays {
                        Stepper(value: $days, in: 1...90) { Text("Flag the handoff after \(days) days") }
                    }
                    VStack(alignment: .leading, spacing: 6) {
                        Text("Ignored folders").font(.callout).foregroundStyle(.secondary)
                        ForEach(ignoredDirs, id: \.self) { dir in
                            HStack {
                                Text(verbatim: dir).font(.callout.monospaced())
                                Spacer()
                                Button("Remove") { ignoredDirs.removeAll { $0 == dir } }.buttonStyle(.link)
                            }
                        }
                        HStack {
                            TextField("generated", text: $newDir).font(.callout.monospaced()).onSubmit(addDir)
                            Button("Add", action: addDir).disabled(newDir.trimmingCharacters(in: .whitespaces).isEmpty)
                        }
                    }
                    VStack(alignment: .leading, spacing: 6) {
                        Text("Ignored issues").font(.callout).foregroundStyle(.secondary)
                        if ignoredIssues.isEmpty {
                            Text("Issues you ignore in Context health appear here.").font(.callout).foregroundStyle(.tertiary)
                        }
                        ForEach(ignoredIssues, id: \.self) { issue in
                            HStack {
                                Text(verbatim: issueTitle(issue)).font(.callout)
                                Spacer()
                                Button("Stop ignoring") { ignoredIssues.removeAll { $0 == issue } }.buttonStyle(.link)
                            }
                        }
                    }
                }
                Section("Context and MCP") {
                    Toggle("Custom relevance threshold", isOn: $customScore)
                    if customScore {
                        LabeledContent("Minimum score") {
                            HStack {
                                Slider(value: $minScore, in: 0...1, step: 0.01)
                                Text(minScore.formatted(.number.precision(.fractionLength(2)))).monospacedDigit().frame(width: 40)
                            }
                        }
                    }
                    TextField("MCP server name", text: $serverName, prompt: Text(verbatim: defaultServerName))
                        .font(.body.monospaced())
                }
                Section("Import and export") {
                    Button("Import from CLAUDE.md, AGENTS.md or .cursorrules…", action: importFile)
                    Button("Export AGENTS.md snippet…") { export(.agentsPointer, name: "AGENTS.md", type: .plainText) }
                    Button("Export architecture as Mermaid…") { export(.mermaid, name: "architecture.mmd", type: .plainText) }
                }
            }
            .formStyle(.grouped)
            Divider()
            HStack {
                Spacer()
                Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction)
                Button("Done", action: save).keyboardShortcut(.defaultAction).disabled(name.trimmingCharacters(in: .whitespaces).isEmpty)
            }
            .padding(16)
        }
        .frame(width: 560, height: 640)
        .onAppear(perform: load)
        .sheet(item: $review) { ImportReviewSheet(model: model, review: $0) }
    }

    private var displayRoot: String {
        let base = model.url.deletingLastPathComponent()
        let url = repoRoot.isEmpty ? base : URL(fileURLWithPath: repoRoot, relativeTo: base).standardizedFileURL
        return url.path.replacingOccurrences(of: NSHomeDirectory(), with: "~")
    }

    private var defaultServerName: String {
        let slug = name.lowercased().map { $0.isLetter || $0.isNumber ? String($0) : "-" }.joined()
            .split(separator: "-").joined(separator: "-")
        return slug.isEmpty ? "engram" : "engram-\(slug)"
    }

    private func issueTitle(_ issue: String) -> String {
        if let known = model.healthReport?.issues.first(where: { $0.id == issue }) { return known.message }
        if issue.allSatisfy({ $0.isLetter || $0 == "_" }) {
            return String(localized: "All \(HealthIssue.title(forKind: issue)) issues")
        }
        return issue
    }

    private func load() {
        guard let project = model.document?.project else { return }
        name = project.name
        description = project.description
        repoRoot = project.repoRoot ?? ""
        customDays = project.settings.staleHandoffDays != nil
        days = project.settings.staleHandoffDays ?? ProjectSettings.defaultStaleHandoffDays
        ignoredDirs = project.settings.healthIgnoreDirs
        ignoredIssues = project.settings.healthIgnoreIssues
        customScore = project.settings.contextMinScore != nil
        minScore = project.settings.contextMinScore ?? ProjectSettings.defaultContextMinScore
        serverName = project.settings.mcpServerName ?? ""
    }

    private func addDir() {
        let dir = newDir.trimmingCharacters(in: CharacterSet(charactersIn: "/ "))
        guard !dir.isEmpty, !ignoredDirs.contains(dir) else { return }
        ignoredDirs.append(dir)
        newDir = ""
    }

    private func chooseRoot() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.directoryURL = model.resolvedRepoRoot
        panel.message = String(localized: "Choose the codebase this project memory describes")
        guard panel.runModal() == .OK, let url = panel.url else { return }
        repoRoot = model.relativeRepoRoot(for: url)
    }

    private func save() {
        let op = EngramOp.batch([
            .updateProject(name: name, description: description, repoRoot: repoRoot),
            .updateSettings([
                .staleHandoffDays(customDays ? days : nil),
                .contextMinScore(customScore ? minScore : nil),
                .mcpServerName(serverName.isEmpty ? nil : serverName),
                .ignoredDirs(ignoredDirs),
                .ignoredIssues(ignoredIssues),
            ]),
        ])
        model.perform(op, success: "Project settings saved") { result in
            if result != nil {
                model.runAudit()
                dismiss()
            }
        }
    }

    private func importFile() {
        let panel = NSOpenPanel()
        panel.directoryURL = model.resolvedRepoRoot
        panel.allowsOtherFileTypes = true
        panel.message = String(localized: "Choose an instruction file such as CLAUDE.md, AGENTS.md or .cursorrules")
        guard panel.runModal() == .OK, let url = panel.url, let engine = model.engine else { return }
        do {
            let memory = try engine.importInstructions(from: url)
            review = ImportReview(title: String(localized: "Import from \(url.lastPathComponent)"), memory: memory, prompts: [])
        } catch {
            model.show(.error, verbatim: error.localizedDescription)
        }
    }

    private func export(_ target: EngramEngine.RenderTarget, name: String, type: UTType) {
        guard let text = try? model.engine?.render(target) else { return }
        let alert = NSAlert()
        alert.messageText = String(localized: "Export \(name)")
        alert.informativeText = String(localized: "Copy the text, or save it to a file.")
        alert.addButton(withTitle: String(localized: "Copy"))
        alert.addButton(withTitle: String(localized: "Save…"))
        alert.addButton(withTitle: String(localized: "Cancel"))
        switch alert.runModal() {
        case .alertFirstButtonReturn:
            model.copy(text, message: "Copied \(name)")
        case .alertSecondButtonReturn:
            let panel = NSSavePanel()
            panel.nameFieldStringValue = name
            panel.directoryURL = model.resolvedRepoRoot
            panel.allowedContentTypes = [type]
            panel.allowsOtherFileTypes = true
            guard panel.runModal() == .OK, let url = panel.url else { return }
            do {
                try text.write(to: url, atomically: true, encoding: .utf8)
                model.show(.success, "Saved \(url.lastPathComponent)")
            } catch {
                model.show(.error, verbatim: error.localizedDescription)
            }
        default:
            break
        }
    }
}

struct ImportReview: Identifiable {
    let id = UUID()
    var title: String
    var memory: [MemoryCandidate]
    var prompts: [PromptCandidate]
}

struct ImportReviewSheet: View {
    var model: WorkspaceModel
    var review: ImportReview
    @Environment(\.dismiss) private var dismiss
    @State private var selectedMemory: Set<String> = []
    @State private var selectedPrompts: Set<String> = []

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(verbatim: review.title).font(.title2.weight(.semibold))
            Text("Choose what to add. Entries that already exist are unchecked.")
                .font(.callout)
                .foregroundStyle(.secondary)
            List {
                if !review.memory.isEmpty {
                    Section("Memory") {
                        ForEach(review.memory) { c in
                            Toggle(isOn: binding(for: c.key, in: $selectedMemory)) {
                                VStack(alignment: .leading, spacing: 2) {
                                    HStack(spacing: 6) {
                                        Text(verbatim: c.key).font(.callout.monospaced().weight(.medium))
                                        Chip(text: "\(c.category.singular)", color: Theme.color(for: c.category))
                                        if c.exists { Chip(text: "exists", color: .secondary) }
                                    }
                                    Text(verbatim: c.content).font(.caption).foregroundStyle(.secondary).lineLimit(3)
                                }
                            }
                            .toggleStyle(.checkbox)
                        }
                    }
                }
                if !review.prompts.isEmpty {
                    Section("Prompts") {
                        ForEach(review.prompts) { p in
                            Toggle(isOn: binding(for: p.name, in: $selectedPrompts)) {
                                VStack(alignment: .leading, spacing: 2) {
                                    HStack(spacing: 6) {
                                        Text(verbatim: p.name).font(.callout.monospaced().weight(.medium))
                                        if p.exists { Chip(text: "exists", color: .secondary) }
                                    }
                                    Text(verbatim: p.description).font(.caption).foregroundStyle(.secondary)
                                }
                            }
                            .toggleStyle(.checkbox)
                        }
                    }
                }
            }
            .frame(minHeight: 300)
            HStack {
                Text("\(selectedMemory.count + selectedPrompts.count) selected").font(.callout).foregroundStyle(.secondary)
                Spacer()
                Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction)
                Button("Add to project", action: apply)
                    .keyboardShortcut(.defaultAction)
                    .disabled(selectedMemory.isEmpty && selectedPrompts.isEmpty)
            }
        }
        .padding(22)
        .frame(width: 620, height: 560)
        .onAppear {
            selectedMemory = Set(review.memory.filter { !$0.exists }.map(\.key))
            selectedPrompts = Set(review.prompts.filter { !$0.exists }.map(\.name))
        }
    }

    private func binding(for id: String, in set: Binding<Set<String>>) -> Binding<Bool> {
        Binding(get: { set.wrappedValue.contains(id) }, set: { on in
            if on { set.wrappedValue.insert(id) } else { set.wrappedValue.remove(id) }
        })
    }

    private func apply() {
        let memory = review.memory.filter { selectedMemory.contains($0.key) }
        let prompts = review.prompts.filter { selectedPrompts.contains($0.name) }
        let count = memory.count + prompts.count
        model.perform(.importCandidates(memory: memory, prompts: prompts), success: "Added \(count) items") { result in
            if result != nil { dismiss() }
        }
    }
}
