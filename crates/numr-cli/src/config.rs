//! Read the display preferences and `[api]` endpoints shared with the native TUI
//! configuration. Only the keys the CLI uses are parsed; everything else is ignored.
use directories::ProjectDirs;
use numr_core::{
    DecimalPlaces, DisplayOptions, FetchConfig, DEFAULT_CRYPTO_RATES_URL, DEFAULT_FIAT_RATES_URL,
};
use serde::Deserialize;
use std::{fs, io, path::Path};

#[derive(Default, Deserialize)]
#[serde(default)]
struct Config {
    preferences: Preferences,
    api: Api,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Preferences {
    decimals: Option<DecimalPlaces>,
}

#[derive(Deserialize)]
#[serde(default)]
struct Api {
    fiat_rates_url: String,
    fiat_rates_fallback_url: Option<String>,
    crypto_rates_url: String,
    keys: ApiKeys,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct ApiKeys {
    coingecko_api_key: Option<String>,
}

impl Default for Api {
    fn default() -> Self {
        Self {
            fiat_rates_url: DEFAULT_FIAT_RATES_URL.to_string(),
            fiat_rates_fallback_url: None,
            crypto_rates_url: DEFAULT_CRYPTO_RATES_URL.to_string(),
            keys: ApiKeys::default(),
        }
    }
}

/// Blank strings mean "not set", so a commented-out or emptied key never produces a bogus request.
fn non_blank(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// Settings the CLI and the JSON-RPC server take from `config.toml`.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Settings {
    pub display: DisplayOptions,
    pub fetch: FetchConfig,
}

/// Load the CLI settings without modifying the shared configuration file.
pub fn load_settings() -> (Settings, Option<String>) {
    let Some(dirs) = ProjectDirs::from("", "", "numr") else {
        return (Settings::default(), None);
    };
    load_from_path(&dirs.config_dir().join("config.toml"))
}

fn load_from_path(path: &Path) -> (Settings, Option<String>) {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return (Settings::default(), None)
        }
        Err(error) => {
            return (
                Settings::default(),
                Some(format!("Could not read config: {error}")),
            )
        }
    };
    match toml::from_str::<Config>(&content) {
        Ok(config) => (Settings::from(config), None),
        Err(error) => (
            Settings::default(),
            Some(format!("Config parse error, using defaults: {error}")),
        ),
    }
}

impl From<Config> for Settings {
    fn from(config: Config) -> Self {
        let api = config.api;
        Self {
            display: DisplayOptions {
                decimals: config.preferences.decimals,
            },
            fetch: FetchConfig {
                fiat_rates_url: non_blank(Some(api.fiat_rates_url))
                    .unwrap_or_else(|| DEFAULT_FIAT_RATES_URL.to_string()),
                fiat_rates_fallback_url: non_blank(api.fiat_rates_fallback_url),
                crypto_rates_url: non_blank(Some(api.crypto_rates_url))
                    .unwrap_or_else(|| DEFAULT_CRYPTO_RATES_URL.to_string()),
                coingecko_api_key: non_blank(api.keys.coingecko_api_key),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_partial_and_legacy_configs() {
        for (input, expected) in [
            ("", None),
            ("[preferences]\nwrap_mode = true", None),
            ("[preferences]\ndecimals = 4", Some(4)),
            ("[preferences]\ndecimals = 0", Some(0)),
            ("[preferences]\ndecimals = 28", Some(28)),
        ] {
            let config: Config = toml::from_str(input).unwrap();
            assert_eq!(
                config.preferences.decimals.map(DecimalPlaces::get),
                expected
            );
        }
    }

    #[test]
    fn api_section_maps_to_fetch_config_with_defaults_for_missing_keys() {
        let config: Config = toml::from_str(
            "[api]\nfiat_rates_url = \"https://example.com/fiat\"\nfiat_rates_fallback_url = \"https://example.com/fallback\"\n[api.keys]\ncoingecko_api_key = \"k\"\n",
        )
        .unwrap();
        let settings = Settings::from(config);
        assert_eq!(settings.fetch.fiat_rates_url, "https://example.com/fiat");
        assert_eq!(
            settings.fetch.fiat_rates_fallback_url.as_deref(),
            Some("https://example.com/fallback")
        );
        assert_eq!(settings.fetch.crypto_rates_url, DEFAULT_CRYPTO_RATES_URL);
        assert_eq!(settings.fetch.coingecko_api_key.as_deref(), Some("k"));

        for input in [
            "",
            "[preferences]\ndecimals = 2",
            "[api]\nfiat_rates_fallback_url = \"  \"\nfiat_rates_url = \"\"",
        ] {
            let settings = Settings::from(toml::from_str::<Config>(input).unwrap());
            assert_eq!(settings.fetch, FetchConfig::default(), "{input:?}");
        }
    }

    #[test]
    fn rejects_invalid_decimals() {
        for value in ["-1", "29", "1.5", "\"4\""] {
            assert!(
                toml::from_str::<Config>(&format!("[preferences]\ndecimals = {value}")).is_err()
            );
        }
    }
}
