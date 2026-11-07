use super::*;
use crate::{record::Stone, settings::DisplaySettings};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
};
pub const POLICY_CONFIG: &str = include_str!("../../resources/strength/humansl40_policy.cfg");
#[derive(Clone, Debug, PartialEq)]
pub struct Capture {
    pub game: Value,
    pub base: Value,
    pub ordinary: Option<StrengthReport>,
}
/// Freeze at most 210 moves and their cached analysis, retaining an ordinary report of the
/// complete main line (Java does not truncate that report). The caller owns document identity.
pub fn capture(input: &MainLineInput, settings: &DisplaySettings) -> Result<Capture, String> {
    capture_impl(input, settings, true)
}
/// Freeze all history without requiring analysis, for HumanSL background policy prefetch.
pub fn preparation_snapshot(input: &MainLineInput, settings: &DisplaySettings) -> Result<Capture, String> {
    capture_impl(input, settings, false)
}
fn capture_impl(input: &MainLineInput, settings: &DisplaySettings, require_analysis: bool) -> Result<Capture, String> {
    if input.board_width != 19 || input.board_height != 19 {
        return Err("HumanSL40.boardSize".into());
    }
    let limit = if require_analysis { 211 } else { usize::MAX };
    let mut moves = Vec::new();
    let mut base = Vec::new();
    for (i, p) in input.positions.iter().take(limit).enumerate() {
        if i > 0 {
            let action = p.action.as_ref().ok_or("HumanSL40.setupUnsupported")?;
            if !action.ordinary {
                return Err("HumanSL40.setupUnsupported".into());
            }
            moves.push(json!([
                if action.color == Stone::Black { "B" } else { "W" },
                action.coordinate
            ]));
        }
        base.push(if require_analysis {
            position_snapshot(p.analysis.as_ref())
        } else {
            Value::Null
        });
    }
    if input.has_setup_stones || input.game["initialStones"].as_array().is_some_and(|a| !a.is_empty()) {
        return Err("HumanSL40.setupUnsupported".into());
    }
    let mut game = input.game.clone();
    let object = game.as_object_mut().ok_or("HumanSL40.needGame")?;
    object.remove("initialStones");
    object.remove("analyzeTurns");
    object.insert("id".into(), json!("strength40-capture"));
    object.insert("maxVisits".into(), json!(1));
    object.insert("moves".into(), json!(moves));
    object.insert("boardXSize".into(), json!(input.board_width));
    object.insert("boardYSize".into(), json!(input.board_height));
    validate_history(&game)?;
    let base = json!(base);
    if require_analysis {
        validate_game(&game, &base)?;
    }
    Ok(Capture {
        game,
        base,
        ordinary: require_analysis.then(|| estimate_with_model(input, &StrengthModel::xgboost20tun(), settings)),
    })
}
/// Java positionSnapshot, after adapting all scores to side-to-move. Candidate priors are
/// already 0–1 here (Java snapshot divides its 0–100 MoveData.policy by 100).
pub fn position_snapshot(analysis: Option<&AnalysisInput>) -> Value {
    let Some(a) = analysis else {
        return Value::Null;
    };
    let Some(score) = a.score_lead.filter(|v| v.is_finite()) else {
        return Value::Null;
    };
    if !a.kata || !a.has_payload || !a.winrate.is_finite() || a.winrate < 0.0 || a.winrate > 100.0 {
        return Value::Null;
    }
    let mut unique = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for c in a.candidates.iter().chain(&a.extra_candidates) {
        if !c.kata || c.score_lead.is_none_or(|v| !v.is_finite()) {
            continue;
        }
        if seen.insert(c.coordinate.to_uppercase()) {
            unique.push(c);
        }
    }
    unique.sort_by_key(|c| c.order);
    json!({"rootInfo":{"scoreMean":score,"winrate":a.winrate/100.0},"moveInfos":unique.iter().map(|c|json!({"move":c.coordinate,"order":c.order,"scoreMean":c.score_lead,"prior":positive(c.prior)})).collect::<Vec<_>>()})
}
pub fn has_analysis(base: &Value, turn: usize) -> bool {
    base.get(turn).is_some_and(Value::is_object)
        && base.get(turn + 1).is_some_and(Value::is_object)
        && base[turn]["moveInfos"].as_array().is_some_and(|a| !a.is_empty())
}
pub fn validate_history(game: &Value) -> Result<(), String> {
    let moves = game["moves"].as_array().ok_or("HumanSL40.needGame")?;
    if game["boardXSize"] != 19 || game["boardYSize"] != 19 || moves.is_empty() {
        return Err("HumanSL40.needGame".into());
    }
    validate_colors(moves)
}
fn validate_colors(moves: &[Value]) -> Result<(), String> {
    for (i, m) in moves.iter().enumerate() {
        let color = m[0].as_str().unwrap_or_default();
        if (color != "B" && color != "W") || (i > 0 && m[0] == moves[i - 1][0]) {
            return Err("HumanSL40.setupUnsupported".into());
        }
    }
    Ok(())
}
pub fn validate_game(game: &Value, base: &Value) -> Result<Vec<usize>, String> {
    let moves = game["moves"].as_array().ok_or("HumanSL40.setupUnsupported")?;
    if game["boardXSize"] != 19
        || game["boardYSize"] != 19
        || moves.len() > 210
        || base.as_array().map(Vec::len) != Some(moves.len() + 1)
        || game["initialStones"].as_array().is_some_and(|a| !a.is_empty())
    {
        return Err("HumanSL40.setupUnsupported".into());
    }
    validate_colors(moves)?;
    let mut counts = [0, 0];
    let mut turns = Vec::new();
    for (i, m) in moves.iter().enumerate() {
        if has_analysis(base, i) {
            turns.push(i);
            counts[usize::from(m[0] != "B")] += 1;
        }
    }
    if counts[0] < 6 || counts[1] < 6 {
        return Err("HumanSL40.needAnalysis".into());
    }
    Ok(turns)
}
pub fn request(game: &Value, turn: usize, profile: &str) -> Result<Value, String> {
    let moves = game["moves"].as_array().ok_or("Invalid turn")?;
    if turn > moves.len() {
        return Err("Invalid turn".into());
    }
    let mut q = game.clone();
    let object = q.as_object_mut().ok_or("Invalid game")?;
    object.remove("analyzeTurns");
    object.insert("moves".into(), json!(&moves[..turn]));
    object.insert("id".into(), json!(format!("strength40-policy-{profile}-{turn}")));
    object.insert("maxVisits".into(), json!(1));
    for (key, value) in [
        ("includePolicy", true),
        ("includeOwnership", false),
        ("includePVVisits", false),
        ("includeMovesOwnership", false),
    ] {
        object.insert(key.into(), json!(value));
    }
    object.insert("analysisPVLen".into(), json!(12));
    object.insert("overrideSettings".into(),json!({"reportAnalysisWinratesAs":"SIDETOMOVE","humanSLProfile":profile,"ignorePreRootHistory":false,"rootNumSymmetriesToSample":2,"humanSLRootExploreProbWeightless":0.0,"humanSLCpuctPermanent":2.0}));
    Ok(q)
}
pub fn validate_policy(response: &Value, id: &str) -> Result<Vec<f64>, String> {
    let invalid = || "HumanSL40.invalidResponse".to_owned();
    if response.get("error").is_some()
        || response.get("warning").is_some()
        || response["isDuringSearch"].as_bool().unwrap_or(false)
        || response["id"] != id
    {
        return Err(invalid());
    }
    let policy = response["humanPolicy"]
        .as_array()
        .or_else(|| response["policy"].as_array())
        .filter(|a| a.len() == 362)
        .ok_or_else(invalid)?;
    let mut result = Vec::with_capacity(362);
    let mut total = 0.0;
    for p in policy {
        let p = p.as_f64().ok_or_else(invalid)?;
        if !p.is_finite() || (p < 0.0 && p != -1.0) || p > 1.0 {
            return Err(invalid());
        }
        total += p.max(0.0);
        result.push(p);
    }
    if (total - 1.0).abs() > 0.001 {
        return Err(invalid());
    }
    Ok(result)
}
/// Recursively sorted object keys, preserving Java Number.toString's floating-point `.0`.
pub fn canonical(value: &Value) -> String {
    match value {
        Value::Object(o) => {
            let mut keys: Vec<_> = o.keys().collect();
            keys.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
            format!(
                "{{{}}}",
                keys.iter()
                    .map(|k| format!("{}:{}", java_quote(k), canonical(&o[*k])))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
        Value::Array(a) => format!("[{}]", a.iter().map(canonical).collect::<Vec<_>>().join(",")),
        Value::String(s) => java_quote(s),
        Value::Number(n) if n.is_f64() => java_double(n.as_f64().expect("JSON number")),
        _ => value.to_string(),
    }
}
fn java_quote(value: &str) -> String {
    use std::fmt::Write;
    let mut result = String::from("\"");
    let mut previous = '\0';
    for c in value.chars() {
        match c {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '/' if previous == '<' => result.push_str("\\/"),
            '\u{8}' => result.push_str("\\b"),
            '\u{c}' => result.push_str("\\f"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            c if c < ' ' || ('\u{80}'..'\u{a0}').contains(&c) || ('\u{2000}'..'\u{2100}').contains(&c) => {
                write!(result, "\\u{:04x}", c as u32).expect("String write");
            }
            _ => result.push(c),
        }
        previous = c;
    }
    result.push('"');
    result
}
fn java_double(value: f64) -> String {
    if value == 0.0 {
        return if value.is_sign_negative() { "-0.0" } else { "0.0" }.into();
    }
    let sign = if value.is_sign_negative() { "-" } else { "" };
    let text = value.abs().to_string();
    let (mantissa, exponent) = text
        .split_once(['e', 'E'])
        .map(|(m, e)| (m, e.parse::<i32>().expect("float exponent")))
        .unwrap_or((&text, 0));
    let point = mantissa.find('.').unwrap_or(mantissa.len()) as i32 + exponent;
    let mut digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let leading = digits.len() - digits.trim_start_matches('0').len();
    let point = point - leading as i32;
    digits.drain(..leading);
    while digits.len() > 1 && digits.ends_with('0') {
        digits.pop();
    }
    if !(0.001..10_000_000.0).contains(&value.abs()) {
        let tail = if digits.len() == 1 { "0" } else { &digits[1..] };
        format!("{sign}{}.{tail}E{}", &digits[..1], point - 1)
    } else if point <= 0 {
        format!("{sign}0.{}{}", "0".repeat((-point) as usize), digits)
    } else if point as usize >= digits.len() {
        format!("{sign}{digits}{}.0", "0".repeat(point as usize - digits.len()))
    } else {
        format!("{sign}{}.{}", &digits[..point as usize], &digits[point as usize..])
    }
}
pub fn cache_key(fingerprint: &str, query: &Value) -> String {
    hash(format!("strength40-policy-v2|{fingerprint}|{HUMAN_SHA}|{}", canonical(query)).as_bytes())
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn absolute(path: &Path) -> io::Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.into())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}
/// Return argv without shell quoting. Writes the embedded policy-only config to the cache.
pub fn policy_command(engine: &Path, human: &Path, cache: &Path) -> io::Result<Vec<String>> {
    fs::create_dir_all(cache)?;
    let config = cache.join("policy-analysis.cfg");
    fs::write(&config, POLICY_CONFIG.as_bytes())?;
    Ok(vec![
        absolute(engine)?.to_string_lossy().into_owned(),
        "analysis".into(),
        "-model".into(),
        absolute(human)?.to_string_lossy().into_owned(),
        "-config".into(),
        config.to_string_lossy().into_owned(),
        "-override-config".into(),
        format!("logDir={}", cache.join("engine-logs").display()),
        "-quit-without-waiting".into(),
    ])
}
fn file_hash(path: &Path) -> io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut buf = [0; 65536];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        digest.update(&buf[..n]);
    }
    Ok(format!("{:x}", digest.finalize()))
}
pub fn fingerprint(engine: &Path, cache: &Path) -> io::Result<String> {
    Ok(format!(
        "{}|{}",
        file_hash(engine)?,
        file_hash(&cache.join("policy-analysis.cfg"))?
    ))
}
pub fn report(captured: &Capture, policies: &Value, settings: &DisplaySettings) -> Result<StrengthReport, String> {
    let len = captured.base.as_array().ok_or("Invalid base")?.len();
    let mut prefix = json!({});
    for profile in PROFILES {
        let source = policies[profile]
            .as_array()
            .filter(|a| a.len() >= len)
            .ok_or_else(|| format!("Incomplete HumanSL profile: {profile}"))?;
        prefix[profile] = json!(&source[..len]);
    }
    let moves = humansl40::extract(&captured.game["moves"], &captured.base, &prefix, 19, true)?;
    let ordinary = captured.ordinary.as_ref().ok_or("HumanSL40.needAnalysis")?;
    let mut black = Vec::new();
    let mut white = Vec::new();
    for h in moves {
        let side = if h.color == "B" {
            &ordinary.black
        } else {
            &ordinary.white
        };
        if let Some(source) = side.samples.iter().find(|s| s.move_number == h.number) {
            let mut s = source.clone();
            s.score_loss = Some(h.loss);
            s.first_choice = h.rank == 0;
            s.ai_rank = h.rank;
            s.category = estimator::category(h.loss);
            s.move_rank = crate::display::classify(settings, -positive(source.winrate_loss), Some(-positive(h.loss)));
            s.score_equivalent_loss = h.loss;
            s.complexity = h.complexity;
            s.adjusted_weight = estimator::weight(h.complexity, h.loss);
            s.human = Some(h);
            if s.color == Stone::Black {
                black.push(s);
            } else {
                white.push(s);
            }
        }
    }
    Ok(estimator::assemble(black, white, &StrengthModel::humansl40()))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn policy_validation_and_query() {
        let game = json!({"boardXSize":19,"boardYSize":19,"moves":[["B","Q16"],["W","D4"]],"analyzeTurns":[2]});
        let q = request(&game, 1, "rank_5d").unwrap();
        assert_eq!(q["maxVisits"], 1);
        assert_eq!(q["moves"].as_array().unwrap().len(), 1);
        assert!(q.get("analyzeTurns").is_none());
        let mut policy = vec![-1.0; 362];
        policy[0] = 1.0;
        let mut response = json!({"id":"x","policy":policy});
        assert!(validate_policy(&response, "x").is_ok());
        response["warning"] = json!("");
        assert_eq!(
            validate_policy(&response, "x").unwrap_err(),
            "HumanSL40.invalidResponse"
        );
    }
    #[test]
    fn canonical_key_order() {
        let a: Value = serde_json::from_str("{\"z\":0,\"a\":{\"c\":1,\"b\":2}}").unwrap();
        let b = json!({"a":{"b":2,"c":1},"z":0});
        assert_eq!(canonical(&a), "{\"a\":{\"b\":2,\"c\":1},\"z\":0}");
        assert_eq!(cache_key("engine", &a), cache_key("engine", &b));
        assert_ne!(cache_key("engine", &a), cache_key("other", &a));
    }
    #[test]
    fn canonical_java_numbers_and_escaping() {
        assert_eq!(
            canonical(&json!([0.0, -0.0, 0.001, 0.0001, 1e7, 2.0])),
            "[0.0,-0.0,0.001,1.0E-4,1.0E7,2.0]"
        );
        assert_eq!(canonical(&json!("</script>\u{2028}")), "\"<\\/script>\\u2028\"");
    }
    #[test]
    fn policy_command_embeds_only_policy_configuration() {
        let temp = tempfile::tempdir().unwrap();
        let args = policy_command(Path::new("engine with spaces"), Path::new("human.bin.gz"), temp.path()).unwrap();
        assert_eq!(&args[1..3], &["analysis", "-model"]);
        assert_eq!(args.last().unwrap(), "-quit-without-waiting");
        assert_eq!(
            fs::read(temp.path().join("policy-analysis.cfg")).unwrap(),
            POLICY_CONFIG.as_bytes()
        );
        assert_eq!(
            fingerprint(temp.path().join("policy-analysis.cfg").as_path(), temp.path())
                .unwrap()
                .split('|')
                .count(),
            2
        );
    }
}
