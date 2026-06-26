use std::collections::HashMap;

use crate::error::{WbError, WbResult};
use crate::parser;

/// Signal configuration from env vars / CLI flags.
pub struct SignalConfig {
    /// Redis URL for signal storage
    pub url: String,
    /// Key prefix for pending signals (e.g., "paracord:cooper:signal")
    pub signal_key: String,
    /// Key prefix for completed runs (e.g., "paracord:cooper:runs")
    #[allow(dead_code)]
    pub complete_key: Option<String>,
    /// TTL for completed run archives
    #[allow(dead_code)]
    pub ttl_secs: u64,
}

impl SignalConfig {
    /// Build full Redis key: <prefix>:<checkpoint_id>
    pub fn signal_redis_key(&self, checkpoint_id: &str) -> String {
        format!("{}:{}", self.signal_key, checkpoint_id)
    }

    /// Build full archive key: <complete_prefix>:<checkpoint_id>
    #[allow(dead_code)]
    pub fn complete_redis_key(&self, checkpoint_id: &str) -> Option<String> {
        self.complete_key
            .as_ref()
            .map(|prefix| format!("{}:{}", prefix, checkpoint_id))
    }
}

/// Try to read a signal from Redis for the given checkpoint.
/// Returns the parsed vars if a signal exists, None otherwise.
/// Deletes the signal key after reading.
pub fn read_signal(
    config: &SignalConfig,
    checkpoint_id: &str,
) -> WbResult<Option<HashMap<String, String>>> {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let client = redis::Client::open(config.url.as_str())
        .map_err(|e| WbError::Io(format!("signal: redis client: {}", e)))?;

    let mut conn = client
        .get_connection_with_timeout(std::time::Duration::from_secs(5))
        .map_err(|e| WbError::Io(format!("signal: redis connect: {}", e)))?;

    let key = config.signal_redis_key(checkpoint_id);

    let value: Option<String> = redis::cmd("GET")
        .arg(&key)
        .query(&mut conn)
        .map_err(|e| WbError::Io(format!("signal: GET {}: {}", key, e)))?;

    let value = match value {
        Some(v) => v,
        None => return Ok(None),
    };

    // Delete the signal key after reading
    let _: () = redis::cmd("DEL")
        .arg(&key)
        .query(&mut conn)
        .map_err(|e| WbError::Io(format!("signal: DEL {}: {}", key, e)))?;

    // Parse as JSON object → HashMap
    let parsed: serde_json::Value = serde_json::from_str(&value)
        .map_err(|e| WbError::Parse(format!("signal: parse JSON from {}: {}", key, e)))?;

    let mut vars = HashMap::new();
    match parsed {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                if let Some(s) = json_scalar_to_string(&v) {
                    vars.insert(k, s);
                }
            }
        }
        other => {
            // Scalar value — can only bind if there's one bind name
            if let Some(s) = json_scalar_to_string(&other) {
                vars.insert("_value".to_string(), s);
            }
        }
    }

    Ok(Some(vars))
}

/// Archive a signal payload to the complete key with TTL.
#[allow(dead_code)]
pub fn archive_signal(config: &SignalConfig, checkpoint_id: &str, payload: &str) -> WbResult<()> {
    let complete_key = match config.complete_redis_key(checkpoint_id) {
        Some(k) => k,
        None => return Ok(()), // no complete key configured, skip
    };

    let _ = rustls::crypto::ring::default_provider().install_default();

    let client = redis::Client::open(config.url.as_str())
        .map_err(|e| WbError::Io(format!("signal: redis client: {}", e)))?;

    let mut conn = client
        .get_connection_with_timeout(std::time::Duration::from_secs(5))
        .map_err(|e| WbError::Io(format!("signal: redis connect: {}", e)))?;

    let _: () = redis::cmd("SET")
        .arg(&complete_key)
        .arg(payload)
        .arg("EX")
        .arg(config.ttl_secs)
        .query(&mut conn)
        .map_err(|e| WbError::Io(format!("signal: SET {}: {}", complete_key, e)))?;

    Ok(())
}

