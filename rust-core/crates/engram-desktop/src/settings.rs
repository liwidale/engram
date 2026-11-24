use engram_core::client_config::McpClient;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};
use std::time::Duration;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    #[default]
    System,
    English,
    Russian,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Startup {
    #[default]
    Welcome,
    Restore,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeChoice {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum EditorMode {
    #[default]
    Edit,
    Split,
    Preview,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub language: Language,
    pub startup: Startup,
    pub autosave: bool,
    pub autosave_ms: u64,
    pub confirm_delete: bool,
    pub notify_external: bool,
    pub conflict_prompt: bool,
    pub theme: ThemeChoice,
    pub mono_font: Option<PathBuf>,
    pub mono_size: f32,
    pub soft_wrap: bool,
    pub memory_mode: EditorMode,
    pub map_grid: bool,
    pub map_paths: bool,
    pub map_curved: bool,
    pub mcp_binary: String,
    pub default_client: McpClient,
    pub handoff_author: String,
    pub context_budget: usize,
    pub context_handoff: bool,
    pub context_markdown: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            language: Language::System,
            startup: Startup::Welcome,
            autosave: true,
            autosave_ms: 900,
            confirm_delete: true,
            notify_external: true,
            conflict_prompt: true,
            theme: ThemeChoice::System,
            mono_font: None,
            mono_size: 13.0,
            soft_wrap: true,
            memory_mode: EditorMode::Edit,
            map_grid: true,
            map_paths: true,
            map_curved: true,
            mcp_binary: String::new(),
            default_client: McpClient::ClaudeCode,
            handoff_author: "human".into(),
            context_budget: 4000,
            context_handoff: true,
            context_markdown: false,
        }
    }
}

impl AppSettings {
    pub fn autosave_delay(&self) -> Duration {
        Duration::from_millis(self.autosave_ms.clamp(500, 3000))
    }

    pub fn author(&self) -> String {
        let a = self.handoff_author.trim();
        if a.is_empty() {
            "human".into()
        } else {
            a.to_owned()
        }
    }
}

fn store() -> &'static RwLock<AppSettings> {
    static SETTINGS: OnceLock<RwLock<AppSettings>> = OnceLock::new();
    SETTINGS.get_or_init(RwLock::default)
}

pub fn get() -> AppSettings {
    store().read().unwrap_or_else(|e| e.into_inner()).clone()
}

pub fn set(settings: AppSettings) {
    *store().write().unwrap_or_else(|e| e.into_inner()) = settings;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn older_settings_fill_in_defaults() {
        let s: AppSettings = serde_json::from_str(r#"{"autosave_ms": 1500, "theme": "Dark"}"#).unwrap();
        assert_eq!(s.autosave_ms, 1500);
        assert_eq!(s.theme, ThemeChoice::Dark);
        assert_eq!(s.handoff_author, "human");
        assert_eq!(s.autosave_delay(), Duration::from_millis(1500));
        assert_eq!(
            AppSettings { autosave_ms: 10, ..s }.autosave_delay(),
            Duration::from_millis(500)
        );
    }
}
