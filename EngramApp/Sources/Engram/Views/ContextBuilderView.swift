import SwiftUI

struct ContextBuilderView: View {
    @Bindable var model: WorkspaceModel
    @State private var bundle: ContextBundle?
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

    // MARK: Controls

    private var controls: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
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
                        Text("\(state.maxTokens.formatted()) tok")
                            .font(.callout.monospacedDigit())
                            .frame(width: 84, alignment: .trailing)
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
                    Toggle("Auto-rank against the task", isOn: $model.contextState.autoRank)
                    Toggle("Include agent handoff", isOn: $model.contextState.includeHandoff)
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
                ForEach(model.document?.prompts ?? []) { Text($0.name).tag($0.name) }
            }
            if let name = state.prompt, let p = model.document?.prompt(name) {
                ForEach(p.variables, id: \.self) { v in
                    HStack {
                        Text(v).font(.caption.monospaced()).foregroundStyle(.secondary).frame(width: 90, alignment: .leading)
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
                    ForEach(items) { item in CandidateRow(item: item, state: $model.contextState) }
                }
            } else {
                Text("Add memory, decisions or architecture nodes to build context from.")
                    .font(.callout)
                    .foregroundStyle(.tertiary)
            }
        }
    }

    // MARK: Preview

    private var preview: some View {
        VStack(spacing: 0) {
            HStack(spacing: 12) {
                VStack(alignment: .leading, spacing: 4) {
                    HStack(spacing: 8) {
                        TokenBadge(tokens: bundle?.tokens ?? 0, budget: state.maxTokens)
                        if building { ProgressView().controlSize(.small) }
                        if let b = bundle, b.deadEnds > 0 {
                            Chip(text: "\(b.deadEnds) dead end\(b.deadEnds == 1 ? "" : "s")", color: Theme.color(for: .error), symbol: "xmark.octagon")
                                .help("Rejected alternatives from ADRs outside the bundle, listed so agents don't re-propose them")
                        }
                    }
                    BudgetBar(tokens: bundle?.tokens ?? 0, budget: state.maxTokens)
                        .frame(width: 240, height: 5)
                }
                Spacer()
                Button {
                    if let xml = bundle?.xml { model.copy(xml, message: "Context copied · \(bundle?.tokens ?? 0) tokens") }
                } label: {
                    Label("Copy as XML Context", systemImage: "doc.on.clipboard")
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
                Text(highlighted(bundle?.xml ?? ""))
                    .font(Theme.monoSmall)
                    .textSelection(.enabled)
                    .padding(16)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            .background(Theme.surface)
        }
    }

    /// Light XML syntax colouring: tags in accent, attribute values dimmed.
    private func highlighted(_ xml: String) -> AttributedString {
        var out = AttributedString()
        var rest = xml[...]
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

    // MARK: Building

    private func request() -> ContextRequest {
        var r = ContextRequest()
        let s = model.contextState
        r.task = s.task
        r.maxTokens = s.maxTokens
        r.autoRank = s.autoRank
        r.includeHandoff = s.includeHandoff
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
                let result = await Task.detached { Result { try engine.buildContext(req) } }.value
                guard gen == generation else { return }
                building = false
                switch result {
                case .success(let b): bundle = b
                case .failure(let e): model.show(.error, e.localizedDescription)
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

    private var forced: Bool { state.forced.contains(item.id) }
    private var excluded: Bool { state.excluded.contains(item.id) }

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
            Image(systemName: symbol).foregroundStyle(.secondary).frame(width: 16)
            VStack(alignment: .leading, spacing: 1) {
                HStack(spacing: 5) {
                    Text(item.itemId).font(.caption.monospaced().weight(.medium)).lineLimit(1)
                    reasonChip
                }
                Text(item.title).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                if !item.matched.isEmpty {
                    Text(item.matched.joined(separator: " · ")).font(.caption2).foregroundStyle(Theme.accent.opacity(0.85)).lineLimit(1)
                }
            }
            Spacer(minLength: 4)
            VStack(alignment: .trailing, spacing: 2) {
                ScoreBar(score: item.score).frame(width: 46, height: 4)
                Text("\(item.tokens) tok").font(.caption2.monospacedDigit()).foregroundStyle(.tertiary)
            }
        }
        .padding(.vertical, 4)
        .padding(.horizontal, 4)
        .background(item.included ? Theme.accent.opacity(0.06) : .clear, in: RoundedRectangle(cornerRadius: 6))
        .opacity(item.reason == .excluded || item.reason == .belowThreshold ? 0.6 : 1)
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
        case .ranked: EmptyView()
        case .overBudget: Chip(text: "over budget", color: Theme.color(for: .warning))
        case .belowThreshold: EmptyView()
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
