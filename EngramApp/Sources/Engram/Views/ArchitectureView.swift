import SwiftUI

struct ArchitectureView: View {
    @Bindable var model: WorkspaceModel
    @State private var scale: CGFloat = 1
    @State private var offset: CGSize = .zero
    @State private var panStart: CGSize?
    @State private var zoomStart: CGFloat?
    @State private var drag: (id: String, delta: CGSize)?
    @State private var showComposer = false

    private let nodeSize = CGSize(width: 176, height: 58)

    var body: some View {
        HSplitView {
            ZStack(alignment: .topLeading) {
                if model.layout.nodes.isEmpty {
                    EmptyState(
                        symbol: "point.3.connected.trianglepath.dotted",
                        title: "Architecture Map",
                        message: "Model modules, services and databases with directional dependencies. Agents add nodes with add_architecture_node.",
                        action: ("Add Node", { showComposer = true })
                    )
                } else {
                    canvas
                    overlayControls
                }
            }
            .frame(minWidth: 480, maxWidth: .infinity, maxHeight: .infinity)
            .background(Theme.canvas)

            if let id = model.selectedNode, let node = model.document?.node(id) {
                NodeInspector(model: model, node: node)
                    .id(node.id)
                    .frame(minWidth: 280, idealWidth: 320, maxWidth: 400)
            }
        }
        .sheet(isPresented: $showComposer) { NodeComposer(model: model) }
        .onChange(of: model.intent) { _, intent in
            if intent == .newNode { showComposer = true; model.intent = nil }
        }
    }

    // MARK: Canvas

    private func position(of node: PlacedNode) -> CGPoint {
        var p = CGPoint(x: node.position.x, y: node.position.y)
        if let drag, drag.id == node.id {
            p.x += drag.delta.width
            p.y += drag.delta.height
        }
        return p
    }

    private func toScreen(_ p: CGPoint) -> CGPoint {
        CGPoint(x: p.x * scale + offset.width, y: p.y * scale + offset.height)
    }

    private var canvas: some View {
        let layout = model.layout
        let positions = Dictionary(uniqueKeysWithValues: layout.nodes.map { ($0.id, position(of: $0)) })
        let highlighted = highlightedEdges

        return ZStack(alignment: .topLeading) {
            Color.clear
                .contentShape(Rectangle())
                .onTapGesture { model.selectedNode = nil }
                .gesture(
                    DragGesture(minimumDistance: 2)
                        .onChanged { v in
                            if panStart == nil { panStart = offset }
                            offset = CGSize(width: panStart!.width + v.translation.width, height: panStart!.height + v.translation.height)
                        }
                        .onEnded { _ in panStart = nil }
                )

            Canvas { ctx, _ in
                drawGrid(&ctx)
                for edge in layout.edges {
                    guard let a = positions[edge.from], let b = positions[edge.to] else { continue }
                    let isHot = highlighted.contains("\(edge.from)>\(edge.to)")
                    drawEdge(&ctx, from: a, to: b, cycle: edge.inCycle, hot: isHot)
                }
            }
            .allowsHitTesting(false)

            ForEach(layout.nodes) { node in
                NodeCard(
                    node: node,
                    path: model.document?.node(node.id)?.path,
                    selected: model.selectedNode == node.id,
                    dimmed: model.selectedNode != nil && !neighborhood.contains(node.id),
                    missingDeps: layout.missing.filter { $0.from == node.id }.count
                )
                .frame(width: nodeSize.width, height: nodeSize.height)
                .scaleEffect(scale)
                .position(toScreen(positions[node.id] ?? .zero))
                .onTapGesture { model.selectedNode = node.id }
                .gesture(
                    DragGesture(minimumDistance: 3)
                        .onChanged { v in
                            model.selectedNode = node.id
                            drag = (node.id, CGSize(width: v.translation.width / scale, height: v.translation.height / scale))
                        }
                        .onEnded { _ in
                            guard let d = drag else { return }
                            let p = position(of: node)
                            drag = nil
                            if abs(d.delta.width) + abs(d.delta.height) > 1 {
                                model.perform(.setNodePosition(id: node.id, x: p.x, y: p.y))
                            }
                        }
                )
                .contextMenu {
                    Button("Reset to Auto Layout") { model.perform(.resetNodePosition(id: node.id)) }
                    Button("Copy Id") { model.copy(node.id, message: "Copied '\(node.id)'") }
                    Divider()
                    Button("Delete Node", role: .destructive) { delete(node.id) }
                }
            }
        }
        .clipped()
        .gesture(
            MagnifyGesture()
                .onChanged { v in
                    if zoomStart == nil { zoomStart = scale }
                    scale = min(max(zoomStart! * v.magnification, 0.3), 2.5)
                }
                .onEnded { _ in zoomStart = nil }
        )
        .onAppear(perform: fit)
    }

