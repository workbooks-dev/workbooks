//! Typed parameters + profiles (#30/#14).
//!
//! A workbook can declare `params:` in frontmatter with a type, default,
//! `required` flag, and `one_of` choices. Values are resolved at run start from
//! (highest precedence first):
//!
//! 1. `--param key=value` CLI flags
//! 2. `--param-file <file>` (a YAML mapping of name → value)
//! 3. the selected `--profile <name>` block under `profiles:`
//! 4. the param's declared `default`
//!
//! Resolved values are injected into every cell's env under their bare name and
//! are visible to `{when=}` / `{skip_if=}`. The resolved set is hashed into the
//! checkpoint identity so that re-running with different params starts fresh
//! rather than silently resuming stale state.
//!
//! Declaration is enforced: a `--param`/profile/param-file key that isn't a
//! declared param is an error (catches typos), and a `required` param with no
//! resolved value aborts the run before any block executes. `wb validate`
//! statically checks the declarations themselves (`wb-param-001`/`wb-param-002`).

use std::collections::{BTreeMap, HashMap};

use serde::Deserialize;
use sha2::{Digest, Sha256};

/// A single declared parameter. Either the full map form or a scalar shorthand
/// (the scalar becomes the `default`, type inferred as string).
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ParamSpec {
    /// `region: { type: string, default: us-east-1, required: false }`
    Full(ParamDef),
    /// `region: us-east-1` — scalar shorthand for a defaulted string param.
    Shorthand(serde_yaml::Value),
}

impl ParamSpec {
    /// Normalize either form into a `ParamDef`.
    pub fn to_def(&self) -> ParamDef {
        match self {
            ParamSpec::Full(d) => d.clone(),
            ParamSpec::Shorthand(v) => ParamDef {
                default: Some(v.clone()),
                ..ParamDef::default()
            },
        }
    }
}

/// The full declaration of a parameter.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ParamDef {
    /// `string` (default) | `int` | `bool` | `enum`.
    #[serde(rename = "type")]
    pub type_: Option<String>,
    /// Default value used when no CLI/profile/param-file value is supplied.
    pub default: Option<serde_yaml::Value>,
    /// When true, the run aborts if no value can be resolved.
    #[serde(default)]
    pub required: bool,
    /// Human description (shown by tooling; not otherwise interpreted).
    #[allow(dead_code)]
    pub description: Option<String>,
    /// Allowed values. Membership is enforced regardless of `type`.
    #[serde(default)]
    pub one_of: Vec<serde_yaml::Value>,
    /// When true, the resolved value is redacted from rendered output.
    #[serde(default)]
    pub secret: bool,
}

/// The known param type names.
pub const KNOWN_TYPES: &[&str] = &["string", "int", "bool", "enum"];

/// Result of resolving a workbook's declared params against CLI inputs.
#[derive(Debug, Clone, Default)]
pub struct ResolvedParams {
    /// name → resolved string value, injected into the cell env.
    pub values: BTreeMap<String, String>,
    /// Values of params marked `secret: true`, to be redacted from output.
    pub secret_values: Vec<String>,
    /// Stable 12-hex digest of the resolved (name, value) set. Feeds the
    /// checkpoint identity. `None` when no params resolved (so a workbook
    /// without params keeps a `None` hash and legacy checkpoints stay valid).
    pub hash: Option<String>,
}

/// Render a YAML scalar as the string we inject into the env. Bools are
/// normalized to `true`/`false` so `{when=$flag}` truthiness is predictable.
fn scalar_to_string(v: &serde_yaml::Value) -> Option<String> {
    match v {
        serde_yaml::Value::Null => None,
        serde_yaml::Value::Bool(b) => Some(b.to_string()),
        serde_yaml::Value::Number(n) => Some(n.to_string()),
        serde_yaml::Value::String(s) => Some(s.clone()),
        _ => None,
    }
}

/// Parse `--param key=value` items into a map. The first `=` splits key/value
/// (values may contain `=`). A missing `=` is an error.
fn parse_cli_params(items: &[String]) -> Result<HashMap<String, String>, String> {
    let mut out = HashMap::new();
    for item in items {
        let (k, v) = item.split_once('=').ok_or_else(|| {
            format!("--param '{item}' is not in key=value form (e.g. --param region=us-east-1)")
        })?;
        if k.is_empty() {
            return Err(format!("--param '{item}' has an empty key"));
        }
        out.insert(k.to_string(), v.to_string());
    }
    Ok(out)
}

