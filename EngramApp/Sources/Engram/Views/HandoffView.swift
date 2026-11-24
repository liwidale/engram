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

struct HandoffDraft: Equatable {
    struct Row: Identifiable {
        var id = UUID()
        var description: String
        var files: [String]
        var verified: Bool
    }

    var objective: String
    var progress: [Row]
    var hypotheses: [String]
    var blockers: [String]
    var notes: String
    var nextStep: String

    init(_ h: Handoff) {
        objective = h.objective
        progress = h.progress.map { Row(description: $0.description, files: $0.files, verified: $0.verified) }
        hypotheses = h.workingMemory.hypotheses
        blockers = h.workingMemory.blockers
        notes = h.workingMemory.notes
        nextStep = h.nextStep
    }

    var items: [ProgressItem] {
        progress
            .filter { !$0.description.trimmingCharacters(in: .whitespaces).isEmpty }
            .map { ProgressItem(description: $0.description.trimmingCharacters(in: .whitespaces), files: $0.files, verified: $0.verified) }
    }

    var isValid: Bool {
        !objective.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && !nextStep.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    static func == (a: HandoffDraft, b: HandoffDraft) -> Bool {
        a.objective == b.objective && a.items == b.items && a.hypotheses == b.hypotheses && a.blockers == b.blockers
            && a.notes == b.notes && a.nextStep == b.nextStep
    }
}

private struct HandoffEditor: View {
    var model: WorkspaceModel
    var handoff: Handoff
    @State private var draft: EditorDraft<HandoffDraft>

    init(model: WorkspaceModel, handoff: Handoff) {
        self.model = model
        self.handoff = handoff
        _draft = State(initialValue: EditorDraft(HandoffDraft(handoff)))
    }

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
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    Card("Active objective") {
                        TextField("What is the agent trying to achieve?", text: $draft.value.objective, axis: .vertical)
                            .textFieldStyle(.plain)
                            .font(.title3.weight(.medium))
                            .lineLimit(1...4)
                    }
                    progressCard
                    HStack(alignment: .top, spacing: 14) {
                        Card("Hypotheses") { LineListEditor(items: $draft.value.hypotheses, placeholder: "One per line") }
                        Card("Blockers") { LineListEditor(items: $draft.value.blockers, placeholder: "One per line") }
                    }
                    Card("Working notes") {
                        CodeEditor(text: $draft.value.notes, placeholder: "Notes that should outlive the context window", minHeight: 80, font: .body)
                    }
                    Card("Next atomic step") {
                        HStack(alignment: .firstTextBaseline, spacing: 10) {
                            Image(systemName: "arrow.right.circle.fill").foregroundStyle(Theme.accent).font(.title3)
                            TextField("The single next action, small enough to verify", text: $draft.value.nextStep, axis: .vertical)
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
            _ = commit()
            model.unregisterCommit("handoff")
        }
        .onChange(of: handoff) { _, new in draft.sync(external: HandoffDraft(new)) }
        .onChange(of: draft.value) {
            if draft.value.isValid { draft.scheduleSave { _ = commit() } }
        }
    }

    private var header: some View {
        HStack(spacing: 10) {
            VStack(alignment: .leading, spacing: 2) {
                Text("Agent handoff").font(.title3.weight(.semibold))
                Text(handoff.updatedAt.map { at in
                    handoff.updatedBy.map { String(localized: "Updated \(EngramDate.relativeString(at)) by \($0)") }
                        ?? String(localized: "Updated \(EngramDate.relativeString(at))")
                } ?? String(localized: "A checkpoint agents overwrite with update_handoff before they stop."))
                .font(.caption)
                .foregroundStyle(.secondary)
            }
            Spacer()
            if draft.dirty && !draft.value.isValid {
                Text("Add an objective and a next step to save").font(.caption).foregroundStyle(Theme.color(for: .warning))
            } else {
                SaveStatus(model: model, dirty: draft.dirty)
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
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
    }

    private var progressCard: some View {
        Card("Verified progress", trailing: AnyView(
            Button { draft.value.progress.append(.init(description: "", files: [], verified: true)) } label: { Label("Add", systemImage: "plus") }
                .buttonStyle(.borderless)
                .font(.caption)
        )) {
            if draft.value.progress.isEmpty {
                Text("Finished, checkable work with the files it touched.").font(.callout).foregroundStyle(.tertiary)
            }
            ForEach($draft.value.progress) { $row in
                HStack(alignment: .top, spacing: 8) {
                    Toggle("", isOn: $row.verified)
                        .toggleStyle(.checkbox)
                        .labelsHidden()
                        .help("Verified")
                    VStack(alignment: .leading, spacing: 6) {
                        TextField("What was done", text: $row.description, axis: .vertical)
                            .textFieldStyle(.plain)
                            .lineLimit(1...3)
                        FileListEditor(model: model, files: $row.files)
                    }
                    Button { draft.value.progress.removeAll { $0.id == row.id } } label: { Image(systemName: "minus.circle") }
                        .buttonStyle(.borderless)
                        .foregroundStyle(.tertiary)
                        .help("Remove")
                }
                .padding(8)
                .background(Theme.subtleFill, in: RoundedRectangle(cornerRadius: 7))
            }
        }
    }

    private func copy(_ target: EngramEngine.RenderTarget) {
        _ = commit()
        do {
            if let s = try model.engine?.render(target) { model.copy(s, message: "Handoff copied") }
        } catch {
            model.show(.error, verbatim: error.localizedDescription)
        }
    }

    @discardableResult
    private func commit() -> Bool {
        guard draft.dirty, draft.value.isValid, draft.conflict == nil else { return false }
        let d = draft.value
        draft.markSaved(d)
        model.perform(.updateHandoff(
            objective: d.objective, progress: d.items, hypotheses: d.hypotheses, blockers: d.blockers,
            notes: d.notes, nextStep: d.nextStep, updatedBy: Pref.handoffAuthorLabel
        ))
        return true
    }
}
