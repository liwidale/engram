import SwiftUI

enum Theme {
    static let accent = Color(red: 0.38, green: 0.40, blue: 0.95)
    static let radius: CGFloat = 8
    static let cardRadius: CGFloat = 10
    static let mono = Font.system(.body, design: .monospaced)
    static let monoSmall = Font.system(.callout, design: .monospaced)

    static let surface = Color(nsColor: .textBackgroundColor)
    static let canvas = Color(nsColor: .windowBackgroundColor)
    static let hairline = Color.primary.opacity(0.08)
    static let subtleFill = Color.primary.opacity(0.045)
    static let hoverFill = Color.primary.opacity(0.07)

    static func color(for c: MemoryCategory) -> Color {
        switch c {
        case .invariant: Color(red: 0.62, green: 0.38, blue: 0.95)
        case .rule: Color(red: 0.93, green: 0.42, blue: 0.33)
        case .techStack: Color(red: 0.25, green: 0.58, blue: 0.96)
        case .convention: Color(red: 0.16, green: 0.70, blue: 0.62)
        case .note: Color.secondary
        }
    }

    static func color(for k: NodeKind) -> Color {
        switch k {
        case .module: accent
        case .service: Color(red: 0.25, green: 0.58, blue: 0.96)
        case .database: Color(red: 0.93, green: 0.60, blue: 0.20)
        case .library: Color(red: 0.16, green: 0.70, blue: 0.62)
        case .queue: Color(red: 0.85, green: 0.38, blue: 0.62)
        case .ui: Color(red: 0.62, green: 0.38, blue: 0.95)
        case .external: Color.gray
        }
    }

    static func color(for s: HealthIssue.Severity) -> Color {
        switch s {
        case .error: Color(red: 0.92, green: 0.30, blue: 0.30)
        case .warning: Color(red: 0.95, green: 0.62, blue: 0.15)
        case .info: Color(red: 0.25, green: 0.58, blue: 0.96)
        }
    }

    static let success = Color(red: 0.20, green: 0.72, blue: 0.45)
    static let superseded = Color(red: 0.95, green: 0.62, blue: 0.15)
    static let neutral = Color.secondary

    static func scoreColor(_ score: Int) -> Color {
        score >= 85 ? Color(red: 0.20, green: 0.72, blue: 0.45) : score >= 60 ? color(for: .warning) : color(for: .error)
    }
}