/// Scan all pending descriptors and check Redis for signals.
/// Returns the first (checkpoint_id, vars) that has a signal ready.
#[allow(clippy::type_complexity)]
pub fn find_ready_signal(
    config: &SignalConfig,
) -> WbResult<Option<(String, HashMap<String, String>)>> {
    let descriptors = crate::pending::list_all();

    for (id, _desc) in &descriptors {
        if let Some(vars) = read_signal(config, id)? {
            return Ok(Some((id.clone(), vars)));
        }
    }

    Ok(None)
}

/// Merge signal vars into bind names from the pending descriptor.
/// If the signal is a single scalar and bind has one name, map it.
pub fn bind_signal_vars(
    signal_vars: &HashMap<String, String>,
    bind: &Option<parser::BindSpec>,
) -> HashMap<String, String> {
    let bind_names: Vec<String> = bind
        .as_ref()
        .map(|b| match b {
            parser::BindSpec::Single(s) => vec![s.clone()],
            parser::BindSpec::Multiple(v) => v.clone(),
        })
        .unwrap_or_default();

    let mut out = HashMap::new();

    // If signal has a "_value" key (scalar) and there's exactly one bind, map it
    if bind_names.len() == 1 {
        if let Some(v) = signal_vars.get("_value") {
            out.insert(bind_names[0].clone(), v.clone());
            return out;
        }
    }

    // Otherwise, match by key name
    for (k, v) in signal_vars {
        if k != "_value" {
            out.insert(k.clone(), v.clone());
        }
    }

    out
}

/// Parse duration strings like "7d", "24h", "3600" into seconds.
pub fn parse_ttl(s: &str) -> WbResult<u64> {
    crate::parser::parse_duration_secs(s)
}

/// Build a SignalConfig from env vars in the context.
/// Returns None if WB_SIGNAL_URL is not set.
pub fn config_from_env(env: &HashMap<String, String>) -> Option<SignalConfig> {
    let url = env.get("WB_SIGNAL_URL").cloned()?;
    let signal_key = env.get("WB_SIGNAL_KEY").cloned()?;

    let complete_key = env.get("WB_COMPLETE_KEY").cloned();
    let ttl_secs = env
        .get("WB_SIGNAL_TTL")
        .and_then(|s| parse_ttl(s).ok())
        .unwrap_or(7 * 24 * 60 * 60); // 7 days default

    Some(SignalConfig {
        url,
        signal_key,
        complete_key,
        ttl_secs,
    })
}