    private var overlayControls: some View {
        VStack {
            HStack(spacing: 8) {
                legend
                Spacer()
                if !model.layout.cycles.isEmpty {
                    Chip(text: "\(model.layout.cycles.count) cycle\(model.layout.cycles.count == 1 ? "" : "s")", color: Theme.color(for: .error), symbol: "arrow.triangle.2.circlepath")
                }
                if !model.layout.missing.isEmpty {
                    Chip(text: "\(model.layout.missing.count) missing dependency", color: Theme.color(for: .warning), symbol: "exclamationmark.triangle")
                }
                Button { showComposer = true } label: { Label("Add Node", systemImage: "plus") }
                Menu {
                    Button("Fit to Window", action: fit)
                    Button("Reset All to Auto Layout") {
                        for n in model.layout.nodes where n.manual { model.perform(.resetNodePosition(id: n.id)) }
                        fit()
                    }
                    Button("Copy Graph as JSON") {
                        if let s = try? model.engine?.render(.architecture) { model.copy(s, message: "Architecture copied") }
                    }
                } label: {
                    Image(systemName: "ellipsis.circle")
                }
                .menuStyle(.borderlessButton)
                .fixedSize()
            }
            .padding(12)
            Spacer()
            HStack {
                Spacer()
                HStack(spacing: 0) {
                    Button { scale = max(scale / 1.2, 0.3) } label: { Image(systemName: "minus").frame(width: 26, height: 22) }
                    Text("\(Int(scale * 100))%").font(.caption.monospacedDigit()).frame(width: 44)
                    Button { scale = min(scale * 1.2, 2.5) } label: { Image(systemName: "plus").frame(width: 26, height: 22) }
                    Divider().frame(height: 16)
                    Button(action: fit) { Image(systemName: "arrow.up.left.and.down.right.magnifyingglass").frame(width: 30, height: 22) }
                        .help("Fit to window")
                }
                .buttonStyle(.borderless)
                .padding(4)
                .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 8))
                .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(Theme.hairline))
            }
            .padding(12)
        }
    }

    private var legend: some View {
        let kinds = Set(model.layout.nodes.map(\.kind))
        return HStack(spacing: 10) {
            ForEach(NodeKind.allCases.filter(kinds.contains)) { k in
                HStack(spacing: 4) {
                    Circle().fill(Theme.color(for: k)).frame(width: 7, height: 7)
                    Text(k.title).font(.caption).foregroundStyle(.secondary)
                }
            }
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 6)
        .background(.regularMaterial, in: Capsule())
    }

    private func fit() {
        let nodes = model.layout.nodes
        guard !nodes.isEmpty else { return }
        let xs = nodes.map(\.position.x), ys = nodes.map(\.position.y)
        let minX = xs.min()! - nodeSize.width, maxX = xs.max()! + nodeSize.width
        let minY = ys.min()! - nodeSize.height * 1.5, maxY = ys.max()! + nodeSize.height * 1.5
        // Use the window's current content size as an estimate of the canvas size.
        let viewport = NSApp.keyWindow?.contentView?.bounds.size ?? CGSize(width: 900, height: 700)
        let available = CGSize(width: max(viewport.width - 560, 400), height: max(viewport.height - 60, 300))
        let s = min(available.width / (maxX - minX), available.height / (maxY - minY), 1.2)
        scale = max(s, 0.3)
        offset = CGSize(
            width: (available.width - (maxX - minX) * scale) / 2 - minX * scale,
            height: (available.height - (maxY - minY) * scale) / 2 - minY * scale + 20
        )
    }

    private var neighborhood: Set<String> {
        guard let id = model.selectedNode, let doc = model.document else { return [] }
        var s: Set<String> = [id]
        doc.node(id)?.dependsOn.forEach { s.insert($0) }
        doc.architecture.nodes.filter { $0.dependsOn.contains(id) }.forEach { s.insert($0.id) }
        return s
    }

    private var highlightedEdges: Set<String> {
        guard let id = model.selectedNode else { return [] }
        return Set(model.layout.edges.filter { $0.from == id || $0.to == id }.map { "\($0.from)>\($0.to)" })
    }

    private func delete(_ id: String) {
        if model.selectedNode == id { model.selectedNode = nil }
        model.perform(.removeNode(id: id), success: "Deleted node '\(id)'")
    }

    // MARK: Drawing

    private func drawGrid(_ ctx: inout GraphicsContext) {
        let step = 28 * scale
        guard step > 8 else { return }
        let size = CGSize(width: 4000, height: 3000)
        var path = Path()
        var x = offset.width.truncatingRemainder(dividingBy: step)
        while x < size.width {
            var y = offset.height.truncatingRemainder(dividingBy: step)
            while y < size.height {
                path.addEllipse(in: CGRect(x: x - 0.75, y: y - 0.75, width: 1.5, height: 1.5))
                y += step
            }
            x += step
        }
        ctx.fill(path, with: .color(.primary.opacity(0.08)))
    }

    private func drawEdge(_ ctx: inout GraphicsContext, from a: CGPoint, to b: CGPoint, cycle: Bool, hot: Bool) {
        let half = CGSize(width: nodeSize.width / 2 + 4, height: nodeSize.height / 2 + 4)
        let start = toScreen(clip(from: a, toward: b, half: half))
        let end = toScreen(clip(from: b, toward: a, half: half))
        let dx = end.x - start.x, dy = end.y - start.y
        let len = max(hypot(dx, dy), 0.001)
        let ux = dx / len, uy = dy / len

        var path = Path()
        path.move(to: start)
        let bend = min(len * 0.25, 40 * scale)
        let mid = CGPoint(x: (start.x + end.x) / 2 - uy * (cycle ? bend : 0), y: (start.y + end.y) / 2 + ux * (cycle ? bend : 0))
        path.addQuadCurve(to: end, control: mid)

        let color: Color = cycle ? Theme.color(for: .error) : hot ? Theme.accent : .secondary.opacity(0.55)
        let style = StrokeStyle(lineWidth: (hot ? 2 : 1.3) * max(scale, 0.6), lineCap: .round, dash: cycle ? [5, 4] : [])
        ctx.stroke(path, with: .color(color), style: style)

        // Arrowhead pointing at the dependency.
        let tx = end.x - mid.x, ty = end.y - mid.y
        let tl = max(hypot(tx, ty), 0.001)
        let (vx, vy) = (tx / tl, ty / tl)
        let size = 8 * max(scale, 0.6)
        var head = Path()
        head.move(to: end)
        head.addLine(to: CGPoint(x: end.x - vx * size - vy * size * 0.55, y: end.y - vy * size + vx * size * 0.55))
        head.addLine(to: CGPoint(x: end.x - vx * size + vy * size * 0.55, y: end.y - vy * size - vx * size * 0.55))
        head.closeSubpath()
        ctx.fill(head, with: .color(color))
    }

    private func clip(from c: CGPoint, toward p: CGPoint, half: CGSize) -> CGPoint {
        let dx = p.x - c.x, dy = p.y - c.y
        guard dx != 0 || dy != 0 else { return c }
        let t = min(dx == 0 ? .infinity : half.width / abs(dx), dy == 0 ? .infinity : half.height / abs(dy))
        return CGPoint(x: c.x + dx * t, y: c.y + dy * t)
    }
}

