use serde_json::{json, Value};
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

fn shell_quote(s: &str, windows: bool) -> String {
    if s.chars().all(|c| c.is_ascii_alphanumeric() || "/\\._-+:@".contains(c)) {
        s.to_owned()
    } else if windows {
        format!("\"{}\"", s.replace('"', "\\\""))
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

pub fn mcp_config(project_name: &str, file: &Path, binary: &str) -> Value {
    mcp_config_for(project_name, file, binary, cfg!(windows))
}

fn mcp_config_for(project_name: &str, file: &Path, binary: &str, windows: bool) -> Value {
    let file = display_path(&std::fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf()));
    let name = server_name(project_name);
    let config = json!({
        "mcpServers": {
            name.clone(): { "type": "stdio", "command": binary, "args": ["--file", file] }
        }
    });
    json!({
        "server_name": name,
        "json": serde_json::to_string_pretty(&config).unwrap_or_default(),
        "claude_code_command": format!(
            "claude mcp add {name} -- {} --file {}",
            shell_quote(binary, windows),
            shell_quote(&file, windows)
        ),
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
        let unix = mcp_config_for("My App", Path::new("/tmp/My App.engram"), "/opt/engram-mcp", false);
        assert_eq!(unix["server_name"], "engram-my-app");
        assert!(unix["claude_code_command"]
            .as_str()
            .unwrap()
            .ends_with("--file '/tmp/My App.engram'"));
        let win = mcp_config_for(
            "x",
            Path::new(r"C:\Users\Me\My App.engram"),
            r"C:\Program Files\Engram\engram-mcp.exe",
            true,
        );
        let cmd = win["claude_code_command"].as_str().unwrap();
        assert!(
            cmd.contains(r#""C:\Program Files\Engram\engram-mcp.exe" --file "C:\Users\Me\My App.engram""#),
            "{cmd}"
        );
        assert_eq!(server_name("!!!"), "engram");
    }
}
