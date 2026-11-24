use crate::analysis::{polarity, Polarity};
use crate::error::{EngramError, Result};
use crate::ops::{Op, UpsertMemory, UpsertPrompt};
use crate::schema::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeSet, HashSet};
use std::fmt::Write as _;
use std::path::Path;

pub const INSTRUCTION_FILES: &[&str] = &[
    "CLAUDE.md",
    "AGENTS.md",
    ".cursorrules",
    ".windsurfrules",
    ".github/copilot-instructions.md",
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MemoryCandidate {
    pub key: String,
    pub category: MemoryCategory,
    pub content: String,
    pub source: String,
    pub exists: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PromptCandidate {
    pub name: String,
    pub category: String,
    pub description: String,
    pub body: String,
    pub source: String,
    pub exists: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Bootstrap {
    pub memory: Vec<MemoryCandidate>,
    pub prompts: Vec<PromptCandidate>,
    pub sources: Vec<String>,
}

fn slug(text: &str, max_words: usize) -> String {
    let words: Vec<String> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .take(max_words)
        .map(str::to_lowercase)
        .collect();
    let mut s = words.join("-");
    if s.chars().count() > 48 {
        s = s.chars().take(48).collect::<String>().trim_end_matches('-').to_owned();
    }
    s
}

fn category_for(heading: &str, text: &str) -> MemoryCategory {
    let h = heading.to_lowercase();
    let any = |words: &[&str]| words.iter().any(|w| h.contains(w));
    if any(&[
        "stack",
        "tech",
        "dependenc",
        "librar",
        "framework",
        "стек",
        "технолог",
        "зависимост",
    ]) {
        MemoryCategory::TechStack
    } else if any(&[
        "architect",
        "invariant",
        "design",
        "structure",
        "архитект",
        "инвариант",
        "структур",
    ]) {
        MemoryCategory::Invariant
    } else if any(&[
        "style",
        "convention",
        "format",
        "naming",
        "code",
        "стиль",
        "соглашен",
        "оформлен",
        "код",
    ]) {
        MemoryCategory::Convention
    } else if any(&[
        "rule",
        "guideline",
        "must",
        "never",
        "important",
        "do not",
        "правил",
        "важно",
    ]) {
        MemoryCategory::Rule
    } else if polarity(text) == Polarity::Neutral {
        MemoryCategory::Note
    } else {
        MemoryCategory::Rule
    }
}

fn unique_key(base: &str, taken: &mut HashSet<String>) -> String {
    let base = if base.is_empty() {
        "imported".to_owned()
    } else {
        base.to_owned()
    };
    let mut key = base.clone();
    let mut n = 2;
    while taken.contains(&key) {
        key = format!("{base}-{n}");
        n += 1;
    }
    taken.insert(key.clone());
    key
}

pub fn import_instructions(text: &str, source: &str, doc: &EngramDocument) -> Vec<MemoryCandidate> {
    let mut items: Vec<(String, String)> = Vec::new();
    let mut heading = String::new();
    let mut paragraph: Vec<&str> = Vec::new();
    let mut in_fence = false;
    let flush = |items: &mut Vec<(String, String)>, heading: &str, paragraph: &mut Vec<&str>| {
        if !paragraph.is_empty() {
            items.push((heading.to_owned(), paragraph.join(" ")));
            paragraph.clear();
        }
    };
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with("```") {
            in_fence = !in_fence;
            flush(&mut items, &heading, &mut paragraph);
            continue;
        }
        if in_fence {
            continue;
        }
        if line.is_empty() || line.starts_with('@') || line.starts_with("<!--") {
            flush(&mut items, &heading, &mut paragraph);
            continue;
        }
        if let Some(h) = line.strip_prefix('#') {
            flush(&mut items, &heading, &mut paragraph);
            heading = h.trim_start_matches('#').trim().to_owned();
            continue;
        }
        let bullet = ["- ", "* ", "+ "]
            .iter()
            .find_map(|b| line.strip_prefix(b))
            .or_else(|| {
                let digits = line.chars().take_while(char::is_ascii_digit).count();
                (digits > 0).then(|| line[digits..].strip_prefix(". ")).flatten()
            });
        match bullet {
            Some(b) => {
                flush(&mut items, &heading, &mut paragraph);
                items.push((heading.clone(), b.trim().to_owned()));
            }
            None if raw.starts_with("  ") && !paragraph.is_empty() => paragraph.push(line),
            None if raw.starts_with("  ") => {
                if let Some(last) = items.last_mut() {
                    last.1.push(' ');
                    last.1.push_str(line);
                } else {
                    paragraph.push(line);
                }
            }
            None => paragraph.push(line),
        }
    }
    flush(&mut items, &heading, &mut paragraph);

    let existing_keys: HashSet<String> = doc.memory.iter().map(|m| m.key.clone()).collect();
    let existing_bodies: HashSet<String> = doc.memory.iter().map(|m| m.body().trim().to_lowercase()).collect();
    let mut taken = existing_keys.clone();
    items
        .into_iter()
        .filter(|(_, text)| text.split_whitespace().count() >= 2)
        .map(|(heading, text)| {
            let base = slug(&text, 5);
            let exists = existing_bodies.contains(&text.trim().to_lowercase());
            MemoryCandidate {
                key: unique_key(&base, &mut taken),
                category: category_for(&heading, &text),
                content: text,
                source: source.to_owned(),
                exists,
            }
        })
        .collect()
}

pub fn import_file(path: &Path, doc: &EngramDocument) -> Result<Vec<MemoryCandidate>> {
    let text = std::fs::read_to_string(path).map_err(|e| EngramError::io(path, e))?;
    let source = path
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(import_instructions(&text, &source, doc))
}

fn toml_section_keys(text: &str, sections: &[&str]) -> Vec<String> {
    let mut keys = Vec::new();
    let mut active = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            let name = line.trim_matches(|c| c == '[' || c == ']').trim();
            active = sections.contains(&name);
            continue;
        }
        if active {
            if let Some((k, _)) = line.split_once('=') {
                let k = k.trim().trim_matches('"');
                if !k.is_empty() && !k.starts_with('#') {
                    keys.push(k.to_owned());
                }
            }
        }
    }
    keys
}

