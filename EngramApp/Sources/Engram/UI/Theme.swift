import SwiftUI

enum Theme {
    static let accent = Color(red: 0.659, green: 0.471, blue: 0.333)
    static let radius: CGFloat = 8
    static let cardRadius: CGFloat = 10
    static let mono = Font.system(.body, design: .monospaced)
    static let monoSmall = Font.system(.callout, design: .monospaced)

    static let surface = Color(nsColor: .textBackgroundColor)
    static let canvas = Color(nsColor: .windowBackgroundColor)
    static let hairline = Color.primary.opacity(0.08)
    static let subtleFill = Color.primary.opacity(0.045)
    static let hoverFill = Color.primary.opacity(0.07)

    static let invariant = Color(red: 0.59, green: 0.44, blue: 0.77)
    static let pinned = accent
    static let success = Color(red: 0.33, green: 0.66, blue: 0.44)
    static let warning = Color(red: 0.84, green: 0.60, blue: 0.22)
    static let error = Color(red: 0.84, green: 0.33, blue: 0.30)
    static let superseded = Color.gray
    static let neutral = Color.secondary

    private static let slate = Color(red: 0.44, green: 0.52, blue: 0.59)
    private static let sand = Color(red: 0.59, green: 0.52, blue: 0.44)
    private static let sage = Color(red: 0.44, green: 0.57, blue: 0.53)
    private static let mauve = Color(red: 0.57, green: 0.46, blue: 0.54)
    private static let heather = Color(red: 0.52, green: 0.48, blue: 0.61)

    static func color(for c: MemoryCategory) -> Color {
        switch c {
        case .invariant: invariant
        case .rule: sand
        case .techStack: slate
        case .convention: sage
        case .note: Color.secondary
        }
    }

    static func color(for k: NodeKind) -> Color {
        switch k {
        case .module: accent
        case .service: slate
        case .database: sand
        case .library: sage
        case .queue: mauve
        case .ui: heather
        case .external: Color.gray
        }
    }

    static func color(for s: HealthIssue.Severity) -> Color {
        switch s {
        case .error: error
        case .warning: warning
        case .info: neutral
        }
    }

    static func scoreColor(_ score: Int) -> Color {
        score >= 85 ? success : score >= 60 ? warning : error
    }
}
