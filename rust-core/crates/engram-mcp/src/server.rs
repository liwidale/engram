use crate::tools;
use engram_core::context::{build_context, ContextRequest};
use engram_core::health::audit;
use engram_core::ops::{
    AddNode, AlternativeInput, LogDecision, Op, UpdateDecision, UpdateHandoff, UpsertMemory, UpsertPrompt,
};
use engram_core::render;
use engram_core::{EngramDocument, EngramError, Store};
use serde::de::DeserializeOwned;
use serde_json::{json, Map, Value};

pub const SERVER_NAME: &str = "engram-mcp";
pub const LATEST_PROTOCOL: &str = "2025-06-18";
const SUPPORTED_PROTOCOLS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
const INTERNAL_ERROR: i64 = -32603;
const RESOURCE_NOT_FOUND: i64 = -32002;

const INSTRUCTIONS: &str = "Engram is this project's canonical, version-controlled memory. \
Start every session by reading engram://handoff, then call build_context with your task. \
Treat project memory as ground truth; entries tagged [SUPERSEDED] and deprecated ADRs are history only. \
Never re-propose an approach listed as a rejected alternative or dead end. \
Record new durable facts with upsert_memory (use supersedes_key instead of contradicting an entry), \
record decisions with log_decision including rejected alternatives, and call update_handoff before you stop.";

pub struct Server {
    store: Store,
    client_name: Option<String>,
}

struct RpcError {
    code: i64,
    message: String,
    data: Option<Value>,
}

impl RpcError {
    fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }
}

impl From<EngramError> for RpcError {
    fn from(e: EngramError) -> Self {
        Self {
            code: INTERNAL_ERROR,
            message: e.to_string(),
            data: Some(json!({ "code": e.code() })),
        }
    }
}

type RpcResult = Result<Value, RpcError>;

struct Resource {
    uri: &'static str,
    name: &'static str,
    title: &'static str,
    description: &'static str,
    mime: &'static str,
}

const RESOURCES: &[Resource] = &[
    Resource {
        uri: "engram://memory",
        name: "memory",
        title: "Project Memory",
        description: "Canonical project rules, tech stack, invariants and conventions.",
        mime: "text/markdown",
    },
    Resource {
        uri: "engram://architecture",
        name: "architecture",
        title: "Architecture Map",
        description: "Module/service dependency graph with build order and cycles.",
        mime: "application/json",
    },
    Resource {
        uri: "engram://decisions",
        name: "decisions",
        title: "Decision Log",
        description: "Architecture Decision Records with rejected alternatives.",
        mime: "text/markdown",
    },
    Resource {
        uri: "engram://handoff",
        name: "handoff",
        title: "Agent Handoff",
        description: "Deterministic checkpoint: objective, verified progress, working memory, next step.",
        mime: "text/markdown",
    },
    Resource {
        uri: "engram://prompts",
        name: "prompts",
        title: "Prompt Vault",
        description: "Saved project prompts and templates.",
        mime: "application/json",
    },
];

impl Server {
    pub fn new(store: Store) -> Self {
        Self {
            store,
            client_name: None,
        }
    }

    pub fn handle_line(&mut self, line: &str) -> Option<String> {
        let line = line.trim();
        if line.is_empty() {
            return None;
        }
        let response = match serde_json::from_str::<Value>(line) {
            Err(e) => Some(error_response(
                Value::Null,
                RpcError::new(PARSE_ERROR, format!("parse error: {e}")),
            )),
            Ok(Value::Array(batch)) => {
                let out: Vec<Value> = batch.into_iter().filter_map(|m| self.handle(m)).collect();
                (!out.is_empty()).then_some(Value::Array(out))
            }
            Ok(msg) => self.handle(msg),
        };
        response.map(|r| r.to_string())
    }