fn toml_value(text: &str, key: &str) -> Option<String> {
    text.lines().map(str::trim).find_map(|l| {
        let (k, v) = l.split_once('=')?;
        (k.trim() == key).then(|| v.trim().trim_matches('"').to_owned())
    })
}

fn known<'a>(names: &[String], table: &[(&str, &'a str)]) -> Vec<&'a str> {
    let mut out: Vec<&str> = table
        .iter()
        .filter(|(dep, _)| names.iter().any(|n| n == dep))
        .map(|(_, label)| *label)
        .collect();
    out.dedup();
    out
}

const NPM: &[(&str, &str)] = &[
    ("typescript", "TypeScript"),
    ("react", "React"),
    ("next", "Next.js"),
    ("vue", "Vue"),
    ("svelte", "Svelte"),
    ("@angular/core", "Angular"),
    ("express", "Express"),
    ("fastify", "Fastify"),
    ("@nestjs/core", "NestJS"),
    ("vite", "Vite"),
    ("tailwindcss", "Tailwind CSS"),
    ("prisma", "Prisma"),
    ("drizzle-orm", "Drizzle"),
    ("jest", "Jest"),
    ("vitest", "Vitest"),
    ("@playwright/test", "Playwright"),
    ("eslint", "ESLint"),
    ("prettier", "Prettier"),
];

const CRATES: &[(&str, &str)] = &[
    ("tokio", "Tokio"),
    ("axum", "Axum"),
    ("actix-web", "Actix Web"),
    ("serde", "Serde"),
    ("clap", "Clap"),
    ("sqlx", "SQLx"),
    ("diesel", "Diesel"),
    ("tracing", "tracing"),
    ("anyhow", "anyhow"),
    ("thiserror", "thiserror"),
    ("eframe", "egui"),
    ("tauri", "Tauri"),
    ("wasm-bindgen", "wasm-bindgen"),
];

const PYTHON: &[(&str, &str)] = &[
    ("django", "Django"),
    ("fastapi", "FastAPI"),
    ("flask", "Flask"),
    ("pydantic", "Pydantic"),
    ("sqlalchemy", "SQLAlchemy"),
    ("pytest", "pytest"),
    ("numpy", "NumPy"),
    ("pandas", "pandas"),
];

