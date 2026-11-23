# Engram

A native macOS workspace for a project's long-lived memory, plus an MCP server that lets AI agents read and update that memory. Everything lives in a single `.engram` file next to your code, versioned with Git.

Agents lose context across sessions and re-propose ideas you already rejected. Engram replaces lossy chat summaries with deterministic, structured state:

| Module | What it holds |
| --- | --- |
| **Project Memory** | Canonical rules, tech stack, architecture invariants, conventions. Markdown editor with live preview. `[SUPERSEDED]` entries are kept for history but never fed to agents. |
| **Architecture Map** | Modules, services, databases and directional dependencies, drawn as an interactive graph (layered layout, cycle detection, draggable nodes). |
| **Decision Log** | ADRs with context, decision, and every rejected alternative with the reason it lost. |
| **Prompt Vault** | Project prompts and templates with `{{variable}}` placeholders. |
| **Context Builder** | Ranks memory, ADRs and nodes against a task and packs the best ones into a token budget, exported as XML. |
| **Context Health** | Audits the file against the codebase: broken paths, zombie references, deprecated ADRs still in use, contradicting rules, memory that endorses a rejected alternative. |
| **Agent Handoff** | Checkpoint of active objective, verified progress (with files), working memory and the next atomic step. |

## Repository layout

```
build.sh                    Builds everything (Rust libs, header, engram-mcp, Engram.app)
rust-core/                  Cargo workspace
  crates/engram-core/       Schema, persistence, graph, context builder, health, C ABI
  crates/engram-mcp/        MCP server (JSON-RPC 2.0 over stdio) and CLI
  cbindgen.toml             C header generation
EngramApp/                  Swift package for the macOS app (macOS 14+)
  Sources/CEngramCore/      Module map for libengram_core.a
  Sources/Engram/           SwiftUI app (App, Core bridge, UI, Views)
  Tests/EngramTests/        Swift ⇄ Rust bridge tests
  Support/                  Info.plist (document types), icon, bridging header
  Vendor/                   Generated: libengram_core.a + engram_core.h
tools/make-icon.swift       Regenerates AppIcon.icns
project.engram              Engram's own project memory
```

## Building

Requirements: macOS 14+, Rust (with `aarch64-apple-darwin` and `x86_64-apple-darwin` targets), and the Xcode Command Line Tools or Xcode. `build.sh` installs `cbindgen` if it is missing.

```bash
./build.sh                    # universal release build -> build/Engram.app
./build.sh --native --debug   # fast host-only build for development
./build.sh --test             # also run the Rust and Swift test suites
./build.sh --install          # copy to /Applications and register the .engram type
```

The script:

1. builds `engram-core` and `engram-mcp` for each architecture and merges them with `lipo`;
2. generates `EngramApp/Vendor/include/engram_core.h` with cbindgen;
3. builds the SwiftUI app per architecture with SwiftPM, merges the slices, and assembles `Engram.app` with `engram-mcp` embedded in `Contents/MacOS/`;
4. signs the bundle ad hoc.

To work in Xcode, run `./build.sh --core-only` once, then open `EngramApp/Package.swift`. Use `build.sh` to produce the distributable bundle. `Support/Engram-Bridging-Header.h` is provided for Xcode app targets that link `libengram_core.a` directly instead of using the module map.

## Connecting an agent (MCP)

Click **Copy MCP Config** in the toolbar (⇧⌘M). It copies a snippet that points at the active file:

```json
{
  "mcpServers": {
    "engram-my-project": {
      "type": "stdio",
      "command": "/Applications/Engram.app/Contents/MacOS/engram-mcp",
      "args": ["--file", "/path/to/my-project/project.engram"]
    }
  }
}
```

Paste it into `claude_desktop_config.json`, a project `.mcp.json`, or the Cursor / Windsurf MCP settings. The button's menu also copies the equivalent `claude mcp add` command for Claude Code.

