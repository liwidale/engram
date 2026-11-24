import SwiftUI

struct ContextBuilderView: View {
    @Bindable var model: WorkspaceModel
    @State private var bundle: ContextBundle?
    @State private var highlighted = AttributedString()
    @State private var building = false
    @State private var debounce = Debouncer()
    @State private var generation = 0
    @FocusState private var taskFocused: Bool

    private var state: ContextBuilderState { model.contextState }

    var body: some View {
        HSplitView {
            controls.frame(minWidth: 360, idealWidth: 420, maxWidth: 560)
            preview.frame(minWidth: 380, maxWidth: .infinity, maxHeight: .infinity)
        }
        .onAppear {
            rebuild(immediately: true)
            if state.task.isEmpty { taskFocused = true }
        }
        .onChange(of: model.contextState) { rebuild() }
        .onChange(of: model.revision) { rebuild() }
        .onChange(of: model.intent) { _, intent in
            if intent == .focusTask { taskFocused = true; model.intent = nil }
        }
    }

    private var controls: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                Text("Preview exactly what an agent gets from build_context, or copy a bundle into a chat that has no MCP access.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                Card("Task") {
                    TextField("Describe what the agent is about to do…", text: $model.contextState.task, axis: .vertical)
                        .textFieldStyle(.plain)
                        .font(.body)
                        .lineLimit(3...8)
                        .focused($taskFocused)
                }
                Card("Budget") {
                    HStack {
                        Slider(value: Binding(
                            get: { Double(state.maxTokens) },
                            set: { model.contextState.maxTokens = Int($0 / 250) * 250 }
                        ), in: 500...32000)
                        Text("\(state.maxTokens) tokens").font(.callout.monospacedDigit()).frame(width: 96, alignment: .trailing)
                    }
                    HStack(spacing: 6) {
                        ForEach([2000, 4000, 8000, 16000, 32000], id: \.self) { n in
                            Button("\(n / 1000)k") { model.contextState.maxTokens = n }
                                .buttonStyle(.bordered)
                                .controlSize(.small)
                                .tint(state.maxTokens == n ? Theme.accent : nil)
                        }
                        Spacer()
                    }
                    Toggle("Rank against the task", isOn: $model.contextState.autoRank)
                    Toggle("Include the agent handoff", isOn: $model.contextState.includeHandoff)
                }
                promptCard
                candidates
            }
            .padding(16)
        }
        .background(Theme.canvas)
    }

    private var promptCard: some View {
        Card("Prompt") {
            Picker("Append prompt", selection: Binding(
                get: { state.prompt ?? "" },
                set: { model.contextState.prompt = $0.isEmpty ? nil : $0 }
            )) {
                Text("None").tag("")
                ForEach(model.document?.prompts ?? []) { Text(verbatim: $0.name).tag($0.name) }
            }
            if let name = state.prompt, let p = model.document?.prompt(name) {
                ForEach(p.variables, id: \.self) { v in
                    HStack {
                        Text(verbatim: v).font(.caption.monospaced()).foregroundStyle(.secondary).frame(width: 90, alignment: .leading)
                        TextField(v, text: Binding(
                            get: { state.promptVars[v, default: ""] },
                            set: { model.contextState.promptVars[v] = $0.isEmpty ? nil : $0 }
                        ))
                        .textFieldStyle(.roundedBorder)
                    }
                }
                if let missing = bundle?.prompt?.missing, !missing.isEmpty {
                    Label("Unfilled: \(missing.joined(separator: ", "))", systemImage: "exclamationmark.circle")
                        .font(.caption)
                        .foregroundStyle(Theme.color(for: .warning))
                }
            }
        }
    }

    private var candidates: some View {
        Card("Candidates", trailing: AnyView(
            HStack(spacing: 8) {
                if !state.forced.isEmpty || !state.excluded.isEmpty {
                    Button("Reset") {
                        model.contextState.forced.removeAll()
                        model.contextState.excluded.removeAll()
                    }
                    .buttonStyle(.link)
                    .font(.caption)
                }
                if let b = bundle {
                    Text("\(b.items.filter(\.included).count) of \(b.items.count)").font(.caption).foregroundStyle(.tertiary)
                }
            }
        )) {
            if let items = bundle?.items, !items.isEmpty {
                VStack(spacing: 2) {
                    ForEach(items) { item in
                        CandidateRow(item: item, state: $model.contextState) { open(item) }
                    }
                }
            } else {
                Text("Add memory, decisions or architecture nodes to build context from.")
                    .font(.callout)
                    .foregroundStyle(.tertiary)
            }
        }
    }

    private func open(_ item: ContextItem) {
        switch item.kind {
        case .memory: model.reveal(EntityRef(kind: "memory", id: item.itemId))
        case .decision: model.reveal(EntityRef(kind: "decision", id: item.itemId))
        case .node: model.reveal(EntityRef(kind: "node", id: item.itemId))
        }
    }

    private var preview: some View {
        VStack(spacing: 0) {
            HStack(spacing: 12) {
                VStack(alignment: .leading, spacing: 4) {
                    HStack(spacing: 8) {
                        TokenBadge(tokens: bundle?.tokens ?? 0, budget: state.maxTokens)
                        if building { ProgressView().controlSize(.small) }
                        if let b = bundle, b.deadEnds > 0 {
                            Chip(text: "\(b.deadEnds) dead ends", color: .secondary, symbol: "xmark.octagon")
                                .help("Rejected alternatives from decisions outside the bundle, listed so agents do not propose them again")
                        }
                    }
                    BudgetBar(tokens: bundle?.tokens ?? 0, budget: state.maxTokens).frame(width: 240, height: 5)
                }
                Spacer()
                Picker("Format", selection: $model.contextState.markdown) {
                    Text(verbatim: "XML").tag(false)
                    Text(verbatim: "Markdown").tag(true)
                }
                .pickerStyle(.segmented)
                .labelsHidden()
                .fixedSize()
                Button {
                    if let b = bundle { model.copy(b.output, message: "Context copied · \(b.tokens) tokens") }
                } label: {
                    Label(state.markdown ? "Copy as Markdown" : "Copy as XML", systemImage: "doc.on.clipboard")
                }
                .buttonStyle(.borderedProminent)
                .tint(Theme.accent)
                .keyboardShortcut("c", modifiers: [.command, .shift])
                .disabled(bundle == nil)
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 12)
            Divider()
            ScrollView([.vertical, .horizontal]) {
                Text(highlighted)
                    .font(Theme.monoSmall)
                    .textSelection(.enabled)
                    .padding(16)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            .background(Theme.surface)
        }
    }

    nonisolated private static func highlight(_ text: String, markdown: Bool) -> AttributedString {
        var out = AttributedString()
        if markdown {
            for line in text.split(separator: "\n", omittingEmptySubsequences: false) {
                var piece = AttributedString(String(line) + "\n")
                if line.hasPrefix("#") { piece.foregroundColor = Theme.accent }
                out += piece
            }
            return out
        }
        var rest = text[...]
        while let open = rest.firstIndex(of: "<") {
            out += AttributedString(String(rest[..<open]))
            guard let close = rest[open...].firstIndex(of: ">") else { break }
            var tag = AttributedString(String(rest[open...close]))
            tag.foregroundColor = Theme.accent
            out += tag
            rest = rest[rest.index(after: close)...]
        }
        out += AttributedString(String(rest))
        return out
    }

    private func request() -> ContextRequest {
        var r = ContextRequest()
        let s = model.contextState
        r.task = s.task
        r.maxTokens = s.maxTokens
        r.autoRank = s.autoRank
        r.includeHandoff = s.includeHandoff
        r.markdown = s.markdown
        r.prompt = s.prompt
        r.promptVars = s.promptVars
        func ids(_ set: Set<String>, _ kind: String) -> [String] {
            set.filter { $0.hasPrefix(kind + ":") }.map { String($0.dropFirst(kind.count + 1)) }.sorted()
        }
        r.includeMemory = ids(s.forced, "memory")
        r.includeDecisions = ids(s.forced, "decision")
        r.includeNodes = ids(s.forced, "node")
        r.excludeMemory = ids(s.excluded, "memory")
        r.excludeDecisions = ids(s.excluded, "decision")
        r.excludeNodes = ids(s.excluded, "node")
        return r
    }

    private func rebuild(immediately: Bool = false) {
        let run = {
            guard let engine = model.engine else { return }
            let req = request()
            generation += 1
            let gen = generation
            building = true
            Task {
                let result = await Task.detached { () -> Result<(ContextBundle, AttributedString), Error> in
                    Result {
                        let b = try engine.buildContext(req)
                        return (b, Self.highlight(b.output, markdown: req.markdown))
                    }
                }.value
                guard gen == generation else { return }
                building = false
                switch result {
                case .success(let (b, text)):
                    bundle = b
                    highlighted = text
                case .failure(let e):
                    model.show(.error, verbatim: e.localizedDescription)
                }
            }
        }
        if immediately { run() } else { debounce.run(after: .milliseconds(220), run) }
    }
}