const GO: &[(&str, &str)] = &[
    ("github.com/gin-gonic/gin", "Gin"),
    ("github.com/labstack/echo/v4", "Echo"),
    ("github.com/go-chi/chi/v5", "chi"),
    ("gorm.io/gorm", "GORM"),
    ("github.com/spf13/cobra", "Cobra"),
];

struct Scan {
    stack: Vec<String>,
    commands: Vec<String>,
    sources: BTreeSet<String>,
}

fn scan_dir(dir: &Path, rel: &str, scan: &mut Scan) {
    let at = |name: &str| {
        if rel.is_empty() {
            name.to_owned()
        } else {
            format!("{rel}/{name}")
        }
    };
    let read = |name: &str| std::fs::read_to_string(dir.join(name)).ok();

    if let Some(text) = read("package.json") {
        if let Ok(pkg) = serde_json::from_str::<Value>(&text) {
            scan.sources.insert(at("package.json"));
            let deps: Vec<String> = ["dependencies", "devDependencies", "peerDependencies"]
                .iter()
                .filter_map(|k| pkg.get(k).and_then(Value::as_object))
                .flat_map(|o| o.keys().cloned())
                .collect();
            let manager = if dir.join("pnpm-lock.yaml").exists() {
                "pnpm"
            } else if dir.join("yarn.lock").exists() {
                "yarn"
            } else if dir.join("bun.lockb").exists() || dir.join("bun.lock").exists() {
                "bun"
            } else {
                "npm"
            };
            let mut line = format!("JavaScript ({manager})");
            let libs = known(&deps, NPM);
            if !libs.is_empty() {
                let _ = write!(line, ": {}", libs.join(", "));
            }
            scan.stack.push(line);
            if let Some(scripts) = pkg.get("scripts").and_then(Value::as_object) {
                for s in ["build", "test", "lint", "typecheck"] {
                    if scripts.contains_key(s) {
                        let run = if manager == "npm" {
                            format!("npm run {s}")
                        } else {
                            format!("{manager} {s}")
                        };
                        scan.commands.push(format!("`{run}`"));
                    }
                }
            }
        }
    }
    if let Some(text) = read("Cargo.toml") {
        scan.sources.insert(at("Cargo.toml"));
        let deps = toml_section_keys(&text, &["dependencies", "workspace.dependencies", "dev-dependencies"]);
        let mut line = "Rust".to_owned();
        if let Some(edition) = toml_value(&text, "edition") {
            let _ = write!(line, " {edition} edition");
        }
        let libs = known(&deps, CRATES);
        if !libs.is_empty() {
            let _ = write!(line, ": {}", libs.join(", "));
        }
        scan.stack.push(line);
        scan.commands.push("`cargo test`".into());
        scan.commands.push("`cargo clippy -- -D warnings`".into());
    }
    if let Some(text) = read("Package.swift") {
        scan.sources.insert(at("Package.swift"));
        let tools = text
            .lines()
            .next()
            .and_then(|l| l.split(':').nth(1))
            .map(|v| v.trim().to_owned());
        let platforms: Vec<String> = [("macOS", ".macOS(.v"), ("iOS", ".iOS(.v")]
            .iter()
            .filter_map(|(name, marker)| {
                let rest = &text[text.find(marker)? + marker.len()..];
                let v: String = rest.chars().take_while(|c| c.is_ascii_digit() || *c == '_').collect();
                Some(format!("{name} {}", v.replace('_', ".")))
            })
            .collect();
        let mut line = "Swift".to_owned();
        if let Some(t) = tools {
            let _ = write!(line, " (tools {t})");
        }
        if !platforms.is_empty() {
            let _ = write!(line, " for {}", platforms.join(" and "));
        }
        scan.stack.push(line);
        scan.commands.push("`swift build`".into());
        scan.commands.push("`swift test`".into());
    }
    if let Some(text) = read("pyproject.toml") {
        scan.sources.insert(at("pyproject.toml"));
        let lower = text.to_lowercase();
        let deps: Vec<String> = PYTHON
            .iter()
            .filter(|(d, _)| lower.contains(&format!("\"{d}")) || lower.contains(&format!("\n{d} =")))
            .map(|(d, _)| (*d).to_owned())
            .collect();
        let mut line = "Python".to_owned();
        if let Some(v) = toml_value(&text, "requires-python") {
            let _ = write!(line, " {v}");
        }
        let libs = known(&deps, PYTHON);
        if !libs.is_empty() {
            let _ = write!(line, ": {}", libs.join(", "));
        }
        scan.stack.push(line);
        for (section, tool) in [
            ("[tool.ruff", "`ruff check`"),
            ("[tool.mypy", "`mypy`"),
            ("[tool.pytest", "`pytest`"),
        ] {
            if text.contains(section) {
                scan.commands.push(tool.into());
            }
        }
    }
    if let Some(text) = read("go.mod") {
        scan.sources.insert(at("go.mod"));
        let version = text
            .lines()
            .find_map(|l| l.trim().strip_prefix("go ").map(str::to_owned));
        let deps: Vec<String> = text
            .lines()
            .filter_map(|l| l.split_whitespace().next().map(str::to_owned))
            .collect();
        let mut line = "Go".to_owned();
        if let Some(v) = version {
            let _ = write!(line, " {v}");
        }
        let libs = known(&deps, GO);
        if !libs.is_empty() {
            let _ = write!(line, ": {}", libs.join(", "));
        }
        scan.stack.push(line);
        scan.commands.push("`go test ./...`".into());
    }
}

