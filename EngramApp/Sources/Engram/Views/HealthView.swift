import SwiftUI

struct HealthView: View {
    @Bindable var model: WorkspaceModel
    @State private var filter: HealthIssue.Severity?

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            if let report = model.healthReport {
                content(report)
            } else if model.isAuditing {
                ProgressView("Scanning codebase…").frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                EmptyState(
                    symbol: "stethoscope",
                    title: "Context health",
                    message: "Issues between this project memory and your codebase appear here: broken file paths, zombie references, deprecated decisions still in use and contradicting rules. Run an audit to check.",
                    action: ("Run audit", model.runAudit)
                )
            }
        }
        .onAppear {
            if model.healthReport == nil || model.healthReport?.revision != model.revision { model.runAudit() }
        }
        .onChange(of: model.revision) { model.runAudit() }
    }

    private var header: some View {
        HStack(spacing: 10) {
            Image(systemName: "folder").foregroundStyle(.secondary)
            VStack(alignment: .leading, spacing: 1) {
                Text("Linked codebase").font(.caption).foregroundStyle(.secondary)
                Text(model.resolvedRepoRoot.path.replacingOccurrences(of: NSHomeDirectory(), with: "~"))
                    .font(.callout.monospaced())
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .textSelection(.enabled)
            }
            if model.repoRoot == nil {
                Chip(text: "defaults to the .engram folder", color: .secondary)
            }
            Spacer()
            Button("Change…", action: chooseRoot)
            if model.repoRoot != nil {
                Button("Use .engram folder") { model.perform(.updateProject(repoRoot: "")) }
            }
            Button {
                model.runAudit()
            } label: {
                Label("Run audit", systemImage: "arrow.clockwise")
            }
            .buttonStyle(.borderedProminent)
            .tint(Theme.accent)
            .disabled(model.isAuditing)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
    }

    private func content(_ r: HealthReport) -> some View {
        let issues = r.issues.filter { filter == nil || $0.severity == filter }
        return ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                HStack(spacing: 20) {
                    ScoreGauge(score: r.score).frame(width: 112, height: 112)
                    VStack(alignment: .leading, spacing: 8) {
                        Text(r.grade == "healthy" ? String(localized: "Healthy") : r.grade == "needs_attention" ? String(localized: "Needs attention") : String(localized: "Critical"))
                            .font(.title2.weight(.semibold))
                            .foregroundStyle(Theme.scoreColor(r.score))
                        Text("\(r.summary.pathsChecked) paths and \(r.summary.referencesChecked) references checked · scanned \(EngramDate.relativeString(r.scannedAt)) at revision \(r.revision)")
                            .font(.callout)
                            .foregroundStyle(.secondary)
                        HStack(spacing: 8) {
                            filterButton(.error, count: r.summary.errors)
                            filterButton(.warning, count: r.summary.warnings)
                            filterButton(.info, count: r.summary.infos)
                            if filter != nil {
                                Button("Show all") { filter = nil }.buttonStyle(.link).font(.caption)
                            }
                        }
                    }
                    Spacer()
                    if model.isAuditing { ProgressView().controlSize(.small) }
                    Button { model.copy(markdown(r), message: "Health report copied") } label: { Label("Copy report", systemImage: "doc.on.doc") }
                }
                .padding(18)
                .background(Theme.surface, in: RoundedRectangle(cornerRadius: Theme.cardRadius))
                .overlay(RoundedRectangle(cornerRadius: Theme.cardRadius).strokeBorder(Theme.hairline))

                if !r.repoFound {
                    Label("The linked codebase directory was not found, so file paths were not checked.", systemImage: "exclamationmark.triangle.fill")
                        .foregroundStyle(Theme.color(for: .error))
                }

                if issues.isEmpty {
                    VStack(spacing: 8) {
                        Image(systemName: "checkmark.seal.fill").font(.system(size: 36)).foregroundStyle(.green)
                        Text(r.issues.isEmpty ? String(localized: "No issues. Agents get clean, current context.") : String(localized: "No issues at this severity."))
                            .foregroundStyle(.secondary)
                    }
                    .frame(maxWidth: .infinity)
                    .padding(40)
                } else {
                    VStack(spacing: 8) {
                        ForEach(issues) { IssueRow(issue: $0, model: model) }
                    }
                }
            }
            .padding(18)
        }
    }

    private func filterButton(_ s: HealthIssue.Severity, count: Int) -> some View {
        Button {
            filter = filter == s ? nil : s
        } label: {
            Chip(text: s == .error ? "\(count) errors" : s == .warning ? "\(count) warnings" : "\(count) info", color: Theme.color(for: s), symbol: symbol(s), filled: filter == s)
        }
        .buttonStyle(.plain)
    }

    private func chooseRoot() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.directoryURL = model.resolvedRepoRoot
        panel.message = String(localized: "Choose the codebase this project memory describes")
        guard panel.runModal() == .OK, let url = panel.url else { return }
        let base = model.url.deletingLastPathComponent().standardizedFileURL.path
        let chosen = url.standardizedFileURL.path
        let value: String
        if chosen == base {
            value = ""
        } else if chosen.hasPrefix(base + "/") {
            value = String(chosen.dropFirst(base.count + 1))
        } else if let rel = relativePath(from: base, to: chosen) {
            value = rel
        } else {
            value = chosen
        }
        model.perform(.updateProject(repoRoot: value), success: "Linked \(url.lastPathComponent)")
    }

    private func relativePath(from base: String, to target: String) -> String? {
        let b = base.split(separator: "/"), t = target.split(separator: "/")
        var i = 0
        while i < min(b.count, t.count), b[i] == t[i] { i += 1 }
        guard i > 0, b.count - i <= 3 else { return nil }
        return (Array(repeating: "..", count: b.count - i) + t[i...].map(String.init)).joined(separator: "/")
    }

    private func markdown(_ r: HealthReport) -> String {
        var s = "# Context Health: \(model.projectName)\n\nScore **\(r.score)/100** (\(r.grade)) · \(r.summary.errors) errors, \(r.summary.warnings) warnings, \(r.summary.infos) info\n\n"
        for i in r.issues {
            s += "- **\(i.severity.rawValue)** \(i.kindTitle): \(i.message)"
            if let sug = i.suggestion { s += " _(\(sug))_" }
            s += "\n"
        }
        return s
    }
}

