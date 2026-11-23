import AppKit
import SwiftUI

struct Chip: View {
    var text: String.LocalizationValue
    var color: Color = .secondary
    var symbol: String?
    var filled = false

    var body: some View {
        HStack(spacing: 4) {
            if let symbol { Image(systemName: symbol).imageScale(.small) }
            Text(String(localized: text)).lineLimit(1)
        }
        .font(.caption.weight(.medium))
        .padding(.horizontal, 7)
        .padding(.vertical, 2.5)
        .foregroundStyle(filled ? Color.white : color)
        .background(filled ? color : color.opacity(0.13), in: Capsule())
    }
}

struct SupersededBadge: View {
    var by: String?
    var body: some View {
        Chip(text: by.map { "SUPERSEDED → \($0)" } ?? "SUPERSEDED", color: .orange, symbol: "arrow.uturn.right")
    }
}

struct EmptyState: View {
    var symbol: String
    var title: String.LocalizationValue
    var message: String.LocalizationValue
    var action: (title: String.LocalizationValue, run: () -> Void)?

    var body: some View {
        VStack(spacing: 10) {
            Image(systemName: symbol)
                .font(.system(size: 34, weight: .light))
                .foregroundStyle(.tertiary)
            Text(String(localized: title)).font(.title3.weight(.semibold))
            Text(String(localized: message))
                .font(.callout)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 360)
            if let action {
                Button(String(localized: action.title), action: action.run)
                    .buttonStyle(.borderedProminent)
                    .tint(Theme.accent)
                    .padding(.top, 4)
            }
        }
        .padding(40)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

struct SectionLabel: View {
    var title: String.LocalizationValue
    var trailing: String?
    init(_ title: String.LocalizationValue, trailing: String? = nil) {
        self.title = title
        self.trailing = trailing
    }
    var body: some View {
        HStack {
            Text(String(localized: title).uppercased())
                .font(.caption2.weight(.semibold))
                .tracking(0.6)
                .foregroundStyle(.secondary)
            Spacer()
            if let trailing {
                Text(trailing).font(.caption2.monospacedDigit()).foregroundStyle(.tertiary)
            }
        }
    }
}

struct Card<Content: View>: View {
    var title: String.LocalizationValue?
    var trailing: AnyView?
    @ViewBuilder var content: Content

    init(_ title: String.LocalizationValue? = nil, trailing: AnyView? = nil, @ViewBuilder content: () -> Content) {
        self.title = title
        self.trailing = trailing
        self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            if title != nil || trailing != nil {
                HStack {
                    if let title { SectionLabel(title) }
                    Spacer()
                    trailing
                }
            }
            content
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Theme.surface, in: RoundedRectangle(cornerRadius: Theme.cardRadius))
        .overlay(RoundedRectangle(cornerRadius: Theme.cardRadius).strokeBorder(Theme.hairline))
    }
}

struct CodeEditor: View {
    @Binding var text: String
    var placeholder: String.LocalizationValue = ""
    var minHeight: CGFloat = 120
    var font: Font = Theme.mono

    var body: some View {
        ZStack(alignment: .topLeading) {
            TextEditor(text: $text)
                .font(font)
                .scrollContentBackground(.hidden)
                .padding(.horizontal, 2)
            if text.isEmpty {
                Text(String(localized: placeholder))
                    .font(font)
                    .foregroundStyle(.tertiary)
                    .padding(.horizontal, 7)
                    .padding(.top, 1)
                    .allowsHitTesting(false)
            }
        }
        .frame(minHeight: minHeight)
    }
}

struct LineListEditor: View {
    @Binding var items: [String]
    var placeholder: String.LocalizationValue
    @State private var text = ""
    @State private var synced: [String] = []

    var body: some View {
        CodeEditor(text: $text, placeholder: placeholder, minHeight: 54, font: Theme.monoSmall)
            .onAppear { load() }
            .onChange(of: items) { _, new in if new != synced { load() } }
            .onChange(of: text) { _, new in
                let parsed = new.split(separator: "\n").map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }
                synced = parsed
                if parsed != items { items = parsed }
            }
    }

    private func load() {
        synced = items
        text = items.joined(separator: "\n")
    }
}

struct SearchField: View {
    @Binding var text: String
    var prompt: String.LocalizationValue

    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: "magnifyingglass").foregroundStyle(.tertiary)
            TextField(String(localized: prompt), text: $text).textFieldStyle(.plain)
            if !text.isEmpty {
                Button { text = "" } label: { Image(systemName: "xmark.circle.fill").foregroundStyle(.tertiary) }
                    .buttonStyle(.plain)
            }
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 6)
        .background(Theme.subtleFill, in: RoundedRectangle(cornerRadius: 7))
    }
}

struct TokenBadge: View {
    var tokens: Int
    var budget: Int?

    var body: some View {
        let over = budget.map { tokens > $0 } ?? false
        HStack(spacing: 4) {
            Image(systemName: "number.square").imageScale(.small)
            Text(budget.map { String(localized: "\(tokens) of \($0) tokens") } ?? String(localized: "\(tokens) tokens"))
                .monospacedDigit()
        }
        .font(.caption.weight(.medium))
        .foregroundStyle(over ? Theme.color(for: .error) : .secondary)
    }
}

@MainActor
final class Debouncer {
    private var task: Task<Void, Never>?
    func run(after delay: Duration = .milliseconds(250), _ work: @escaping @MainActor () -> Void) {
        task?.cancel()
        task = Task { @MainActor in
            try? await Task.sleep(for: delay)
            guard !Task.isCancelled else { return }
            work()
        }
    }
    func cancel() { task?.cancel() }
}

extension View {
    func hostingWindow(_ configure: @escaping (NSWindow) -> Void) -> some View {
        background(WindowAccessor(configure: configure))
    }
}

private struct WindowAccessor: NSViewRepresentable {
    var configure: (NSWindow) -> Void
    func makeNSView(context: Context) -> NSView {
        let v = NSView()
        DispatchQueue.main.async { if let w = v.window { configure(w) } }
        return v
    }
    func updateNSView(_ nsView: NSView, context: Context) {}
}