**Resources:** `engram://memory`, `engram://architecture`, `engram://decisions`, `engram://handoff`, `engram://prompts`, plus the templates `engram://memory/{key}` and `engram://decisions/{id}`.

**Tools:**

| Tool | Purpose |
| --- | --- |
| `build_context(task_description, max_tokens)` | Ranked XML context bundle within the budget |
| `upsert_memory(key, category, content, supersedes_key)` | Create or update memory; atomically supersede an older entry |
| `add_architecture_node(id, type, dependencies)` | Create or update a graph node |
| `log_decision(context, decision, rejected_alternatives)` | Append an ADR with the next sequential id |
| `deprecate_decision(id, superseded_by)` | Retire an ADR |
| `update_handoff(objective, progress, working_memory, next_step)` | Overwrite the handoff checkpoint |
| `audit_health(repo_root_path)` | Run Context Health |
| `get_prompt(name, variables)` / `save_prompt(name, body)` | Use and maintain the Prompt Vault |

Vault prompts are also exposed as MCP prompts. The CLI has a few utilities that are handy in scripts and CI:

```bash
engram-mcp init project.engram --name "My Project"
engram-mcp audit --file project.engram --repo .          # exits 1 if there are errors
engram-mcp context --file project.engram --task "add refunds" --max-tokens 4000
```

## How it fits together

- **One write path.** Every mutation, from the app (via the C ABI `engram_apply`) or from an agent (via MCP tools), is an `engram_core::ops::Op` executed inside `Store::transact`.
- **Safe concurrency.** A transaction takes an exclusive `flock` on a sidecar `.<name>.engram.lock`, re-reads the file, applies the change, validates it, and replaces the file atomically (temp file, fsync, rename). The app and any number of `engram-mcp` processes can write at the same time without losing updates.
- **Live reload.** The app watches the file with a `DispatchSource`, re-arming after each atomic rename. It reloads only when the bytes on disk differ from its own snapshot, so agent edits show up within about 100 ms, and the app's own saves don't cause a reload.
- **FFI contract.** Every C function returns a JSON envelope `{"ok":true,"data":…}` or `{"ok":false,"error":{"code","message"}}`. Strings are freed with `engram_string_free`, and panics are caught at the boundary. `EngramEngine.swift` wraps this API in typed Swift calls.

## The `.engram` format

Pretty-printed JSON with a fixed key order, two-space indentation and a trailing newline. Multi-line Markdown is stored as an array of lines, so editing one sentence produces a one-line diff:

```json
{
  "format": "engram",
  "version": 1,
  "revision": 12,
  "project": { "name": "Shop", "repo_root": ".", "created_at": "…", "updated_at": "…" },
  "memory": [
    {
      "key": "payments-idempotency",
      "category": "invariant",
      "status": "active",
      "content": [
        "Every payment mutation carries an idempotency key.",
        "Retries must never double-charge."
      ],
      "files": ["services/payments/charge.ts"],
      "created_at": "…",
      "updated_at": "…"
    }
  ],
  "architecture": { "nodes": [ … ] },
  "decisions": [ … ],
  "prompts": [ … ],
  "handoff": { … }
}
```

`revision` increases with every persisted change. Add `*.engram.lock` to your project's `.gitignore`.

## Keyboard

| Shortcut | Action |
| --- | --- |
| ⌘K | Command palette (commands, modules, every entry) |
| ⌘1 to ⌘7 | Switch module |
| ⌘S | Save pending edits (editors also autosave) |
| ⌘R | Reload from disk |
| ⇧⌘N / ⇧⌘D | New memory entry / log decision |
| ⇧⌘C | Copy context XML (Context Builder) or rendered prompt (Prompt Vault) |
| ⇧⌘M | Copy MCP config |
| ⇧⌘H | Run health audit |

Workspace windows support native macOS tabs; opening several `.engram` files groups them into one window.

## Tests

```bash
(cd rust-core && cargo test --workspace)   # core, MCP server, stdio end-to-end
./build.sh --native --debug --test         # also the Swift bridge tests
```
