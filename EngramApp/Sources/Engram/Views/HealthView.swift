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
            } else {
                ProgressView("Checking the project…").frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .onAppear {
            if model.healthReport?.revision != model.revision { model.runAudit() }
        }
    }

    private var header: some View {
        HStack(spacing: 10) {
            Image(systemName: "folder").foregroundStyle(.secondary)
            Text(verbatim: model.resolvedRepoRoot.path.replacingOccurrences(of: NSHomeDirectory(), with: "~"))
                .font(.callout.monospaced())
                .lineLimit(1)
                .truncationMode(.middle)
                .textSelection(.enabled)
            Button("Change in Project settings") { model.showProjectSettings = true }.buttonStyle(.link).font(.callout)
            Spacer()
            if model.isAuditing { ProgressView().controlSize(.small) }
            Button {
                model.runAudit()
            } label: {
                Label("Check again", systemImage: "arrow.clockwise")
            }
            .disabled(model.isAuditing)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
    }

    private func content(_ r: HealthReport) -> some View {
        let issues = r.issues.filter { filter == nil || $0.severity == filter }
        let fixable = r.issues.compactMap(\.fix)
        return ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                HStack(spacing: 20) {
                    ScoreGauge(score: r.score).frame(width: 112, height: 112)
                    VStack(alignment: .leading, spacing: 8) {
                        Text(r.grade == "healthy" ? String(localized: "Healthy") : r.grade == "needs_attention" ? String(localized: "Needs attention") : String(localized: "Critical"))
                            .font(.title2.weight(.semibold))
                            .foregroundStyle(Theme.scoreColor(r.score))
                        Text("\(r.summary.pathsChecked) paths and \(r.summary.referencesChecked) references checked at revision \(r.revision)")
                            .font(.callout)
                            .foregroundStyle(.secondary)
                        HStack(spacing: 8) {
                            filterButton(.error, count: r.summary.errors)
                            filterButton(.warning, count: r.summary.warnings)
                            filterButton(.info, count: r.summary.infos)
                            if r.summary.ignored > 0 {
                                Button("\(r.summary.ignored) ignored") { model.showProjectSettings = true }
                                    .buttonStyle(.link)
                                    .font(.caption)
                            }
                        }
                    }
                    Spacer()
                    VStack(alignment: .trailing, spacing: 8) {
                        if fixable.count > 1 {
                            Button("Fix all \(fixable.count)") {
                                let ops = fixable.compactMap { EngramOp.raw($0.op) }
                                model.perform(.batch(ops), success: "Applied \(ops.count) fixes") { _ in model.runAudit() }
                            }
                            .buttonStyle(.borderedProminent)
                            .tint(Theme.accent)
                        }
                        Button { model.copy(markdown(r), message: "Health report copied") } label: { Label("Copy report", systemImage: "doc.on.doc") }
                    }
                }
                .padding(18)
                .background(Theme.surface, in: RoundedRectangle(cornerRadius: Theme.cardRadius))
                .overlay(RoundedRectangle(cornerRadius: Theme.cardRadius).strokeBorder(Theme.hairline))

                if !r.repoFound {
                    Label("The linked codebase folder was not found, so file paths were not checked.", systemImage: "exclamationmark.triangle.fill")
                        .foregroundStyle(Theme.color(for: .error))
                }

                if issues.isEmpty {
                    VStack(spacing: 8) {
                        Image(systemName: "checkmark.seal.fill").font(.system(size: 36)).foregroundStyle(Theme.success)
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
        .help("Show only this severity")
    }

    private func markdown(_ r: HealthReport) -> String {
        var s = "# Context health: \(model.projectName)\n\nScore **\(r.score)/100** (\(r.grade)) · \(r.summary.errors) errors, \(r.summary.warnings) warnings, \(r.summary.infos) info\n\n"
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

struct ScoreGauge: View {
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
                    Text(verbatim: "\(issue.subject.kind) · \(issue.subject.id)")
                        .font(.caption.monospaced())
                        .foregroundStyle(.secondary)
                }
                Text(verbatim: issue.message).font(.callout).textSelection(.enabled).fixedSize(horizontal: false, vertical: true)
                if let s = issue.suggestion {
                    Label(s, systemImage: "lightbulb").font(.caption).foregroundStyle(.secondary)
                }
            }
            Spacer()
            VStack(alignment: .trailing, spacing: 6) {
                if let fix = issue.fix, let op = EngramOp.raw(fix.op) {
                    Button(fix.label) {
                        model.perform(op, success: "Fixed: \(fix.label)") { _ in model.runAudit() }
                    }
                    .buttonStyle(.borderedProminent)
                    .tint(Theme.accent)
                    .controlSize(.small)
                }
                HStack(spacing: 10) {
                    if issue.subject.kind != "project" {
                        Button("Open") { model.reveal(issue.subject) }.buttonStyle(.link)
                    }
                    if let related = issue.related {
                        Button("Open \(related.id)") { model.reveal(related) }.buttonStyle(.link)
                    }
                    Button("Ignore") { ignore() }.buttonStyle(.link).help("Hide this issue in future checks")
                }
                .font(.callout)
            }
        }
        .padding(12)
        .background(hover ? Theme.hoverFill : Theme.surface, in: RoundedRectangle(cornerRadius: Theme.radius))
        .overlay(RoundedRectangle(cornerRadius: Theme.radius).strokeBorder(Theme.hairline))
        .onHover { hover = $0 }
    }

    private func ignore() {
        var ignored = model.document?.project.settings.healthIgnoreIssues ?? []
        guard !ignored.contains(issue.id) else { return }
        ignored.append(issue.id)
        model.perform(.updateSettings([.ignoredIssues(ignored)]), success: "Issue ignored") { _ in model.runAudit() }
    }
}