private struct BudgetBar: View {
    var tokens: Int
    var budget: Int

    var body: some View {
        GeometryReader { geo in
            let ratio = min(Double(tokens) / Double(max(budget, 1)), 1)
            ZStack(alignment: .leading) {
                Capsule().fill(Theme.subtleFill)
                Capsule()
                    .fill(tokens > budget ? Theme.color(for: .error) : ratio > 0.85 ? Theme.color(for: .warning) : Theme.accent)
                    .frame(width: geo.size.width * ratio)
            }
        }
        .animation(.easeOut(duration: 0.25), value: tokens)
    }
}

private struct CandidateRow: View {
    var item: ContextItem
    @Binding var state: ContextBuilderState
    var onOpen: () -> Void
    @State private var hover = false

    var body: some View {
        HStack(spacing: 8) {
            Toggle("", isOn: Binding(get: { item.included }, set: { on in
                state.forced.remove(item.id)
                state.excluded.remove(item.id)
                if on && !item.included { state.forced.insert(item.id) }
                if !on && item.included { state.excluded.insert(item.id) }
            }))
            .toggleStyle(.checkbox)
            .labelsHidden()
            Button(action: onOpen) {
                HStack(spacing: 8) {
                    Image(systemName: symbol).foregroundStyle(.secondary).frame(width: 16)
                    VStack(alignment: .leading, spacing: 1) {
                        HStack(spacing: 5) {
                            Text(verbatim: item.itemId).font(.caption.monospaced().weight(.medium)).lineLimit(1)
                            reasonChip
                        }
                        Text(verbatim: item.title).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                        if !item.matched.isEmpty {
                            Text(verbatim: item.matched.joined(separator: " · ")).font(.caption2).foregroundStyle(Theme.accent.opacity(0.85)).lineLimit(1)
                        }
                    }
                    Spacer(minLength: 4)
                    VStack(alignment: .trailing, spacing: 2) {
                        ScoreBar(score: item.score).frame(width: 46, height: 4)
                        Text("\(item.tokens) tok").font(.caption2.monospacedDigit()).foregroundStyle(.tertiary)
                    }
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .help("Open this item")
        }
        .padding(.vertical, 4)
        .padding(.horizontal, 4)
        .background(hover ? Theme.hoverFill : item.included ? Theme.accent.opacity(0.06) : .clear, in: RoundedRectangle(cornerRadius: 6))
        .opacity(item.reason == .excluded || item.reason == .belowThreshold ? 0.6 : 1)
        .onHover { hover = $0 }
    }

    private var symbol: String {
        switch item.kind {
        case .memory: "brain"
        case .decision: "signpost.right.and.left"
        case .node: "point.3.connected.trianglepath.dotted"
        }
    }

    @ViewBuilder
    private var reasonChip: some View {
        switch item.reason {
        case .pinned: Chip(text: "pinned", color: Theme.accent, symbol: "pin.fill")
        case .selected: Chip(text: "forced", color: Theme.accent)
        case .ranked, .belowThreshold: EmptyView()
        case .overBudget: Chip(text: "over budget", color: Theme.color(for: .warning))
        case .excluded: Chip(text: "excluded", color: .secondary)
        }
    }
}

private struct ScoreBar: View {
    var score: Double
    var body: some View {
        GeometryReader { geo in
            ZStack(alignment: .leading) {
                Capsule().fill(Theme.subtleFill)
                Capsule().fill(Theme.accent.opacity(0.7)).frame(width: geo.size.width * min(score / 1.5, 1))
            }
        }
    }
}
