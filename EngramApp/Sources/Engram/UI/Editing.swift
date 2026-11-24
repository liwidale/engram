import SwiftUI

@Observable
@MainActor
final class EditorDraft<Value: Equatable> {
    var value: Value
    private(set) var base: Value
    private(set) var conflict: Value?
    @ObservationIgnored private var saveTask: Task<Void, Never>?

    init(_ value: Value) {
        self.value = value
        self.base = value
    }

    var dirty: Bool { value != base }

    func sync(external: Value) {
        guard external != base else { return }
        if !dirty {
            value = external
            base = external
        } else if external == value {
            base = external
        } else if Pref.promptsOnConflict {
            conflict = external
        } else {
            base = external
        }
    }

    func reset(_ value: Value) {
        saveTask?.cancel()
        self.value = value
        base = value
        conflict = nil
    }

    func loadExternal() {
        guard let external = conflict else { return }
        reset(external)
    }

    func keepMine() {
        guard let external = conflict else { return }
        base = external
        conflict = nil
    }

    func markSaved(_ saved: Value) {
        base = saved
    }

    func scheduleSave(_ commit: @escaping @MainActor () -> Void) {
        saveTask?.cancel()
        guard dirty, conflict == nil, Pref.autosaveEnabled else { return }
        saveTask = Task { @MainActor in
            try? await Task.sleep(for: Pref.autosaveInterval)
            guard !Task.isCancelled else { return }
            commit()
        }
    }
}

struct ConflictBar: View {
    var onLoad: () -> Void
    var onKeep: () -> Void

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: "exclamationmark.arrow.triangle.2.circlepath").foregroundStyle(Theme.color(for: .warning))
            Text("Modified outside Engram while you were editing.").font(.callout)
            Spacer()
            Button("Load external", action: onLoad)
            Button("Keep mine", action: onKeep).buttonStyle(.borderedProminent).tint(Theme.accent)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 8)
        .background(Theme.color(for: .warning).opacity(0.12))
    }
}

struct SaveStatus: View {
    var model: WorkspaceModel
    var dirty: Bool

    var body: some View {
        Group {
            if model.isSaving {
                Text("Saving…")
            } else if dirty {
                Text(Pref.autosaveEnabled ? "Edited" : "Unsaved · ⌘S")
            } else {
                Text("Saved")
            }
        }
        .font(.caption)
        .foregroundStyle(dirty && !Pref.autosaveEnabled ? Theme.accent : Color.secondary)
    }
}

struct PathField: View {
    var model: WorkspaceModel
    @Binding var path: String
    var placeholder: String.LocalizationValue = "src/main.rs"
    var onCommit: () -> Void = {}
    @State private var suggestions: [String] = []
    @State private var showSuggestions = false
    @State private var debounce = Debouncer()
    @FocusState private var focused: Bool

    var body: some View {
        TextField(String(localized: placeholder), text: $path)
            .textFieldStyle(.roundedBorder)
            .font(.callout.monospaced())
            .focused($focused)
            .onSubmit {
                showSuggestions = false
                onCommit()
            }
            .onChange(of: path) { _, query in
                guard focused else { return }
                debounce.run(after: .milliseconds(150)) {
                    suggestions = model.repoFiles(matching: query).filter { $0 != query }
                    showSuggestions = !suggestions.isEmpty
                }
            }
            .popover(isPresented: $showSuggestions, arrowEdge: .bottom) {
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(suggestions, id: \.self) { s in
                        Button {
                            path = s
                            showSuggestions = false
                            onCommit()
                        } label: {
                            Text(verbatim: s)
                                .font(.callout.monospaced())
                                .lineLimit(1)
                                .truncationMode(.head)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .padding(.horizontal, 10)
                                .padding(.vertical, 4)
                                .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                    }
                }
                .padding(.vertical, 6)
                .frame(width: 360)
            }
    }
}

struct FileListEditor: View {
    var model: WorkspaceModel
    @Binding var files: [String]
    @State private var draft = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            ForEach(files, id: \.self) { file in
                HStack(spacing: 6) {
                    Image(systemName: "doc").foregroundStyle(.tertiary).font(.caption)
                    Text(verbatim: file).font(.callout.monospaced()).lineLimit(1).truncationMode(.middle)
                    Spacer()
                    Button { files.removeAll { $0 == file } } label: { Image(systemName: "xmark").font(.caption2) }
                        .buttonStyle(.borderless)
                        .foregroundStyle(.tertiary)
                        .help("Remove")
                }
            }
            HStack {
                PathField(model: model, path: $draft, placeholder: "Add a file or folder", onCommit: add)
                Button("Add", action: add).disabled(draft.trimmingCharacters(in: .whitespaces).isEmpty)
            }
        }
    }

    private func add() {
        let path = draft.trimmingCharacters(in: .whitespaces)
        guard !path.isEmpty, !files.contains(path) else { return }
        files.append(path)
        draft = ""
    }
}
