import SwiftUI

struct DecisionsView: View {
    @Bindable var model: WorkspaceModel
    @State private var search = ""
    @State private var showDeprecated = true
    @State private var composer: DecisionComposer.Seed?

    var body: some View {
        HSplitView {
            list.frame(minWidth: 260, idealWidth: 310, maxWidth: 420)
            Group {
                if let id = model.selectedDecision, let d = model.document?.decision(id) {
                    DecisionEditor(model: model, decision: d) { composer = .supersede(d) }
                        .id(d.id)
                } else {
                    EmptyState(
                        symbol: "signpost.right.and.left",
                        title: "Decision Log",
                        message: "Architecture Decision Records with the alternatives you rejected and why, so agents never re-propose dead ends.",
                        action: ("Log Decision", { composer = .new })
                    )
                }
            }
            .frame(minWidth: 460, maxWidth: .infinity, maxHeight: .infinity)
        }
        .sheet(item: $composer) { DecisionComposer(model: model, seed: $0) }
        .onChange(of: model.intent) { _, intent in
            if intent == .newDecision { composer = .new; model.intent = nil }
        }
        .onAppear {
            if model.selectedDecision == nil { model.selectedDecision = model.document?.decisions.last(where: \.isActive)?.id }
        }
    }

    private var filtered: [Decision] {
        let q = search.lowercased()
        return (model.document?.decisions ?? [])
            .filter { d in
                (showDeprecated || d.isActive)
                    && (q.isEmpty || "\(d.id) \(d.title) \(d.context) \(d.decision)".lowercased().contains(q)
                        || d.rejectedAlternatives.contains { $0.option.lowercased().contains(q) })
            }
            .sorted { ($0.isActive ? 0 : 1, $1.id) < ($1.isActive ? 0 : 1, $0.id) }
    }

    private var list: some View {
        VStack(spacing: 0) {
            HStack(spacing: 6) {
                SearchField(text: $search, prompt: "Filter decisions")
                Toggle(isOn: $showDeprecated) { Image(systemName: "archivebox") }
                    .toggleStyle(.button)
                    .help("Show deprecated decisions")
                Button { composer = .new } label: { Image(systemName: "plus") }
                    .buttonStyle(.borderless)
                    .help("Log decision (⇧⌘D)")
            }
            .padding(10)
            Divider()
            List(filtered, selection: $model.selectedDecision) { d in
                DecisionRow(decision: d)
                    .tag(d.id)
                    .contextMenu {
                        Button("Copy as Markdown") { copy(d) }
                        Divider()
                        if d.isActive {
                            Button("Supersede with New Decision…") { composer = .supersede(d) }
                            Button("Deprecate") { model.perform(.updateDecision(id: d.id, status: .deprecated)) }
                        } else {
                            Button("Reactivate") { model.perform(.updateDecision(id: d.id, status: .active)) }
                        }
                        Divider()
                        Button("Delete", role: .destructive) {
                            if model.selectedDecision == d.id { model.selectedDecision = nil }
                            model.perform(.deleteDecision(id: d.id), success: "Deleted \(d.id)")
                        }
                    }
            }
            .listStyle(.inset)
            .overlay {
                if filtered.isEmpty { Text("No decisions").foregroundStyle(.tertiary) }
            }
        }
    }

    private func copy(_ d: Decision) {
        var s = "## \(d.id): \(d.title)\n\n**Context**\n\n\(d.context)\n\n**Decision**\n\n\(d.decision)\n"
        if !d.rejectedAlternatives.isEmpty {
            s += "\n**Rejected alternatives**\n\n" + d.rejectedAlternatives.map { "- **\($0.option)**: \($0.reason)" }.joined(separator: "\n") + "\n"
        }
        model.copy(s, message: "Copied \(d.id)")
    }
}

private struct DecisionRow: View {
    var decision: Decision

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 6) {
                Text(decision.id).font(.caption.monospaced().weight(.semibold)).foregroundStyle(Theme.accent)
                Spacer()
                if decision.isActive {
                    Chip(text: "Active", color: .green)
                } else {
                    Chip(text: decision.supersededBy.map { "→ \($0)" } ?? "Deprecated", color: .secondary)
                }
            }
            Text(decision.title)
                .font(.callout.weight(.medium))
                .strikethrough(!decision.isActive)
                .lineLimit(2)
            if !decision.rejectedAlternatives.isEmpty {
                Text("Rejected: " + decision.rejectedAlternatives.map(\.option).joined(separator: ", "))
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
        }
        .padding(.vertical, 3)
        .opacity(decision.isActive ? 1 : 0.65)
    }
}

