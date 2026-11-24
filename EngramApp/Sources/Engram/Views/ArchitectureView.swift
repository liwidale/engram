import SwiftUI

struct ArchitectureView: View {
    @Bindable var model: WorkspaceModel
    @AppStorage(Pref.mapGrid) private var showGrid = true
    @AppStorage(Pref.mapPaths) private var showPaths = true
    @AppStorage(Pref.mapEdges) private var edgeStyle = "curved"
    @State private var scale: CGFloat = 1
    @State private var offset: CGSize = .zero
    @State private var panStart: CGSize?
    @State private var zoomStart: CGFloat?
    @State private var drag: (id: String, delta: CGSize)?
    @State private var link: (from: String, to: CGPoint)?
    @State private var highlight: Set<String> = []
    @State private var canvasSize: CGSize = .zero
    @State private var fitted = false
    @State private var showComposer = false

    private let nodeSize = CGSize(width: 184, height: 60)

    var body: some View {
        HSplitView {
            ZStack(alignment: .topLeading) {
                if model.layout.nodes.isEmpty {
                    EmptyState(
                        symbol: "point.3.connected.trianglepath.dotted",
                        title: "Architecture map",
                        message: "Draw the modules, services and databases of this project and what depends on what.",
                        note: "Agents add nodes with add_architecture_node.",
                        action: ("Add node", { showComposer = true })
                    )
                } else {
                    GeometryReader { geo in
                        canvas
                            .onAppear {
                                canvasSize = geo.size
                                if !fitted { fit(); fitted = true }
                            }
                            .onChange(of: geo.size) { _, size in canvasSize = size }
                    }
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

    private func toCanvas(_ p: CGPoint) -> CGPoint {
        CGPoint(x: (p.x - offset.width) / scale, y: (p.y - offset.height) / scale)
    }

    private var canvas: some View {
        let layout = model.layout
        let positions = Dictionary(uniqueKeysWithValues: layout.nodes.map { ($0.id, position(of: $0)) })
        let focus = focusSet
        return ZStack(alignment: .topLeading) {
            Color.clear
                .contentShape(Rectangle())
                .onTapGesture {
                    model.selectedNode = nil
                    highlight = []
                }
                .gesture(
                    DragGesture(minimumDistance: 2)
                        .onChanged { v in
                            if panStart == nil { panStart = offset }
                            offset = CGSize(width: panStart!.width + v.translation.width, height: panStart!.height + v.translation.height)
                        }
                        .onEnded { _ in panStart = nil }
                )

            Canvas { ctx, size in
                if showGrid { drawGrid(&ctx, size: size) }
                for edge in layout.edges {
                    guard let a = positions[edge.from], let b = positions[edge.to] else { continue }
                    let hot = focus.contains(edge.from) && focus.contains(edge.to) && !focus.isEmpty
                    drawEdge(&ctx, from: a, to: b, cycle: edge.inCycle, hot: hot, dim: !focus.isEmpty && !hot)
                }
                if let link, let a = positions[link.from] {
                    var path = Path()
                    let start = toScreen(CGPoint(x: a.x, y: a.y + nodeSize.height / 2))
                    path.move(to: start)
                    path.addLine(to: link.to)
                    ctx.stroke(path, with: .color(Theme.accent), style: StrokeStyle(lineWidth: 1.5, dash: [4, 3]))
                }
            }
            .allowsHitTesting(false)

            ForEach(layout.nodes) { node in
                let node = node
                NodeCard(
                    node: node,
                    path: showPaths ? model.document?.node(node.id)?.path : nil,
                    selected: model.selectedNode == node.id,
                    dimmed: !focus.isEmpty && !focus.contains(node.id),
                    missingDeps: layout.missing.filter { $0.from == node.id }.count,
                    onLinkChanged: { location in
                        let global = CGPoint(
                            x: toScreen(positions[node.id] ?? .zero).x + location.x * scale,
                            y: toScreen(positions[node.id] ?? .zero).y + location.y * scale
                        )
                        link = (node.id, global)
                    },
                    onLinkEnded: { finishLink() }
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
                    Button("Reset to automatic layout") { model.perform(.resetNodePosition(id: node.id)) }
                    Button("Copy id") { model.copy(node.id, message: "Copied \(node.id)") }
                    Divider()
                    Button("Delete node…", role: .destructive) { delete(node.id) }
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
    }

    private var focusSet: Set<String> {
        if !highlight.isEmpty { return highlight }
        guard let id = model.selectedNode, let doc = model.document else { return [] }
        var s: Set<String> = [id]
        doc.node(id)?.dependsOn.forEach { s.insert($0) }
        doc.architecture.nodes.filter { $0.dependsOn.contains(id) }.forEach { s.insert($0.id) }
        return s
    }

    private func finishLink() {
        guard let link else { return }
        self.link = nil
        let target = toCanvas(link.to)
        let hit = model.layout.nodes.first { n in
            let p = position(of: n)
            return abs(p.x - target.x) <= nodeSize.width / 2 && abs(p.y - target.y) <= nodeSize.height / 2
        }
        guard let hit, hit.id != link.from else { return }
        if model.document?.node(link.from)?.dependsOn.contains(hit.id) == true { return }
        model.perform(.addDependency(from: link.from, to: hit.id), success: "\(link.from) now depends on \(hit.id)")
    }

    private var overlayControls: some View {
        VStack {
            HStack(spacing: 8) {
                legend
                Spacer()
                ForEach(Array(model.layout.cycles.enumerated()), id: \.offset) { _, cycle in
                    Button { focus(on: Set(cycle)) } label: {
                        Chip(text: "Cycle: \(cycle.joined(separator: " → "))", color: Theme.color(for: .error), symbol: "arrow.triangle.2.circlepath")
                    }
                    .buttonStyle(.plain)
                    .help("Show the nodes in this cycle")
                }
                if !model.layout.missing.isEmpty {
                    Button { focus(on: Set(model.layout.missing.map(\.from))) } label: {
                        Chip(text: "\(model.layout.missing.count) missing dependencies", color: Theme.color(for: .warning), symbol: "exclamationmark.triangle")
                    }
                    .buttonStyle(.plain)
                    .help("Show the nodes that point at missing dependencies")
                }
                Button { showComposer = true } label: { Label("Add node", systemImage: "plus") }
                Menu {
                    Button("Fit to window", action: fit)
                    Button("Reset all to automatic layout") {
                        model.perform(.resetAllNodePositions) { _ in fit() }
                    }
                    Button("Copy as JSON") {
                        if let s = try? model.engine?.render(.architecture) { model.copy(s, message: "Architecture copied") }
                    }
                    Button("Copy as Mermaid") {
                        if let s = try? model.engine?.render(.mermaid) { model.copy(s, message: "Mermaid diagram copied") }
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
                Text("Drag from a node's bottom handle onto another node to add a dependency.")
                    .font(.caption)
                    .foregroundStyle(.tertiary)
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
                    Image(systemName: k.symbol).font(.caption2).foregroundStyle(Theme.color(for: k))
                    Text(k.title).font(.caption).foregroundStyle(.secondary)
                }
            }
            Divider().frame(height: 12)
            Text("A → B means A depends on B").font(.caption).foregroundStyle(.secondary)
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 6)
        .background(.regularMaterial, in: Capsule())
    }

    private func bounds(of ids: Set<String>?) -> CGRect? {
        let nodes = model.layout.nodes.filter { ids?.contains($0.id) ?? true }
        guard !nodes.isEmpty else { return nil }
        let xs = nodes.map(\.position.x), ys = nodes.map(\.position.y)
        return CGRect(
            x: xs.min()! - nodeSize.width / 2, y: ys.min()! - nodeSize.height / 2,
            width: xs.max()! - xs.min()! + nodeSize.width, height: ys.max()! - ys.min()! + nodeSize.height
        )
    }

    private func fit() {
        guard let box = bounds(of: nil), canvasSize.width > 0 else { return }
        center(on: box, maxScale: 1.2)
    }

    private func focus(on ids: Set<String>) {
        highlight = ids
        guard let box = bounds(of: ids) else { return }
        withAnimation(.easeInOut(duration: 0.3)) { center(on: box, maxScale: 1.4) }
    }

    private func center(on box: CGRect, maxScale: CGFloat) {
        let padded = box.insetBy(dx: -60, dy: -80)
        let available = CGSize(width: max(canvasSize.width, 200), height: max(canvasSize.height - 90, 200))
        scale = max(min(available.width / padded.width, available.height / padded.height, maxScale), 0.3)
        offset = CGSize(
            width: canvasSize.width / 2 - padded.midX * scale,
            height: (canvasSize.height + 20) / 2 - padded.midY * scale
        )
    }

    private func delete(_ id: String) {
        model.delete(.removeNode(id: id), title: id) {
            if model.selectedNode == id { model.selectedNode = nil }
        }
    }

    private func drawGrid(_ ctx: inout GraphicsContext, size: CGSize) {
        let step = 28 * scale
        guard step > 8 else { return }
        var path = Path()
        var x = offset.width.truncatingRemainder(dividingBy: step)
        if x < 0 { x += step }
        while x < size.width {
            var y = offset.height.truncatingRemainder(dividingBy: step)
            if y < 0 { y += step }
            while y < size.height {
                path.addEllipse(in: CGRect(x: x - 0.75, y: y - 0.75, width: 1.5, height: 1.5))
                y += step
            }
            x += step
        }
        ctx.fill(path, with: .color(.primary.opacity(0.08)))
    }

    private func drawEdge(_ ctx: inout GraphicsContext, from a: CGPoint, to b: CGPoint, cycle: Bool, hot: Bool, dim: Bool) {
        let half = CGSize(width: nodeSize.width / 2, height: nodeSize.height / 2)
        var path = Path()
        let start: CGPoint, end: CGPoint, control: CGPoint
        if edgeStyle == "curved", b.y - a.y > nodeSize.height {
            start = toScreen(CGPoint(x: a.x, y: a.y + half.height))
            end = toScreen(CGPoint(x: b.x, y: b.y - half.height - 4))
            let bend = max((end.y - start.y) / 2, 30 * scale)
            let c1 = CGPoint(x: start.x, y: start.y + bend)
            control = CGPoint(x: end.x, y: end.y - bend)
            path.move(to: start)
            path.addCurve(to: end, control1: c1, control2: control)
        } else {
            start = toScreen(clip(from: a, toward: b, half: half))
            end = toScreen(clip(from: b, toward: a, half: CGSize(width: half.width + 4, height: half.height + 4)))
            let dx = end.x - start.x, dy = end.y - start.y
            let len = max(hypot(dx, dy), 0.001)
            let bend = cycle || edgeStyle == "curved" ? min(len * 0.2, 36 * scale) : 0
            control = CGPoint(x: (start.x + end.x) / 2 - dy / len * bend, y: (start.y + end.y) / 2 + dx / len * bend)
            path.move(to: start)
            path.addQuadCurve(to: end, control: control)
        }
        let color: Color = cycle ? Theme.color(for: .error) : hot ? Theme.accent : Color.secondary.opacity(dim ? 0.25 : 0.55)
        ctx.stroke(path, with: .color(color), style: StrokeStyle(lineWidth: (hot ? 2 : 1.3) * max(scale, 0.6), lineCap: .round, dash: cycle ? [5, 4] : []))

        let tx = end.x - control.x, ty = end.y - control.y
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
    var onLinkChanged: (CGPoint) -> Void
    var onLinkEnded: () -> Void
    @State private var hover = false

    var body: some View {
        let color = Theme.color(for: node.kind)
        let shape = RoundedRectangle(cornerRadius: node.kind == .database ? 16 : 10)
        HStack(spacing: 9) {
            Image(systemName: node.kind.symbol)
                .font(.system(size: 14, weight: .medium))
                .foregroundStyle(color)
                .frame(width: 30, height: 30)
                .background(color.opacity(0.14), in: RoundedRectangle(cornerRadius: 7))
            VStack(alignment: .leading, spacing: 1) {
                Text(verbatim: node.label).font(.callout.weight(.semibold)).lineLimit(1)
                Text(verbatim: path ?? node.kind.title)
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
                    .help("Points at dependencies missing from the map")
            }
        }
        .padding(.horizontal, 10)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(node.kind == .external ? Theme.canvas : Theme.surface, in: shape)
        .overlay(
            shape.strokeBorder(
                selected ? Theme.accent : node.kind == .external ? Color.secondary.opacity(0.5) : Theme.hairline,
                style: StrokeStyle(lineWidth: selected ? 2 : 1, dash: node.kind == .external ? [5, 3] : [])
            )
        )
        .overlay(alignment: .bottom) {
            if hover || selected {
                Circle()
                    .fill(Theme.accent)
                    .frame(width: 10, height: 10)
                    .overlay(Circle().strokeBorder(.white, lineWidth: 1.5))
                    .offset(y: 5)
                    .gesture(
                        DragGesture(minimumDistance: 2, coordinateSpace: .named("card"))
                            .onChanged { v in onLinkChanged(CGPoint(x: v.location.x - 92, y: v.location.y - 30)) }
                            .onEnded { _ in onLinkEnded() }
                    )
                    .help("Drag onto another node to add a dependency")
            }
        }
        .coordinateSpace(name: "card")
        .shadow(color: .black.opacity(selected ? 0.18 : 0.07), radius: selected ? 10 : 4, y: 2)
        .opacity(dimmed ? 0.4 : 1)
        .contentShape(shape)
        .onHover { hover = $0 }
    }
}

struct NodeDraft: Equatable {
    var label: String
    var path: String
    var description: String

    init(_ n: ArchNode) {
        label = n.label ?? ""
        path = n.path ?? ""
        description = n.description
    }
}

private struct NodeInspector: View {
    var model: WorkspaceModel
    var node: ArchNode
    @State private var draft: EditorDraft<NodeDraft>

    init(model: WorkspaceModel, node: ArchNode) {
        self.model = model
        self.node = node
        _draft = State(initialValue: EditorDraft(NodeDraft(node)))
    }

    private var doc: EngramDocument? { model.document }
    private var dependents: [String] { doc?.architecture.nodes.filter { $0.dependsOn.contains(node.id) }.map(\.id) ?? [] }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                HStack(spacing: 10) {
                    Image(systemName: node.kind.symbol).font(.title2).foregroundStyle(Theme.color(for: node.kind))
                    VStack(alignment: .leading, spacing: 2) {
                        Text(verbatim: node.displayName).font(.title3.weight(.semibold))
                        Text(verbatim: node.id).font(.caption.monospaced()).foregroundStyle(.secondary).textSelection(.enabled)
                    }
                    Spacer()
                    SaveStatus(model: model, dirty: draft.dirty)
                    Button { model.selectedNode = nil } label: { Image(systemName: "xmark") }.buttonStyle(.borderless).help("Close")
                }
                if draft.conflict != nil {
                    ConflictBar(onLoad: { draft.loadExternal() }, onKeep: {
                        draft.keepMine()
                        _ = commit()
                    })
                    .clipShape(RoundedRectangle(cornerRadius: 8))
                }
                Card("Details") {
                    Picker("Type", selection: Binding(get: { node.kind }, set: { model.perform(.addNode(id: node.id, kind: $0)) })) {
                        ForEach(NodeKind.allCases) { Label($0.title, systemImage: $0.symbol).tag($0) }
                    }
                    TextField("Label", text: $draft.value.label)
                    VStack(alignment: .leading, spacing: 4) {
                        Text("Path in repository").font(.caption).foregroundStyle(.secondary)
                        PathField(model: model, path: $draft.value.path, placeholder: "services/billing")
                    }
                    VStack(alignment: .leading, spacing: 4) {
                        Text("Description").font(.caption).foregroundStyle(.secondary)
                        CodeEditor(text: $draft.value.description, minHeight: 70, font: .callout)
                            .padding(4)
                            .background(Theme.subtleFill, in: RoundedRectangle(cornerRadius: 6))
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
                            set: { on in model.perform(on ? .addDependency(from: node.id, to: other.id) : .removeDependency(from: node.id, to: other.id)) }
                        )) {
                            Label(other.displayName, systemImage: other.kind.symbol)
                        }
                        .toggleStyle(.checkbox)
                    }
                    ForEach(node.dependsOn.filter { doc?.node($0) == nil }, id: \.self) { m in
                        HStack {
                            Label("\(m) (missing)", systemImage: "exclamationmark.triangle.fill").foregroundStyle(Theme.color(for: .warning))
                            Spacer()
                            Button("Remove") { model.perform(.removeDependency(from: node.id, to: m)) }.buttonStyle(.link)
                        }
                    }
                }
                Card("Used by") {
                    if dependents.isEmpty {
                        Text("Nothing depends on this node.").font(.callout).foregroundStyle(.tertiary)
                    }
                    ForEach(dependents, id: \.self) { d in
                        Button { model.selectedNode = d } label: { Label(d, systemImage: "arrow.turn.left.up") }.buttonStyle(.link)
                    }
                }
                let decisions = (doc?.decisions ?? []).filter { $0.related.contains(node.id) }
                if !decisions.isEmpty {
                    Card("Decisions") {
                        ForEach(decisions) { d in
                            Button { model.reveal(EntityRef(kind: "decision", id: d.id)) } label: {
                                Text(verbatim: "\(d.id) · \(d.title)").strikethrough(!d.isActive).lineLimit(1)
                            }
                            .buttonStyle(.link)
                        }
                    }
                }
                HStack {
                    if node.position != nil {
                        Button("Reset position") { model.perform(.resetNodePosition(id: node.id)) }
                    }
                    Spacer()
                    Button("Delete node…", role: .destructive) {
                        model.delete(.removeNode(id: node.id), title: node.id) { model.selectedNode = nil }
                    }
                }
            }
            .padding(16)
        }
        .background(Theme.canvas)
        .onAppear { model.registerCommit("node") { commit() } }
        .onDisappear {
            _ = commit()
            model.unregisterCommit("node")
        }
        .onChange(of: node) { _, new in draft.sync(external: NodeDraft(new)) }
        .onChange(of: draft.value) { draft.scheduleSave { _ = commit() } }
    }

    @discardableResult
    private func commit() -> Bool {
        guard draft.dirty, draft.conflict == nil else { return false }
        let v = draft.value
        draft.markSaved(v)
        model.perform(.addNode(id: node.id, kind: nil, label: v.label, description: v.description, path: v.path))
        return true
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
            Text("Add architecture node").font(.title2.weight(.semibold))
            Form {
                TextField("Id", text: $id, prompt: Text(verbatim: "billing-api")).font(.body.monospaced())
                TextField("Label", text: $label, prompt: Text(verbatim: "Billing API"))
                Picker("Type", selection: $kind) {
                    ForEach(NodeKind.allCases) { Label($0.title, systemImage: $0.symbol).tag($0) }
                }
                LabeledContent("Path") { PathField(model: model, path: $path, placeholder: "services/billing") }
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
                    model.perform(.addNode(id: clean, kind: kind, dependencies: ordered, label: label, path: path), success: "Added \(clean)") { result in
                        guard result != nil else { return }
                        model.selectedNode = clean
                        dismiss()
                    }
                }
                .keyboardShortcut(.defaultAction)
                .disabled(id.trimmingCharacters(in: .whitespaces).isEmpty)
            }
        }
        .padding(22)
        .frame(width: 460)
    }
}
