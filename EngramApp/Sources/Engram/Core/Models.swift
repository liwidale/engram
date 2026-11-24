import Foundation

@propertyWrapper
struct Lines: Decodable, Hashable {
    var wrappedValue: String

    init(wrappedValue: String = "") { self.wrappedValue = wrappedValue }

    init(from decoder: Decoder) throws {
        let c = try decoder.singleValueContainer()
        if c.decodeNil() {
            wrappedValue = ""
        } else if let s = try? c.decode(String.self) {
            wrappedValue = s
        } else {
            wrappedValue = try c.decode([String].self).joined(separator: "\n")
        }
    }
}

protocol DefaultProvider {
    associatedtype Value: Decodable & Hashable
    static var value: Value { get }
}

@propertyWrapper
struct Default<P: DefaultProvider>: Decodable, Hashable {
    var wrappedValue: P.Value
    init() { wrappedValue = P.value }
    init(wrappedValue: P.Value) { self.wrappedValue = wrappedValue }
    init(from decoder: Decoder) throws {
        wrappedValue = try decoder.singleValueContainer().decode(P.Value.self)
    }
}

enum EmptyStrings: DefaultProvider { static var value: [String] { [] } }
enum False: DefaultProvider { static var value: Bool { false } }
enum True: DefaultProvider { static var value: Bool { true } }
enum EmptyString: DefaultProvider { static var value: String { "" } }

extension KeyedDecodingContainer {
    func decode<P>(_ type: Default<P>.Type, forKey key: Key) throws -> Default<P> {
        try decodeIfPresent(type, forKey: key) ?? Default()
    }
    func decode(_ type: Lines.Type, forKey key: Key) throws -> Lines {
        try decodeIfPresent(type, forKey: key) ?? Lines()
    }
}

extension JSONDecoder {
    static let engram: JSONDecoder = {
        let d = JSONDecoder()
        d.keyDecodingStrategy = .convertFromSnakeCase
        return d
    }()
}

struct EngramDocument: Decodable, Hashable {
    var format: String
    var version: Int
    var revision: UInt64
    var project: Project
    var memory: [MemoryEntry]
    var architecture: Architecture
    var decisions: [Decision]
    var prompts: [Prompt]
    var handoff: Handoff

    func memoryEntry(_ key: String) -> MemoryEntry? { memory.first { $0.key == key } }
    func decision(_ id: String) -> Decision? { decisions.first { $0.id.caseInsensitiveCompare(id) == .orderedSame } }
    func node(_ id: String) -> ArchNode? { architecture.nodes.first { $0.id == id } }
    func prompt(_ name: String) -> Prompt? { prompts.first { $0.name == name } }
}

struct Project: Decodable, Hashable {
    var name: String
    @Lines var description: String
    var repoRoot: String?
    var createdAt: String
    var updatedAt: String
}

enum MemoryCategory: String, Decodable, CaseIterable, Identifiable, Hashable {
    case invariant, rule, techStack = "tech_stack", convention, note
    var id: String { rawValue }

    var title: String {
        switch self {
        case .invariant: String(localized: "Invariants")
        case .rule: String(localized: "Rules")
        case .techStack: String(localized: "Tech stack")
        case .convention: String(localized: "Conventions")
        case .note: String(localized: "Notes")
        }
    }
    var singular: String {
        switch self {
        case .invariant: String(localized: "Invariant")
        case .rule: String(localized: "Rule")
        case .techStack: String(localized: "Tech stack")
        case .convention: String(localized: "Convention")
        case .note: String(localized: "Note")
        }
    }
    var symbol: String {
        switch self {
        case .invariant: "lock.shield"
        case .rule: "checkmark.seal"
        case .techStack: "shippingbox"
        case .convention: "textformat"
        case .note: "note.text"
        }
    }
}

enum EntryStatus: String, Decodable, Hashable { case active, superseded }

struct MemoryEntry: Decodable, Hashable, Identifiable {
    var id: String { key }
    var key: String
    var category: MemoryCategory
    var status: EntryStatus
    @Default<False> var pinned: Bool
    @Lines var content: String
    @Default<EmptyStrings> var tags: [String]
    @Default<EmptyStrings> var files: [String]
    var supersedes: String?
    var supersededBy: String?
    var createdAt: String
    var updatedAt: String

    var isActive: Bool { status == .active }