fn starter_prompts(stack: &str) -> Vec<(&'static str, &'static str, &'static str, String)> {
    let context = if stack.is_empty() {
        String::new()
    } else {
        format!("\n\nStack: {stack}.")
    };
    vec![
        (
            "implement-feature",
            "Build",
            "Start a feature with the project guardrails",
            format!(
                "Implement {{{{feature}}}}.{context}\n\nRead engram://handoff and call build_context first. Follow every invariant and never propose a rejected alternative. When you finish, call update_handoff with verified progress and the next step."
            ),
        ),
        (
            "fix-bug",
            "Build",
            "Investigate and fix a bug",
            format!(
                "Fix this bug: {{{{bug}}}}.{context}\n\nReproduce it first, find the root cause, add a test that fails without the fix and record anything durable with upsert_memory."
            ),
        ),
        (
            "review-diff",
            "Review",
            "Review a change against the project memory",
            "Review the change below against the project invariants and the decision log. Flag anything that contradicts an active decision.\n\n{{diff}}".to_owned(),
        ),
    ]
}

pub fn bootstrap(root: &Path, doc: &EngramDocument) -> Bootstrap {
    let mut scan = Scan {
        stack: Vec::new(),
        commands: Vec::new(),
        sources: BTreeSet::new(),
    };
    scan_dir(root, "", &mut scan);
    if let Ok(entries) = std::fs::read_dir(root) {
        let mut dirs: Vec<_> = entries
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| {
                !n.starts_with('.') && !matches!(n.as_str(), "node_modules" | "target" | "build" | "dist" | "vendor")
            })
            .collect();
        dirs.sort();
        for d in dirs {
            scan_dir(&root.join(&d), &d, &mut scan);
        }
    }
    scan.stack.dedup();
    scan.commands.dedup();

    let existing: HashSet<&str> = doc.memory.iter().map(|m| m.key.as_str()).collect();
    let mut out = Bootstrap::default();
    if !scan.stack.is_empty() {
        let content = scan
            .stack
            .iter()
            .map(|s| format!("- {s}"))
            .collect::<Vec<_>>()
            .join("\n");
        out.memory.push(MemoryCandidate {
            key: "tech-stack".into(),
            category: MemoryCategory::TechStack,
            content,
            source: scan.sources.iter().cloned().collect::<Vec<_>>().join(", "),
            exists: existing.contains("tech-stack"),
        });
    }
    if !scan.commands.is_empty() {
        out.memory.push(MemoryCandidate {
            key: "commands".into(),
            category: MemoryCategory::Convention,
            content: format!("Before you finish a change, run {}.", scan.commands.join(", ")),
            source: "build files".into(),
            exists: existing.contains("commands"),
        });
    }
    for name in INSTRUCTION_FILES {
        if let Ok(text) = std::fs::read_to_string(root.join(name)) {
            scan.sources.insert((*name).to_owned());
            let mut imported = import_instructions(&text, name, doc);
            let taken: HashSet<String> = out.memory.iter().map(|m| m.key.clone()).collect();
            imported.retain(|c| !taken.contains(&c.key) && !out.memory.iter().any(|m| m.content == c.content));
            out.memory.extend(imported);
        }
    }
    let stack = scan.stack.join("; ");
    for (name, category, description, body) in starter_prompts(&stack) {
        out.prompts.push(PromptCandidate {
            name: name.into(),
            category: category.into(),
            description: description.into(),
            body,
            source: "Engram".into(),
            exists: doc.prompt(name).is_some(),
        });
    }
    out.sources = scan.sources.into_iter().collect();
    out
}

