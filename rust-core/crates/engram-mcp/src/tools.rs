use serde_json::{json, Value};

fn tool(name: &str, title: &str, description: &str, schema: Value, read_only: bool, idempotent: bool) -> Value {
    json!({
        "name": name,
        "title": title,
        "description": description,
        "inputSchema": schema,
        "annotations": {
            "title": title,
            "readOnlyHint": read_only,
            "destructiveHint": false,
            "idempotentHint": idempotent,
            "openWorldHint": false
        }
    })
}

fn string_list(description: &str) -> Value {
    json!({ "type": "array", "items": { "type": "string" }, "description": description })
}

pub fn definitions() -> Vec<Value> {
    vec![
        tool(
            "build_context",
            "Build task context",
            "Rank the project's memory, architecture and decision records against a task and return a minimal, \
             high-signal XML context bundle that fits the token budget. Superseded memory and deprecated decisions \
             are never included; rejected alternatives are listed as dead ends. Call this before starting a task.",
            json!({
                "type": "object",
                "properties": {
                    "task_description": { "type": "string", "description": "What you are about to do, in plain language." },
                    "max_tokens": { "type": "integer", "minimum": 200, "maximum": 200000, "default": 4000, "description": "Token budget for the bundle (o200k_base)." },
                    "prompt": { "type": "string", "description": "Optional Prompt Vault entry to append, rendered with prompt_vars." },
                    "prompt_vars": { "type": "object", "additionalProperties": { "type": "string" }, "description": "Values for {{variable}} placeholders." },
                    "include_handoff": { "type": "boolean", "default": true }
                },
                "required": ["task_description"]
            }),
            true,
            true,
        ),
        tool(
            "upsert_memory",
            "Upsert project memory",
            "Create or update a canonical Project Memory entry (rule, tech stack, invariant, convention or note). \
             Pass supersedes_key to retire an older entry atomically: it is tagged [SUPERSEDED], linked to the new \
             key and excluded from all future context.",
            json!({
                "type": "object",
                "properties": {
                    "key": { "type": "string", "description": "Stable kebab-case identifier, e.g. 'db-engine'." },
                    "category": { "type": "string", "enum": ["rule", "tech_stack", "invariant", "convention", "note"], "description": "Required when creating." },
                    "content": { "type": "string", "description": "Markdown body." },
                    "supersedes_key": { "type": "string", "description": "Key of an entry this one replaces." },
                    "tags": string_list("Optional tags."),
                    "files": string_list("Codebase paths this entry is about (validated by audit_health)."),
                    "pinned": { "type": "boolean", "description": "Always include in built context." }
                },
                "required": ["key", "content"]
            }),
            false,
            true,
        ),
        tool(
            "add_architecture_node",
            "Add architecture node",
            "Create or update a node in the Architecture Map. dependencies are outgoing edges (this node depends \
             on each listed node) and replace the existing list when provided.",
            json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Node id without whitespace, e.g. 'billing-api'." },
                    "type": { "type": "string", "enum": ["module", "service", "database", "library", "queue", "ui", "external"] },
                    "dependencies": string_list("Ids of nodes this node depends on."),
                    "label": { "type": "string" },
                    "description": { "type": "string" },
                    "path": { "type": "string", "description": "Directory or file implementing the node, relative to the repo root." }
                },
                "required": ["id", "type"]
            }),
            false,
            true,
        ),
        tool(
            "log_decision",
            "Log architecture decision",
            "Append an Architecture Decision Record with a new sequential id. Record every rejected alternative and \
             why, so future agents never re-propose it. Pass supersedes to deprecate an earlier ADR.",
            json!({
                "type": "object",
                "properties": {
                    "title": { "type": "string", "description": "Short title; derived from the decision when omitted." },
                    "context": { "type": "string", "description": "The forces and constraints that led to the decision." },
                    "decision": { "type": "string", "description": "What was decided." },
                    "rejected_alternatives": {
                        "type": "array",
                        "items": {
                            "anyOf": [
                                { "type": "object", "properties": { "option": { "type": "string" }, "reason": { "type": "string" } }, "required": ["option"] },
                                { "type": "string", "description": "'Option: reason'" }
                            ]
                        }
                    },
                    "consequences": { "type": "string" },
                    "related": string_list("Related memory keys, node ids or ADR ids."),
                    "files": string_list("Affected codebase paths."),
                    "supersedes": { "type": "string", "description": "ADR id this decision replaces (it becomes deprecated)." }
                },
                "required": ["context", "decision", "rejected_alternatives"]
            }),
            false,
            false,
        ),
        tool(
            "deprecate_decision",
            "Deprecate decision",
            "Mark an ADR as deprecated so it is excluded from context. Optionally point at the ADR that replaced it.",
            json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "superseded_by": { "type": "string" }
                },
                "required": ["id"]
            }),
            false,
            true,
        ),
        tool(
            "update_handoff",
            "Update agent handoff",
            "Overwrite the Agent Handoff checkpoint with deterministic state instead of a lossy chat summary. Call \
             this whenever you finish a unit of work or before your context runs out.",
            json!({
                "type": "object",
                "properties": {
                    "objective": { "type": "string", "description": "The active objective." },
                    "progress": {
                        "type": "array",
                        "description": "Verified progress, with the files touched.",
                        "items": {
                            "anyOf": [
                                {
                                    "type": "object",
                                    "properties": {
                                        "description": { "type": "string" },
                                        "files": { "type": "array", "items": { "type": "string" } },
                                        "verified": { "type": "boolean", "default": true }
                                    },
                                    "required": ["description"]
                                },
                                { "type": "string" }
                            ]
                        }
                    },
                    "working_memory": {
                        "anyOf": [
                            {
                                "type": "object",
                                "properties": {
                                    "hypotheses": { "type": "array", "items": { "type": "string" } },
                                    "blockers": { "type": "array", "items": { "type": "string" } },
                                    "notes": { "type": "string" }
                                }
                            },
                            { "type": "string" }
                        ]
                    },
                    "next_step": { "type": "string", "description": "The single next atomic step." },
                    "updated_by": { "type": "string", "description": "Agent name; defaults to the MCP client name." }
                },
                "required": ["objective", "progress", "working_memory", "next_step"]
            }),
            false,
            true,
        ),
        tool(
            "audit_health",
            "Audit context health",
            "Scan the .engram file against the codebase for broken file paths, zombie references, deprecated ADRs \
             still in use, contradicting rules and endorsed rejected alternatives.",
            json!({
                "type": "object",
                "properties": {
                    "repo_root_path": { "type": "string", "description": "Codebase root. Defaults to the project's linked repo, else the .engram file's directory." }
                }
            }),
            true,
            true,
        ),
        tool(
            "get_prompt",
            "Get vault prompt",
            "Retrieve a prompt from the project's Prompt Vault, optionally rendering its {{variable}} placeholders.",
            json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string" },
                    "variables": { "type": "object", "additionalProperties": { "type": "string" } }
                },
                "required": ["name"]
            }),
            true,
            true,
        ),
        tool(
            "save_prompt",
            "Save vault prompt",
            "Create or update a reusable prompt template in the Prompt Vault. Use {{variable}} placeholders.",
            json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string" },
                    "body": { "type": "string" },
                    "category": { "type": "string" },
                    "description": { "type": "string" }
                },
                "required": ["name", "body"]
            }),
            false,
            true,
        ),
    ]
}
