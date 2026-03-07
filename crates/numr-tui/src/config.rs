//! Configuration file handling for numr TUI

use directories::ProjectDirs;
use numr_core::{FetchConfig, DEFAULT_CRYPTO_RATES_URL, DEFAULT_FIAT_RATES_URL};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::PathBuf;

/// Main configuration struct
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub preferences: Preferences,
    pub files: FilesConfig,
    pub api: ApiConfig,
}

/// Keybinding mode for the editor
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeybindingMode {
    #[default]
    Vim,
    Standard,
}

/// User preferences
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub keybinding_mode: KeybindingMode,
    pub wrap_mode: bool,
    pub show_line_numbers: bool,
    pub show_header: bool,
    pub debug_mode: bool,
}

/// File path configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct FilesConfig {
    /// Override default .numr file location
    pub default_path: Option<String>,
}

/// API configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ApiConfig {
    pub fiat_rates_url: String,
    pub crypto_rates_url: String,
    pub keys: ApiKeys,
}

/// API keys for premium services
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ApiKeys {
    pub coingecko_api_key: Option<String>,
}

impl Config {
    /// Get the config file path
    pub fn path() -> Option<PathBuf> {
        ProjectDirs::from("com", "numr", "numr").map(|dirs| dirs.config_dir().join("config.toml"))
    }

    /// Load config from file, or return defaults if not found.
    /// Returns (Config, Option<warning_message>).
    /// Warning is set if config file exists but has read/parse errors.
    pub fn load() -> (Self, Option<String>) {
        let Some(path) = Self::path() else {
            return (Self::default(), None);
        };

        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return (Self::default(), None); // File doesn't exist - normal case
            }
            Err(e) => {
                let warning = format!("Could not read config: {e}");
                return (Self::default(), Some(warning));
            }
        };

        match toml::from_str(&content) {
            Ok(config) => (config, None),
            Err(e) => {
                let warning = format!("Config parse error, using defaults: {e}");
                (Self::default(), Some(warning))
            }
        }
    }

    /// Save config to file
    pub fn save(&self) -> io::Result<()> {
        let path = Self::path().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "Could not determine config directory",
            )
        })?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self).map_err(io::Error::other)?;
        fs::write(path, content)
    }
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            fiat_rates_url: DEFAULT_FIAT_RATES_URL.to_string(),
            crypto_rates_url: DEFAULT_CRYPTO_RATES_URL.to_string(),
            keys: ApiKeys::default(),
        }
    }
}

impl From<&ApiConfig> for FetchConfig {
    fn from(config: &ApiConfig) -> Self {
        Self {
            fiat_rates_url: config.fiat_rates_url.clone(),
            crypto_rates_url: config.crypto_rates_url.clone(),
            coingecko_api_key: config.keys.coingecko_api_key.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialization_roundtrip() {
        let config = Config {
            preferences: Preferences {
                keybinding_mode: KeybindingMode::Standard,
                wrap_mode: true,
                show_line_numbers: true,
                show_header: true,
                debug_mode: true,
            },
            files: FilesConfig {
                default_path: Some("~/custom/path.numr".to_string()),
            },
            api: ApiConfig {
                fiat_rates_url: "https://example.com/fiat".to_string(),
                crypto_rates_url: "https://example.com/crypto".to_string(),
                keys: ApiKeys {
                    coingecko_api_key: Some("test-key".to_string()),
                },
            },
        };

        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(config, parsed);
    }

    #[test]
    fn test_partial_config_loading() {
        // Only preferences section - files and api should get defaults
        let toml_str = r#"
[preferences]
keybinding_mode = "standard"
wrap_mode = true
"#;
        let config: Config = toml::from_str(toml_str).unwrap();

        assert_eq!(config.preferences.keybinding_mode, KeybindingMode::Standard);
        assert!(config.preferences.wrap_mode);
        // Other preferences should be defaults
        assert!(!config.preferences.show_line_numbers);
        assert!(!config.preferences.show_header);
        assert!(!config.preferences.debug_mode);
        // Files should be default
        assert!(config.files.default_path.is_none());
        // Api should be default
        assert_eq!(config.api.fiat_rates_url, DEFAULT_FIAT_RATES_URL);
    }

    #[test]
    fn test_empty_config_uses_defaults() {
        let config: Config = toml::from_str("").unwrap();
        assert_eq!(config, Config::default());
    }

    #[test]
    fn test_default_values() {
        let config = Config::default();

        // Preferences defaults
        assert_eq!(config.preferences.keybinding_mode, KeybindingMode::Vim);
        assert!(!config.preferences.wrap_mode);
        assert!(!config.preferences.show_line_numbers);
        assert!(!config.preferences.show_header);
        assert!(!config.preferences.debug_mode);

        // Files defaults
        assert!(config.files.default_path.is_none());

        // Api defaults
        assert_eq!(config.api.fiat_rates_url, DEFAULT_FIAT_RATES_URL);
        assert_eq!(config.api.crypto_rates_url, DEFAULT_CRYPTO_RATES_URL);
        assert!(config.api.keys.coingecko_api_key.is_none());
    }

    #[test]
    fn test_api_config_converts_to_core_fetch_config() {
        let api = ApiConfig {
            fiat_rates_url: "https://example.com/fiat".to_string(),
            crypto_rates_url: "https://pro-api.coingecko.com/api/v3/simple/price".to_string(),
            keys: ApiKeys {
                coingecko_api_key: Some("test-key".to_string()),
            },
        };

        let fetch: FetchConfig = (&api).into();
        assert_eq!(fetch.fiat_rates_url, "https://example.com/fiat");
        assert_eq!(
            fetch.crypto_rates_url,
            "https://pro-api.coingecko.com/api/v3/simple/price"
        );
        assert_eq!(fetch.coingecko_api_key.as_deref(), Some("test-key"));
    }

    #[test]
    fn test_keybinding_mode_serialization() {
        // Test via Preferences struct (TOML requires struct wrapper)
        let prefs_vim = Preferences {
            keybinding_mode: KeybindingMode::Vim,
            ..Default::default()
        };
        let toml_str = toml::to_string(&prefs_vim).unwrap();
        assert!(toml_str.contains("keybinding_mode = \"vim\""));

        let prefs_standard = Preferences {
            keybinding_mode: KeybindingMode::Standard,
            ..Default::default()
        };
        let toml_str = toml::to_string(&prefs_standard).unwrap();
        assert!(toml_str.contains("keybinding_mode = \"standard\""));

        // Parse back
        let parsed: Preferences = toml::from_str("keybinding_mode = \"vim\"").unwrap();
        assert_eq!(parsed.keybinding_mode, KeybindingMode::Vim);

        let parsed: Preferences = toml::from_str("keybinding_mode = \"standard\"").unwrap();
        assert_eq!(parsed.keybinding_mode, KeybindingMode::Standard);
    }

    #[test]
    fn test_unknown_keybinding_mode_fails() {
        // Unknown mode should fail to parse (type safety)
        let result: Result<Preferences, _> = toml::from_str("keybinding_mode = \"emacs\"");
        assert!(result.is_err());
    }

    #[test]
    fn test_malformed_toml_returns_error() {
        let bad_toml = "this is not valid toml [[[";
        let result: Result<Config, _> = toml::from_str(bad_toml);
        assert!(result.is_err());
    }
}