pub fn apply_op(memory: &[MemoryCandidate], prompts: &[PromptCandidate]) -> Op {
    let mut ops: Vec<Op> = memory
        .iter()
        .map(|m| {
            Op::UpsertMemory(UpsertMemory {
                key: m.key.clone(),
                category: Some(m.category.as_str().to_owned()),
                content: m.content.clone(),
                supersedes_key: None,
                tags: None,
                files: None,
                pinned: None,
            })
        })
        .collect();
    ops.extend(prompts.iter().map(|p| {
        Op::UpsertPrompt(UpsertPrompt {
            name: p.name.clone(),
            body: p.body.clone(),
            category: Some(p.category.clone()),
            description: Some(p.description.clone()),
            rename_from: None,
        })
    }));
    Op::Batch { ops }
}

pub fn agents_pointer(doc: &EngramDocument, file_name: &str, server_name: &str) -> String {
    format!(
        "## Project memory\n\n\
         {name} keeps its rules, architecture, decisions and current state in `{file_name}`, managed with Engram. \
         Treat it as the source of truth.\n\n\
         - Read `engram://handoff` at the start of every session.\n\
         - Call `build_context` with your task before you change code.\n\
         - Record durable facts with `upsert_memory`, decisions with `log_decision` and progress with `update_handoff`.\n\
         - Never propose an approach listed as a rejected alternative.\n\n\
         The MCP server is registered as `{server_name}`.\n",
        name = doc.project.name
    )
}

fn mermaid_id(id: &str) -> String {
    let clean: String = id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    format!("n_{clean}")
}