    pub fn handle(&mut self, msg: Value) -> Option<Value> {
        let Value::Object(mut obj) = msg else {
            return Some(error_response(
                Value::Null,
                RpcError::new(INVALID_REQUEST, "message must be a JSON object"),
            ));
        };
        let id = obj.remove("id");
        let Some(method) = obj.get("method").and_then(Value::as_str).map(str::to_owned) else {
            return id
                .filter(|_| !obj.contains_key("result") && !obj.contains_key("error"))
                .map(|id| error_response(id, RpcError::new(INVALID_REQUEST, "missing method")));
        };
        let params = match obj.remove("params") {
            None | Some(Value::Null) => Value::Object(Map::new()),
            Some(p) => p,
        };
        let Some(id) = id else {
            self.notification(&method, &params);
            return None;
        };
        let result = self.request(&method, params);
        Some(match result {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err(e) => error_response(id, e),
        })
    }

    fn notification(&mut self, method: &str, _params: &Value) {
        match method {
            "notifications/initialized" | "notifications/cancelled" | "notifications/roots/list_changed" => {}
            other => eprintln!("[engram-mcp] ignoring notification {other}"),
        }
    }

    fn request(&mut self, method: &str, params: Value) -> RpcResult {
        match method {
            "initialize" => Ok(self.initialize(&params)),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tools::definitions() })),
            "tools/call" => self.call_tool(params),
            "resources/list" => Ok(self.list_resources()),
            "resources/templates/list" => Ok(json!({ "resourceTemplates": [
                { "uriTemplate": "engram://memory/{key}", "name": "memory-entry", "title": "Memory entry", "mimeType": "text/markdown" },
                { "uriTemplate": "engram://decisions/{id}", "name": "decision", "title": "Decision record", "mimeType": "text/markdown" }
            ]})),
            "resources/read" => self.read_resource(&params),
            "prompts/list" => self.list_prompts(),
            "prompts/get" => self.get_mcp_prompt(&params),
            other => Err(RpcError::new(METHOD_NOT_FOUND, format!("method not found: {other}"))),
        }
    }

    fn initialize(&mut self, params: &Value) -> Value {
        self.client_name = params
            .pointer("/clientInfo/name")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let requested = params
            .get("protocolVersion")
            .and_then(Value::as_str)
            .unwrap_or(LATEST_PROTOCOL);
        let version = if SUPPORTED_PROTOCOLS.contains(&requested) {
            requested
        } else {
            LATEST_PROTOCOL
        };
        json!({
            "protocolVersion": version,
            "capabilities": {
                "tools": { "listChanged": false },
                "resources": { "subscribe": false, "listChanged": false },
                "prompts": { "listChanged": false }
            },
            "serverInfo": { "name": SERVER_NAME, "title": "Engram Project Memory", "version": env!("CARGO_PKG_VERSION") },
            "instructions": INSTRUCTIONS
        })
    }

    fn load(&self) -> Result<EngramDocument, RpcError> {
        Ok(self.store.load()?.document)
    }

    fn list_resources(&self) -> Value {
        let resources: Vec<Value> = RESOURCES
            .iter()
            .map(|r| json!({ "uri": r.uri, "name": r.name, "title": r.title, "description": r.description, "mimeType": r.mime }))
            .collect();
        json!({ "resources": resources })
    }

    fn read_resource(&self, params: &Value) -> RpcResult {
        let uri = params
            .get("uri")
            .and_then(Value::as_str)
            .ok_or_else(|| RpcError::new(INVALID_PARAMS, "uri is required"))?;
        let doc = self.load()?;
        let (mime, text) = match uri.trim_end_matches('/') {
            "engram://memory" => ("text/markdown", render::memory_markdown(&doc)),
            "engram://architecture" => ("application/json", pretty(&render::architecture_json(&doc))),
            "engram://decisions" => ("text/markdown", render::decisions_markdown(&doc)),
            "engram://handoff" => ("text/markdown", render::handoff_markdown(&doc)),
            "engram://prompts" => ("application/json", pretty(&render::prompts_json(&doc))),
            other => {
                if let Some(key) = other.strip_prefix("engram://memory/") {
                    let m = doc.memory_entry(key).ok_or_else(|| not_found(uri))?;
                    ("text/markdown", render::memory_entry_markdown(m))
                } else if let Some(id) = other.strip_prefix("engram://decisions/") {
                    let d = doc.decision(id).ok_or_else(|| not_found(uri))?;
                    ("text/markdown", render::decision_markdown(d))
                } else {
                    return Err(not_found(uri));
                }
            }
        };
        Ok(json!({ "contents": [{ "uri": uri, "mimeType": mime, "text": text }] }))
    }

    fn list_prompts(&self) -> RpcResult {
        let doc = self.load()?;
        let prompts: Vec<Value> = doc
            .prompts
            .iter()
            .map(|p| {
                let args: Vec<Value> = p.variables().into_iter().map(|v| json!({ "name": v, "required": true })).collect();
                json!({
                    "name": p.name,
                    "title": p.name,
                    "description": if p.description.is_empty() { format!("{} prompt from the Engram Prompt Vault", if p.category.is_empty() { "Project" } else { &p.category }) } else { p.description.clone() },
                    "arguments": args
                })
            })
            .collect();
        Ok(json!({ "prompts": prompts }))
    }

    fn get_mcp_prompt(&self, params: &Value) -> RpcResult {
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| RpcError::new(INVALID_PARAMS, "name is required"))?;
        let doc = self.load()?;
        let p = doc
            .prompt(name)
            .ok_or_else(|| RpcError::new(INVALID_PARAMS, format!("unknown prompt: {name}")))?;
        let vars = params
            .get("arguments")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let rendered = p.render(&vars);
        Ok(json!({
            "description": p.description,
            "messages": [{ "role": "user", "content": { "type": "text", "text": rendered.text } }]
        }))
    }

    fn call_tool(&mut self, params: Value) -> RpcResult {
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| RpcError::new(INVALID_PARAMS, "tool name is required"))?
            .to_owned();
        let args = match params.get("arguments") {
            None | Some(Value::Null) => Value::Object(Map::new()),
            Some(a @ Value::Object(_)) => a.clone(),
            Some(_) => return Err(RpcError::new(INVALID_PARAMS, "arguments must be an object")),
        };
        if !tools::definitions().iter().any(|t| t["name"] == name.as_str()) {
            return Err(RpcError::new(INVALID_PARAMS, format!("unknown tool: {name}")));
        }
        Ok(match self.run_tool(&name, args) {
            Ok(out) => out.into_result(),
            Err(e) => json!({ "content": [{ "type": "text", "text": format!("Error: {e}") }], "isError": true }),
        })
    }

    fn run_tool(&mut self, name: &str, args: Value) -> Result<ToolOutput, ToolError> {
        match name {
            "build_context" => {
                let mut req: ContextRequest = parse_args(args)?;
                if req.task.trim().is_empty() {
                    return Err(ToolError::Input("task_description must not be empty".into()));
                }
                req.max_tokens = req.max_tokens.clamp(200, 200_000);
                let doc = self.store.load()?.document;
                let bundle = build_context(&doc, &req);
                let included: Vec<Value> = bundle
                    .items
                    .iter()
                    .filter(|i| i.included)
                    .map(|i| json!({ "kind": i.kind, "id": i.id }))
                    .collect();
                let structured = json!({
                    "tokens": bundle.tokens,
                    "max_tokens": bundle.max_tokens,
                    "over_budget": bundle.over_budget,
                    "tokenizer": bundle.tokenizer,
                    "included": included,
                    "dead_ends": bundle.dead_ends,
                    "prompt": bundle.prompt,
                });
                Ok(ToolOutput {
                    text: bundle.xml,
                    structured: Some(structured),
                })
            }
            "upsert_memory" => self.mutate(Op::UpsertMemory(parse_args::<UpsertMemory>(args)?)),
            "add_architecture_node" => {
                let n: AddNode = parse_args(args)?;
                if n.node_type.as_deref().is_none_or(|t| t.trim().is_empty())
                    && self.store.load()?.document.node(&n.id).is_none()
                {
                    return Err(ToolError::Input("type is required when creating a node".into()));
                }
                self.mutate(Op::AddArchitectureNode(n))
            }
            "log_decision" => self.mutate(Op::LogDecision(parse_args::<LogDecision>(args)?)),
            "deprecate_decision" => {
                #[derive(serde::Deserialize)]
                struct A {
                    id: String,
                    #[serde(default)]
                    superseded_by: Option<String>,
                }
                let a: A = parse_args(args)?;
                self.mutate(Op::UpdateDecision(UpdateDecision {
                    id: a.id,
                    title: None,
                    status: Some(engram_core::DecisionStatus::Deprecated),
                    context: None,
                    decision: None,
                    rejected_alternatives: None::<Vec<AlternativeInput>>,
                    consequences: None,
                    related: None,
                    files: None,
                    superseded_by: a.superseded_by,
                }))
            }
            "update_handoff" => {
                let mut h: UpdateHandoff = parse_args(args)?;
                if h.updated_by.is_none() {
                    h.updated_by = self.client_name.clone();
                }
                self.mutate(Op::UpdateHandoff(h))
            }
            "audit_health" => {
                let root = args.get("repo_root_path").and_then(Value::as_str).map(str::to_owned);
                let doc = self.store.load()?.document;
                let report = audit(&doc, &self.store.base_dir(), root.as_deref());
                let mut text = format!(
                    "Context health {}/100 ({}) · {} errors, {} warnings, {} info · repo: {}\n",
                    report.score,
                    report.grade,
                    report.summary.errors,
                    report.summary.warnings,
                    report.summary.infos,
                    report.repo_root.as_deref().unwrap_or("?")
                );
                for i in &report.issues {
                    text.push_str(&format!(
                        "- [{}] {}: {}",
                        enum_name(&i.severity),
                        enum_name(&i.kind),
                        i.message
                    ));
                    if let Some(s) = &i.suggestion {
                        text.push_str(&format!(" ({s})"));
                    }
                    text.push('\n');
                }
                Ok(ToolOutput {
                    text,
                    structured: Some(serde_json::to_value(&report).map_err(EngramError::from)?),
                })
            }
            "get_prompt" => {
                let name = args
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| ToolError::Input("name is required".into()))?;
                let doc = self.store.load()?.document;
                let p = doc.prompt(name).ok_or_else(|| {
                    let known: Vec<&str> = doc.prompts.iter().map(|p| p.name.as_str()).collect();
                    ToolError::Input(format!(
                        "no prompt named '{name}'. Available: {}",
                        if known.is_empty() {
                            "(none)".into()
                        } else {
                            known.join(", ")
                        }
                    ))
                })?;
                let vars = args
                    .get("variables")
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default();
                let rendered = p.render(&vars);
                Ok(ToolOutput {
                    text: rendered.text.clone(),
                    structured: Some(json!({
                        "name": p.name, "category": p.category, "variables": p.variables(),
                        "missing": rendered.missing, "text": rendered.text
                    })),
                })
            }
            "save_prompt" => self.mutate(Op::UpsertPrompt(parse_args::<UpsertPrompt>(args)?)),
            _ => unreachable!("checked against tool definitions"),
        }
    }

    fn mutate(&self, op: Op) -> Result<ToolOutput, ToolError> {
        let (mut result, snap) = op.execute(&self.store)?;
        if let Value::Object(o) = &mut result {
            o.insert("revision".into(), json!(snap.document.revision));
            if o.get("warnings").and_then(Value::as_array).is_some_and(Vec::is_empty) {
                o.remove("warnings");
            }
        }
        Ok(ToolOutput {
            text: pretty(&result),
            structured: Some(result),
        })
    }
}