private func symbol(_ s: HealthIssue.Severity) -> String {
    switch s {
    case .error: "xmark.octagon.fill"
    case .warning: "exclamationmark.triangle.fill"
    case .info: "info.circle.fill"
    }
}

private struct ScoreGauge: View {
    var score: Int
    var body: some View {
        ZStack {
            Circle().stroke(Theme.subtleFill, lineWidth: 10)
            Circle()
                .trim(from: 0, to: CGFloat(score) / 100)
                .stroke(Theme.scoreColor(score), style: StrokeStyle(lineWidth: 10, lineCap: .round))
                .rotationEffect(.degrees(-90))
                .animation(.easeOut(duration: 0.6), value: score)
            VStack(spacing: 0) {
                Text("\(score)").font(.system(size: 34, weight: .bold, design: .rounded)).monospacedDigit()
                Text("/ 100").font(.caption).foregroundStyle(.secondary)
            }
        }
    }
}

private struct IssueRow: View {
    var issue: HealthIssue
    var model: WorkspaceModel
    @State private var hover = false

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: symbol(issue.severity))
                .foregroundStyle(Theme.color(for: issue.severity))
                .font(.title3)
            VStack(alignment: .leading, spacing: 5) {
                HStack(spacing: 6) {
                    Chip(text: "\(issue.kindTitle)", color: Theme.color(for: issue.severity))
                    Text("\(issue.subject.kind) · \(issue.subject.id)")
                        .font(.caption.monospaced())
                        .foregroundStyle(.secondary)
                }
                Text(issue.message).font(.callout).textSelection(.enabled).fixedSize(horizontal: false, vertical: true)
                if let s = issue.suggestion {
                    Label(s, systemImage: "lightbulb")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            Spacer()
            VStack(alignment: .trailing, spacing: 6) {
                if issue.subject.kind != "project" {
                    Button("Open") { model.reveal(issue.subject) }.buttonStyle(.link)
                }
                if let related = issue.related {
                    Button("Open \(related.id)") { model.reveal(related) }.buttonStyle(.link)
                }
            }
            .font(.callout)
        }
        .padding(12)
        .background(hover ? Theme.hoverFill : Theme.surface, in: RoundedRectangle(cornerRadius: Theme.radius))
        .overlay(RoundedRectangle(cornerRadius: Theme.radius).strokeBorder(Theme.hairline))
        .onHover { hover = $0 }
    }
}