pub fn architecture_mermaid(doc: &EngramDocument) -> String {
    let mut out = String::from("flowchart TD\n");
    for n in &doc.architecture.nodes {
        let label = n.display_name().replace('"', "#quot;");
        let shape = match n.kind {
            NodeKind::Database => format!("[(\"{label}\")]"),
            NodeKind::External => format!("[[\"{label}\"]]"),
            NodeKind::Ui => format!("([\"{label}\"])"),
            NodeKind::Queue => format!("[/\"{label}\"/]"),
            _ => format!("[\"{label}\"]"),
        };
        let _ = writeln!(out, "    {}{shape}", mermaid_id(&n.id));
    }
    let ids: HashSet<&str> = doc.architecture.nodes.iter().map(|n| n.id.as_str()).collect();
    for n in &doc.architecture.nodes {
        for d in n.depends_on.iter().filter(|d| ids.contains(d.as_str())) {
            let _ = writeln!(out, "    {} --> {}", mermaid_id(&n.id), mermaid_id(d));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const CLAUDE_MD: &str = "# Project guide\n\n@AGENTS.md\n\n## Tech stack\n\n- Next.js 15 with the app router\n- Postgres 16 through Prisma\n\n## Rules\n\n- Never commit secrets to the repository.\n- Always run the tests before pushing.\n\n```bash\nnpm test\n```\n\n## Code style\n\nUse named exports\nacross the codebase.\n\n## Правила\n\n- Никогда не используй moment.js в новом коде.\n";

    #[test]
    fn imports_instruction_files_into_candidates() {
        let mut doc = EngramDocument::new("P");
        serde_json::from_value::<Op>(json!({"op":"upsert_memory","key":"secrets","category":"rule","content":"Never commit secrets to the repository."}))
            .unwrap()
            .apply(&mut doc)
            .unwrap();
        let c = import_instructions(CLAUDE_MD, "CLAUDE.md", &doc);
        let find = |text: &str| {
            c.iter()
                .find(|m| m.content.contains(text))
                .unwrap_or_else(|| panic!("{text}: {c:#?}"))
        };
        assert_eq!(find("Next.js").category, MemoryCategory::TechStack);
        assert_eq!(find("Always run").category, MemoryCategory::Rule);
        assert_eq!(find("named exports").content, "Use named exports across the codebase.");
        assert_eq!(find("named exports").category, MemoryCategory::Convention);
        assert_eq!(find("moment.js").category, MemoryCategory::Rule);
        assert!(find("Never commit secrets").exists);
        assert!(!c
            .iter()
            .any(|m| m.content.contains("npm test") || m.content.starts_with('@')));
        let keys: HashSet<&str> = c.iter().map(|m| m.key.as_str()).collect();
        assert_eq!(keys.len(), c.len(), "keys are unique");
        assert!(!keys.contains("secrets"));
    }

    #[test]
    fn bootstraps_from_build_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(
            root.join("package.json"),
            r#"{"name":"web","dependencies":{"react":"19","next":"15"},"devDependencies":{"typescript":"5","vitest":"2"},"scripts":{"build":"next build","test":"vitest"}}"#,
        )
        .unwrap();
        std::fs::write(root.join("pnpm-lock.yaml"), "").unwrap();
        std::fs::create_dir_all(root.join("core")).unwrap();
        std::fs::write(root.join("core/Cargo.toml"), "[package]\nname = \"core\"\nedition = \"2021\"\n\n[dependencies]\ntokio = \"1\"\nserde = { version = \"1\" }\n").unwrap();
        std::fs::write(
            root.join("AGENTS.md"),
            "## Rules\n\n- Never edit generated files by hand.\n",
        )
        .unwrap();

        let doc = EngramDocument::new("P");
        let b = bootstrap(root, &doc);
        let stack = b.memory.iter().find(|m| m.key == "tech-stack").unwrap();
        assert!(
            stack
                .content
                .contains("JavaScript (pnpm): TypeScript, React, Next.js, Vitest"),
            "{}",
            stack.content
        );
        assert!(
            stack.content.contains("Rust 2021 edition: Tokio, Serde"),
            "{}",
            stack.content
        );
        let commands = b.memory.iter().find(|m| m.key == "commands").unwrap();
        assert!(commands.content.contains("`pnpm build`") && commands.content.contains("`cargo test`"));
        assert!(b
            .memory
            .iter()
            .any(|m| m.content == "Never edit generated files by hand."));
        assert_eq!(b.sources, vec!["AGENTS.md", "core/Cargo.toml", "package.json"]);
        assert_eq!(b.prompts.len(), 3);
        assert!(b.prompts[0].body.contains("{{feature}}"));

        let mut applied = doc.clone();
        apply_op(&b.memory, &b.prompts).apply(&mut applied).unwrap();
        assert_eq!(applied.memory.len(), b.memory.len());
        assert_eq!(applied.prompt("fix-bug").unwrap().variables(), vec!["bug"]);
        assert!(
            bootstrap(root, &applied)
                .memory
                .iter()
                .find(|m| m.key == "tech-stack")
                .unwrap()
                .exists
        );
    }

    #[test]
    fn exports_pointer_and_mermaid() {
        let mut doc = EngramDocument::new("Shop");
        for v in [
            json!({"op":"add_architecture_node","id":"web-app","type":"ui","label":"Web \"app\"","dependencies":["db"]}),
            json!({"op":"add_architecture_node","id":"db","type":"database"}),
            json!({"op":"add_architecture_node","id":"end","type":"external","dependencies":["ghost"]}),
        ] {
            serde_json::from_value::<Op>(v).unwrap().apply(&mut doc).unwrap();
        }
        let m = architecture_mermaid(&doc);
        assert!(m.starts_with("flowchart TD\n"));
        assert!(m.contains("n_web_app([\"Web #quot;app#quot;\"])"));
        assert!(m.contains("n_db[(\"db\")]"));
        assert!(m.contains("n_end[[\"end\"]]"));
        assert!(m.contains("n_web_app --> n_db"));
        assert!(!m.contains("ghost"));
        let p = agents_pointer(&doc, "project.engram", "engram-shop");
        assert!(p.starts_with("## Project memory"));
        assert!(p.contains("`project.engram`") && p.contains("`engram-shop`"));
    }
}
