use std::fs;
use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Settings {
    pub(super) title_max: usize,
    pub(super) alias_max: usize,
    pub(super) content_max: usize,
    pub(super) timeout_secs: u64,
    pub(super) socks: String,
    pub(super) syntax_theme: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            title_max: 128,
            alias_max: 32,
            content_max: 128_000,
            timeout_secs: 3,
            socks: String::new(),
            syntax_theme: "base16-ocean.dark".to_string(),
        }
    }
}

impl Settings {
    pub(super) fn load() -> Result<Self, String> {
        Self::load_from(Path::new("Config.toml"))
    }

    pub(super) fn load_from(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Ok(Settings::default());
        }
        let content =
            fs::read_to_string(path).map_err(|e| format!("Failed to read Config.toml: {e}"))?;
        let file: File =
            toml::from_str(&content).map_err(|e| format!("Failed to parse Config.toml: {e}"))?;
        Ok(file.into())
    }

    pub(super) fn timeout(&self) -> Duration {
        Duration::from_secs(self.timeout_secs.max(1))
    }

    /// `Ok(None)` connects directly. `Err` means `socks` was set but is not a socket address.
    pub(super) fn socks_addr(&self) -> Result<Option<SocketAddr>, ()> {
        let trimmed = self.socks.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        match trimmed.parse() {
            Ok(addr) => Ok(Some(addr)),
            Err(_) => Err(()),
        }
    }
}

#[derive(Deserialize)]
struct File {
    #[serde(default)]
    limits: LimitsFile,
    #[serde(default)]
    theme: ThemeFile,
    #[serde(default)]
    nostr: NostrFile,
}

#[derive(Deserialize)]
struct LimitsFile {
    #[serde(default = "default_title_max")]
    title_max_length: usize,
    #[serde(default = "default_alias_max")]
    alias_max_length: usize,
    #[serde(default = "default_content_max")]
    content_max_length: usize,
}

#[derive(Deserialize)]
struct ThemeFile {
    #[serde(default = "default_syntax_theme")]
    syntax_highlighting: String,
}

#[derive(Deserialize)]
struct NostrFile {
    #[serde(default = "default_timeout_secs")]
    timeout_secs: u64,
    #[serde(default)]
    socks: String,
}

impl Default for LimitsFile {
    fn default() -> Self {
        Self {
            title_max_length: default_title_max(),
            alias_max_length: default_alias_max(),
            content_max_length: default_content_max(),
        }
    }
}

impl Default for ThemeFile {
    fn default() -> Self {
        Self {
            syntax_highlighting: default_syntax_theme(),
        }
    }
}

impl Default for NostrFile {
    fn default() -> Self {
        Self {
            timeout_secs: default_timeout_secs(),
            socks: String::new(),
        }
    }
}

impl From<File> for Settings {
    fn from(file: File) -> Self {
        Self {
            title_max: file.limits.title_max_length,
            alias_max: file.limits.alias_max_length,
            content_max: file.limits.content_max_length,
            timeout_secs: file.nostr.timeout_secs,
            socks: file.nostr.socks,
            syntax_theme: file.theme.syntax_highlighting,
        }
    }
}

fn default_title_max() -> usize {
    128
}

fn default_alias_max() -> usize {
    32
}

fn default_content_max() -> usize {
    128_000
}

fn default_timeout_secs() -> u64 {
    3
}

fn default_syntax_theme() -> String {
    "base16-ocean.dark".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_uses_the_same_defaults() {
        let dir = tempfile::tempdir().expect("temp dir");
        let settings = Settings::load_from(&dir.path().join("Config.toml")).expect("defaults");
        assert_eq!(settings, Settings::default());
        assert_eq!(settings.timeout(), Duration::from_secs(3));
        assert_eq!(settings.socks_addr(), Ok(None));
    }

    #[test]
    fn the_file_supplies_socks_limits_timeout_and_theme() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("Config.toml");
        std::fs::write(
            &path,
            r#"
[server]
port = 8009

[limits]
title_max_length = 7
alias_max_length = 5
content_max_length = 11

[theme]
syntax_highlighting = "InspiredGitHub"

[nostr]
timeout_secs = 30
socks = "127.0.0.1:9050"
relays = ["wss://relay.example"]
"#,
        )
        .expect("write");
        let settings = Settings::load_from(&path).expect("settings");
        assert_eq!(settings.title_max, 7);
        assert_eq!(settings.alias_max, 5);
        assert_eq!(settings.content_max, 11);
        assert_eq!(settings.timeout_secs, 30);
        assert_eq!(settings.syntax_theme, "InspiredGitHub");
        assert_eq!(
            settings.socks_addr(),
            Ok(Some("127.0.0.1:9050".parse().unwrap()))
        );
    }

    #[test]
    fn a_blank_section_keeps_that_default() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("Config.toml");
        std::fs::write(&path, "[nostr]\nsocks = \"\"\n").expect("write");
        let settings = Settings::load_from(&path).expect("settings");
        assert_eq!(settings.title_max, 128);
        assert_eq!(settings.timeout_secs, 3);
        assert_eq!(settings.syntax_theme, "base16-ocean.dark");
        assert_eq!(settings.socks_addr(), Ok(None));
    }
}
