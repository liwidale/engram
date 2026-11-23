import Foundation
import Testing
@testable import Engram

/// Exercises the real Swift ⇄ Rust bridge against real files on disk.
@MainActor
struct BridgeTests {
    let dir: URL

    init() throws {
        dir = FileManager.default.temporaryDirectory.appendingPathComponent("engram-tests-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    }

    private func fresh(_ name: String = "t.engram") throws -> EngramEngine {
        try EngramEngine.create(dir.appendingPathComponent(name), projectName: "Bridge")
    }

    @Test func decodesEveryModuleFromRust() throws {
        let e = try fresh()
        try e.apply(.upsertMemory(key: "old", category: .techStack, content: "MySQL"))
        try e.apply(.upsertMemory(key: "db", category: .techStack, content: "Postgres 16\n\n- pooled via pgbouncer", supersedesKey: "old", tags: ["data"], files: ["src/db.rs"], pinned: true))
        try e.apply(.addNode(id: "api", kind: .service, dependencies: ["pg"], label: "API", description: "line 1\nline 2", path: "src/api"))
        try e.apply(.addNode(id: "pg", kind: .database))
        try e.apply(.setNodePosition(id: "pg", x: 10, y: 20))
        let r = try e.apply(.logDecision(title: nil, context: "ctx\nmore", decision: "Use NATS.", rejected: [RejectedAlternative(option: "Kafka", reason: "heavy")]))
        #expect(r.result["id"]?.string == "ADR-0001")
        try e.apply(.upsertPrompt(name: "fix", body: "Fix {{bug}} in {{file}}", category: "QA", description: "d"))
        try e.apply(.updateHandoff(objective: "Ship", progress: [ProgressItem(description: "did", files: ["a.rs"], verified: false)], hypotheses: ["h"], blockers: [], notes: "n\nm", nextStep: "next", updatedBy: "tests"))

        let doc = try e.document().document
        #expect(doc.revision == 9) // create + 8 operations
        let db = try #require(doc.memoryEntry("db"))
        #expect(db.content == "Postgres 16\n\n- pooled via pgbouncer")
        #expect(db.pinned && db.tags == ["data"] && db.files == ["src/db.rs"] && db.supersedes == "old")
        #expect(doc.memoryEntry("old")?.status == .superseded)
        #expect(doc.memoryEntry("old")?.supersededBy == "db")
        #expect(doc.memoryEntry("old")?.pinned == false)
        let api = try #require(doc.node("api"))
        #expect(api.kind == .service && api.dependsOn == ["pg"] && api.description == "line 1\nline 2" && api.position == nil)
        #expect(doc.node("pg")?.position == Point(x: 10, y: 20))
        let adr = try #require(doc.decision("adr-0001"))
        #expect(adr.context == "ctx\nmore" && adr.rejectedAlternatives.first?.option == "Kafka" && adr.consequences.isEmpty)
        #expect(doc.prompt("fix")?.variables == ["bug", "file"])
        #expect(doc.handoff.progress.first?.verified == false)
        #expect(doc.handoff.workingMemory.notes == "n\nm")
        #expect(doc.handoff.updatedBy == "tests")
    }

    @Test func analysisResultsDecode() throws {
        let e = try fresh()
        try e.apply(.upsertMemory(key: "payments", category: .invariant, content: "Payments need idempotency keys. See `missing/file.rs`."))
        try e.apply(.addNode(id: "a", kind: .module, dependencies: ["b"]))
        try e.apply(.addNode(id: "b", kind: .module, dependencies: ["a"]))
        try e.apply(.upsertPrompt(name: "p", body: "Do {{x}}", category: nil, description: nil))

        var req = ContextRequest()
        req.task = "payments idempotency"
        req.prompt = "p"
        req.promptVars = ["x": "it"]
        let bundle = try e.buildContext(req)
        #expect(bundle.xml.contains("<entry key=\"payments\""))
        #expect(bundle.items.contains { $0.itemId == "payments" && $0.included && $0.reason == .ranked })
        #expect(bundle.prompt?.missing == [])
        #expect(bundle.tokens == EngramEngine.countTokens(bundle.xml))

        let report = try e.audit(repoRoot: dir.path)
        #expect(report.issues.contains { $0.kind == "broken_path" && $0.severity == .warning })
        #expect(report.issues.contains { $0.kind == "dependency_cycle" })

        let layout = try e.layout()
        #expect(layout.cycles == [["a", "b"]])
        let allCyclic = layout.edges.allSatisfy { $0.inCycle }
        #expect(allCyclic)

        #expect(try e.renderPrompt("p", variables: ["x": "that"]).text == "Do that")
        let cfg = try e.mcpConfig(serverBinary: "/Applications/Engram.app/Contents/MacOS/engram-mcp")
        #expect(cfg.json.contains("\"--file\""))
        #expect(cfg.serverName == "engram-bridge")
        #expect(try e.render(.memory).contains("### payments"))
    }

    @Test func errorsSurfaceAsEngramError() throws {
        #expect(throws: EngramError.self) { try EngramEngine.open(dir.appendingPathComponent("nope.engram")) }
        let e = try fresh()
        do {
            try e.apply(.deleteMemory(key: "ghost"))
            Issue.record("expected not_found")
        } catch let err as EngramError {
            #expect(err.code == "not_found")
        }
        #expect(throws: EngramError.self) { try EngramEngine.create(e.url, projectName: "dup") }
    }

    @Test func watcherSeesExternalAtomicWritesRepeatedly() async throws {
        let app = try fresh("watched.engram")
        let agent = try EngramEngine.open(app.url) // separate handle, like engram-mcp
        var events = 0
        let watcher = FileWatcher(url: app.url) { events += 1 }
        watcher.start()
        defer { watcher.stop() }
        try await Task.sleep(for: .milliseconds(150))

        for i in 1...3 {
            let before = events
            try agent.apply(.upsertMemory(key: "k\(i)", category: .note, content: "from agent \(i)"))
            let deadline = Date().addingTimeInterval(3)
            while events == before, Date() < deadline { try await Task.sleep(for: .milliseconds(25)) }
            #expect(events > before, "watcher fired for write \(i) (re-armed after atomic rename)")
            let r = try app.reload()
            #expect(r.changed)
            #expect(try app.document().document.memoryEntry("k\(i)") != nil)
        }
        // The app's own write leaves its snapshot in sync: no spurious reload.
        try app.apply(.upsertMemory(key: "mine", category: .note, content: "x"))
        #expect(try app.reload().changed == false)
    }

    @Test func markdownBlocksParse() {
        let blocks = MarkdownBlock.parse("# Title\n\nPara one\ncontinues.\n\n- [x] done\n- todo\n  more\n\n```swift\nlet x = 1\n```\n> quote\n\n[SUPERSEDED] old rule")
        #expect(blocks.first == .heading(1, "Title"))
        #expect(blocks.contains(.paragraph("Para one continues.")))
        #expect(blocks.contains(.code("swift", "let x = 1")))
        #expect(blocks.contains(.quote("quote")))
        #expect(blocks.last == .superseded("old rule"))
        guard case .list(let items) = blocks[2] else { Issue.record("expected list"); return }
        #expect(items.map(\.marker) == [.task(true), .bullet])
        #expect(items[1].text == "todo more")
    }

    @Test func fuzzyMatchRanksPrefixesFirst() {
        #expect(FuzzyMatch.score("adr", "ADR-0001 · Use Stripe") > FuzzyMatch.score("adr", "Add Architecture Node"))
        #expect(FuzzyMatch.score("cmc", "Copy MCP Config") > 0)
        #expect(FuzzyMatch.score("zzz", "Copy MCP Config") == 0)
    }
}
