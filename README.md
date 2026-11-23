<p align="center">
  <img src=".github/assets/icon.png" width="96" height="96" alt="Engram">
</p>

<h1 align="center">Engram</h1>

<p align="center">
  Project memory for you and your AI agents. Rules, architecture, decisions and the current state of work live in one file next to your code, so every agent session starts with the right context.
</p>

<p align="center">
  <a href="https://github.com/liwidale/engram/releases/latest"><img src="https://img.shields.io/github/v/release/liwidale/engram?label=download&color=4c9dff" alt="Download"></a>
  <a href="https://github.com/liwidale/engram/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/liwidale/engram/ci.yml?label=tests" alt="Tests"></a>
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-lightgrey" alt="macOS, Windows and Linux">
  <a href="LICENSE"><img src="https://img.shields.io/github/license/liwidale/engram?color=3ad37e" alt="MIT License"></a>
</p>

<p align="center">English · <a href="README.ru.md">Русский</a></p>

AI agents forget. Each new session loses the conventions you agreed on, re-proposes approaches you already rejected and summarizes its own progress in ways that drift a little further every time. Engram keeps that knowledge in a structured, versioned `.engram` file. You edit it in a fast desktop app, and agents read and update it through the Model Context Protocol, so both of you work from the same source of truth.

## Contents

