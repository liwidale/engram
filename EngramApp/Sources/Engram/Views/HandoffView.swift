import SwiftUI

struct HandoffView: View {
    var model: WorkspaceModel

    var body: some View {
        if let handoff = model.document?.handoff {
            HandoffEditor(model: model, handoff: handoff)
        } else {
            ProgressView()
        }
    }
}

private struct HandoffDraft: Equatable {
    struct Row: Equatable, Identifiable {
        var id = UUID()
        var description: String
        var files: String
        var verified: Bool
        static func == (a: Row, b: Row) -> Bool { a.description == b.description && a.files == b.files && a.verified == b.verified }
    }

    var objective: String
    var progress: [Row]
    var hypotheses: [String]
    var blockers: [String]
    var notes: String
    var nextStep: String

    init(_ h: Handoff) {
        objective = h.objective
        progress = h.progress.map { Row(description: $0.description, files: $0.files.joined(separator: ", "), verified: $0.verified) }
        hypotheses = h.workingMemory.hypotheses
        blockers = h.workingMemory.blockers
        notes = h.workingMemory.notes
        nextStep = h.nextStep
    }

    var items: [ProgressItem] {
        progress
            .filter { !$0.description.trimmingCharacters(in: .whitespaces).isEmpty }
            .map { r in
                ProgressItem(
                    description: r.description.trimmingCharacters(in: .whitespaces),
                    files: r.files.split(separator: ",").map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty },
                    verified: r.verified
                )
            }
    }

    var isValid: Bool {
        !objective.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && !nextStep.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }
}

private struct HandoffEditor: View {
    var model: WorkspaceModel
    var handoff: Handoff
    @State private var draft: HandoffDraft
    @State private var baseline: HandoffDraft
    @State private var saveDebounce = Debouncer()

    init(model: WorkspaceModel, handoff: Handoff) {
        self.model = model
        self.handoff = handoff
        _draft = State(initialValue: HandoffDraft(handoff))
        _baseline = State(initialValue: HandoffDraft(handoff))
    }

    private var dirty: Bool { draft != baseline }

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    Card("Active objective") {
                        TextField("What is the agent trying to achieve?", text: $draft.objective, axis: .vertical)
                            .textFieldStyle(.plain)
                            .font(.title3.weight(.medium))
                            .lineLimit(1...4)
                    }
                    progressCard
                    HStack(alignment: .top, spacing: 14) {
                        Card("Hypotheses") {
                            LineListEditor(items: $draft.hypotheses, placeholder: "One per line")
                        }
                        Card("Blockers") {
                            LineListEditor(items: $draft.blockers, placeholder: "One per line")
                        }
                    }
                    Card("Working notes") {
                        CodeEditor(text: $draft.notes, placeholder: "Scratchpad that should survive the context window", minHeight: 80, font: .body)
                    }
                    Card("Next atomic step") {
                        HStack(alignment: .firstTextBaseline, spacing: 10) {
                            Image(systemName: "arrow.right.circle.fill").foregroundStyle(Theme.accent).font(.title3)
                            TextField("The single next action, small enough to verify", text: $draft.nextStep, axis: .vertical)
                                .textFieldStyle(.plain)
                                .font(.title3)
                                .lineLimit(1...3)
                        }
                    }
                }
                .padding(18)
            }
        }
        .onAppear { model.registerCommit("handoff") { commit() } }
        .onDisappear {
            commit()
            model.unregisterCommit("handoff")
        }
        .onChange(of: handoff) { _, new in
            if !dirty {
                draft = HandoffDraft(new)
                baseline = draft
            }
        }
        .onChange(of: draft) {
            if dirty && draft.isValid { saveDebounce.run(after: .milliseconds(1200)) { commit() } }
        }
    }

    private var header: some View {
        HStack(spacing: 10) {
            VStack(alignment: .leading, spacing: 2) {
                Text("Agent handoff").font(.title3.weight(.semibold))
                Text(handoff.updatedAt.map { "Updated \(EngramDate.relativeString($0))" + (handoff.updatedBy.map { " by \($0)" } ?? "") }
                    ?? "Agents overwrite this with update_handoff before they stop.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer()
            if dirty {
                Text(draft.isValid ? String(localized: "Edited") : String(localized: "Objective and next step are required"))
                    .font(.caption)
                    .foregroundStyle(draft.isValid ? Theme.accent : Theme.color(for: .warning))
            }
            Menu {
                Button("Copy as XML") { copy(.handoffXML) }
                Button("Copy as Markdown") { copy(.handoff) }
            } label: {
                Label("Copy", systemImage: "doc.on.doc")
            } primaryAction: {
                copy(.handoffXML)
            }
            .fixedSize()
            Button("Save") { commit() }
                .disabled(!dirty || !draft.isValid)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
    }

    private var progressCard: some View {
        Card("Verified progress", trailing: AnyView(
            Button { draft.progress.append(.init(description: "", files: "", verified: true)) } label: { Label("Add", systemImage: "plus") }
                .buttonStyle(.borderless)
                .font(.caption)
        )) {
            if draft.progress.isEmpty {
                Text("Completed, checkable work with the files it touched.").font(.callout).foregroundStyle(.tertiary)
            }
            ForEach($draft.progress) { $row in
                HStack(alignment: .top, spacing: 8) {
                    Toggle("", isOn: $row.verified)
                        .toggleStyle(.checkbox)
                        .labelsHidden()
                        .help("Verified")
                    VStack(alignment: .leading, spacing: 4) {
                        TextField("What was done", text: $row.description, axis: .vertical)
                            .textFieldStyle(.plain)
                            .lineLimit(1...3)
                        TextField("files, comma separated", text: $row.files)
                            .textFieldStyle(.plain)
                            .font(.caption.monospaced())
                            .foregroundStyle(.secondary)
                    }
                    Button { draft.progress.removeAll { $0.id == row.id } } label: { Image(systemName: "minus.circle") }
                        .buttonStyle(.borderless)
                        .foregroundStyle(.tertiary)
                }
                .padding(8)
                .background(Theme.subtleFill, in: RoundedRectangle(cornerRadius: 7))
            }
        }
    }

    private func copy(_ target: EngramEngine.RenderTarget) {
        commit()
        do {
            if let s = try model.engine?.render(target) { model.copy(s, message: "Handoff copied") }
        } catch {
            model.show(.error, verbatim: error.localizedDescription)
        }
    }

    private func commit() {
        guard dirty, draft.isValid else { return }
        let d = draft
        let op = EngramOp.updateHandoff(
            objective: d.objective, progress: d.items, hypotheses: d.hypotheses, blockers: d.blockers,
            notes: d.notes, nextStep: d.nextStep, updatedBy: "Engram (\(NSUserName()))"
        )
        if model.perform(op) != nil { baseline = d }
    }
}
