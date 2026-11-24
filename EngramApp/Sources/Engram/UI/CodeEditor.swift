import AppKit
import SwiftUI

enum EditorFont {
    case mono, monoSmall, body, callout

    @MainActor
    func nsFont(monoSize: Double) -> NSFont {
        switch self {
        case .mono: Pref.monospacedFont(size: monoSize)
        case .monoSmall: Pref.monospacedFont(size: max(monoSize - 1, 10))
        case .body: .systemFont(ofSize: NSFont.systemFontSize)
        case .callout: .systemFont(ofSize: NSFont.systemFontSize - 1)
        }
    }

    var isMono: Bool { self == .mono || self == .monoSmall }
}

struct CodeEditor: View {
    @Binding var text: String
    var placeholder: String.LocalizationValue = ""
    var minHeight: CGFloat = 120
    var font: EditorFont = .mono
    @AppStorage(Pref.monoSize) private var monoSize = 13.0
    @AppStorage(Pref.monoFont) private var monoFamily = ""
    @AppStorage(Pref.softWrap) private var softWrap = true

    var body: some View {
        let nsFont = font.nsFont(monoSize: monoSize)
        ZStack(alignment: .topLeading) {
            TextViewRepresentable(text: $text, font: nsFont, wraps: softWrap || !font.isMono)
            if text.isEmpty {
                Text(String(localized: placeholder))
                    .font(Font(nsFont))
                    .foregroundStyle(.tertiary)
                    .padding(.horizontal, 5)
                    .padding(.top, 1)
                    .allowsHitTesting(false)
            }
        }
        .frame(minHeight: minHeight)
        .id(monoFamily)
    }
}

private struct TextViewRepresentable: NSViewRepresentable {
    @Binding var text: String
    var font: NSFont
    var wraps: Bool

    func makeCoordinator() -> Coordinator { Coordinator(text: $text) }

    func makeNSView(context: Context) -> NSScrollView {
        let scroll = NSTextView.scrollableTextView()
        scroll.drawsBackground = false
        scroll.hasHorizontalScroller = !wraps
        guard let view = scroll.documentView as? NSTextView else { return scroll }
        view.delegate = context.coordinator
        view.drawsBackground = false
        view.isRichText = false
        view.allowsUndo = true
        view.isAutomaticQuoteSubstitutionEnabled = false
        view.isAutomaticDashSubstitutionEnabled = false
        view.isAutomaticTextReplacementEnabled = false
        view.isAutomaticSpellingCorrectionEnabled = false
        view.textContainerInset = NSSize(width: 0, height: 2)
        view.textContainer?.lineFragmentPadding = 5
        view.string = text
        apply(view, scroll: scroll)
        return scroll
    }

    func updateNSView(_ scroll: NSScrollView, context: Context) {
        guard let view = scroll.documentView as? NSTextView else { return }
        context.coordinator.text = $text
        if view.string != text {
            let selection = view.selectedRanges
            view.string = text
            view.selectedRanges = selection.filter { $0.rangeValue.upperBound <= (text as NSString).length }
        }
        apply(view, scroll: scroll)
    }

    private func apply(_ view: NSTextView, scroll: NSScrollView) {
        if view.font != font { view.font = font }
        guard let container = view.textContainer else { return }
        if wraps {
            container.widthTracksTextView = true
            view.isHorizontallyResizable = false
            view.maxSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: .greatestFiniteMagnitude)
        } else {
            container.widthTracksTextView = false
            container.containerSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: .greatestFiniteMagnitude)
            view.isHorizontallyResizable = true
            view.maxSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: .greatestFiniteMagnitude)
        }
        scroll.hasHorizontalScroller = !wraps
    }

    final class Coordinator: NSObject, NSTextViewDelegate {
        var text: Binding<String>
        init(text: Binding<String>) { self.text = text }

        func textDidChange(_ notification: Notification) {
            guard let view = notification.object as? NSTextView else { return }
            text.wrappedValue = view.string
        }
    }
}
