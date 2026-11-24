import Foundation

struct EngramOp {
    let name: String
    let fields: [String: Any]

    private init(_ name: String, _ fields: [String: Any?]) {
        self.name = name
        self.fields = fields.compactMapValues { $0 }
    }

    func json() throws -> String {
        var payload = fields
        payload["op"] = name
        let data = try JSONSerialization.data(withJSONObject: payload, options: [.sortedKeys])
        return String(decoding: data, as: UTF8.self)
    }

    static func updateProject(name: String? = nil, description: String? = nil, repoRoot: String? = nil) -> EngramOp {
        EngramOp("update_project", ["name": name, "description": description, "repo_root": repoRoot])
    }

    enum Setting {
        case staleHandoffDays(Int?)
        case contextMinScore(Double?)
        case mcpServerName(String?)
        case ignoredDirs([String])
        case ignoredIssues([String])
    }

    static func updateSettings(_ settings: [Setting]) -> EngramOp {
        var fields: [String: Any?] = [:]
        for s in settings {
            switch s {
            case .staleHandoffDays(let d): fields["stale_handoff_days"] = .some(d.map { $0 as Any } ?? NSNull())
            case .contextMinScore(let v): fields["context_min_score"] = .some(v.map { $0 as Any } ?? NSNull())
            case .mcpServerName(let n): fields["mcp_server_name"] = .some(n.map { $0 as Any } ?? NSNull())
            case .ignoredDirs(let d): fields["health_ignore_dirs"] = d
            case .ignoredIssues(let i): fields["health_ignore_issues"] = i
            }
        }
        return EngramOp("update_project", fields)
    }

    static func raw(_ op: JSONValue) -> EngramOp? {
        guard var dict = op.foundation as? [String: Any], let name = dict.removeValue(forKey: "op") as? String else { return nil }
        return EngramOp(name, dict)
    }

    static func batch(_ ops: [EngramOp]) -> EngramOp {
        EngramOp("batch", ["ops": ops.map { op -> [String: Any] in
            var d = op.fields
            d["op"] = op.name
            return d
        }])
    }

    static func importCandidates(memory: [MemoryCandidate], prompts: [PromptCandidate]) -> EngramOp {
        batch(
            memory.map { upsertMemory(key: $0.key, category: $0.category, content: $0.content) }
                + prompts.map { upsertPrompt(name: $0.name, body: $0.body, category: $0.category, description: $0.description) }
        )
    }

    static func upsertMemory(
        key: String, category: MemoryCategory?, content: String, supersedesKey: String? = nil,
        tags: [String]? = nil, files: [String]? = nil, pinned: Bool? = nil
    ) -> EngramOp {
        EngramOp("upsert_memory", [
            "key": key, "category": category?.rawValue, "content": content, "supersedes_key": supersedesKey,
            "tags": tags, "files": files, "pinned": pinned,
        ])
    }

    static func renameMemory(key: String, to newKey: String) -> EngramOp {
        EngramOp("rename_memory", ["key": key, "new_key": newKey])
    }

    static func setMemoryStatus(key: String, _ status: EntryStatus) -> EngramOp {
        EngramOp("set_memory_status", ["key": key, "status": status.rawValue])
    }

    static func deleteMemory(key: String) -> EngramOp { EngramOp("delete_memory", ["key": key]) }

    static func addNode(
        id: String, kind: NodeKind?, dependencies: [String]? = nil, label: String? = nil,
        description: String? = nil, path: String? = nil
    ) -> EngramOp {
        EngramOp("add_architecture_node", [
            "id": id, "type": kind?.rawValue, "dependencies": dependencies, "label": label,
            "description": description, "path": path,
        ])
    }

    static func removeNode(id: String) -> EngramOp { EngramOp("remove_architecture_node", ["id": id]) }

    static func setNodePosition(id: String, x: Double, y: Double) -> EngramOp {
        EngramOp("set_node_position", ["id": id, "position": ["x": x, "y": y]])
    }

    static func resetNodePosition(id: String) -> EngramOp { EngramOp("set_node_position", ["id": id]) }

    static let resetAllNodePositions = EngramOp("reset_all_node_positions", [:])

    static func addDependency(from: String, to: String) -> EngramOp {
        EngramOp("add_dependency", ["from": from, "to": to])
    }

    static func removeDependency(from: String, to: String) -> EngramOp {
        EngramOp("remove_dependency", ["from": from, "to": to])
    }

    static func logDecision(
        title: String?, context: String, decision: String, rejected: [RejectedAlternative],
        consequences: String? = nil, related: [String] = [], supersedes: String? = nil
    ) -> EngramOp {
        EngramOp("log_decision", [
            "title": title, "context": context, "decision": decision,
            "rejected_alternatives": rejected.map { ["option": $0.option, "reason": $0.reason] },
            "consequences": consequences, "related": related, "supersedes": supersedes,
        ])
    }

    static func updateDecision(
        id: String, title: String? = nil, status: DecisionStatus? = nil, context: String? = nil,
        decision: String? = nil, rejected: [RejectedAlternative]? = nil, consequences: String? = nil,
        related: [String]? = nil, files: [String]? = nil
    ) -> EngramOp {
        EngramOp("update_decision", [
            "id": id, "title": title, "status": status?.rawValue, "context": context, "decision": decision,
            "rejected_alternatives": rejected.map { $0.map { ["option": $0.option, "reason": $0.reason] } },
            "consequences": consequences, "related": related, "files": files,
        ])
    }

    static func deleteDecision(id: String) -> EngramOp { EngramOp("delete_decision", ["id": id]) }

    static func upsertPrompt(name: String, body: String, category: String?, description: String?, renameFrom: String? = nil) -> EngramOp {
        EngramOp("upsert_prompt", [
            "name": name, "body": body, "category": category, "description": description, "rename_from": renameFrom,
        ])
    }

    static func deletePrompt(name: String) -> EngramOp { EngramOp("delete_prompt", ["name": name]) }

    static func updateHandoff(
        objective: String, progress: [ProgressItem], hypotheses: [String], blockers: [String], notes: String,
        nextStep: String, updatedBy: String?
    ) -> EngramOp {
        EngramOp("update_handoff", [
            "objective": objective,
            "progress": progress.map { ["description": $0.description, "files": $0.files, "verified": $0.verified] },
            "working_memory": ["hypotheses": hypotheses, "blockers": blockers, "notes": notes],
            "next_step": nextStep,
            "updated_by": updatedBy,
        ])
    }
}