private struct DecisionDraft: Equatable {
    var title: String
    var context: String
    var decision: String
    var rejected: [RejectedAlternative]
    var consequences: String
    var related: [String]

    init(_ d: Decision) {
        title = d.title
        context = d.context
        decision = d.decision
        rejected = d.rejectedAlternatives
        consequences = d.consequences
        related = d.related
    }

    var cleanRejected: [RejectedAlternative] {
        rejected.filter { !$0.option.trimmingCharacters(in: .whitespaces).isEmpty }
    }
}

private struct DecisionEditor: View {
    var model: WorkspaceModel
    var decision: Decision
    var onSupersede: () -> Void
    @State private var draft: DecisionDraft
    @State private var baseline: DecisionDraft
    @State private var saveDebounce = Debouncer()
    @State private var preview = false

    init(model: WorkspaceModel, decision: Decision, onSupersede: @escaping () -> Void) {
        self.model = model
        self.decision = decision
        self.onSupersede = onSupersede
        _draft = State(initialValue: DecisionDraft(decision))
        _baseline = State(initialValue: DecisionDraft(decision))
    }

    private var dirty: Bool {
        var a = draft, b = baseline
        a.rejected = a.cleanRejected
        b.rejected = b.cleanRejected
        return a != b
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    if !decision.isActive {
                        HStack(spacing: 8) {
                            Image(systemName: "archivebox.fill")
                            Text(decision.supersededBy.map { "Deprecated and superseded by \($0). Agents no longer receive this decision." }
                                ?? "Deprecated. Agents no longer receive this decision.")
                            Spacer()
                            if let by = decision.supersededBy {
                                Button("Open \(by)") { model.selectedDecision = by }.buttonStyle(.link)
                            }
                        }
                        .font(.callout)
                        .padding(10)
                        .background(Color.orange.opacity(0.12), in: RoundedRectangle(cornerRadius: 8))
                    }
                    section("Context", text: $draft.context, placeholder: "What forces and constraints led here?")
                    section("Decision", text: $draft.decision, placeholder: "What did we decide?")
                    rejectedCard
                    section("Consequences", text: $draft.consequences, placeholder: "Trade-offs we accept (optional)")
                    relatedCard
                }
                .padding(18)
            }
            Divider()
            HStack {
                Text("Recorded \(EngramDate.relativeString(decision.createdAt)) · updated \(EngramDate.relativeString(decision.updatedAt))")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Spacer()
                Text(dirty ? "Edited" : "Saved").font(.caption).foregroundStyle(dirty ? Theme.accent : .secondary)
                Button("Save") { commit() }.disabled(!dirty)
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 8)
        }
        .onAppear { model.registerCommit("decision") { commit() } }
        .onDisappear {
            commit()
            model.unregisterCommit("decision")
        }
        .onChange(of: decision) { _, new in
            if !dirty {
                draft = DecisionDraft(new)
                baseline = draft
            }
        }
        .onChange(of: draft) { _, _ in
            if dirty { saveDebounce.run(after: .milliseconds(1200)) { commit() } }
        }
    }

    private var header: some View {
        HStack(spacing: 10) {
            Text(decision.id)
                .font(.callout.monospaced().weight(.semibold))
                .foregroundStyle(Theme.accent)
                .padding(.horizontal, 7)
                .padding(.vertical, 3)
                .background(Theme.accent.opacity(0.12), in: RoundedRectangle(cornerRadius: 5))
            TextField("Title", text: $draft.title)
                .textFieldStyle(.plain)
                .font(.title3.weight(.semibold))
                .strikethrough(!decision.isActive)
            Spacer()
            Picker("Status", selection: Binding(get: { decision.status }, set: { s in
                commit()
                model.perform(.updateDecision(id: decision.id, status: s))
            })) {
                ForEach(DecisionStatus.allCases, id: \.self) { Text($0.title).tag($0) }
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .fixedSize()
            Toggle(isOn: $preview) { Image(systemName: preview ? "eye.fill" : "eye") }
                .toggleStyle(.button)
                .help("Preview Markdown")
            Menu {
                if decision.isActive { Button("Supersede with New Decision…", action: onSupersede) }
                Button("Delete", role: .destructive) {
                    model.selectedDecision = nil
                    model.perform(.deleteDecision(id: decision.id), success: "Deleted \(decision.id)")
                }
            } label: { Image(systemName: "ellipsis.circle") }
                .menuStyle(.borderlessButton)
                .fixedSize()
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
    }

    @ViewBuilder
    private func section(_ title: String, text: Binding<String>, placeholder: String) -> some View {
        Card(title) {
            if preview {
                MarkdownView(text: text.wrappedValue.isEmpty ? "_Empty_" : text.wrappedValue)
            } else {
                CodeEditor(text: text, placeholder: placeholder, minHeight: 64, font: .body)
            }
        }
    }

    private var rejectedCard: some View {
        Card("Rejected alternatives", trailing: AnyView(
            Button {
                draft.rejected.append(RejectedAlternative(option: "", reason: ""))
            } label: { Label("Add", systemImage: "plus") }
                .buttonStyle(.borderless)
                .font(.caption)
        )) {
            if draft.rejected.isEmpty {
                Text("Record what you considered and why it lost. Agents treat these as dead ends.")
                    .font(.callout)
                    .foregroundStyle(.tertiary)
            }
            ForEach(draft.rejected.indices, id: \.self) { i in
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    Image(systemName: "xmark.octagon").foregroundStyle(Theme.color(for: .error).opacity(0.8))
                    TextField("Option", text: $draft.rejected[i].option)
                        .textFieldStyle(.roundedBorder)
                        .frame(width: 170)
                    TextField("Why it was rejected", text: $draft.rejected[i].reason, axis: .vertical)
                        .textFieldStyle(.roundedBorder)
                    Button { draft.rejected.remove(at: i) } label: { Image(systemName: "minus.circle") }
                        .buttonStyle(.borderless)
                        .foregroundStyle(.secondary)
                }
            }
        }
    }

    private var relatedCard: some View {
        Card("Related") {
            if !draft.related.isEmpty {
                FlowLayout(spacing: 6) {
                    ForEach(draft.related, id: \.self) { r in
                        HStack(spacing: 4) {
                            Button { navigate(r) } label: { Chip(text: r, color: color(for: r), symbol: symbol(for: r)) }
                                .buttonStyle(.plain)
                            Button { draft.related.removeAll { $0 == r } } label: { Image(systemName: "xmark").font(.caption2) }
                                .buttonStyle(.borderless)
                                .foregroundStyle(.tertiary)
                        }
                    }
                }
            }
            Menu("Link memory, node or decision") {
                let doc = model.document
                Section("Memory") {
                    ForEach(doc?.memory.filter { $0.isActive && !draft.related.contains($0.key) } ?? []) { m in
                        Button(m.key) { draft.related.append(m.key) }
                    }
                }
                Section("Architecture") {
                    ForEach(doc?.architecture.nodes.filter { !draft.related.contains($0.id) } ?? []) { n in
                        Button(n.displayName) { draft.related.append(n.id) }
                    }
                }
                Section("Decisions") {
                    ForEach(doc?.decisions.filter { $0.id != decision.id && !draft.related.contains($0.id) } ?? []) { d in
                        Button("\(d.id) · \(d.title)") { draft.related.append(d.id) }
                    }
                }
            }
            .menuStyle(.borderlessButton)
            .fixedSize()
            .font(.callout)
        }
    }

    private func color(for ref: String) -> Color {
        guard let doc = model.document else { return .secondary }
        if let m = doc.memoryEntry(ref) { return m.isActive ? Theme.color(for: m.category) : .orange }
        if let n = doc.node(ref) { return Theme.color(for: n.kind) }
        if doc.decision(ref) != nil { return Theme.accent }
        return Theme.color(for: .warning)
    }

    private func symbol(for ref: String) -> String {
        guard let doc = model.document else { return "questionmark" }
        if let m = doc.memoryEntry(ref) { return m.category.symbol }
        if let n = doc.node(ref) { return n.kind.symbol }
        if doc.decision(ref) != nil { return "signpost.right" }
        return "exclamationmark.triangle"
    }

    private func navigate(_ ref: String) {
        guard let doc = model.document else { return }
        if doc.memoryEntry(ref) != nil { model.reveal(EntityRef(kind: "memory", id: ref)) }
        else if doc.node(ref) != nil { model.reveal(EntityRef(kind: "node", id: ref)) }
        else if doc.decision(ref) != nil { model.selectedDecision = doc.decision(ref)!.id }
    }

    private func commit() {
        guard dirty else { return }
        let d = draft
        let op = EngramOp.updateDecision(
            id: decision.id, title: d.title, context: d.context, decision: d.decision,
            rejected: d.cleanRejected, consequences: d.consequences, related: d.related
        )
        if model.perform(op) != nil {
            baseline = d
        }
    }
}