- [Download](#download)
- [What you get](#what-you-get)
- [Getting started](#getting-started)
- [Everyday use](#everyday-use)
- [Configuration](#configuration)
- [Privacy](#privacy)
- [Troubleshooting](#troubleshooting)
- [Building from source](#building-from-source)
- [How it works](#how-it-works)
- [Project structure](#project-structure)
- [Contributing](#contributing)
- [License](#license)

## Download

Get the latest version from the [releases page](https://github.com/liwidale/engram/releases/latest).

| Platform | File | Requirements |
| --- | --- | --- |
| macOS | `Engram-<version>-macos-universal.zip` | macOS 14 or later, Apple silicon or Intel |
| Windows | `Engram-<version>-windows-x64-setup.exe` or the portable `.zip` | Windows 10 or later, x64 |
| Linux | `engram-<version>-linux-x86_64.tar.gz` | glibc 2.35 or later, X11 or Wayland |

- **macOS:** unzip and move `Engram.app` to `Applications`.
- **Windows:** run the installer. It installs for your user account and registers the `.engram` file type.
- **Linux:** unpack the archive and run `./install.sh`. It installs to `~/.local` and registers the `.engram` file type; pass `--prefix /usr/local` with `sudo` for a system-wide install.

Every download includes `engram-mcp`, the MCP server agents connect to.

## What you get

- **Project memory.** Rules, tech stack, architecture invariants and conventions, written in Markdown with a live preview. Replace an entry with a new one and the old one is tagged `[SUPERSEDED]`, kept for history and never shown to agents again.
- **Architecture map.** Modules, services and databases with their dependencies, drawn as an interactive graph. Engram lays it out automatically, highlights dependency cycles and remembers where you drag nodes.
- **Decision log.** Architecture decision records with the context, the decision and every alternative you rejected with the reason. Agents see rejected options as dead ends and stop proposing them.
- **Prompt vault.** Reusable prompts for this project with `{{variable}}` placeholders. Copy one with its variables filled in, or let an agent fetch it.
- **Context builder.** Describe a task and Engram ranks memory, decisions and architecture by relevance, then packs the best items into your token budget. You can force or exclude any item, and copy the result as XML.
- **Context health.** An audit of the file against your codebase. It finds broken file paths, references to deleted entries, deprecated decisions still in use, rules that contradict each other and memory that recommends something a decision rejected.
- **Agent handoff.** A structured checkpoint with the active objective, verified progress with file paths, working memory and the next atomic step. It replaces lossy chat summaries between sessions.
- **Live reload.** When an agent changes the file, the open window updates within a fraction of a second.
- **Keyboard first.** A command palette searches every command and entry, and every module has a shortcut.

## Getting started

1. Open Engram and choose **New project memory**. Save the file in the root of your repository, for example `project.engram`.
2. Add a few memory entries: the stack, the rules every change must follow and anything agents keep getting wrong.
3. Click **Copy MCP config** in the toolbar and paste the snippet into your agent's MCP settings (see [Configuration](#configuration)).
4. Start an agent session. It reads `engram://handoff`, calls `build_context` with its task and records new decisions as it works.

You can also create a file from the command line:

```bash
engram-mcp init project.engram --name "My project"
```

## Everyday use

| Shortcut on macOS | On Windows and Linux | Action |
| --- | --- | --- |
| `⌘K` | `Ctrl+K` | Command palette |
| `⌘1` to `⌘7` | `Ctrl+1` to `Ctrl+7` | Switch module |
| `⌘S` | `Ctrl+S` | Save pending edits (editors also save on their own) |
| `⌘R` | `Ctrl+R` | Reload from disk |
| `⇧⌘N` | `Ctrl+Shift+N` | New memory entry |
| `⇧⌘D` | `Ctrl+Shift+D` | Log decision |
| `⇧⌘H` | `Ctrl+Shift+H` | Run health audit |
| `⇧⌘M` | `Ctrl+Shift+M` | Copy MCP config |

- **Open several projects.** Each file opens in its own tab. On Windows and Linux you can also drop `.engram` files onto the window.
- **Keep the file in Git.** The format is plain JSON with one Markdown line per array item, so diffs and merges stay readable. Add `*.engram.lock` to your `.gitignore`.
- **Check health in CI.** `engram-mcp audit --file project.engram` exits with code 1 when the audit finds errors.

## Configuration

### Connecting an agent

**Copy MCP config** produces a snippet like this, already pointing at the installed `engram-mcp` and the open file:

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

Paste it into `claude_desktop_config.json`, a project `.mcp.json` or the MCP settings of Cursor or Windsurf. For Claude Code, the same menu copies a ready `claude mcp add` command.

The server exposes these resources and tools:

| Resources | Tools |
| --- | --- |
| `engram://memory` | `build_context(task_description, max_tokens)` |
| `engram://architecture` | `upsert_memory(key, category, content, supersedes_key)` |
| `engram://decisions` | `add_architecture_node(id, type, dependencies)` |
| `engram://handoff` | `log_decision(context, decision, rejected_alternatives)` |
| `engram://prompts` | `update_handoff(objective, progress, working_memory, next_step)` |
| `engram://memory/{key}` | `audit_health(repo_root_path)` |
| `engram://decisions/{id}` | `get_prompt(name)`, `save_prompt(name, body)`, `deprecate_decision(id)` |

Prompts from the vault are also available as MCP prompts.

### Linked codebase

Context health checks paths against the folder that contains the `.engram` file. If your code lives elsewhere, choose **Change folder** in Context health. The path is stored relative to the file when possible, so the file stays portable.

### Language

The interface is in English. Every string is ready for translation. On Windows and Linux you can force a language with the `ENGRAM_LANG` environment variable.

## Privacy

Engram works entirely on your computer. It has no accounts, no telemetry, no analytics and no network access. `engram-mcp` reads and writes only the file you pass to it, and the health audit only reads file names inside the linked folder.

## Troubleshooting

- **macOS says the app cannot be opened.** Builds are signed ad hoc, not notarized. Right-click `Engram.app`, choose **Open**, then confirm.
- **The agent does not see the server.** Check that the `command` path in the config exists and that the agent was restarted after you changed its settings. Run `engram-mcp --file <path>` in a terminal; it should print the project name and wait for input.
- **File dialogs do not open on Linux.** Engram uses the XDG desktop portal. Install `xdg-desktop-portal` and the portal for your desktop, for example `xdg-desktop-portal-gtk` or `xdg-desktop-portal-kde`.
- **Health reports a missing codebase folder.** The linked folder was moved or renamed. Choose **Change folder** in Context health.
- **Edits from an agent do not appear.** Make sure the agent and the app point at the same file. **Reload from disk** forces a refresh.

## Building from source

You need [Rust](https://rustup.rs) 1.89 or later.

**macOS** (Xcode or the Command Line Tools):

```bash
./build.sh                    # universal app in build/Engram.app
./build.sh --native --debug   # faster build for the current architecture
./build.sh --test             # also run the Rust and Swift tests
./build.sh --install          # copy to /Applications and register the file type
```

**Windows and Linux:**

```bash
cd rust-core
cargo build --release -p engram-desktop -p engram-mcp
```

The binaries are `target/release/engram` and `target/release/engram-mcp`. On Linux, `packaging/linux/package.sh <version>` builds the same archive the release uses.

Run the tests on any platform with `cargo test --workspace` inside `rust-core`. The logo, the wordmark and every platform icon are generated by `swift tools/make-icons.swift` on macOS.

## How it works

- **One engine.** `engram-core` is a Rust library that owns the file format, the dependency graph, the context builder and the health audit. The macOS app links it as a static library through a C interface, the Windows and Linux app uses it directly, and `engram-mcp` is built from the same code.
- **One write path.** Every change, from the app or from an agent, is an operation applied inside a transaction: lock the file, read it, apply the change, validate it and replace the file atomically. Several apps and agents can write at the same time without losing each other's updates.
- **Live updates.** The macOS app watches the file with a kernel event source, and the Windows and Linux app compares a fingerprint of the file twice a second. Either way, the window refreshes only when the bytes on disk differ from what it last saw.
- **Ranking.** The context builder scores items with BM25, raises items connected to matching architecture nodes, weights invariants and rules above notes and fills the token budget with the highest-scoring items. Token counts use the `o200k_base` encoding.

The file itself is readable JSON:

```json
{
  "format": "engram",
  "version": 1,
  "revision": 12,
  "project": { "name": "Shop", "created_at": "2026-11-23T08:15:17Z", "updated_at": "2026-11-23T09:02:41Z" },
  "memory": [
    {
      "key": "payments-idempotency",
      "category": "invariant",
      "status": "active",
      "content": [
        "Every payment mutation carries an idempotency key.",
        "Retries must never charge a customer twice."
      ],
      "files": ["services/payments/charge.ts"],
      "created_at": "2026-11-23T08:15:17Z",
      "updated_at": "2026-11-23T08:15:17Z"
    }
  ],
  "architecture": { "nodes": [] },
  "decisions": [],
  "prompts": [],
  "handoff": {}
}
```

## Project structure

```
rust-core/
  crates/engram-core/       File format, graph, context builder, health audit, C interface
  crates/engram-mcp/        MCP server and command-line tools
  crates/engram-desktop/    App for Windows and Linux
EngramApp/                  App for macOS (SwiftUI)
packaging/                  Windows installer and Linux desktop integration
branding/                   Logo, wordmark and generated icons
tools/                      Icon generator
project.engram              Engram's own project memory
```

## Contributing

Issues and pull requests are welcome.

- Run `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` before you open a pull request.
- Changes to the file format need a migration path for existing files.
- Every change to `.engram` files goes through an operation in `engram-core`; the apps and the MCP server never write the file directly.
- User-facing text goes through the translation layer: `String(localized:)` in Swift and `tr()` in Rust.

## License

[MIT](LICENSE) © 2026 Liwidale. Claude, Cursor and Windsurf are trademarks of their respective owners; Engram is not affiliated with them.

<p align="center"><sub>ゝ made by <a href="https://github.com/liwidale">liwidale</a></sub></p>
