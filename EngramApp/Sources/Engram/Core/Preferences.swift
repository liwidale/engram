import AppKit
import SwiftUI

enum Pref {
    static let language = "pref.language"
    static let startup = "pref.startup"
    static let autosave = "pref.autosave"
    static let autosaveDelay = "pref.autosaveDelay"
    static let confirmDelete = "pref.confirmDelete"
    static let externalNotifications = "pref.externalNotifications"
    static let conflictPrompt = "pref.conflictPrompt"
    static let theme = "pref.theme"
    static let monoFont = "pref.monoFont"
    static let monoSize = "pref.monoSize"
    static let softWrap = "pref.softWrap"
    static let memoryMode = "pref.memoryMode"
    static let mapGrid = "pref.mapGrid"
    static let mapPaths = "pref.mapPaths"
    static let mapEdges = "pref.mapEdges"
    static let mcpBinary = "pref.mcpBinary"
    static let mcpClient = "pref.mcpClient"
    static let handoffAuthor = "pref.handoffAuthor"
    static let contextBudget = "pref.contextBudget"
    static let contextHandoff = "pref.contextHandoff"
    static let contextFormat = "pref.contextFormat"
    static let session = "pref.session"

    static let defaults: [String: Any] = [
        language: "system",
        startup: "welcome",
        autosave: true,
        autosaveDelay: 900.0,
        confirmDelete: true,
        externalNotifications: true,
        conflictPrompt: true,
        theme: "system",
        monoFont: "",
        monoSize: 13.0,
        softWrap: true,
        memoryMode: "edit",
        mapGrid: true,
        mapPaths: true,
        mapEdges: "curved",
        mcpBinary: "",
        mcpClient: MCPClient.claudeCode.rawValue,
        handoffAuthor: "human",
        contextBudget: 4000,
        contextHandoff: true,
        contextFormat: "xml",
    ]

    static func register() {
        UserDefaults.standard.register(defaults: defaults)
    }

    private static var store: UserDefaults { .standard }
    static var autosaveEnabled: Bool { store.bool(forKey: autosave) }
    static var autosaveInterval: Duration { .milliseconds(Int(store.double(forKey: autosaveDelay))) }
    static var confirmsDeletion: Bool { store.bool(forKey: confirmDelete) }
    static var notifiesExternalEdits: Bool { store.bool(forKey: externalNotifications) }
    static var promptsOnConflict: Bool { store.bool(forKey: conflictPrompt) }
    static var handoffAuthorLabel: String {
        let label = store.string(forKey: handoffAuthor)?.trimmingCharacters(in: .whitespaces) ?? ""
        return label.isEmpty ? "human" : label
    }
    static var defaultClient: MCPClient { MCPClient(rawValue: store.string(forKey: mcpClient) ?? "") ?? .claudeCode }
    static var mcpBinaryOverride: String { store.string(forKey: mcpBinary)?.trimmingCharacters(in: .whitespaces) ?? "" }

    static var sessionURLs: [URL] {
        get { (store.stringArray(forKey: session) ?? []).map { URL(fileURLWithPath: $0) } }
        set { store.set(newValue.map(\.path), forKey: session) }
    }

    static func monospacedFont(size: CGFloat? = nil) -> NSFont {
        let size = size ?? CGFloat(store.double(forKey: monoSize))
        let name = store.string(forKey: monoFont) ?? ""
        if !name.isEmpty, let font = NSFont(name: name, size: size) { return font }
        return .monospacedSystemFont(ofSize: size, weight: .regular)
    }

    static var monospacedFamilies: [String] {
        NSFontManager.shared.availableFontFamilies.filter { family in
            guard let font = NSFont(name: family, size: 12) else { return false }
            return font.isFixedPitch || family.localizedCaseInsensitiveContains("mono")
        }
    }

    static func applyTheme(_ value: String? = nil) {
        switch value ?? store.string(forKey: theme) {
        case "light": NSApp.appearance = NSAppearance(named: .aqua)
        case "dark": NSApp.appearance = NSAppearance(named: .darkAqua)
        default: NSApp.appearance = nil
        }
    }

    static func applyLanguage(_ value: String) {
        switch value {
        case "en", "ru": store.set([value], forKey: "AppleLanguages")
        default: store.removeObject(forKey: "AppleLanguages")
        }
    }

}