private struct NodeCard: View {
    var node: PlacedNode
    var path: String?
    var selected: Bool
    var dimmed: Bool
    var missingDeps: Int

    var body: some View {
        let color = Theme.color(for: node.kind)
        HStack(spacing: 9) {
            Image(systemName: node.kind.symbol)
                .font(.system(size: 15, weight: .medium))
                .foregroundStyle(color)
                .frame(width: 30, height: 30)
                .background(color.opacity(0.14), in: RoundedRectangle(cornerRadius: 7))
            VStack(alignment: .leading, spacing: 1) {
                Text(node.label).font(.callout.weight(.semibold)).lineLimit(1)
                Text(path ?? node.kind.title)
                    .font(.caption2.monospaced())
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            Spacer(minLength: 0)
            if missingDeps > 0 {
                Image(systemName: "exclamationmark.triangle.fill")
                    .font(.caption)
                    .foregroundStyle(Theme.color(for: .warning))
                    .help("\(missingDeps) dependency target(s) missing from the map")
            }
        }
        .padding(.horizontal, 10)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Theme.surface, in: RoundedRectangle(cornerRadius: 11))
        .overlay(
            RoundedRectangle(cornerRadius: 11)
                .strokeBorder(selected ? Theme.accent : Theme.hairline, lineWidth: selected ? 2 : 1)
        )
        .overlay(alignment: .leading) {
            UnevenRoundedRectangle(topLeadingRadius: 11, bottomLeadingRadius: 11)
                .fill(color)
                .frame(width: 3)
        }
        .shadow(color: .black.opacity(selected ? 0.18 : 0.07), radius: selected ? 10 : 4, y: 2)
        .opacity(dimmed ? 0.4 : 1)
        .contentShape(RoundedRectangle(cornerRadius: 11))
    }
}

