//! The command line of a KataGo analysis process: made from a GTP engine command (Java
//! `AnalysisEngineCommandHelper`) and tuned for a launch (`KataGoRuntimeHelper
//! .optimizeAnalysisEngineCommand`).

use std::path::{Path, PathBuf};

use super::gtp::{join_command, prepare_launch, split_command};

/// An analysis command made from an engine's GTP command.
#[derive(Clone, Debug, PartialEq)]
pub struct Generated {
    pub command: String,
    /// The `analysis.cfg` beside the engine's config, when its location could be resolved; the
    /// caller creates it from KataGo's example when it is missing.
    pub config: Option<PathBuf>,
}

fn sibling_config(config: &Path) -> PathBuf {
    match config.parent().filter(|d| !d.as_os_str().is_empty()) {
        Some(dir) => dir.join("analysis.cfg"),
        None => PathBuf::from("analysis.cfg"),
    }
}

/// `AnalysisEngineCommandHelper.fromSavedEngine`: the standalone `gtp` subcommand becomes
/// `analysis`, the `-config`/`--config` file becomes the `analysis.cfg` beside it, and
/// `-quit-without-waiting` is added. `roots` resolve relative paths like a launch does. The error
/// is the Java message key.
pub fn from_engine_command(command: &str, roots: &[PathBuf]) -> Result<Generated, &'static str> {
    let mut parts = split_command(command.trim());
    if parts.first().is_none_or(String::is_empty) {
        return Err("AnalysisEngineCommandHelper.emptyCommand");
    }
    let gtp = parts
        .iter()
        .position(|t| t.eq_ignore_ascii_case("gtp"))
        .ok_or("AnalysisEngineCommandHelper.missingGtp")?;
    parts[gtp] = "analysis".into();
    let at = parts
        .iter()
        .position(|t| t == "-config" || t == "--config")
        .filter(|&i| i + 1 < parts.len())
        .ok_or("AnalysisEngineCommandHelper.missingConfig")?;
    // The launch resolves the config against the engine's root; its arguments are the command's
    // tokens after the program.
    let config = prepare_launch(command, roots)
        .and_then(|spec| spec.args.get(at).map(PathBuf::from))
        .filter(|p| p.is_absolute())
        .map(|p| sibling_config(&p));
    parts[at + 1] = sibling_config(Path::new(&parts[at + 1])).to_string_lossy().into_owned();
    if !parts.iter().any(|t| t == "-quit-without-waiting") {
        parts.push("-quit-without-waiting".into());
    }
    let command = join_command(&parts).ok_or("AnalysisEngineCommandHelper.missingConfig")?;
    Ok(Generated { command, config })
}

fn override_key(entry: &str) -> String {
    entry.split('=').next().unwrap_or("").trim().to_lowercase()
}

fn has_override(parts: &[String], key: &str) -> bool {
    let key = key.to_lowercase();
    parts
        .windows(2)
        .filter(|w| w[0] == "-override-config")
        .any(|w| w[1].split(',').any(|e| !e.trim().is_empty() && override_key(e) == key))
}

/// `appendOverrideConfig`: added to the first `-override-config` unless that names the key.
fn append_override(parts: &mut Vec<String>, entry: &str) {
    let key = override_key(entry);
    if let Some(i) = parts.iter().position(|t| t == "-override-config") {
        match parts.get_mut(i + 1) {
            None => parts.push(entry.into()),
            Some(existing)
                if existing
                    .split(',')
                    .any(|e| !e.trim().is_empty() && override_key(e) == key) => {}
            Some(existing) if existing.trim().is_empty() => *existing = entry.into(),
            Some(existing) => {
                existing.push(',');
                existing.push_str(entry);
            }
        }
        return;
    }
    parts.push("-override-config".into());
    parts.push(entry.into());
}

/// `appendManagedAnalysisSearchThreads`: an empty `numSearchThreads` drops the GTP alias KataGo
/// rejects next to the analysis one.
fn append_search_threads(parts: &mut Vec<String>, threads: u32) {
    append_override(parts, "numSearchThreads=");
    append_override(parts, &format!("numSearchThreadsPerAnalysisThread={threads}"));
}

/// `Utils.getRecommendedKataGoThreads` for `processors` logical processors.
pub fn recommended_threads(processors: u32) -> u32 {
    let processors = processors.max(1);
    if processors <= 4 {
        processors
    } else {
        (processors - 1).clamp(2, 16)
    }
}

/// `resolveAppleSiliconAnalysisProfile` (not batch): analysis threads and search threads per
/// analysis thread.
fn apple_profile(max_visits: u32, recommended: u32) -> (u32, u32) {
    let budget = recommended.clamp(4, 16);
    let visits = max_visits.max(1);
    let (per_thread, parallel) = match visits {
        0..=8 => (1, 8),
        9..=36 => (2, 6),
        37..=100 => (2, 5),
        101..=220 => (3, 4),
        _ => ((budget / 3).clamp(2, 4), 3),
    };
    let mut threads = parallel.min((budget / per_thread).max(1)).max(2);
    if visits <= 12 && budget >= 6 {
        threads = threads.max(budget.min(8));
    }
    (threads, per_thread)
}

