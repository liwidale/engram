import AppKit
import SwiftUI

struct SettingsView: View {
    var body: some View {
        TabView {
            GeneralSettings().tabItem { Label("General", systemImage: "gearshape") }
            AppearanceSettings().tabItem { Label("Appearance", systemImage: "paintbrush") }
            AgentSettings().tabItem { Label("Agents", systemImage: "point.3.filled.connected.trianglepath.dotted") }
            ContextSettings().tabItem { Label("Context", systemImage: "square.stack.3d.down.right") }
        }
        .frame(width: 560)
        .scenePadding()
    }
}

private struct GeneralSettings: View {
    @AppStorage(Pref.language) private var language = "system"
    @AppStorage(Pref.startup) private var startup = "welcome"
    @AppStorage(Pref.autosave) private var autosave = true
    @AppStorage(Pref.autosaveDelay) private var delay = 900.0
    @AppStorage(Pref.confirmDelete) private var confirmDelete = true
    @AppStorage(Pref.externalNotifications) private var notify = true
    @AppStorage(Pref.conflictPrompt) private var conflictPrompt = true
    @State private var languageChanged = false

    var body: some View {
        Form {
            Section {
                Picker("Language", selection: $language) {
                    Text("System").tag("system")
                    Text(verbatim: "English").tag("en")
                    Text(verbatim: "Русский").tag("ru")
                }
                .onChange(of: language) { _, value in
                    Pref.applyLanguage(value)
                    languageChanged = true
                }
                if languageChanged {
                    Text("Restart Engram to switch the language.")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                }
                Picker("On launch", selection: $startup) {
                    Text("Show the welcome window").tag("welcome")
                    Text("Reopen the previous session").tag("restore")
                }
            }
            Section("Saving") {
                Toggle("Save edits automatically", isOn: $autosave)
                LabeledContent("Save after") {
                    HStack {
                        Slider(value: $delay, in: 500...3000, step: 100)
                        Text("\(Int(delay)) ms").monospacedDigit().frame(width: 64, alignment: .trailing)
                    }
                }
                .disabled(!autosave)
                Toggle("Confirm before deleting entries", isOn: $confirmDelete)
            }
            Section("Agent edits") {
                Toggle("Show a notice when an agent changes the file", isOn: $notify)
                Toggle("Ask before replacing my unsaved edits", isOn: $conflictPrompt)
            }
        }
        .formStyle(.grouped)
    }
}

private struct AppearanceSettings: View {
    @AppStorage(Pref.theme) private var theme = "system"
    @AppStorage(Pref.monoFont) private var monoFont = ""
    @AppStorage(Pref.monoSize) private var monoSize = 13.0
    @AppStorage(Pref.softWrap) private var softWrap = true
    @AppStorage(Pref.memoryMode) private var memoryMode = "edit"
    @AppStorage(Pref.mapGrid) private var mapGrid = true
    @AppStorage(Pref.mapPaths) private var mapPaths = true
    @AppStorage(Pref.mapEdges) private var mapEdges = "curved"
    @State private var families: [String] = []

    var body: some View {
        Form {
            Section {
                Picker("Theme", selection: $theme) {
                    Text("System").tag("system")
                    Text("Light").tag("light")
                    Text("Dark").tag("dark")
                }
                .onChange(of: theme) { _, value in Pref.applyTheme(value) }
            }
            Section("Editor") {
                Picker("Monospaced font", selection: $monoFont) {
                    Text("System monospaced").tag("")
                    ForEach(families, id: \.self) { Text(verbatim: $0).tag($0) }
                }
                LabeledContent("Font size") {
                    HStack {
                        Slider(value: $monoSize, in: 12...18, step: 1)
                        Text("\(Int(monoSize)) pt").monospacedDigit().frame(width: 44, alignment: .trailing)
                    }
                }
                Toggle("Wrap long lines", isOn: $softWrap)
                Picker("Memory editor opens in", selection: $memoryMode) {
                    Text("Edit").tag("edit")
                    Text("Split").tag("split")
                    Text("Preview").tag("preview")
                }
                Text(verbatim: "fn context(task: &str) -> Bundle")
                    .font(Font(Pref.monospacedFont(size: monoSize)))
                    .foregroundStyle(.secondary)
                    .id(monoFont)
            }
            Section("Architecture map") {
                Toggle("Show dot grid", isOn: $mapGrid)
                Toggle("Show file paths on nodes", isOn: $mapPaths)
                Picker("Edges", selection: $mapEdges) {
                    Text("Curved").tag("curved")
                    Text("Straight").tag("straight")
                }
            }
        }
        .formStyle(.grouped)
        .onAppear { families = Pref.monospacedFamilies }
    }
}

private struct AgentSettings: View {
    @AppStorage(Pref.mcpBinary) private var binary = ""
    @AppStorage(Pref.mcpClient) private var client = MCPClient.claudeCode.rawValue
    @AppStorage(Pref.handoffAuthor) private var author = "human"
    @State private var check: MCPServer.Check?
    @State private var checking = false

    var body: some View {
        Form {
            Section("Server binary") {
                TextField("Path", text: $binary, prompt: Text(verbatim: MCPServer.detectedPath))
                    .font(.system(.body, design: .monospaced))
                    .onChange(of: binary) { check = nil }
                HStack {
                    Button("Choose…", action: choose)
                    if !binary.isEmpty { Button("Use detected binary") { binary = "" } }
                    Spacer()
                    if checking { ProgressView().controlSize(.small) }
                    Button("Test server binary") {
                        checking = true
                        Task {
                            check = await MCPServer.verify(MCPServer.path)
                            checking = false
                        }
                    }
                }
                switch check {
                case .ok(let version):
                    Label(version, systemImage: "checkmark.circle.fill").foregroundStyle(Theme.success)
                case .failed(let reason):
                    Label(reason, systemImage: "xmark.octagon.fill").foregroundStyle(Theme.color(for: .error))
                case nil:
                    Text("Leave empty to use the binary inside Engram.app.").font(.callout).foregroundStyle(.secondary)
                }
            }
            Section {
                Picker("Default client", selection: $client) {
                    ForEach(MCPClient.allCases) { Text(verbatim: $0.title).tag($0.rawValue) }
                }
                TextField("Handoff author", text: $author, prompt: Text(verbatim: "human"))
                Text("Saved with handoffs you edit by hand. It is committed to Git with the file.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
    }

    private func choose() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = false
        panel.message = String(localized: "Choose the engram-mcp binary")
        if panel.runModal() == .OK, let url = panel.url { binary = url.path }
    }
}

private struct ContextSettings: View {
    @AppStorage(Pref.contextBudget) private var budget = 4000
    @AppStorage(Pref.contextHandoff) private var includeHandoff = true
    @AppStorage(Pref.contextFormat) private var format = "xml"

    var body: some View {
        Form {
            Section {
                LabeledContent("Token budget") {
                    HStack {
                        Slider(value: Binding(get: { Double(budget) }, set: { budget = Int($0 / 250) * 250 }), in: 500...32000)
                        Text("\(budget)").monospacedDigit().frame(width: 56, alignment: .trailing)
                    }
                }
                Toggle("Include the agent handoff", isOn: $includeHandoff)
                Picker("Output format", selection: $format) {
                    Text(verbatim: "XML").tag("xml")
                    Text(verbatim: "Markdown").tag("markdown")
                }
            } footer: {
                Text("Defaults for new Context builder sessions.").font(.callout).foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
    }
}