    var summary: String {
        let body = content.replacingOccurrences(of: "[SUPERSEDED]", with: "", options: .caseInsensitive)
        return body.split(separator: "\n").map { $0.trimmingCharacters(in: CharacterSet(charactersIn: "#-*> \t")) }
            .first { !$0.isEmpty } ?? String(localized: "Empty")
    }
}

enum NodeKind: String, Decodable, CaseIterable, Identifiable, Hashable {
    case module, service, database, library, queue, ui, external
    var id: String { rawValue }
    var title: String {
        switch self {
        case .module: String(localized: "Module")
        case .service: String(localized: "Service")
        case .database: String(localized: "Database")
        case .library: String(localized: "Library")
        case .queue: String(localized: "Queue")
        case .ui: String(localized: "UI")
        case .external: String(localized: "External")
        }
    }
    var symbol: String {
        switch self {
        case .module: "square.stack.3d.up"
        case .service: "server.rack"
        case .database: "cylinder.split.1x2"
        case .library: "books.vertical"
        case .queue: "tray.full"
        case .ui: "macwindow"
        case .external: "globe"
        }
    }
}

struct Point: Decodable, Hashable { var x: Double; var y: Double }

struct ArchNode: Decodable, Hashable, Identifiable {
    var id: String
    var kind: NodeKind
    var label: String?
    @Lines var description: String
    var path: String?
    @Default<EmptyStrings> var dependsOn: [String]
    var position: Point?

    var displayName: String { (label?.isEmpty == false ? label : nil) ?? id }

    enum CodingKeys: String, CodingKey {
        case id, kind = "type", label, description, path, dependsOn, position
    }
}

struct Architecture: Decodable, Hashable { var nodes: [ArchNode] }

enum DecisionStatus: String, Decodable, CaseIterable, Hashable {
    case active, deprecated
    var title: String {
        switch self {
        case .active: String(localized: "Active")
        case .deprecated: String(localized: "Deprecated")
        }
    }
}

struct RejectedAlternative: Decodable, Hashable {
    var option: String
    var reason: String
}

struct Decision: Decodable, Hashable, Identifiable {
    var id: String
    var title: String
    var status: DecisionStatus
    @Lines var context: String
    @Lines var decision: String
    var rejectedAlternatives: [RejectedAlternative]
    @Lines var consequences: String
    @Default<EmptyStrings> var related: [String]
    @Default<EmptyStrings> var files: [String]
    var supersededBy: String?
    var createdAt: String
    var updatedAt: String

    var isActive: Bool { status == .active }
}

struct Prompt: Decodable, Hashable, Identifiable {
    var id: String { name }
    var name: String
    @Default<EmptyString> var category: String
    @Default<EmptyString> var description: String
    @Lines var body: String
    var createdAt: String
    var updatedAt: String

    var variables: [String] { Prompt.variables(in: body) }

    static func variables(in body: String) -> [String] {
        var seen = Set<String>(), out: [String] = []
        var rest = body[...]
        while let open = rest.range(of: "{{"), let close = rest[open.upperBound...].range(of: "}}") {
            let name = rest[open.upperBound..<close.lowerBound].trimmingCharacters(in: .whitespaces)
            if !name.isEmpty, name.allSatisfy({ $0.isLetter || $0.isNumber || "_-.".contains($0) }), seen.insert(name).inserted {
                out.append(name)
            }
            rest = rest[close.upperBound...]
        }
        return out
    }
}

struct ProgressItem: Decodable, Hashable {
    var description: String
    @Default<EmptyStrings> var files: [String]
    @Default<True> var verified: Bool

    init(description: String, files: [String], verified: Bool) {
        self.description = description
        self.files = files
        self.verified = verified
    }
}

struct WorkingMemory: Decodable, Hashable {
    @Default<EmptyStrings> var hypotheses: [String]
    @Default<EmptyStrings> var blockers: [String]
    @Lines var notes: String
}

struct Handoff: Decodable, Hashable {
    @Default<EmptyString> var objective: String
    var progress: [ProgressItem]
    var workingMemory: WorkingMemory
    @Default<EmptyString> var nextStep: String
    var updatedAt: String?
    var updatedBy: String?

    var isEmpty: Bool {
        objective.isEmpty && progress.isEmpty && nextStep.isEmpty
            && workingMemory.hypotheses.isEmpty && workingMemory.blockers.isEmpty && workingMemory.notes.isEmpty
    }
}

struct DocumentEnvelope: Decodable {
    var path: String
    var revision: UInt64
    var document: EngramDocument
}

