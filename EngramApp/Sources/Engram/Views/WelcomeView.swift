import SwiftUI

struct WelcomeView: View {
    @Environment(\.openWindow) private var openWindow
    @Environment(\.dismissWindow) private var dismissWindow
    @State private var recents: [URL] = []

    private static let logo: NSImage = Bundle.main.url(forResource: "Logo", withExtension: "png")
        .flatMap(NSImage.init(contentsOf:)) ?? NSApplication.shared.applicationIconImage

    var body: some View {
        HStack(spacing: 0) {
            VStack(alignment: .leading, spacing: 22) {
                VStack(alignment: .leading, spacing: 6) {
                    Image(nsImage: Self.logo)
                        .resizable()
                        .interpolation(.high)
                        .frame(width: 64, height: 64)
                        .accessibilityHidden(true)
                    Text("Engram").font(.largeTitle.weight(.bold))
                    Text("Project memory for you and your AI agents, versioned with your code.")
                        .foregroundStyle(.secondary)
                }
                VStack(alignment: .leading, spacing: 8) {
                    WelcomeAction(symbol: "plus.square", title: "New Engram…", shortcut: "⌘N") {
                        OpenRouter.shared.presentNewPanel()
                    }
                    WelcomeAction(symbol: "folder", title: "Open…", shortcut: "⌘O") {
                        OpenRouter.shared.presentOpenPanel()
                    }
                }
                Spacer()
                Text("engram-core \(EngramEngine.coreVersion)")
                    .font(.caption.monospaced())
                    .foregroundStyle(.tertiary)
            }
            .padding(32)
            .frame(width: 330, alignment: .leading)

            Divider()

            VStack(alignment: .leading, spacing: 8) {
                SectionLabel("Recent")
                    .padding(.horizontal, 12)
                if recents.isEmpty {
                    Text("Projects you open appear here.")
                        .font(.callout)
                        .foregroundStyle(.tertiary)
                        .padding(12)
                    Spacer()
                } else {
                    ScrollView {
                        VStack(spacing: 2) {
                            ForEach(recents, id: \.self) { url in RecentRow(url: url) }
                        }
                    }
                }
            }
            .padding(.vertical, 28)
            .padding(.horizontal, 12)
            .frame(width: 330)
            .background(Theme.subtleFill)
        }
        .frame(height: 400)
        .onAppear {
            OpenRouter.shared.register(open: openWindow, dismiss: dismissWindow)
            recents = NSDocumentController.shared.recentDocumentURLs.filter { FileManager.default.fileExists(atPath: $0.path) }
        }
    }
}

private struct WelcomeAction: View {
    var symbol: String
    var title: String
    var shortcut: String
    var action: () -> Void
    @State private var hover = false

    var body: some View {
        Button(action: action) {
            HStack {
                Image(systemName: symbol).frame(width: 20).foregroundStyle(Theme.accent)
                Text(title)
                Spacer()
                Text(shortcut).font(.caption).foregroundStyle(.tertiary)
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 7)
            .background(hover ? Theme.hoverFill : .clear, in: RoundedRectangle(cornerRadius: 7))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hover = $0 }
    }
}

private struct RecentRow: View {
    var url: URL
    @State private var hover = false

    var body: some View {
        Button { OpenRouter.shared.open(url) } label: {
            HStack(spacing: 10) {
                Image(systemName: "doc.text").foregroundStyle(.secondary)
                VStack(alignment: .leading, spacing: 1) {
                    Text(url.deletingPathExtension().lastPathComponent).lineLimit(1)
                    Text(url.deletingLastPathComponent().path.replacingOccurrences(of: NSHomeDirectory(), with: "~"))
                        .font(.caption)
                        .foregroundStyle(.tertiary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
                Spacer()
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            .background(hover ? Theme.hoverFill : .clear, in: RoundedRectangle(cornerRadius: 7))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hover = $0 }
    }
}
