import Foundation

struct ProjectSettings: Decodable, Hashable {
    var staleHandoffDays: Int?
    @Default<EmptyStrings> var healthIgnoreDirs: [String]
    @Default<EmptyStrings> var healthIgnoreIssues: [String]
    var contextMinScore: Double?
    var mcpServerName: String?

    static let defaultStaleHandoffDays = 7
    static let defaultContextMinScore = 0.12

    init() {}
}

enum DefaultSettings: DefaultProvider { static var value: ProjectSettings { ProjectSettings() } }

struct HealthFix: Decodable, Hashable {
    var label: String
    var op: JSONValue
}

struct DocumentChange: Decodable, Hashable, Identifiable {
    enum Kind: String, Decodable { case added, updated, deleted, superseded }
    enum Entity: String, Decodable { case project, memory, node, decision, prompt, handoff }
    var id: String { "\(entity.rawValue):\(entityId)" }
    var kind: Kind
    var entity: Entity
    var entityId: String
    var title: String
    var fields: [String]

    enum CodingKeys: String, CodingKey { case kind, entity, entityId = "id", title, fields }

    var ref: EntityRef? {
        switch entity {
        case .memory: EntityRef(kind: "memory", id: entityId)
        case .node: EntityRef(kind: "node", id: entityId)
        case .decision: EntityRef(kind: "decision", id: entityId)
        case .prompt: EntityRef(kind: "prompt", id: entityId)
        case .handoff: EntityRef(kind: "handoff", id: entityId)
        case .project: nil
        }
    }
}

struct DocumentDiff: Decodable, Hashable {
    var fromRevision: UInt64
    var toRevision: UInt64
    var author: String?
    var changes: [DocumentChange]

    var summary: String {
        let count = { (k: DocumentChange.Kind) in changes.filter { $0.kind == k }.count }
        let parts = [
            (count(.added), String(localized: "added")),
            (count(.updated), String(localized: "updated")),
            (count(.superseded), String(localized: "superseded")),
            (count(.deleted), String(localized: "deleted")),
        ]
        .filter { $0.0 > 0 }
        .map { "\($0.0) \($0.1)" }
        return parts.isEmpty ? String(localized: "No content changes") : parts.joined(separator: ", ")
    }
}

struct HistoryState: Decodable, Hashable {
    var canUndo: Bool
    var canRedo: Bool
    var undoLabel: String?
    var redoLabel: String?

    static let empty = HistoryState(canUndo: false, canRedo: false, undoLabel: nil, redoLabel: nil)
}

struct ApplyResult: Decodable {
    var revision: UInt64
    var result: JSONValue
    var diff: DocumentDiff?
    var history: HistoryState?
}

struct HistoryStepResult: Decodable {
    var revision: UInt64
    var label: String?
    var diff: DocumentDiff?
    var history: HistoryState
}

struct ReplaceResult: Decodable {
    var revision: UInt64
    var history: HistoryState
}

enum MCPClient: String, Decodable, CaseIterable, Identifiable, Hashable {
    case claudeCode = "claude_code", claudeDesktop = "claude_desktop", cursor, windsurf, vsCode = "vs_code", generic
    var id: String { rawValue }

    var title: String {
        switch self {
        case .claudeCode: "Claude Code"
        case .claudeDesktop: "Claude Desktop"
        case .cursor: "Cursor"
        case .windsurf: "Windsurf"
        case .vsCode: "VS Code"
        case .generic: String(localized: "Custom JSON")
        }
    }
}

struct ClientConfig: Decodable, Hashable, Identifiable {
    var id: String { client.rawValue }
    var client: MCPClient
    var title: String
    var serverName: String
    var snippet: String
    var command: String?
    var target: String?
    var projectScoped: Bool
}

struct WriteOutcome: Decodable, Hashable {
    var path: String
    var created: Bool
    var replaced: Bool
}

struct MemoryCandidate: Decodable, Hashable, Identifiable {
    var id: String { key }
    var key: String
    var category: MemoryCategory
    var content: String
    var source: String
    var exists: Bool
}

struct PromptCandidate: Decodable, Hashable, Identifiable {
    var id: String { name }
    var name: String
    var category: String
    var description: String
    var body: String
    var source: String
    var exists: Bool
}

struct Bootstrap: Decodable, Hashable {
    var memory: [MemoryCandidate]
    var prompts: [PromptCandidate]
    var sources: [String]
}

extension JSONValue {
    // The engine decoder camelCases keys, but operations expect snake_case.
    var foundation: Any {
        switch self {
        case .null: NSNull()
        case .bool(let b): b
        case .number(let n): n
        case .string(let s): s
        case .array(let a): a.map(\.foundation)
        case .object(let o): Dictionary(uniqueKeysWithValues: o.map { (Self.snakeCase($0.key), $0.value.foundation) })
        }
    }

    static func snakeCase(_ key: String) -> String {
        key.reduce(into: "") { out, c in
            if c.isUppercase {
                out += "_" + c.lowercased()
            } else {
                out.append(c)
            }
        }
    }
}