// MARK: - Inspector

private struct NodeInspector: View {
    var model: WorkspaceModel
    var node: ArchNode
    @State private var label = ""
    @State private var description = ""
    @State private var path = ""

    private var doc: EngramDocument? { model.document }
    private var dependents: [String] { doc?.architecture.nodes.filter { $0.dependsOn.contains(node.id) }.map(\.id) ?? [] }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                HStack(spacing: 10) {
                    Image(systemName: node.kind.symbol)
                        .font(.title2)
                        .foregroundStyle(Theme.color(for: node.kind))
                    VStack(alignment: .leading, spacing: 2) {
                        Text(node.displayName).font(.title3.weight(.semibold))
                        Text(node.id).font(.caption.monospaced()).foregroundStyle(.secondary).textSelection(.enabled)
                    }
                    Spacer()
                    Button { model.selectedNode = nil } label: { Image(systemName: "xmark") }
                        .buttonStyle(.borderless)
                }

                Card("Details") {
                    Picker("Type", selection: Binding(get: { node.kind }, set: { model.perform(.addNode(id: node.id, kind: $0)) })) {
                        ForEach(NodeKind.allCases) { Label($0.title, systemImage: $0.symbol).tag($0) }
                    }
                    TextField("Label", text: $label).onSubmit { save() }
                    TextField("Path in repo", text: $path).font(Theme.monoSmall).onSubmit { save() }
                    VStack(alignment: .leading, spacing: 4) {
                        Text("Description").font(.caption).foregroundStyle(.secondary)
                        CodeEditor(text: $description, minHeight: 70, font: .callout)
                            .padding(4)
                            .background(Theme.subtleFill, in: RoundedRectangle(cornerRadius: 6))
                    }
                    HStack {
                        Spacer()
                        Button("Save") { save() }
                            .disabled(label == (node.label ?? "") && description == node.description && path == (node.path ?? ""))
                    }
                }

                Card("Depends on", trailing: AnyView(Text("\(node.dependsOn.count)").font(.caption).foregroundStyle(.tertiary))) {
                    let others = (doc?.architecture.nodes ?? []).filter { $0.id != node.id }
                    if others.isEmpty {
                        Text("Add more nodes to connect them.").font(.callout).foregroundStyle(.tertiary)
                    }
                    ForEach(others) { other in
                        Toggle(isOn: Binding(
                            get: { node.dependsOn.contains(other.id) },
                            set: { on in
                                model.perform(on ? .addDependency(from: node.id, to: other.id) : .removeDependency(from: node.id, to: other.id))
                            }
                        )) {
                            Label(other.displayName, systemImage: other.kind.symbol)
                        }
                        .toggleStyle(.checkbox)
                    }
                    let missing = node.dependsOn.filter { doc?.node($0) == nil }
                    ForEach(missing, id: \.self) { m in
                        HStack {
                            Label("\(m) (missing)", systemImage: "exclamationmark.triangle.fill")
                                .foregroundStyle(Theme.color(for: .warning))
                            Spacer()
                            Button("Remove") { model.perform(.removeDependency(from: node.id, to: m)) }
                                .buttonStyle(.link)
                        }
                    }
                }

