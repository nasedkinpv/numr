//! Read the display preferences shared with the native TUI configuration.
use directories::ProjectDirs;
use numr_core::{DecimalPlaces, DisplayOptions};
use serde::Deserialize;
use std::{fs, io, path::Path};

#[derive(Default, Deserialize)]
#[serde(default)]
struct Config {
    preferences: Preferences,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Preferences {
    decimals: Option<DecimalPlaces>,
}

/// Load display settings without modifying the shared configuration file.
pub fn load_display_options() -> (DisplayOptions, Option<String>) {
    let Some(dirs) = ProjectDirs::from("", "", "numr") else {
        return (DisplayOptions::default(), None);
    };
    load_from_path(&dirs.config_dir().join("config.toml"))
}

fn load_from_path(path: &Path) -> (DisplayOptions, Option<String>) {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return (DisplayOptions::default(), None)
        }
        Err(error) => {
            return (
                DisplayOptions::default(),
                Some(format!("Could not read config: {error}")),
            )
        }
    };
    match toml::from_str::<Config>(&content) {
        Ok(config) => (
            DisplayOptions {
                decimals: config.preferences.decimals,
            },
            None,
        ),
        Err(error) => (
            DisplayOptions::default(),
            Some(format!("Config parse error, using defaults: {error}")),
        ),
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
    fn rejects_invalid_decimals() {
        for value in ["-1", "29", "1.5", "\"4\""] {
            assert!(
                toml::from_str::<Config>(&format!("[preferences]\ndecimals = {value}")).is_err()
            );
        }
    }
}