fn enum_name<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_value(v)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

struct ToolOutput {
    text: String,
    structured: Option<Value>,
}

impl ToolOutput {
    fn into_result(self) -> Value {
        let mut r = json!({ "content": [{ "type": "text", "text": self.text }], "isError": false });
        if let Some(s) = self.structured.filter(Value::is_object) {
            r["structuredContent"] = s;
        }
        r
    }
}

enum ToolError {
    Input(String),
    Engram(EngramError),
}

impl From<EngramError> for ToolError {
    fn from(e: EngramError) -> Self {
        Self::Engram(e)
    }
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Input(m) => write!(f, "invalid arguments: {m}"),
            Self::Engram(e) => write!(f, "{e}"),
        }
    }
}

fn parse_args<T: DeserializeOwned>(args: Value) -> Result<T, ToolError> {
    serde_json::from_value(args).map_err(|e| ToolError::Input(e.to_string()))
}

fn pretty(v: &Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_default()
}

fn not_found(uri: &str) -> RpcError {
    RpcError {
        code: RESOURCE_NOT_FOUND,
        message: format!("resource not found: {uri}"),
        data: Some(json!({ "uri": uri })),
    }
}

fn error_response(id: Value, e: RpcError) -> Value {
    let mut err = json!({ "code": e.code, "message": e.message });
    if let Some(d) = e.data {
        err["data"] = d;
    }
    json!({ "jsonrpc": "2.0", "id": id, "error": err })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server() -> (tempfile::TempDir, Server) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("t.engram"));
        store.create("Test").unwrap();
        (dir, Server::new(store))
    }

    fn call(s: &mut Server, id: i64, method: &str, params: Value) -> Value {
        s.handle(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))
            .expect("response")
    }

    fn tool(s: &mut Server, name: &str, args: Value) -> Value {
        let r = call(s, 99, "tools/call", json!({ "name": name, "arguments": args }));
        assert!(r.get("error").is_none(), "{r}");
        r["result"].clone()
    }

    #[test]
    fn initialize_negotiates_protocol() {
        let (_d, mut s) = server();
        let r = call(
            &mut s,
            1,
            "initialize",
            json!({"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"claude-code","version":"1"}}),
        );
        assert_eq!(r["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(r["result"]["serverInfo"]["name"], SERVER_NAME);
        let r = call(&mut s, 2, "initialize", json!({"protocolVersion":"1999-01-01"}));
        assert_eq!(r["result"]["protocolVersion"], LATEST_PROTOCOL);
        assert!(s
            .handle(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
            .is_none());
        assert_eq!(call(&mut s, 3, "ping", Value::Null)["result"], json!({}));
        assert_eq!(call(&mut s, 4, "nope", Value::Null)["error"]["code"], METHOD_NOT_FOUND);
    }

    #[test]
    fn transport_level_errors() {
        let (_d, mut s) = server();
        let r: Value = serde_json::from_str(&s.handle_line("{not json").unwrap()).unwrap();
        assert_eq!(r["error"]["code"], PARSE_ERROR);
        assert!(s.handle_line("   ").is_none());
        let batch = s
            .handle_line(
                r#"[{"jsonrpc":"2.0","id":1,"method":"ping"},{"jsonrpc":"2.0","method":"notifications/initialized"}]"#,
            )
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&batch).unwrap().as_array().unwrap().len(),
            1
        );
        assert_eq!(
            call(&mut s, 5, "tools/call", json!({"name":"rm_rf"}))["error"]["code"],
            INVALID_PARAMS
        );
    }

    #[test]
    fn lists_everything() {
        let (_d, mut s) = server();
        let tools = call(&mut s, 1, "tools/list", Value::Null)["result"]["tools"].clone();
        let names: Vec<&str> = tools
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        for required in [
            "build_context",
            "upsert_memory",
            "add_architecture_node",
            "log_decision",
            "update_handoff",
            "audit_health",
            "get_prompt",
        ] {
            assert!(names.contains(&required), "{required}");
        }
        let res = call(&mut s, 2, "resources/list", Value::Null)["result"]["resources"].clone();
        let uris: Vec<&str> = res
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["uri"].as_str().unwrap())
            .collect();
        assert_eq!(
            uris,
            vec![
                "engram://memory",
                "engram://architecture",
                "engram://decisions",
                "engram://handoff",
                "engram://prompts"
            ]
        );
    }

    #[test]
    fn tools_persist_and_resources_reflect_them() {
        let (dir, mut s) = server();
        let r = tool(
            &mut s,
            "upsert_memory",
            json!({"key":"db","category":"tech_stack","content":"Postgres 16"}),
        );
        assert_eq!(r["structuredContent"]["created"], true);
        assert_eq!(r["structuredContent"]["revision"], 2);
        tool(
            &mut s,
            "upsert_memory",
            json!({"key":"db2","category":"tech_stack","content":"Postgres 17","supersedes_key":"db"}),
        );
        tool(
            &mut s,
            "add_architecture_node",
            json!({"id":"api","type":"service","dependencies":["pg"]}),
        );
        tool(&mut s, "add_architecture_node", json!({"id":"pg","type":"database"}));
        let r = tool(
            &mut s,
            "log_decision",
            json!({"context":"need queue","decision":"Use NATS","rejected_alternatives":[{"option":"Kafka","reason":"too heavy"}]}),
        );
        assert_eq!(r["structuredContent"]["id"], "ADR-0001");
        tool(&mut s, "save_prompt", json!({"name":"fix","body":"Fix {{bug}}"}));
        call(&mut s, 0, "initialize", json!({"clientInfo":{"name":"cursor"}}));
        tool(
            &mut s,
            "update_handoff",
            json!({"objective":"o","progress":[{"description":"did x","files":["a.rs"]}],"working_memory":{"hypotheses":["h"],"blockers":[]},"next_step":"n"}),
        );

        let doc = Store::new(dir.path().join("t.engram")).load().unwrap().document;
        assert_eq!(doc.memory.len(), 2);
        assert_eq!(doc.handoff.updated_by.as_deref(), Some("cursor"));

        let read = |s: &mut Server, uri: &str| {
            call(s, 7, "resources/read", json!({"uri": uri}))["result"]["contents"][0]["text"]
                .as_str()
                .unwrap()
                .to_owned()
        };
        assert!(read(&mut s, "engram://memory").contains("[SUPERSEDED] db"));
        assert!(read(&mut s, "engram://decisions").contains("**Kafka**: too heavy"));
        assert!(read(&mut s, "engram://handoff").contains("- [x] did x (`a.rs`)"));
        let arch: Value = serde_json::from_str(&read(&mut s, "engram://architecture")).unwrap();
        assert_eq!(arch["build_order"], json!(["pg", "api"]));
        assert!(read(&mut s, "engram://prompts").contains("\"bug\""));
        assert!(read(&mut s, "engram://decisions/adr-0001").contains("Use NATS"));
        assert_eq!(
            call(&mut s, 8, "resources/read", json!({"uri":"engram://nope"}))["error"]["code"],
            RESOURCE_NOT_FOUND
        );

        let ctx = tool(
            &mut s,
            "build_context",
            json!({"task_description":"add an api endpoint that writes to postgres","max_tokens":2000}),
        );
        let xml = ctx["content"][0]["text"].as_str().unwrap();
        assert!(xml.starts_with("<engram_context"));
        assert!(xml.contains("Postgres 17") && !xml.contains("Postgres 16"));
        assert!(ctx["structuredContent"]["tokens"].as_u64().unwrap() > 0);

        let p = tool(
            &mut s,
            "get_prompt",
            json!({"name":"fix","variables":{"bug":"the leak"}}),
        );
        assert_eq!(p["content"][0]["text"], "Fix the leak");
        let mp = call(&mut s, 9, "prompts/get", json!({"name":"fix","arguments":{"bug":"it"}}));
        assert_eq!(mp["result"]["messages"][0]["content"]["text"], "Fix it");

        let h = tool(&mut s, "audit_health", json!({}));
        assert!(h["content"][0]["text"].as_str().unwrap().starts_with("Context health"));
        assert!(h["structuredContent"]["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["kind"] == "broken_path"));

        let d = tool(&mut s, "deprecate_decision", json!({"id":"ADR-0001"}));
        assert_eq!(d["structuredContent"]["status"], "deprecated");
    }

    #[test]
    fn domain_errors_are_tool_errors() {
        let (_d, mut s) = server();
        let r = tool(&mut s, "upsert_memory", json!({"key":"x","content":"no category"}));
        assert_eq!(r["isError"], true);
        let r = tool(
            &mut s,
            "upsert_memory",
            json!({"key":"x","category":"weird","content":"c"}),
        );
        assert!(r["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("unknown memory category"));
        let r = tool(&mut s, "get_prompt", json!({"name":"missing"}));
        assert_eq!(r["isError"], true);
        let r = tool(&mut s, "add_architecture_node", json!({"id":"x"}));
        assert_eq!(r["isError"], true);
        let r = tool(&mut s, "build_context", json!({"task_description":""}));
        assert_eq!(r["isError"], true);
    }
}