                Card("Used by") {
                    if dependents.isEmpty {
                        Text("Nothing depends on this node.").font(.callout).foregroundStyle(.tertiary)
                    }
                    ForEach(dependents, id: \.self) { d in
                        Button { model.selectedNode = d } label: { Label(d, systemImage: "arrow.turn.left.up") }
                            .buttonStyle(.link)
                    }
                }

                let decisions = (doc?.decisions ?? []).filter { $0.related.contains(node.id) }
                if !decisions.isEmpty {
                    Card("Decisions") {
                        ForEach(decisions) { d in
                            Button { model.selectedDecision = d.id; model.module = .decisions } label: {
                                Text("\(d.id) · \(d.title)").strikethrough(!d.isActive).lineLimit(1)
                            }
                            .buttonStyle(.link)
                        }
                    }
                }

                HStack {
                    if node.position != nil {
                        Button("Reset Position") { model.perform(.resetNodePosition(id: node.id)) }
                    }
                    Spacer()
                    Button("Delete Node", role: .destructive) {
                        model.selectedNode = nil
                        model.perform(.removeNode(id: node.id), success: "Deleted node '\(node.id)'")
                    }
                }
            }
            .padding(16)
        }
        .background(Theme.canvas)
        .onAppear(perform: load)
        .onChange(of: node) { load() }
    }

    private func load() {
        label = node.label ?? ""
        description = node.description
        path = node.path ?? ""
    }

    private func save() {
        model.perform(.addNode(id: node.id, kind: nil, label: label, description: description, path: path), success: "Saved '\(node.id)'")
    }
}

private struct NodeComposer: View {
    var model: WorkspaceModel
    @Environment(\.dismiss) private var dismiss
    @State private var id = ""
    @State private var label = ""
    @State private var kind: NodeKind = .module
    @State private var path = ""
    @State private var deps: Set<String> = []

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Add Architecture Node").font(.title2.weight(.semibold))
            Form {
                TextField("Id", text: $id, prompt: Text("billing-api")).font(Theme.mono)
                TextField("Label", text: $label, prompt: Text("Billing API"))
                Picker("Type", selection: $kind) {
                    ForEach(NodeKind.allCases) { Label($0.title, systemImage: $0.symbol).tag($0) }
                }
                TextField("Path", text: $path, prompt: Text("services/billing")).font(Theme.monoSmall)
            }
            let nodes = model.document?.architecture.nodes ?? []
            if !nodes.isEmpty {
                SectionLabel("Depends on")
                ScrollView {
                    VStack(alignment: .leading, spacing: 4) {
                        ForEach(nodes) { n in
                            Toggle(isOn: Binding(get: { deps.contains(n.id) }, set: { on in if on { deps.insert(n.id) } else { deps.remove(n.id) } })) {
                                Label(n.displayName, systemImage: n.kind.symbol)
                            }
                            .toggleStyle(.checkbox)
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
                .frame(maxHeight: 160)
            }
            HStack {
                Spacer()
                Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction)
                Button("Add") {
                    let clean = id.trimmingCharacters(in: .whitespaces)
                    let ordered = nodes.map(\.id).filter(deps.contains)
                    if model.perform(.addNode(id: clean, kind: kind, dependencies: ordered, label: label, path: path), success: "Added '\(clean)'") != nil {
                        model.selectedNode = clean
                        dismiss()
                    }
                }
                .keyboardShortcut(.defaultAction)
                .disabled(id.trimmingCharacters(in: .whitespaces).isEmpty)
            }
        }
        .padding(22)
        .frame(width: 440)
    }
}