/// `optimizeAnalysisEngineCommand` for a launch searching `max_visits` per request: a KataGo
/// command reports variations of `pv_len` moves (`analysisPVLen`, the 分支长度 setting); small
/// searches use few search threads. `apple_threads` is the recommended thread count when the
/// command runs this application's KataGo on Apple Silicon, which gets its own thread profile.
/// Overrides already in the command are kept.
pub fn optimize(command: &str, max_visits: u32, pv_len: u32, apple_threads: Option<u32>) -> String {
    let mut parts = split_command(command.trim());
    if parts.first().is_none_or(String::is_empty) {
        return command.to_string();
    }
    let build = |parts: &[String]| join_command(parts).unwrap_or_else(|| command.to_string());
    let search_override =
        has_override(&parts, "numSearchThreadsPerAnalysisThread") || has_override(&parts, "numSearchThreads");
    let analysis_override = has_override(&parts, "numAnalysisThreads");
    let mut changed = false;
    if command.to_lowercase().contains("katago") && pv_len > 0 && !has_override(&parts, "analysisPVLen") {
        append_override(&mut parts, &format!("analysisPVLen={pv_len}"));
        changed = true;
    }
    if let Some(recommended) = apple_threads.filter(|_| command.to_lowercase().contains(" analysis")) {
        let (threads, per_thread) = apple_profile(max_visits, recommended);
        if !analysis_override {
            append_override(&mut parts, &format!("numAnalysisThreads={threads}"));
        }
        if !search_override {
            append_search_threads(&mut parts, per_thread);
        }
        return build(&parts);
    }
    if max_visits <= 36 && !search_override {
        append_search_threads(&mut parts, (max_visits / 10).max(1));
        return build(&parts);
    }
    if changed {
        build(&parts)
    } else {
        command.to_string()
    }
}

/// `resolveWholeGameAnalysisProfile`: throughput rather than quick-analysis latency. Explicit
/// command overrides stay authoritative, as in Java's wholeGameThroughput branch.
pub fn whole_game(command: &str, pv_len: u32, recommended: u32) -> String {
    let mut parts = split_command(command.trim());
    if parts.first().is_none_or(String::is_empty) || !command.to_lowercase().contains("katago") {
        return command.to_string();
    }
    if pv_len > 0 { append_override(&mut parts, &format!("analysisPVLen={pv_len}")); }
    let budget = recommended.clamp(2, 24);
    let parallel = (budget / 2).clamp(2, 8);
    append_override(&mut parts, &format!("numAnalysisThreads={parallel}"));
    if !has_override(&parts, "numSearchThreads") && !has_override(&parts, "numSearchThreadsPerAnalysisThread") {
        append_search_threads(&mut parts, (budget / parallel).clamp(1, 2));
    }
    join_command(&parts).unwrap_or_else(|| command.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_searches_get_few_threads_and_existing_overrides_stay() {
        assert_eq!(
            optimize("katago analysis -model m.bin.gz -config a.cfg", 2, 15, None),
            "katago analysis -model m.bin.gz -config a.cfg -override-config \
             analysisPVLen=15,numSearchThreads=,numSearchThreadsPerAnalysisThread=1"
        );
        assert_eq!(
            optimize(
                "katago analysis -override-config analysisPVLen=9,numSearchThreads=4",
                2,
                15,
                None
            ),
            "katago analysis -override-config analysisPVLen=9,numSearchThreads=4"
        );
        assert_eq!(
            optimize("katago analysis -config a.cfg", 200, 15, None),
            "katago analysis -config a.cfg -override-config analysisPVLen=15"
        );
        assert_eq!(optimize("other analysis", 200, 15, None), "other analysis");
    }

    #[test]
    fn apple_silicon_launches_follow_java_table_and_keep_existing_overrides() {
        assert_eq!(
            optimize("katago analysis -config a.cfg", 2, 15, Some(recommended_threads(10))),
            "katago analysis -config a.cfg -override-config \
             analysisPVLen=15,numAnalysisThreads=8,numSearchThreads=,numSearchThreadsPerAnalysisThread=1"
        );
        assert_eq!(
            optimize("katago analysis -override-config numAnalysisThreads=2", 30, 0, Some(recommended_threads(8))),
            "katago analysis -override-config numAnalysisThreads=2,numSearchThreads=,numSearchThreadsPerAnalysisThread=2"
        );
        assert_eq!(
            optimize("katago analysis", 500, 15, Some(recommended_threads(16))),
            "katago analysis -override-config \
             analysisPVLen=15,numAnalysisThreads=3,numSearchThreads=,numSearchThreadsPerAnalysisThread=4"
        );
    }

    #[test]
    fn generation_needs_gtp_and_config() {
        let g = from_engine_command("katago gtp -model m.bin.gz -config cfg/gtp.cfg", &[]).unwrap();
        assert_eq!(
            g.command,
            "katago analysis -model m.bin.gz -config cfg/analysis.cfg -quit-without-waiting"
        );
        assert_eq!(
            from_engine_command("katago -config a.cfg", &[]),
            Err("AnalysisEngineCommandHelper.missingGtp")
        );
        assert_eq!(
            from_engine_command("katago gtp", &[]),
            Err("AnalysisEngineCommandHelper.missingConfig")
        );
    }
}
