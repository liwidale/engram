import SwiftUI

struct ConnectAgentView: View {
    var model: WorkspaceModel
    @Environment(\.dismiss) private var dismiss
    @State private var configs: [ClientConfig] = []
    @State private var client: MCPClient = Pref.defaultClient
    @State private var check: MCPServer.Check?
    @State private var checking = false
    @State private var written: WriteOutcome?

    private var config: ClientConfig? { configs.first { $0.client == client } }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Connect an agent").font(.title2.weight(.semibold))
            Text("Give an AI agent read and write access to this project memory through engram-mcp.")
                .font(.callout)
                .foregroundStyle(.secondary)
            Picker("Client", selection: $client) {
                ForEach(MCPClient.allCases) { Text(verbatim: $0.title).tag($0) }
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .onChange(of: client) { written = nil }
            if let config {
                if let command = config.command {
                    VStack(alignment: .leading, spacing: 4) {
                        SectionLabel("Terminal command")
                        snippet(command)
                    }
                }
                VStack(alignment: .leading, spacing: 4) {
                    SectionLabel("Config", trailing: config.target.map(abbreviate))
                    snippet(config.snippet)
                }
                if let written {
                    Label(
                        written.created ? String(localized: "Created \(abbreviate(written.path))") : String(localized: "Updated \(abbreviate(written.path))"),
                        systemImage: "checkmark.circle.fill"
                    )
                    .foregroundStyle(Theme.success)
                    .font(.callout)
                }
                switch check {
                case .ok(let version):
                    Label("Server works: \(version)", systemImage: "checkmark.circle.fill").foregroundStyle(Theme.success).font(.callout)
                case .failed(let reason):
                    Label(reason, systemImage: "xmark.octagon.fill").foregroundStyle(Theme.color(for: .error)).font(.callout)
                case nil:
                    EmptyView()
                }
                HStack {
                    Button("Verify server") {
                        checking = true
                        Task {
                            check = await MCPServer.verify(model.mcpBinaryPath)
                            checking = false
                        }
                    }
                    if checking { ProgressView().controlSize(.small) }
                    Spacer()
                    Button("Done") { dismiss() }.keyboardShortcut(.cancelAction)
                    if let target = config.target {
                        Button("Write to \(URL(fileURLWithPath: target).lastPathComponent)") {
                            written = model.writeClientConfig(config.client)
                        }
                        .help(config.projectScoped ? String(localized: "Adds Engram to this repository's config, keeping other servers") : String(localized: "Adds Engram to your user config, keeping other servers"))
                    }
                    Button("Copy") {
                        model.copy(config.command ?? config.snippet, message: "Copied the \(config.title) config")
                    }
                    .buttonStyle(.borderedProminent)
                    .tint(Theme.accent)
                    .keyboardShortcut(.defaultAction)
                }
            }
        }
        .padding(22)
        .frame(width: 620)
        .onAppear { configs = model.clientConfigs() }
    }

    private func snippet(_ text: String) -> some View {
        ScrollView(.horizontal, showsIndicators: false) {
            Text(verbatim: text).font(.callout.monospaced()).textSelection(.enabled).fixedSize().padding(10)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Theme.subtleFill, in: RoundedRectangle(cornerRadius: 7))
    }

    private func abbreviate(_ path: String) -> String {
        path.replacingOccurrences(of: NSHomeDirectory(), with: "~")
    }
}

struct ShortcutsView: View {
    @Environment(\.dismiss) private var dismiss

    private let rows: [(String, LocalizedStringKey)] = [
        ("⌘K", "Command palette"),
        ("⌘1 – ⌘7", "Switch module"),
        ("⌘S", "Save pending edits"),
        ("⌘Z / ⇧⌘Z", "Undo and redo"),
        ("⌘R", "Reload from disk"),
        ("⇧⌘N", "New memory entry"),
        ("⇧⌘D", "Log decision"),
        ("⇧⌘H", "Run health check"),
        ("⇧⌘M", "Connect agent"),
        ("⇧⌘C", "Copy context or prompt"),
        ("⌘,", "Settings"),
        ("⌘/", "Keyboard shortcuts"),
    ]

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Keyboard shortcuts").font(.title2.weight(.semibold))
            Grid(alignment: .leading, horizontalSpacing: 24, verticalSpacing: 8) {
                ForEach(rows, id: \.0) { row in
                    GridRow {
                        Text(verbatim: row.0).font(.callout.monospaced()).foregroundStyle(.secondary)
                        Text(row.1)
                    }
                }
            }
            HStack {
                Spacer()
                Button("Done") { dismiss() }.keyboardShortcut(.defaultAction)
            }
        }
        .padding(22)
        .frame(width: 380)
    }
}
