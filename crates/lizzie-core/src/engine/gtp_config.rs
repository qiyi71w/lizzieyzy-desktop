//! Optional engine-owned visual configuration (`GtpConfigurationProbe`, `GtpConfigurationSchema`).
use super::tuning::GtpConfiguration;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// The shell supplies a timeout-bound isolated GTP process; core owns protocol validation.
pub trait Session {
    fn request(&mut self, command: &str) -> Result<String, String>;
}
pub fn inspect(session: &mut impl Session) -> Result<Option<Value>, String> {
    if !session
        .request("known_command zengtp_config_schema")?
        .trim()
        .eq_ignore_ascii_case("true")
    {
        return Ok(None);
    }
    let schema: Value = serde_json::from_str(&session.request("zengtp_config_schema")?).map_err(|e| e.to_string())?;
    validate_schema(&schema)?;
    Ok(Some(schema))
}
pub fn apply(session: &mut impl Session, profile: &BTreeMap<String, Value>) -> Result<GtpConfiguration, String> {
    let schema = inspect(session)?.ok_or("The selected engine does not support visual configuration")?;
    for (key, value) in profile {
        let field = schema["fields"]
            .as_array()
            .and_then(|fields| fields.iter().find(|field| field["name"].as_str() == Some(key)))
            .ok_or_else(|| format!("Invalid engine configuration value: {key}"))?;
        if !accepts(field, value) {
            return Err(format!("Invalid engine configuration value: {key}"));
        }
    }
    let json = serde_json::to_string(profile).map_err(|e| e.to_string())?;
    session.request(&format!("zengtp_config_set {json}"))?;
    let saved: Value = serde_json::from_str(&session.request("zengtp_config_save")?).map_err(|e| e.to_string())?;
    let profile: BTreeMap<String, Value> = serde_json::from_value(saved["profile"].clone())
        .map_err(|_| "Engine did not return a saved configuration profile")?;
    Ok(GtpConfiguration {
        protocol: "zengtp-config".into(),
        profile,
    })
}
pub fn validate_schema(schema: &Value) -> Result<(), String> {
    if schema["protocol"] != "zengtp-config"
        || schema["version"] != 1
        || schema["batchSemantics"]
            .as_str()
            .is_none_or(|s| !s.eq_ignore_ascii_case("atomic"))
        || schema["persistenceOwner"]
            .as_str()
            .is_none_or(|s| !s.eq_ignore_ascii_case("client"))
    {
        return Err("Unsupported GTP configuration protocol".into());
    }
    let fields = schema["fields"]
        .as_array()
        .filter(|a| !a.is_empty())
        .ok_or("GTP configuration schema contains no fields")?;
    let mut names = BTreeSet::new();
    for field in fields {
        let name = field["name"]
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .ok_or("Invalid configuration field name")?;
        if !names.insert(name)
            || field["apply"] != "next-search"
            || field["requiresRestart"].as_bool().unwrap_or(false)
            || !accepts(field, &field["defaultValue"])
        {
            return Err(format!("Unsupported GTP configuration field: {name}"));
        }
    }
    Ok(())
}
fn accepts(field: &Value, value: &Value) -> bool {
    match field["type"].as_str() {
        Some("string") => value.as_str().is_some_and(|s| {
            field["enumValues"]
                .as_array()
                .is_none_or(|a| a.is_empty() || a.iter().any(|v| v.as_str() == Some(s)))
        }),
        Some("integer" | "number") => value.as_f64().is_some_and(|n| {
            n.is_finite()
                && (field["type"] != "integer" || n.fract() == 0.0)
                && field["minimum"].as_f64().is_none_or(|min| n >= min)
                && field["maximum"].as_f64().is_none_or(|max| n <= max)
        }),
        _ => false,
    }
}
