import Foundation
import CEngramCore

/// An error reported by engram-core (or by decoding its response).
struct EngramError: LocalizedError, Decodable, Equatable {
    var code: String
    var message: String
    var errorDescription: String? { message }
}

/// Swift wrapper around an `EngramHandle` from libengram_core.
///
/// All calls are thread-safe: the Rust side guards the cached snapshot with a mutex and
/// every write is a locked, atomic read-modify-write of the `.engram` file.
final class EngramEngine: @unchecked Sendable {
    let url: URL
    private let handle: OpaquePointer

    private init(url: URL, handle: OpaquePointer) {
        self.url = url
        self.handle = handle
    }

    deinit { engram_close(handle) }

    static func open(_ url: URL) throws -> EngramEngine {
        try make(url) { path, err in engram_open(path, err) }
    }

    static func create(_ url: URL, projectName: String) throws -> EngramEngine {
        try make(url) { path, err in projectName.withCString { engram_create(path, $0, err) } }
    }

    private static func make(
        _ url: URL,
        _ body: (UnsafePointer<CChar>, UnsafeMutablePointer<UnsafeMutablePointer<CChar>?>) -> OpaquePointer?
    ) throws -> EngramEngine {
        var err: UnsafeMutablePointer<CChar>?
        let handle = url.path.withCString { body($0, &err) }
        if let handle { return EngramEngine(url: url, handle: handle) }
        if let err {
            defer { engram_string_free(err) }
            if let env = try? JSONDecoder.engram.decode(Envelope<Empty>.self, from: Data(String(cString: err).utf8)), let e = env.error {
                throw e
            }
        }
        throw EngramError(code: "internal", message: "engram-core failed to open \(url.lastPathComponent)")
    }

    // MARK: Calls

    func document() throws -> DocumentEnvelope { try call { engram_document(handle) } }

    func reload() throws -> ReloadResult { try call { engram_reload(handle) } }

    struct ApplyResult: Decodable {
        var revision: UInt64
        var result: JSONValue
    }

    @discardableResult
    func apply(_ op: EngramOp) throws -> ApplyResult {
        let json = try op.json()
        return try call { json.withCString { engram_apply(handle, $0) } }
    }

    func buildContext(_ request: ContextRequest) throws -> ContextBundle {
        let json = try request.json()
        return try call { json.withCString { engram_build_context(handle, $0) } }
    }

    func audit(repoRoot: String?) throws -> HealthReport {
        try call {
            if let repoRoot, !repoRoot.isEmpty {
                return repoRoot.withCString { engram_audit(handle, $0) }
            }
            return engram_audit(handle, nil)
        }
    }

    func layout() throws -> GraphLayout { try call { engram_layout(handle) } }

    enum RenderTarget: String { case memory, decisions, handoff, handoffXML = "handoff_xml", architecture, prompts }

    func render(_ target: RenderTarget) throws -> String {
        try call { target.rawValue.withCString { engram_render(handle, $0) } }
    }

    func renderPrompt(_ name: String, variables: [String: String]) throws -> RenderedPrompt {
        let vars = String(decoding: try JSONSerialization.data(withJSONObject: variables), as: UTF8.self)
        return try call { name.withCString { n in vars.withCString { engram_render_prompt(handle, n, $0) } } }
    }

    func mcpConfig(serverBinary: String) throws -> MCPConfig {
        try call { serverBinary.withCString { engram_mcp_config(handle, $0) } }
    }

    static func countTokens(_ text: String) -> Int {
        Int(text.withCString { engram_count_tokens($0) })
    }

    static var coreVersion: String {
        guard let p = engram_version() else { return "?" }
        defer { engram_string_free(p) }
        return String(cString: p)
    }

    // MARK: Envelope decoding

    private struct Empty: Decodable {}

    private struct Envelope<T: Decodable>: Decodable {
        var ok: Bool
        var data: T?
        var error: EngramError?
    }

    private func call<T: Decodable>(_ body: () -> UnsafeMutablePointer<CChar>?) throws -> T {
        guard let ptr = body() else { throw EngramError(code: "internal", message: "engram-core returned no response") }
        defer { engram_string_free(ptr) }
        let data = Data(bytes: ptr, count: strlen(ptr))
        let env: Envelope<T>
        do {
            env = try JSONDecoder.engram.decode(Envelope<T>.self, from: data)
        } catch {
            throw EngramError(code: "decode", message: "Could not decode engram-core response: \(error)")
        }
        if env.ok, let value = env.data { return value }
        throw env.error ?? EngramError(code: "internal", message: "engram-core returned an empty response")
    }
}

/// Request payload for `engram_build_context` (mirrors `engram_core::context::ContextRequest`).
struct ContextRequest: Equatable {
    var task: String = ""
    var maxTokens: Int = 4000
    var includeMemory: [String] = []
    var includeDecisions: [String] = []
    var includeNodes: [String] = []
    var excludeMemory: [String] = []
    var excludeDecisions: [String] = []
    var excludeNodes: [String] = []
    var prompt: String?
    var promptVars: [String: String] = [:]
    var includeHandoff = true
    var autoRank = true

    func json() throws -> String {
        var payload: [String: Any] = [
            "task": task,
            "max_tokens": maxTokens,
            "include": ["memory": includeMemory, "decisions": includeDecisions, "nodes": includeNodes],
            "exclude": ["memory": excludeMemory, "decisions": excludeDecisions, "nodes": excludeNodes],
            "prompt_vars": promptVars,
            "include_handoff": includeHandoff,
            "auto_rank": autoRank,
        ]
        if let prompt { payload["prompt"] = prompt }
        return String(decoding: try JSONSerialization.data(withJSONObject: payload), as: UTF8.self)
    }
}

/// Minimal dynamic JSON value for untyped operation results.
enum JSONValue: Decodable, Hashable {
    case null, bool(Bool), number(Double), string(String), array([JSONValue]), object([String: JSONValue])

    init(from decoder: Decoder) throws {
        let c = try decoder.singleValueContainer()
        if c.decodeNil() { self = .null }
        else if let b = try? c.decode(Bool.self) { self = .bool(b) }
        else if let n = try? c.decode(Double.self) { self = .number(n) }
        else if let s = try? c.decode(String.self) { self = .string(s) }
        else if let a = try? c.decode([JSONValue].self) { self = .array(a) }
        else { self = .object(try c.decode([String: JSONValue].self)) }
    }

    subscript(key: String) -> JSONValue? {
        if case .object(let o) = self { return o[key] }
        return nil
    }

    var string: String? {
        if case .string(let s) = self { return s }
        return nil
    }

    var strings: [String] {
        if case .array(let a) = self { return a.compactMap(\.string) }
        return []
    }
}