struct ContextItem: Decodable, Hashable, Identifiable {
    enum Kind: String, Decodable { case memory, decision, node }
    enum Reason: String, Decodable { case pinned, selected, ranked, overBudget = "over_budget", belowThreshold = "below_threshold", excluded }
    var id: String { "\(kind.rawValue):\(itemId)" }
    var kind: Kind
    var itemId: String
    var title: String
    var score: Double
    var tokens: Int
    var included: Bool
    var reason: Reason
    var matched: [String]

    enum CodingKeys: String, CodingKey { case kind, itemId = "id", title, score, tokens, included, reason, matched }
}

struct PromptUsage: Decodable, Hashable {
    var name: String
    var missing: [String]
    var found: Bool
}

struct ContextBundle: Decodable, Hashable {
    var output: String
    var format: String
    var tokens: Int
    var maxTokens: Int
    var overBudget: Bool
    var tokenizer: String
    var items: [ContextItem]
    var deadEnds: Int
    var prompt: PromptUsage?
}

struct EntityRef: Decodable, Hashable {
    var kind: String
    var id: String
}

struct HealthIssue: Decodable, Hashable, Identifiable {
    enum Severity: String, Decodable, Comparable {
        case error, warning, info
        static func < (a: Severity, b: Severity) -> Bool { a.rank < b.rank }
        var rank: Int { switch self { case .error: 0; case .warning: 1; case .info: 2 } }
    }
    var id: String
    var severity: Severity
    var kind: String
    var message: String
    var subject: EntityRef
    var related: EntityRef?
    var path: String?
    var suggestion: String?

    var kindTitle: String {
        switch kind {
        case "missing_repo": String(localized: "Missing repository")
        case "broken_path": String(localized: "Broken path")
        case "zombie_reference": String(localized: "Zombie reference")
        case "deprecated_reference": String(localized: "Deprecated reference")
        case "deprecated_decision": String(localized: "Deprecated decision")
        case "contradiction": String(localized: "Contradiction")
        case "rejected_alternative_endorsed": String(localized: "Rejected alternative endorsed")
        case "superseded_tag_mismatch": String(localized: "Superseded tag mismatch")
        case "dependency_cycle": String(localized: "Dependency cycle")
        case "stale_handoff": String(localized: "Stale handoff")
        case "empty_content": String(localized: "Empty content")
        default: kind
        }
    }
}

struct HealthSummary: Decodable, Hashable {
    var errors: Int
    var warnings: Int
    var infos: Int
    var pathsChecked: Int
    var referencesChecked: Int
}

struct HealthReport: Decodable, Hashable {
    var score: Int
    var grade: String
    var repoRoot: String?
    var repoFound: Bool
    var scannedAt: String
    var revision: UInt64
    var summary: HealthSummary
    var issues: [HealthIssue]
}

struct PlacedNode: Decodable, Hashable, Identifiable {
    var id: String
    var label: String
    var kind: NodeKind
    var layer: Int
    var position: Point
    var manual: Bool
    enum CodingKeys: String, CodingKey { case id, label, kind = "type", layer, position, manual }
}

struct LayoutEdge: Decodable, Hashable {
    var from: String
    var to: String
    var inCycle: Bool
}

struct MissingEdge: Decodable, Hashable {
    var from: String
    var to: String
}

struct GraphLayout: Decodable, Hashable {
    var nodes: [PlacedNode]
    var edges: [LayoutEdge]
    var missing: [MissingEdge]
    var cycles: [[String]]
    var width: Double
    var height: Double

    static let empty = GraphLayout(nodes: [], edges: [], missing: [], cycles: [], width: 0, height: 0)
}

struct RenderedPrompt: Decodable, Hashable {
    var text: String
    var missing: [String]
    var variables: [String]
}

struct MCPConfig: Decodable, Hashable {
    var serverName: String
    var json: String
    var claudeCodeCommand: String
}

struct ReloadResult: Decodable { var changed: Bool; var revision: UInt64 }

enum EngramDate {
    private static let parser: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime]
        return f
    }()
    private static let relative: RelativeDateTimeFormatter = {
        let f = RelativeDateTimeFormatter()
        f.unitsStyle = .short
        return f
    }()

    static func date(_ s: String?) -> Date? { s.flatMap { parser.date(from: $0) } }

    static func relativeString(_ s: String?) -> String {
        guard let d = date(s) else { return String(localized: "never") }
        if abs(d.timeIntervalSinceNow) < 45 { return String(localized: "a moment ago") }
        return relative.localizedString(for: d, relativeTo: .now)
    }
}
