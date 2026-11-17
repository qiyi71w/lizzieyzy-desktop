//! Java FoxPdaCompatibility: absence of dynamic PDA must be proven from the physical launch,
//! never inferred from a subsequently edited engine profile or configuration file.
use super::gtp::LaunchSpec;
use std::path::Path;
const DYNAMIC: &str = "dynamicPlayoutDoublingAdvantageCapPerOppLead";
pub(super) fn inspect(spec: &LaunchSpec) -> &'static str {
    let program = Path::new(&spec.program).file_stem().and_then(|p| p.to_str()).unwrap_or("").to_ascii_lowercase();
    if !program.contains("katago") || spec.args.first().map(String::as_str) != Some("gtp") { return "Fox.pdaUnknown"; }
    let mut config = false;
    let mut dynamic = false;
    let mut args = spec.args.iter().skip(1);
    while let Some(option) = args.next() {
        if option == "-quit-without-waiting" { continue; }
        if !["-model", "--model", "-weights", "--weights", "-config", "--config", "-override-config", "--override-config"].contains(&option.as_str()) { return "Fox.pdaUnknown"; }
        let Some(value) = args.next() else { return "Fox.pdaUnknown"; };
        if option.ends_with("override-config") {
            dynamic |= value.split(',').filter_map(|s| s.split_once('=')).any(|(k,_)|k.trim() == DYNAMIC);
        }
        if option != "-config" && option != "--config" { continue; }
        config = true;
        let path = spec.working_dir.as_deref().unwrap_or(Path::new(".")).join(value);
        let Ok(metadata) = std::fs::metadata(&path) else { return "Fox.pdaUnknown"; };
        if metadata.len() > 4 * 1024 * 1024 { return "Fox.pdaUnknown"; }
        let Ok(text) = std::fs::read_to_string(path) else { return "Fox.pdaUnknown"; };
        for raw in text.lines() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() { continue; }
            if line.starts_with('@') { return "Fox.pdaUnknown"; }
            let Some((key,_)) = line.split_once('=') else { return "Fox.pdaUnknown"; };
            if key.trim().is_empty() { return "Fox.pdaUnknown"; }
            dynamic |= key.trim() == DYNAMIC;
        }
    }
    if dynamic { "Fox.pdaDynamic" } else if config { "" } else { "Fox.pdaUnknown" }
}
