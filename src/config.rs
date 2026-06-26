// `wb config` — a small persistent key/value store for machine-wide defaults.
//
// Stored as a flat YAML map at `~/.wb/config.yaml` (overridable with
// `$WB_CONFIG_PATH`, which the tests use). Reusing serde_yaml keeps the binary
// dependency-free; the file is the same flavor of YAML as workbook frontmatter.
//
// Keys are an allowlist (`KNOWN_KEYS`) so a typo is rejected at `set` time
// rather than silently stored and ignored. Every listed key is actually
// consumed somewhere in the run path — there are no decorative settings.
//
// Precedence wherever a config value is consulted: CLI flag > env var > config.

use std::collections::BTreeMap;
use std::path::PathBuf;

/// Allowlisted config keys and what each one does. Adding a key here is a
/// promise that the run path reads it (see the call sites in `main.rs`).
pub const KNOWN_KEYS: &[(&str, &str)] = &[
    (
        "callback.url",
        "default --callback endpoint (http/https/redis); used when no flag or WB_CALLBACK_URL is set",
    ),
    (
        "callback.secret",
        "default HMAC secret for signing HTTP callbacks (fallback for --callback-secret / WB_CALLBACK_SECRET)",
    ),
    (
        "callback.key",
        "default Redis stream key for callbacks (fallback for --callback-key / WB_CALLBACK_KEY)",
    ),
];

pub fn is_known_key(key: &str) -> bool {
    KNOWN_KEYS.iter().any(|(k, _)| *k == key)
}

/// Resolve the config file path. `$WB_CONFIG_PATH` wins (tests + power users);
/// otherwise `~/.wb/config.yaml`.
pub fn config_path() -> PathBuf {
    if let Ok(p) = std::env::var("WB_CONFIG_PATH") {
        return PathBuf::from(p);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".wb").join("config.yaml")
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Config {
    pub values: BTreeMap<String, String>,
}

impl Config {
    /// Load the config file, returning an empty config if it doesn't exist.
    /// A malformed file is an error (so `wb config` can report it) rather than
    /// being silently treated as empty.
    pub fn load() -> Result<Config, String> {
        let path = config_path();
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Config::default()),
            Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
        };
        if text.trim().is_empty() {
            return Ok(Config::default());
        }
        let values: BTreeMap<String, String> = serde_yaml::from_str(&text)
            .map_err(|e| format!("malformed config at {}: {e}", path.display()))?;
        Ok(Config { values })
    }

    /// Load the config, but never fail — used on the hot run path where a broken
    /// config file should warn, not abort the run. Returns an empty config and
    /// prints a warning on parse failure.
    pub fn load_lenient() -> Config {
        match Config::load() {
            Ok(c) => c,
            Err(e) => {
                crate::log_warn!("warning: ignoring config: {e}");
                Config::default()
            }
        }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(|s| s.as_str())
    }

    pub fn save(&self) -> Result<(), String> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
        }
        let text = serde_yaml::to_string(&self.values)
            .map_err(|e| format!("cannot serialize config: {e}"))?;
        std::fs::write(&path, text).map_err(|e| format!("cannot write {}: {e}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Serialize tests that mutate the shared WB_CONFIG_PATH env var.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_temp_config<T>(f: impl FnOnce(&std::path::Path) -> T) -> T {
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        std::env::set_var("WB_CONFIG_PATH", &path);
        let out = f(&path);
        std::env::remove_var("WB_CONFIG_PATH");
        out
    }

    #[test]
    fn load_missing_returns_empty() {
        with_temp_config(|_p| {
            let c = Config::load().unwrap();
            assert!(c.values.is_empty());
        });
    }

    #[test]
    fn set_save_load_roundtrip() {
        with_temp_config(|_p| {
            let mut c = Config::default();
            c.values
                .insert("callback.url".into(), "https://x/wb".into());
            c.save().unwrap();

            let loaded = Config::load().unwrap();
            assert_eq!(loaded.get("callback.url"), Some("https://x/wb"));
        });
    }

    #[test]
    fn malformed_config_is_error_but_lenient_load_recovers() {
        with_temp_config(|p| {
            std::fs::write(p, "not: : valid: yaml:\n  - [").unwrap();
            assert!(Config::load().is_err());
            // Lenient load swallows the error.
            assert!(Config::load_lenient().values.is_empty());
        });
    }

    #[test]
    fn known_key_allowlist() {
        assert!(is_known_key("callback.url"));
        assert!(!is_known_key("callback.nonsense"));
    }
}
