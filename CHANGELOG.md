# Changelog

## 0.1.0

### Added

- **Project memory in one file.** Rules, tech stack, architecture invariants and conventions live in a `.engram` file next to your code, versioned with Git.
- **Seven modules.** Project memory, architecture map, decision log, prompt vault, context builder, context health and agent handoff.
- **MCP server.** `engram-mcp` gives Claude Code, Cursor, Windsurf and any other MCP client read and write access to the file, with 5 resources and 9 tools.
- **Context builder.** Ranks memory, decisions and architecture against a task and packs the most relevant items into a token budget, exported as XML.
- **Context health.** Finds broken file paths, references to deleted entries, deprecated decisions still in use and rules that contradict each other.
- **Live reload.** When an agent writes to the file, the open window updates within a fraction of a second.
- **Apps for every desktop.** A native SwiftUI app for macOS 14 and later, and a native app for Windows 10 and later and for Linux.
