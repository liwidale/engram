import SwiftUI

/// Native Markdown preview: block structure (headings, lists, task lists, quotes, fenced
/// code, rules, tables as text) rendered with SwiftUI, inline styling via AttributedString.
struct MarkdownView: View {
    var text: String

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            ForEach(Array(MarkdownBlock.parse(text).enumerated()), id: \.offset) { _, block in
                view(for: block)
            }
        }
        .textSelection(.enabled)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    @ViewBuilder
    private func view(for block: MarkdownBlock) -> some View {
        switch block {
        case .heading(let level, let s):
            inline(s)
                .font(level == 1 ? .title2.weight(.bold) : level == 2 ? .title3.weight(.semibold) : .headline)
                .padding(.top, level <= 2 ? 6 : 2)
        case .paragraph(let s):
            inline(s).fixedSize(horizontal: false, vertical: true)
        case .superseded(let s):
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                SupersededBadge(by: nil)
                inline(s).strikethrough().foregroundStyle(.secondary)
            }
        case .list(let items):
            VStack(alignment: .leading, spacing: 4) {
                ForEach(Array(items.enumerated()), id: \.offset) { i, item in
                    HStack(alignment: .firstTextBaseline, spacing: 6) {
                        Group {
                            switch item.marker {
                            case .bullet: Text("•").foregroundStyle(.secondary)
                            case .number(let n): Text("\(n).").monospacedDigit().foregroundStyle(.secondary)
                            case .task(let done):
                                Image(systemName: done ? "checkmark.square.fill" : "square")
                                    .foregroundStyle(done ? Theme.accent : .secondary)
                            }
                        }
                        .frame(minWidth: 14, alignment: .trailing)
                        inline(item.text).fixedSize(horizontal: false, vertical: true)
                    }
                    .padding(.leading, CGFloat(item.indent) * 16)
                }
            }
        case .quote(let s):
            HStack(spacing: 10) {
                RoundedRectangle(cornerRadius: 1).fill(Theme.accent.opacity(0.5)).frame(width: 3)
                inline(s).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
            }
        case .code(let lang, let s):
            VStack(alignment: .leading, spacing: 4) {
                if !lang.isEmpty { Text(lang).font(.caption2.monospaced()).foregroundStyle(.tertiary) }
                ScrollView(.horizontal, showsIndicators: false) {
                    Text(s).font(Theme.monoSmall).fixedSize()
                }
            }
            .padding(10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Theme.subtleFill, in: RoundedRectangle(cornerRadius: 6))
        case .rule:
            Divider().padding(.vertical, 4)
        }
    }

    private func inline(_ s: String) -> Text {
        let options = AttributedString.MarkdownParsingOptions(interpretedSyntax: .inlineOnlyPreservingWhitespace)
        if let a = try? AttributedString(markdown: s, options: options) { return Text(a) }
        return Text(s)
    }
}

enum MarkdownBlock: Equatable {
    enum Marker: Equatable { case bullet, number(Int), task(Bool) }
    struct Item: Equatable { var indent: Int; var marker: Marker; var text: String }

    case heading(Int, String)
    case paragraph(String)
    case superseded(String)
    case list([Item])
    case quote(String)
    case code(String, String)
    case rule

    static func parse(_ text: String) -> [MarkdownBlock] {
        var blocks: [MarkdownBlock] = []
        var paragraph: [String] = []
        var items: [Item] = []
        var quote: [String] = []
        var code: (lang: String, lines: [String])?

        func flush() {
            if !paragraph.isEmpty {
                let p = paragraph.joined(separator: " ")
                if p.uppercased().hasPrefix("[SUPERSEDED]") {
                    blocks.append(.superseded(String(p.dropFirst(12)).trimmingCharacters(in: .whitespaces)))
                } else {
                    blocks.append(.paragraph(p))
                }
                paragraph = []
            }
            if !items.isEmpty { blocks.append(.list(items)); items = [] }
            if !quote.isEmpty { blocks.append(.quote(quote.joined(separator: " "))); quote = [] }
        }

        for raw in text.components(separatedBy: "\n") {
            if var c = code {
                if raw.trimmingCharacters(in: .whitespaces).hasPrefix("```") {
                    blocks.append(.code(c.lang, c.lines.joined(separator: "\n")))
                    code = nil
                } else {
                    c.lines.append(raw)
                    code = c
                }
                continue
            }
            let line = raw.trimmingCharacters(in: .whitespaces)
            let indent = raw.prefix { $0 == " " || $0 == "\t" }.count / 2

            if line.hasPrefix("```") {
                flush()
                code = (String(line.dropFirst(3)).trimmingCharacters(in: .whitespaces), [])
            } else if line.isEmpty {
                flush()
            } else if let level = headingLevel(line) {
                flush()
                blocks.append(.heading(level, String(line.dropFirst(level)).trimmingCharacters(in: .whitespaces)))
            } else if line == "---" || line == "***" || line == "___" {
                flush()
                blocks.append(.rule)
            } else if line.hasPrefix(">") {
                if !paragraph.isEmpty || !items.isEmpty { flush() }
                quote.append(String(line.dropFirst()).trimmingCharacters(in: .whitespaces))
            } else if let item = listItem(line, indent: indent) {
                if !paragraph.isEmpty || !quote.isEmpty { flush() }
                items.append(item)
            } else if !items.isEmpty, indent > 0 {
                items[items.count - 1].text += " " + line
            } else {
                if !items.isEmpty || !quote.isEmpty { flush() }
                paragraph.append(line)
            }
        }
        if let c = code { blocks.append(.code(c.lang, c.lines.joined(separator: "\n"))) }
        flush()
        return blocks
    }

    private static func headingLevel(_ line: String) -> Int? {
        let hashes = line.prefix { $0 == "#" }.count
        guard (1...6).contains(hashes), line.dropFirst(hashes).first == " " else { return nil }
        return hashes
    }

    private static func listItem(_ line: String, indent: Int) -> Item? {
        for bullet in ["- ", "* ", "+ "] where line.hasPrefix(bullet) {
            let rest = String(line.dropFirst(2))
            if rest.hasPrefix("[ ] ") { return Item(indent: indent, marker: .task(false), text: String(rest.dropFirst(4))) }
            if rest.lowercased().hasPrefix("[x] ") { return Item(indent: indent, marker: .task(true), text: String(rest.dropFirst(4))) }
            return Item(indent: indent, marker: .bullet, text: rest)
        }
        let digits = line.prefix { $0.isNumber }
        if !digits.isEmpty, digits.count <= 3, line.dropFirst(digits.count).hasPrefix(". "), let n = Int(digits) {
            return Item(indent: indent, marker: .number(n), text: String(line.dropFirst(digits.count + 2)))
        }
        return nil
    }
}
