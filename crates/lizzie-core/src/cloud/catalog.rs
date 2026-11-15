use super::*;
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DiscoverySource { OfficialDocumented, BuiltInCurrent, ServerCapabilities, CachedLegacy, UserPreserved }
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Weight { pub name: String, pub description: String, pub source: DiscoverySource }
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "CloudCatalog")]
pub struct Catalog { pub server_version: String, pub default_weight: String, pub weights: Vec<Weight> }
const BUILTIN: [(&str, &str); 4] = [
    ("20b", "20B, commonly used for handicap games"),
    ("10b384t", "v1.17 small Transformer, b10c384"),
    ("10b512t", "v1.17 medium Transformer, b10c512"),
    ("11b768t", "v1.17 large Transformer, b11c768"),
];
impl Default for Catalog {
    fn default() -> Self {
        Self { server_version: String::new(), default_weight: "10b512t".into(),
            weights: BUILTIN.iter().map(|(name, description)| Weight {
                name: (*name).into(), description: (*description).into(), source: DiscoverySource::BuiltInCurrent,
            }).collect() }
    }
}
fn safe(value: &str) -> bool {
    !value.is_empty() && value.len() <= 64 && value.as_bytes()[0].is_ascii_alphanumeric()
        && value.bytes().all(|b| b.is_ascii_alphanumeric() || b"._+-".contains(&b))
}
fn text(value: &str) -> String { value.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(180).collect() }
/// Java `RemoteComputeConfig.kataWeightForArgs`: the model a Zhizi launch selects.
pub fn kata_weight(args: &str) -> &str {
    args.split_whitespace().skip_while(|word| *word != "--kata-weight").nth(1).unwrap_or("10b512t")
}
impl Catalog {
    pub fn resolve_weight(&self, requested: &str) -> &str {
        self.weights.iter().find(|w| w.name.eq_ignore_ascii_case(requested.trim()))
            .map_or(self.default_weight.as_str(), |w| w.name.as_str())
    }
    /// Java's model refresh retains the other launch options and replaces a retired selected
    /// model with the authoritative server default, using canonical advertised spelling.
    pub fn resolve_args(&self, draft: &str) -> String {
        let draft = if draft.trim().is_empty() { VIP_ARGS } else { draft };
        let requested = kata_weight(draft);
        let resolved = self.resolve_weight(requested);
        let mut result = String::with_capacity(draft.len() + resolved.len() + 16);
        let mut words = draft.split_whitespace();
        let mut replaced = false;
        while let Some(word) = words.next() {
            if !result.is_empty() { result.push(' '); }
            result.push_str(word);
            if word == "--kata-weight" {
                words.next();
                result.push(' ');
                result.push_str(resolved);
                replaced = true;
            }
        }
        if !replaced {
            result.push_str(" --kata-weight ");
            result.push_str(resolved);
        }
        result
    }
    pub fn server_authoritative(&self) -> bool {
        !self.weights.is_empty() && self.weights.iter().all(|w| w.source == DiscoverySource::ServerCapabilities)
    }
    /// Live server advertisements are authoritative, never padded with cached defaults.
    pub fn from_server(value: &Value) -> Result<Self, CloudFailure> { Self::parse(value, true) }
    /// Explicit Java import keeps cache provenance and refreshes the documented baseline.
    pub fn from_java(value: &Value) -> Result<Self, CloudFailure> { Self::parse(value, false).map(|v| v.with_documented_weights()) }
    fn parse(value: &Value, server: bool) -> Result<Self, CloudFailure> {
        let mut weights = Vec::<Weight>::new();
        for item in value.get("supportKataWeights").and_then(Value::as_array).into_iter().flatten() {
            let name = item.as_str().or_else(|| item.get("name").and_then(Value::as_str)).unwrap_or("").trim();
            if !safe(name) || weights.iter().any(|w| w.name == name) { continue; }
            let source = if server { DiscoverySource::ServerCapabilities } else {
                item.get("source").and_then(Value::as_str).and_then(|v|
                    serde_json::from_value::<DiscoverySource>(Value::String(v.to_ascii_uppercase())).ok())
                    .unwrap_or(DiscoverySource::CachedLegacy)
            };
            weights.push(Weight { name: name.into(), description: text(item.get("description").and_then(Value::as_str).unwrap_or("")), source });
        }
        let preferred = value.get("defaultKataWeight").and_then(Value::as_str).unwrap_or("").trim();
        let default_weight = if let Some(found) = weights.iter().find(|w| w.name.eq_ignore_ascii_case(preferred)) {
            found.name.clone()
        } else if !server && safe(preferred) {
            weights.push(Weight { name: preferred.into(), description: String::new(), source: DiscoverySource::UserPreserved });
            preferred.into()
        } else { weights.first().ok_or_else(|| CloudFailure::new("invalid_catalog"))?.name.clone() };
        Ok(Self { server_version: text(value.get("serverVersion").and_then(Value::as_str).unwrap_or("")), default_weight, weights })
    }
    pub fn with_documented_weights(mut self) -> Self {
        if self.weights.iter().any(|w| w.source == DiscoverySource::ServerCapabilities) { return self; }
        let mut merged = Self::default();
        merged.server_version = self.server_version;
        for builtin in &mut merged.weights {
            if let Some(old) = self.weights.iter().find(|w| w.name.eq_ignore_ascii_case(&builtin.name)) {
                if !old.description.is_empty() { builtin.description = old.description.clone(); }
            }
        }
        if let Some(known) = merged.weights.iter().find(|w| w.name.eq_ignore_ascii_case(&self.default_weight)) {
            merged.default_weight = known.name.clone();
        }
        for mut old in self.weights.drain(..) {
            if !safe(&old.name) || merged.weights.iter().any(|w| w.name.eq_ignore_ascii_case(&old.name))
                || ["18bnbt", "28bnbt", "fdx", "60b", "40b"].iter().any(|w| old.name.eq_ignore_ascii_case(w)) { continue; }
            if old.source == DiscoverySource::OfficialDocumented { old.source = DiscoverySource::CachedLegacy; }
            merged.weights.push(old);
        }
        merged
    }
}