/// Load a `--param-file` (a YAML mapping of name → scalar) into string values.
fn load_param_file(path: &str) -> Result<HashMap<String, String>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("--param-file '{path}': {e}"))?;
    let map: serde_yaml::Mapping = serde_yaml::from_str(&text).map_err(|e| {
        format!("--param-file '{path}': expected a YAML mapping of name: value ({e})")
    })?;
    let mut out = HashMap::new();
    for (k, v) in map {
        let key = match k {
            serde_yaml::Value::String(s) => s,
            other => return Err(format!("--param-file '{path}': non-string key {other:?}")),
        };
        match scalar_to_string(&v) {
            Some(s) => {
                out.insert(key, s);
            }
            None => {
                return Err(format!(
                    "--param-file '{path}': value for '{key}' must be a scalar (string/int/bool)"
                ))
            }
        }
    }
    Ok(out)
}

/// Validate a resolved string value against a param's declared type + choices.
/// Returns the canonical string to inject (bools normalized) or an error.
fn validate_value(name: &str, def: &ParamDef, raw: &str) -> Result<String, String> {
    // one_of membership is enforced regardless of declared type.
    if !def.one_of.is_empty() {
        let allowed: Vec<String> = def.one_of.iter().filter_map(scalar_to_string).collect();
        if !allowed.iter().any(|a| a == raw) {
            return Err(format!(
                "param '{name}': value '{raw}' is not one of [{}]",
                allowed.join(", ")
            ));
        }
    }
    match def.type_.as_deref().unwrap_or("string") {
        "int" => raw
            .parse::<i64>()
            .map(|_| raw.to_string())
            .map_err(|_| format!("param '{name}': '{raw}' is not a valid int")),
        "bool" => match raw.to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Ok("true".to_string()),
            "false" | "0" | "no" | "off" => Ok("false".to_string()),
            _ => Err(format!(
                "param '{name}': '{raw}' is not a valid bool (use true/false)"
            )),
        },
        // "enum" and "string" accept any string (enum membership already
        // enforced above via one_of).
        _ => Ok(raw.to_string()),
    }
}

/// Compute the 12-hex param identity digest over a sorted (name=value) set.
fn hash_values(values: &BTreeMap<String, String>) -> String {
    let mut hasher = Sha256::new();
    for (k, v) in values {
        hasher.update(k.as_bytes());
        hasher.update(b"=");
        hasher.update(v.as_bytes());
        hasher.update(b"\n");
    }
    let digest = hasher.finalize();
    digest.iter().take(6).map(|b| format!("{b:02x}")).collect()
}