fn json_scalar_to_string(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        serde_json::Value::Null => Some(String::new()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_redis_key_format() {
        let config = SignalConfig {
            url: "redis://localhost".to_string(),
            signal_key: "paracord:cooper:signal".to_string(),
            complete_key: Some("paracord:cooper:runs".to_string()),
            ttl_secs: 604800,
        };
        assert_eq!(
            config.signal_redis_key("deploy-approval"),
            "paracord:cooper:signal:deploy-approval"
        );
        assert_eq!(
            config.complete_redis_key("deploy-approval"),
            Some("paracord:cooper:runs:deploy-approval".to_string())
        );
    }

    #[test]
    fn signal_redis_key_no_complete() {
        let config = SignalConfig {
            url: "redis://localhost".to_string(),
            signal_key: "org:agent:signal".to_string(),
            complete_key: None,
            ttl_secs: 3600,
        };
        assert!(config.complete_redis_key("test").is_none());
    }

    #[test]
    fn config_from_env_full() {
        let mut env = HashMap::new();
        env.insert("WB_SIGNAL_URL".to_string(), "rediss://host".to_string());
        env.insert("WB_SIGNAL_KEY".to_string(), "org:agent:signal".to_string());
        env.insert("WB_COMPLETE_KEY".to_string(), "org:agent:runs".to_string());
        env.insert("WB_SIGNAL_TTL".to_string(), "3d".to_string());

        let config = config_from_env(&env).unwrap();
        assert_eq!(config.url, "rediss://host");
        assert_eq!(config.signal_key, "org:agent:signal");
        assert_eq!(config.complete_key.as_deref(), Some("org:agent:runs"));
        assert_eq!(config.ttl_secs, 3 * 24 * 60 * 60);
    }

    #[test]
    fn config_from_env_defaults() {
        let mut env = HashMap::new();
        env.insert("WB_SIGNAL_URL".to_string(), "redis://localhost".to_string());
        env.insert("WB_SIGNAL_KEY".to_string(), "test:signal".to_string());

        let config = config_from_env(&env).unwrap();
        assert!(config.complete_key.is_none());
        assert_eq!(config.ttl_secs, 7 * 24 * 60 * 60); // 7 days
    }

    #[test]
    fn config_from_env_missing_url() {
        let mut env = HashMap::new();
        env.insert("WB_SIGNAL_KEY".to_string(), "test:signal".to_string());
        assert!(config_from_env(&env).is_none());
    }

    #[test]
    fn config_from_env_missing_key() {
        let mut env = HashMap::new();
        env.insert("WB_SIGNAL_URL".to_string(), "redis://localhost".to_string());
        assert!(config_from_env(&env).is_none());
    }

    #[test]
    fn bind_signal_single_scalar() {
        let mut vars = HashMap::new();
        vars.insert("_value".to_string(), "yes".to_string());
        let bind = Some(parser::BindSpec::Single("approved".to_string()));
        let result = bind_signal_vars(&vars, &bind);
        assert_eq!(result.get("approved").unwrap(), "yes");
    }

    #[test]
    fn bind_signal_object_keys() {
        let mut vars = HashMap::new();
        vars.insert("approved_by".to_string(), "justin".to_string());
        vars.insert("reason".to_string(), "looks good".to_string());
        let bind = Some(parser::BindSpec::Multiple(vec![
            "approved_by".to_string(),
            "reason".to_string(),
        ]));
        let result = bind_signal_vars(&vars, &bind);
        assert_eq!(result.get("approved_by").unwrap(), "justin");
        assert_eq!(result.get("reason").unwrap(), "looks good");
    }

    #[test]
    fn bind_signal_no_bind_spec() {
        let mut vars = HashMap::new();
        vars.insert("key".to_string(), "val".to_string());
        let result = bind_signal_vars(&vars, &None);
        assert_eq!(result.get("key").unwrap(), "val");
    }

    #[test]
    fn parse_ttl_variants() {
        assert_eq!(parse_ttl("7d").unwrap(), 604800);
        assert_eq!(parse_ttl("24h").unwrap(), 86400);
        assert_eq!(parse_ttl("30m").unwrap(), 1800);
        assert_eq!(parse_ttl("3600").unwrap(), 3600);
    }

    #[test]
    fn parse_ttl_invalid_propagates_error() {
        // A malformed duration bubbles up as an error from parse_ttl.
        assert!(parse_ttl("not-a-duration").is_err());
    }

    // ---- config_from_env: bad TTL falls back to default ----

    #[test]
    fn config_from_env_bad_ttl_uses_default() {
        let mut env = HashMap::new();
        env.insert("WB_SIGNAL_URL".to_string(), "redis://localhost".to_string());
        env.insert("WB_SIGNAL_KEY".to_string(), "test:signal".to_string());
        // Unparseable TTL → parse_ttl(...).ok() is None → 7-day default.
        env.insert("WB_SIGNAL_TTL".to_string(), "garbage".to_string());

        let config = config_from_env(&env).unwrap();
        assert_eq!(config.ttl_secs, 7 * 24 * 60 * 60);
    }

    // ---- bind_signal_vars: remaining branches ----

    #[test]
    fn bind_signal_single_scalar_returns_only_bound_name() {
        // Single bind + "_value" scalar: takes the early-return branch and
        // ignores everything else in the map.
        let mut vars = HashMap::new();
        vars.insert("_value".to_string(), "yes".to_string());
        vars.insert("ignored".to_string(), "nope".to_string());
        let bind = Some(parser::BindSpec::Single("approved".to_string()));
        let result = bind_signal_vars(&vars, &bind);
        assert_eq!(result.len(), 1);
        assert_eq!(result.get("approved").unwrap(), "yes");
    }

    #[test]
    fn bind_signal_single_bind_without_value_falls_through() {
        // Single bind but no "_value" key: skips the early return and matches
        // by key name, dropping any "_value"-only entries (none here).
        let mut vars = HashMap::new();
        vars.insert("approved".to_string(), "true".to_string());
        let bind = Some(parser::BindSpec::Single("approved".to_string()));
        let result = bind_signal_vars(&vars, &bind);
        assert_eq!(result.get("approved").unwrap(), "true");
    }

    #[test]
    fn bind_signal_multi_bind_drops_value_key() {
        // With >1 bind name, the "_value" early-return is skipped and the
        // "_value" key itself is filtered out of the by-name copy.
        let mut vars = HashMap::new();
        vars.insert("_value".to_string(), "scalar".to_string());
        vars.insert("a".to_string(), "1".to_string());
        vars.insert("b".to_string(), "2".to_string());
        let bind = Some(parser::BindSpec::Multiple(vec![
            "a".to_string(),
            "b".to_string(),
        ]));
        let result = bind_signal_vars(&vars, &bind);
        assert_eq!(result.get("a").unwrap(), "1");
        assert_eq!(result.get("b").unwrap(), "2");
        assert!(!result.contains_key("_value"));
    }

    // ---- json_scalar_to_string: every match arm ----

    #[test]
    fn json_scalar_to_string_all_arms() {
        use serde_json::json;
        assert_eq!(
            json_scalar_to_string(&json!("hello")),
            Some("hello".to_string())
        );
        assert_eq!(json_scalar_to_string(&json!(42)), Some("42".to_string()));
        assert_eq!(
            json_scalar_to_string(&json!(true)),
            Some("true".to_string())
        );
        assert_eq!(
            json_scalar_to_string(&json!(false)),
            Some("false".to_string())
        );
        // Null maps to an empty string (not None).
        assert_eq!(
            json_scalar_to_string(&serde_json::Value::Null),
            Some(String::new())
        );
        // Composite values are not scalars → None.
        assert_eq!(json_scalar_to_string(&json!([1, 2, 3])), None);
        assert_eq!(json_scalar_to_string(&json!({"k": "v"})), None);
    }

    // ---- read_signal / archive_signal: offline-reachable paths ----

    fn bad_url_config() -> SignalConfig {
        // A URL with no recognizable scheme makes redis::Client::open fail
        // immediately, with no network access.
        SignalConfig {
            url: "this is not a redis url".to_string(),
            signal_key: "test:signal".to_string(),
            complete_key: Some("test:runs".to_string()),
            ttl_secs: 3600,
        }
    }

    #[test]
    fn read_signal_bad_url_is_io_error() {
        let err = read_signal(&bad_url_config(), "ckpt").unwrap_err();
        match err {
            WbError::Io(m) => assert!(m.contains("signal: redis client")),
            other => panic!("expected Io error, got {:?}", other),
        }
    }

    #[test]
    fn archive_signal_no_complete_key_is_noop() {
        // No complete_key configured → returns Ok without touching redis.
        let config = SignalConfig {
            url: "this is not a redis url".to_string(),
            signal_key: "test:signal".to_string(),
            complete_key: None,
            ttl_secs: 3600,
        };
        assert!(archive_signal(&config, "ckpt", "{}").is_ok());
    }

    #[test]
    fn archive_signal_bad_url_is_io_error() {
        // complete_key is set, so it proceeds to redis client open and fails.
        let err = archive_signal(&bad_url_config(), "ckpt", "{}").unwrap_err();
        match err {
            WbError::Io(m) => assert!(m.contains("signal: redis client")),
            other => panic!("expected Io error, got {:?}", other),
        }
    }

    #[test]
    fn find_ready_signal_empty_pending_returns_none() {
        // Point pending::list_all() at an empty temp dir (per-thread override),
        // so the loop body never runs and redis is never contacted.
        let tmp = std::env::temp_dir().join(format!("wb-signal-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&tmp);
        let prev = crate::checkpoint::set_test_checkpoint_dir(Some(tmp.clone()));
        let result = find_ready_signal(&bad_url_config());
        crate::checkpoint::set_test_checkpoint_dir(prev);
        let _ = std::fs::remove_dir_all(&tmp);
        assert!(result.unwrap().is_none());
    }
}
