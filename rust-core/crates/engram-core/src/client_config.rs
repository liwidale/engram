use crate::error::{EngramError, Result};
use crate::schema::EngramDocument;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

pub fn home_dir() -> Option<PathBuf> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var).filter(|h| !h.is_empty()).map(PathBuf::from)
}

pub fn display_path(path: &Path) -> String {
    let s = path.to_string_lossy();
    match s.strip_prefix(r"\\?\UNC\") {
        Some(rest) => format!(r"\\{rest}"),
        None => s.strip_prefix(r"\\?\").unwrap_or(&s).to_owned(),
    }
}

pub fn server_name(project_name: &str) -> String {
    let slug = project_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        "engram".to_owned()
    } else {
        format!("engram-{slug}")
    }
}

pub fn server_name_for(doc: &EngramDocument) -> String {
    doc.project
        .settings
        .mcp_server_name
        .clone()
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| server_name(&doc.project.name))
}

fn shell_quote(s: &str, windows: bool) -> String {
    if s.chars().all(|c| c.is_ascii_alphanumeric() || "/\\._-+:@".contains(c)) {
        s.to_owned()
    } else if windows {
        format!("\"{}\"", s.replace('"', "\\\""))
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpClient {
    ClaudeCode,
    ClaudeDesktop,
    Cursor,
    Windsurf,
    VsCode,
    Generic,
}

impl McpClient {
    pub const ALL: [McpClient; 6] = [
        McpClient::ClaudeCode,
        McpClient::ClaudeDesktop,
        McpClient::Cursor,
        McpClient::Windsurf,
        McpClient::VsCode,
        McpClient::Generic,
    ];

    pub fn title(self) -> &'static str {
        match self {
            McpClient::ClaudeCode => "Claude Code",
            McpClient::ClaudeDesktop => "Claude Desktop",
            McpClient::Cursor => "Cursor",
            McpClient::Windsurf => "Windsurf",
            McpClient::VsCode => "VS Code",
            McpClient::Generic => "Custom JSON",
        }
    }

    fn servers_key(self) -> &'static str {
        match self {
            McpClient::VsCode => "servers",
            _ => "mcpServers",
        }
    }

    fn typed(self) -> bool {
        matches!(self, McpClient::ClaudeCode | McpClient::VsCode | McpClient::Generic)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ClientConfig {
    pub client: McpClient,
    pub title: &'static str,
    pub server_name: String,
    pub entry: Value,
    pub snippet: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    pub project_scoped: bool,
}

fn user_config(client: McpClient) -> Option<PathBuf> {
    let home = home_dir()?;
    match client {
        McpClient::ClaudeDesktop => Some(if cfg!(target_os = "macos") {
            home.join("Library/Application Support/Claude/claude_desktop_config.json")
        } else if cfg!(windows) {
            std::env::var_os("APPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join("AppData").join("Roaming"))
                .join("Claude")
                .join("claude_desktop_config.json")
        } else {
            home.join(".config/Claude/claude_desktop_config.json")
        }),
        McpClient::Windsurf => Some(home.join(".codeium").join("windsurf").join("mcp_config.json")),
        _ => None,
    }
}

pub fn client_config(client: McpClient, name: &str, binary: &str, file: &Path, repo_root: &Path) -> ClientConfig {
    client_config_for(client, name, binary, file, repo_root, cfg!(windows))
}

fn client_config_for(
    client: McpClient,
    name: &str,
    binary: &str,
    file: &Path,
    repo_root: &Path,
    windows: bool,
) -> ClientConfig {
    let file = display_path(&std::fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf()));
    let mut entry = json!({ "command": binary, "args": ["--file", file] });
    if client.typed() {
        entry["type"] = json!("stdio");
    }
    let snippet = serde_json::to_string_pretty(&json!({ client.servers_key(): { name: entry } })).unwrap_or_default();
    let target = match client {
        McpClient::ClaudeCode => Some(repo_root.join(".mcp.json")),
        McpClient::Cursor => Some(repo_root.join(".cursor").join("mcp.json")),
        McpClient::VsCode => Some(repo_root.join(".vscode").join("mcp.json")),
        McpClient::ClaudeDesktop | McpClient::Windsurf => user_config(client),
        McpClient::Generic => None,
    };
    ClientConfig {
        client,
        title: client.title(),
        server_name: name.to_owned(),
        entry,
        snippet,
        command: (client == McpClient::ClaudeCode).then(|| {
            format!(
                "claude mcp add {name} -- {} --file {}",
                shell_quote(binary, windows),
                shell_quote(&file, windows)
            )
        }),
        target: target.as_deref().map(display_path),
        project_scoped: matches!(client, McpClient::ClaudeCode | McpClient::Cursor | McpClient::VsCode),
    }
}

pub fn client_configs(doc: &EngramDocument, binary: &str, file: &Path, repo_root: &Path) -> Vec<ClientConfig> {
    let name = server_name_for(doc);
    McpClient::ALL
        .iter()
        .map(|c| client_config(*c, &name, binary, file, repo_root))
        .collect()
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct WriteOutcome {
    pub path: String,
    pub created: bool,
    pub replaced: bool,
}

pub fn write_client_config(config: &ClientConfig) -> Result<WriteOutcome> {
    let Some(target) = config.target.as_deref().map(PathBuf::from) else {
        return Err(EngramError::Invalid(format!(
            "{} has no config file to write; copy the snippet instead",
            config.title
        )));
    };
    let existing = match std::fs::read_to_string(&target) {
        Ok(text) if text.trim().is_empty() => None,
        Ok(text) => Some(serde_json::from_str::<Value>(&text).map_err(|e| {
            EngramError::Invalid(format!(
                "{} is not valid JSON ({e}); fix or remove it first",
                display_path(&target)
            ))
        })?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(EngramError::io(&target, e)),
    };
    let created = existing.is_none();
    let mut root = match existing {
        Some(Value::Object(o)) => o,
        Some(_) => {
            return Err(EngramError::Invalid(format!(
                "{} does not contain a JSON object",
                display_path(&target)
            )))
        }
        None => Map::new(),
    };
    let servers = root
        .entry(config.client.servers_key())
        .or_insert_with(|| Value::Object(Map::new()));
    let Value::Object(servers) = servers else {
        return Err(EngramError::Invalid(format!(
            "\"{}\" in {} is not an object",
            config.client.servers_key(),
            display_path(&target)
        )));
    };
    let replaced = servers
        .insert(config.server_name.clone(), config.entry.clone())
        .is_some();
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir).map_err(|e| EngramError::io(dir, e))?;
    }
    let mut bytes = serde_json::to_vec_pretty(&Value::Object(root))?;
    bytes.push(b'\n');
    let tmp = target.with_extension("json.engram-tmp");
    std::fs::write(&tmp, &bytes).map_err(|e| EngramError::io(&tmp, e))?;
    std::fs::rename(&tmp, &target).map_err(|e| EngramError::io(&target, e))?;
    Ok(WriteOutcome {
        path: display_path(&target),
        created,
        replaced,
    })
}

pub fn mcp_config(name: &str, file: &Path, binary: &str) -> Value {
    let base = file.parent().unwrap_or(Path::new("."));
    let c = client_config(McpClient::ClaudeCode, name, binary, file, base);
    json!({
        "server_name": c.server_name,
        "json": serde_json::to_string_pretty(&json!({ "mcpServers": { name: c.entry } })).unwrap_or_default(),
        "claude_code_command": c.command,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_verbatim_prefixes() {
        assert_eq!(display_path(Path::new(r"\\?\C:\work\p.engram")), r"C:\work\p.engram");
        assert_eq!(
            display_path(Path::new(r"\\?\UNC\server\share\p.engram")),
            r"\\server\share\p.engram"
        );
        assert_eq!(display_path(Path::new("/Users/me/p.engram")), "/Users/me/p.engram");
    }

    #[test]
    fn quotes_for_each_shell() {
        let root = Path::new("/tmp");
        let unix = client_config_for(
            McpClient::ClaudeCode,
            "engram-my-app",
            "/opt/engram-mcp",
            Path::new("/tmp/My App.engram"),
            root,
            false,
        );
        assert!(unix.command.unwrap().ends_with("--file '/tmp/My App.engram'"));
        let win = client_config_for(
            McpClient::ClaudeCode,
            "x",
            r"C:\Program Files\Engram\engram-mcp.exe",
            Path::new(r"C:\Users\Me\My App.engram"),
            root,
            true,
        );
        let cmd = win.command.unwrap();
        assert!(
            cmd.contains(r#""C:\Program Files\Engram\engram-mcp.exe" --file "C:\Users\Me\My App.engram""#),
            "{cmd}"
        );
        assert_eq!(server_name("!!!"), "engram");
    }

    #[test]
    fn every_client_gets_the_right_shape() {
        let mut doc = EngramDocument::new("My App");
        let root = Path::new("/repo");
        let configs = client_configs(&doc, "/bin/engram-mcp", Path::new("/repo/p.engram"), root);
        assert_eq!(configs.len(), McpClient::ALL.len());
        let get = |c: McpClient| configs.iter().find(|x| x.client == c).unwrap();
        assert!(get(McpClient::VsCode).snippet.contains("\"servers\""));
        assert!(get(McpClient::Cursor).snippet.contains("\"mcpServers\""));
        let cursor = display_path(&root.join(".cursor").join("mcp.json"));
        assert_eq!(get(McpClient::Cursor).target.as_deref(), Some(cursor.as_str()));
        assert_eq!(get(McpClient::ClaudeDesktop).entry.get("type"), None);
        assert_eq!(get(McpClient::VsCode).entry["type"], "stdio");
        assert!(get(McpClient::Generic).target.is_none());
        assert_eq!(get(McpClient::ClaudeCode).server_name, "engram-my-app");
        doc.project.settings.mcp_server_name = Some("memory".into());
        let configs = client_configs(&doc, "/bin/engram-mcp", Path::new("/repo/p.engram"), root);
        assert!(configs.iter().all(|c| c.server_name == "memory"));
    }

    #[test]
    fn writes_and_merges_config_files() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join(".cursor/mcp.json");
        let config = client_config(
            McpClient::Cursor,
            "engram-x",
            "/bin/engram-mcp",
            Path::new("/p.engram"),
            dir.path(),
        );
        let first = write_client_config(&config).unwrap();
        assert!(first.created && !first.replaced);

        let mut existing: Value = serde_json::from_str(&std::fs::read_to_string(&target).unwrap()).unwrap();
        existing["mcpServers"]["other"] = json!({"command": "other"});
        existing["theme"] = json!("dark");
        std::fs::write(&target, existing.to_string()).unwrap();
        let second = write_client_config(&config).unwrap();
        assert!(!second.created && second.replaced);
        let merged: Value = serde_json::from_str(&std::fs::read_to_string(&target).unwrap()).unwrap();
        assert_eq!(merged["theme"], "dark");
        assert_eq!(merged["mcpServers"]["other"]["command"], "other");
        assert_eq!(merged["mcpServers"]["engram-x"]["args"][0], "--file");

        std::fs::write(&target, "{ not json").unwrap();
        assert_eq!(write_client_config(&config).unwrap_err().code(), "invalid");
        assert_eq!(
            std::fs::read_to_string(&target).unwrap(),
            "{ not json",
            "broken files are left untouched"
        );
    }
}