struct DecisionComposer: View {
    enum Seed: Identifiable {
        case new
        case supersede(Decision)
        var id: String {
            switch self {
            case .new: "new"
            case .supersede(let d): "supersede.\(d.id)"
            }
        }
    }

    var model: WorkspaceModel
    var seed: Seed
    @Environment(\.dismiss) private var dismiss
    @State private var title = ""
    @State private var context = ""
    @State private var decision = ""
    @State private var rejected: [RejectedAlternative] = [RejectedAlternative(option: "", reason: "")]
    @State private var supersedes: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Text(supersedes.map { "Supersede \($0)" } ?? "Log Decision").font(.title2.weight(.semibold))
                Spacer()
                Text(model.document?.decisions.isEmpty == false ? "next id assigned on save" : "ADR-0001")
                    .font(.caption)
                    .foregroundStyle(.tertiary)
            }
            TextField("Title (derived from the decision if empty)", text: $title)
                .textFieldStyle(.roundedBorder)
            labeled("Context") { CodeEditor(text: $context, placeholder: "What problem, forces, constraints?", minHeight: 70, font: .body) }
            labeled("Decision") { CodeEditor(text: $decision, placeholder: "What we will do.", minHeight: 70, font: .body) }
            labeled("Rejected alternatives") {
                VStack(spacing: 6) {
                    ForEach(rejected.indices, id: \.self) { i in
                        HStack {
                            TextField("Option", text: $rejected[i].option).textFieldStyle(.roundedBorder).frame(width: 150)
                            TextField("Why not", text: $rejected[i].reason).textFieldStyle(.roundedBorder)
                        }
                    }
                    Button { rejected.append(RejectedAlternative(option: "", reason: "")) } label: { Label("Add alternative", systemImage: "plus") }
                        .buttonStyle(.borderless)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
            HStack {
                Spacer()
                Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction)
                Button("Log Decision") { save() }
                    .keyboardShortcut(.defaultAction)
                    .disabled(context.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || decision.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
        }
        .padding(22)
        .frame(width: 600)
        .onAppear {
            if case .supersede(let d) = seed {
                supersedes = d.id
                context = "Supersedes \(d.id) (\(d.title)).\n\n\(d.context)"
                rejected = [RejectedAlternative(option: d.title, reason: "")]
            }
        }
    }

    private func labeled<C: View>(_ title: String, @ViewBuilder content: () -> C) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            SectionLabel(title)
            content()
                .padding(4)
                .background(Theme.surface, in: RoundedRectangle(cornerRadius: 7))
                .overlay(RoundedRectangle(cornerRadius: 7).strokeBorder(Theme.hairline))
        }
    }

    private func save() {
        let clean = rejected.filter { !$0.option.trimmingCharacters(in: .whitespaces).isEmpty }
        let op = EngramOp.logDecision(
            title: title.isEmpty ? nil : title, context: context, decision: decision, rejected: clean,
            related: supersedes.map { [$0] } ?? [], supersedes: supersedes
        )
        if let result = model.perform(op), let id = result["id"]?.string {
            model.selectedDecision = id
            model.show(.success, "Logged \(id)")
            dismiss()
        }
    }
}

struct FlowLayout: Layout {
    var spacing: CGFloat = 6

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        arrange(subviews, width: proposal.width ?? .infinity).size
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        for (i, p) in arrange(subviews, width: bounds.width).points.enumerated() {
            subviews[i].place(at: CGPoint(x: bounds.minX + p.x, y: bounds.minY + p.y), proposal: .unspecified)
        }
    }

    private func arrange(_ subviews: Subviews, width: CGFloat) -> (points: [CGPoint], size: CGSize) {
        var points: [CGPoint] = [], x: CGFloat = 0, y: CGFloat = 0, row: CGFloat = 0, maxX: CGFloat = 0
        for s in subviews {
            let size = s.sizeThatFits(.unspecified)
            if x > 0, x + size.width > width {
                x = 0
                y += row + spacing
                row = 0
            }
            points.append(CGPoint(x: x, y: y))
            x += size.width + spacing
            row = max(row, size.height)
            maxX = max(maxX, x)
        }
        return (points, CGSize(width: maxX, height: y + row))
    }
}