/// Resolve declared params against CLI inputs.
///
/// `params` is the frontmatter `params:` map (may be `None`). `profiles` is the
/// frontmatter `profiles:` map. `profile`/`param_file`/`cli_params` are the CLI
/// inputs. Returns the injectable values + redaction list + identity hash, or a
/// human-readable error (which the caller turns into a usage-error exit).
pub fn resolve(
    params: Option<&HashMap<String, ParamSpec>>,
    profiles: Option<&HashMap<String, HashMap<String, serde_yaml::Value>>>,
    profile: Option<&str>,
    param_file: Option<&str>,
    cli_params: &[String],
) -> Result<ResolvedParams, String> {
    let cli = parse_cli_params(cli_params)?;
    let file_vals = match param_file {
        Some(p) => load_param_file(p)?,
        None => HashMap::new(),
    };

    // Selected profile (validated to exist).
    let profile_vals: HashMap<String, String> = match profile {
        Some(name) => {
            let profiles = profiles.ok_or_else(|| {
                format!("--profile '{name}' but the workbook declares no profiles:")
            })?;
            let block = profiles.get(name).ok_or_else(|| {
                let mut known: Vec<&str> = profiles.keys().map(|s| s.as_str()).collect();
                known.sort_unstable();
                format!(
                    "--profile '{name}' is not declared (known profiles: {})",
                    known.join(", ")
                )
            })?;
            block
                .iter()
                .filter_map(|(k, v)| scalar_to_string(v).map(|s| (k.clone(), s)))
                .collect()
        }
        None => HashMap::new(),
    };

    let declared = params.cloned().unwrap_or_default();

    // Reject inputs that reference an undeclared param (typo guard).
    for (src, keys) in [
        ("--param", cli.keys().collect::<Vec<_>>()),
        ("--param-file", file_vals.keys().collect()),
    ] {
        for k in keys {
            if !declared.contains_key(k) {
                let mut known: Vec<&str> = declared.keys().map(|s| s.as_str()).collect();
                known.sort_unstable();
                return Err(format!(
                    "{src} '{k}' is not a declared parameter (declared: {})",
                    if known.is_empty() {
                        "none".to_string()
                    } else {
                        known.join(", ")
                    }
                ));
            }
        }
    }
    // A profile naming an undeclared param is a workbook bug, but `wb validate`
    // reports it (wb-param-002); at runtime we just ignore unknown profile keys
    // so a stale profile entry doesn't block a run.

    let mut values = BTreeMap::new();
    let mut secret_values = Vec::new();
    let mut missing_required = Vec::new();

    for (name, spec) in &declared {
        let def = spec.to_def();
        // Precedence: CLI > param-file > profile > default.
        let raw = cli
            .get(name)
            .or_else(|| file_vals.get(name))
            .or_else(|| profile_vals.get(name))
            .cloned()
            .or_else(|| def.default.as_ref().and_then(scalar_to_string));

        match raw {
            Some(raw) => {
                let value = validate_value(name, &def, &raw)?;
                if def.secret && !value.is_empty() {
                    secret_values.push(value.clone());
                }
                values.insert(name.clone(), value);
            }
            None => {
                if def.required {
                    missing_required.push(name.clone());
                }
            }
        }
    }

    if !missing_required.is_empty() {
        missing_required.sort();
        return Err(format!(
            "missing required parameter(s): {}. Pass with --param NAME=VALUE, --param-file, or --profile.",
            missing_required.join(", ")
        ));
    }

    let hash = if values.is_empty() {
        None
    } else {
        Some(hash_values(&values))
    };

    Ok(ResolvedParams {
        values,
        secret_values,
        hash,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec_full(yaml: &str) -> HashMap<String, ParamSpec> {
        serde_yaml::from_str(yaml).unwrap()
    }

    #[test]
    fn shorthand_scalar_becomes_default() {
        let p = spec_full("region: us-east-1\nreplicas: 3\n");
        let r = resolve(Some(&p), None, None, None, &[]).unwrap();
        assert_eq!(r.values.get("region").unwrap(), "us-east-1");
        assert_eq!(r.values.get("replicas").unwrap(), "3");
    }

    #[test]
    fn cli_overrides_default() {
        let p = spec_full("region:\n  type: string\n  default: us-east-1\n");
        let r = resolve(
            Some(&p),
            None,
            None,
            None,
            &["region=eu-west-1".to_string()],
        )
        .unwrap();
        assert_eq!(r.values.get("region").unwrap(), "eu-west-1");
    }

    #[test]
    fn missing_required_is_error() {
        let p = spec_full("token:\n  required: true\n");
        let err = resolve(Some(&p), None, None, None, &[]).unwrap_err();
        assert!(err.contains("missing required"), "{err}");
    }

    #[test]
    fn int_type_validated() {
        let p = spec_full("n:\n  type: int\n  default: 5\n");
        assert!(resolve(Some(&p), None, None, None, &["n=notnum".to_string()]).is_err());
        let r = resolve(Some(&p), None, None, None, &["n=9".to_string()]).unwrap();
        assert_eq!(r.values.get("n").unwrap(), "9");
    }

    #[test]
    fn bool_is_normalized() {
        let p = spec_full("flag:\n  type: bool\n  default: false\n");
        let r = resolve(Some(&p), None, None, None, &["flag=yes".to_string()]).unwrap();
        assert_eq!(r.values.get("flag").unwrap(), "true");
    }

    #[test]
    fn one_of_membership_enforced() {
        let p = spec_full("env:\n  type: enum\n  one_of: [dev, prod]\n  default: dev\n");
        assert!(resolve(Some(&p), None, None, None, &["env=staging".to_string()]).is_err());
        assert!(resolve(Some(&p), None, None, None, &["env=prod".to_string()]).is_ok());
    }

    #[test]
    fn unknown_cli_param_rejected() {
        let p = spec_full("region: us-east-1\n");
        let err = resolve(Some(&p), None, None, None, &["nope=1".to_string()]).unwrap_err();
        assert!(err.contains("not a declared parameter"), "{err}");
    }

    #[test]
    fn profile_supplies_values() {
        let p = spec_full("region:\n  type: string\nreplicas:\n  type: int\n");
        let mut profiles: HashMap<String, HashMap<String, serde_yaml::Value>> = HashMap::new();
        let mut prod = HashMap::new();
        prod.insert(
            "region".to_string(),
            serde_yaml::Value::String("us-east-1".into()),
        );
        prod.insert("replicas".to_string(), serde_yaml::Value::Number(10.into()));
        profiles.insert("prod".to_string(), prod);
        let r = resolve(Some(&p), Some(&profiles), Some("prod"), None, &[]).unwrap();
        assert_eq!(r.values.get("region").unwrap(), "us-east-1");
        assert_eq!(r.values.get("replicas").unwrap(), "10");
    }

    #[test]
    fn unknown_profile_is_error() {
        let p = spec_full("region: us-east-1\n");
        let profiles: HashMap<String, HashMap<String, serde_yaml::Value>> = HashMap::new();
        let err = resolve(Some(&p), Some(&profiles), Some("ghost"), None, &[]).unwrap_err();
        assert!(err.contains("not declared"), "{err}");
    }

    #[test]
    fn hash_changes_with_values() {
        let p = spec_full("region:\n  type: string\n  default: a\n");
        let h1 = resolve(Some(&p), None, None, None, &[]).unwrap().hash;
        let h2 = resolve(Some(&p), None, None, None, &["region=b".to_string()])
            .unwrap()
            .hash;
        assert!(h1.is_some() && h2.is_some());
        assert_ne!(h1, h2);
    }

    #[test]
    fn secret_param_collected_for_redaction() {
        let p = spec_full("token:\n  secret: true\n  default: hunter2\n");
        let r = resolve(Some(&p), None, None, None, &[]).unwrap();
        assert!(r.secret_values.contains(&"hunter2".to_string()));
    }

    #[test]
    fn no_params_yields_none_hash() {
        let r = resolve(None, None, None, None, &[]).unwrap();
        assert!(r.hash.is_none());
        assert!(r.values.is_empty());
    }

    // ---- scalar_to_string (lines 93-99) ----

    #[test]
    fn scalar_to_string_covers_every_variant() {
        use serde_yaml::Value;
        assert_eq!(scalar_to_string(&Value::Null), None);
        assert_eq!(scalar_to_string(&Value::Bool(true)), Some("true".into()));
        assert_eq!(scalar_to_string(&Value::Bool(false)), Some("false".into()));
        assert_eq!(
            scalar_to_string(&Value::Number(42.into())),
            Some("42".into())
        );
        assert_eq!(
            scalar_to_string(&Value::String("hi".into())),
            Some("hi".into())
        );
        // Non-scalar (sequence / mapping) is not injectable.
        let seq = Value::Sequence(vec![Value::Number(1.into())]);
        assert_eq!(scalar_to_string(&seq), None);
        let map = Value::Mapping(serde_yaml::Mapping::new());
        assert_eq!(scalar_to_string(&map), None);
    }

    // ---- parse_cli_params (lines 103-115) ----

    #[test]
    fn parse_cli_params_requires_equals() {
        let err = parse_cli_params(&["noequals".to_string()]).unwrap_err();
        assert!(err.contains("not in key=value form"), "{err}");
    }

    #[test]
    fn parse_cli_params_rejects_empty_key() {
        let err = parse_cli_params(&["=value".to_string()]).unwrap_err();
        assert!(err.contains("empty key"), "{err}");
    }

    #[test]
    fn parse_cli_params_value_may_contain_equals() {
        let out = parse_cli_params(&["conn=a=b=c".to_string()]).unwrap();
        assert_eq!(out.get("conn").unwrap(), "a=b=c");
    }

    // ---- load_param_file (lines 118-141, 202) ----

    fn write_tmp(contents: &str) -> tempfile::NamedTempFile {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(contents.as_bytes()).unwrap();
        f.flush().unwrap();
        f
    }

    #[test]
    fn load_param_file_reads_scalar_mapping() {
        let f = write_tmp("region: eu-west-1\nreplicas: 7\nflag: true\n");
        let out = load_param_file(f.path().to_str().unwrap()).unwrap();
        assert_eq!(out.get("region").unwrap(), "eu-west-1");
        assert_eq!(out.get("replicas").unwrap(), "7");
        assert_eq!(out.get("flag").unwrap(), "true");
    }

    #[test]
    fn load_param_file_missing_file_errors() {
        let err = load_param_file("/no/such/param-file.yaml").unwrap_err();
        assert!(err.contains("--param-file"), "{err}");
    }

    #[test]
    fn load_param_file_non_mapping_errors() {
        let f = write_tmp("- just\n- a\n- list\n");
        let err = load_param_file(f.path().to_str().unwrap()).unwrap_err();
        assert!(err.contains("expected a YAML mapping"), "{err}");
    }

    #[test]
    fn load_param_file_non_string_key_errors() {
        let f = write_tmp("123: value\n");
        let err = load_param_file(f.path().to_str().unwrap()).unwrap_err();
        assert!(err.contains("non-string key"), "{err}");
    }

    #[test]
    fn load_param_file_non_scalar_value_errors() {
        let f = write_tmp("region:\n  nested: map\n");
        let err = load_param_file(f.path().to_str().unwrap()).unwrap_err();
        assert!(err.contains("must be a scalar"), "{err}");
    }

    #[test]
    fn param_file_feeds_resolve() {
        let p = spec_full("region:\n  type: string\n  default: us-east-1\n");
        let f = write_tmp("region: ap-south-1\n");
        let r = resolve(Some(&p), None, None, Some(f.path().to_str().unwrap()), &[]).unwrap();
        assert_eq!(r.values.get("region").unwrap(), "ap-south-1");
    }

    #[test]
    fn param_file_key_must_be_declared() {
        let p = spec_full("region: us-east-1\n");
        let f = write_tmp("undeclared: 1\n");
        let err = resolve(Some(&p), None, None, Some(f.path().to_str().unwrap()), &[]).unwrap_err();
        assert!(err.contains("not a declared parameter"), "{err}");
        assert!(err.contains("--param-file"), "{err}");
    }

    // ---- precedence: CLI > param-file > profile > default ----

    #[test]
    fn cli_beats_param_file() {
        let p = spec_full("region:\n  type: string\n  default: d\n");
        let f = write_tmp("region: from-file\n");
        let r = resolve(
            Some(&p),
            None,
            None,
            Some(f.path().to_str().unwrap()),
            &["region=from-cli".to_string()],
        )
        .unwrap();
        assert_eq!(r.values.get("region").unwrap(), "from-cli");
    }

    #[test]
    fn param_file_beats_profile() {
        let p = spec_full("region:\n  type: string\n");
        let mut profiles: HashMap<String, HashMap<String, serde_yaml::Value>> = HashMap::new();
        let mut prod = HashMap::new();
        prod.insert(
            "region".to_string(),
            serde_yaml::Value::String("from-profile".into()),
        );
        profiles.insert("prod".to_string(), prod);
        let f = write_tmp("region: from-file\n");
        let r = resolve(
            Some(&p),
            Some(&profiles),
            Some("prod"),
            Some(f.path().to_str().unwrap()),
            &[],
        )
        .unwrap();
        assert_eq!(r.values.get("region").unwrap(), "from-file");
    }

    // ---- bool false + invalid (lines 163-166) ----

    #[test]
    fn bool_false_aliases_normalize() {
        let p = spec_full("flag:\n  type: bool\n  default: true\n");
        for raw in ["false", "0", "no", "off", "OFF"] {
            let r = resolve(Some(&p), None, None, None, &[format!("flag={raw}")]).unwrap();
            assert_eq!(r.values.get("flag").unwrap(), "false", "raw={raw}");
        }
    }

    #[test]
    fn bool_invalid_value_errors() {
        let p = spec_full("flag:\n  type: bool\n  default: true\n");
        let err = resolve(Some(&p), None, None, None, &["flag=maybe".to_string()]).unwrap_err();
        assert!(err.contains("not a valid bool"), "{err}");
    }

    // ---- profile given but workbook declares none (lines 210-211) ----

    #[test]
    fn profile_without_profiles_block_errors() {
        let p = spec_full("region: us-east-1\n");
        let err = resolve(Some(&p), None, Some("prod"), None, &[]).unwrap_err();
        assert!(err.contains("declares no profiles"), "{err}");
    }

    // ---- undeclared key when nothing is declared → "none" (line 242) ----

    #[test]
    fn unknown_param_with_no_declarations_says_none() {
        // No params declared at all: the error lists "declared: none".
        let empty: HashMap<String, ParamSpec> = HashMap::new();
        let err = resolve(Some(&empty), None, None, None, &["x=1".to_string()]).unwrap_err();
        assert!(err.contains("declared: none"), "{err}");
    }

    #[test]
    fn empty_secret_value_not_collected() {
        // A secret param resolving to the empty string is not added to the
        // redaction list (guards the `!value.is_empty()` branch).
        let p = spec_full("token:\n  secret: true\n  default: \"\"\n");
        let r = resolve(Some(&p), None, None, None, &[]).unwrap();
        assert!(r.secret_values.is_empty());
    }

    #[test]
    fn hash_is_stable_across_runs() {
        let p = spec_full("region:\n  type: string\n  default: a\nn:\n  type: int\n  default: 1\n");
        let h1 = resolve(Some(&p), None, None, None, &[]).unwrap().hash;
        let h2 = resolve(Some(&p), None, None, None, &[]).unwrap().hash;
        assert_eq!(h1, h2);
        assert_eq!(h1.unwrap().len(), 12);
    }
}
