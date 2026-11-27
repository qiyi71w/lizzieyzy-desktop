//! Explanations for an engine that cannot start: Windows loader error 206 in its output (Java
//! `EngineOutputDiagnostic`) and a packaged engine built for a newer macOS than the running one
//! (Java `MacOsEngineCompatibility`).
use crate::resources::{Catalog, Probe, ResourceReceipt};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::Path;
use std::sync::LazyLock;
use std::time::Duration;
use ts_rs::TS;

/// Why an engine failed to start, when the failure has a specific explanation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type")]
pub enum StartupDiagnosis {
    /// Windows reported error 206 loading a DLL: a file name or path is too long.
    PathTooLong,
    /// The packaged engine needs macOS `minimum` or later; this Mac runs `actual`.
    MacOsTooOld { minimum: String, actual: String },
}

impl fmt::Display for StartupDiagnosis {
    /// The English explanation, for logs and the failure channels that show a plain message.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StartupDiagnosis::PathTooLong => {
                f.write_str("Windows reported a file name or path that is too long (error 206)")
            }
            StartupDiagnosis::MacOsTooOld { minimum, actual } => write!(
                f,
                "This bundled KataGo requires macOS {minimum} or later; this Mac uses {actual}"
            ),
        }
    }
}

/// A macOS version Java compares: one to three numbers of up to three digits.
pub const MACOS_VERSION: &str = r"^\d{1,3}(?:\.\d{1,3}){0,2}$";

/// The operating system engines start on: its name as `std::env::consts::OS` gives it, and its
/// version where an engine requirement compares it (macOS only).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HostSystem {
    pub os: String,
    pub version: Option<String>,
}

impl HostSystem {
    /// Reads the running macOS version (`sw_vers -productVersion`, the version Java's
    /// `os.version` reports) through `probe`; other systems have no version to compare.
    pub fn detect(probe: &dyn Probe, os: &str) -> HostSystem {
        let version = (os == "macos")
            .then(|| {
                probe
                    .run("/usr/bin/sw_vers", &["-productVersion".into()], Duration::from_secs(3))
                    .ok()
            })
            .flatten()
            .filter(|out| out.success())
            .map(|out| out.stdout.trim().to_string())
            .filter(|v| !v.is_empty());
        HostSystem { os: os.into(), version }
    }

    /// `MacOsEngineCompatibility.requireSupported`: the packaged engine at `executable` (the store
    /// receipt beside it names its catalog asset) refuses to start on a macOS older than the
    /// asset's recorded minimum. Other systems, engines without a receipt (the user's own) and
    /// assets without a minimum are not checked; an unreadable version or receipt is no refusal.
    pub fn refuse_engine(&self, catalog: &Catalog, executable: &Path) -> Option<StartupDiagnosis> {
        if self.os != "macos" {
            return None;
        }
        let actual = self.version.as_deref()?;
        let directory = executable.parent()?;
        let receipt = directory.join("resource.json");
        if std::fs::metadata(&receipt).ok()?.len() > 65536 {
            return None;
        }
        let receipt: ResourceReceipt = serde_json::from_slice(&std::fs::read(receipt).ok()?).ok()?;
        if directory.join(&receipt.executable) != executable {
            return None;
        }
        let minimum = catalog.assets.get(&receipt.asset_id)?.minimum_macos.as_deref()?;
        is_older(actual, minimum).then(|| StartupDiagnosis::MacOsTooOld {
            minimum: minimum.into(),
            actual: actual.into(),
        })
    }
}

/// `MacOsEngineCompatibility.isOlder`: numeric, missing parts are zero; unparseable is not older.
fn is_older(actual: &str, required: &str) -> bool {
    static FORMAT: LazyLock<fancy_regex::Regex> =
        LazyLock::new(|| fancy_regex::Regex::new(MACOS_VERSION).expect("version pattern"));
    if !FORMAT.is_match(actual).unwrap_or(false) || !FORMAT.is_match(required).unwrap_or(false) {
        return false;
    }
    let parts = |v: &str| {
        let mut n = [0u32; 3];
        for (slot, part) in n.iter_mut().zip(v.split('.')) {
            *slot = part.parse().unwrap_or(0);
        }
        n
    };
    parts(actual) < parts(required)
}

/// Java `EngineOutputDiagnostic.DLL_TOKEN`: a quoted or bracketed name, or a bare path token,
/// ending in `.dll`.
const DLL_TOKEN: &str =
    r#"(?:['"‘“\[<]([^'"’”\]>\r\n]+?(?i:\.dll))['"’”\]>]|(?<![a-zA-Z0-9_.+~\\/-])([a-zA-Z0-9_.+~\\/:-]+?(?i:\.dll)))"#;

struct Rule {
    pattern: fancy_regex::Regex,
    /// A loader failure whose category comes from its error code (Java rules 15–18). The other
    /// rules name a fixed category that is never the path error.
    load_failure: bool,
}

/// Java's rules in their order: the first one matching a clause decides its category.
static RULES: LazyLock<Vec<Rule>> = LazyLock::new(|| {
    let fixed = [
        format!(
            r"(?i)(?:the\s+code\s+execution\s+cannot\s+proceed\s+because|the\s+program\s+can'?t\s+start\s+because)\s+{DLL_TOKEN}\s+(?:was\s+not\s+found|is\s+missing)"
        ),
        format!(
            r#"(?i)(?:^|[\s"'‘“\[<:;]){DLL_TOKEN}\s+(?:was\s+not\s+found|could\s+not\s+be\s+found|cannot\s+be\s+found|can\s+not\s+be\s+found|is\s+missing(?:\s+from\s+(?:your\s+)?computer)?|is\s+not\s+found)"#
        ),
        format!(
            r"(?i)(?:could\s+not\s+find|cannot\s+find|can\s+not\s+find|failed\s+to\s+find|unable\s+to\s+find|missing)\s+(?:required\s+)?(?:library|module|dll|file)?\s*[:=-]?\s*{DLL_TOKEN}"
        ),
        format!(
            r"(?i)(?:error\s+while\s+loading\s+shared\s+libraries|cannot\s+open\s+shared\s+object\s+file)\s*:\s*{DLL_TOKEN}"
        ),
        format!(
            r"(?i){DLL_TOKEN}\s*:\s*(?:the\s+specified\s+module\s+could\s+not\s+be\s+found|can'?t\s+find\s+dependent\s+libraries|no\s+such\s+file|cannot\s+open\s+shared\s+object\s+file)"
        ),
        format!(r"(?i)(?:由于找不到|计算机中丢失|丢失\s*(?:dll)?)\s*{DLL_TOKEN}"),
        format!(
            r"(?i)(?:procedure\s+entry\s+point|entry\s*point|entrypoint).*?(?:could\s+not\s+be\s+located|not\s+found).*?in\s+(?:the\s+dynamic\s+link\s+library\s+)?{DLL_TOKEN}"
        ),
        format!(r"(?i)(?:无法定位|无法找到)程序输入点.*?于动态链接库\s*{DLL_TOKEN}"),
        format!(
            r"(?i)(?:bad\s+image|invalid\s+image(?:\s+format)?|not\s+a\s+valid\s+win32\s+application).*?{DLL_TOKEN}"
        ),
        format!(
            r"(?i){DLL_TOKEN}\s+(?:is\s+either\s+not\s+designed\s+to\s+run\s+on\s+windows|is\s+not\s+a\s+valid\s+win32\s+application)"
        ),
        format!(
            r"(?i){DLL_TOKEN}\s*:\s*(?:%1\s+is\s+not\s+a\s+valid\s+win32\s+application|not\s+a\s+valid\s+win32\s+application|bad\s+image)"
        ),
        format!(r"(?i)(?:initialization\s+routine\s+failed|dll\s+initialization\s+failed).*?{DLL_TOKEN}"),
        format!(r"(?i){DLL_TOKEN}\s+(?:initialization\s+routine\s+failed|initialization\s+failed)"),
        format!(
            r"(?i){DLL_TOKEN}\s*:\s*(?:a\s+dynamic\s+link\s+library\s+\(dll\)\s+initialization\s+routine\s+failed|initialization\s+routine\s+failed)"
        ),
    ];
    let load = [
        format!(
            r"(?i)(?:could\s+not\s+load|cannot\s+load|can\s+not\s+load|failed\s+to\s+load|unable\s+to\s+load|error\s+loading)\s+(?:library|module|dll|file)?\s*[:=-]?\s*{DLL_TOKEN}"
        ),
        format!(
            r"(?i){DLL_TOKEN}\s+(?:failed\s+to\s+load|could\s+not\s+be\s+loaded|cannot\s+be\s+loaded|can\s+not\s+be\s+loaded)"
        ),
        format!(r"(?i)loadlibrary(?:[a-w]*)?\s+(?:failed|error).*?{DLL_TOKEN}"),
        format!(r"(?i)(?:无法加载|加载失败|加载动态链接库失败)\s*[:=-]?\s*{DLL_TOKEN}"),
    ];
    let rule = |pattern: &String, load_failure| Rule {
        pattern: fancy_regex::Regex::new(pattern).expect("diagnostic pattern"),
        load_failure,
    };
    fixed
        .iter()
        .map(|p| rule(p, false))
        .chain(load.iter().map(|p| rule(p, true)))
        .collect()
});

static NEGATED_FAILURE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
    fancy_regex::Regex::new(r"(?i)\b(?:(?:did\s+not|didn'?t|does\s+not|doesn'?t|never)\s+fail|(?:is|was|are|were)?\s*not\s+missing|(?:no|zero|0)\s+missing|without\s+(?:any\s+)?(?:error|missing|failing)|not\s+found\s+to\s+be\s+missing|(?:was|is)\s+found\b(?!\s+(?:to\s+be\s+missing|that|where)))")
        .expect("negation pattern")
});
static SUCCESS_WORDS: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
    fancy_regex::Regex::new(r"(?i)\b(?:success|succeeded|successfully|successful)\b").expect("success pattern")
});
static WINDOWS_PATH_LENGTH_ERROR: LazyLock<fancy_regex::Regex> =
    LazyLock::new(|| fancy_regex::Regex::new(r"\berror\s+code\s*[:=]?\s*206\b").expect("error 206 pattern"));

/// `EngineOutputDiagnostic.parse` reduced to its path error: some `;`-separated clause of the
/// engine's output names a DLL whose loading failed with Windows error code 206. A clause that
/// reports success, negates the failure, or that an earlier rule (a missing DLL, an entry point,
/// a bad image or a failed initialisation) claims is not this error.
pub fn diagnose_output<S: AsRef<str>>(lines: &[S]) -> Option<StartupDiagnosis> {
    let path_error = |clause: &str| {
        if !clause.to_lowercase().contains(".dll")
            || SUCCESS_WORDS.is_match(clause).unwrap_or(false)
            || NEGATED_FAILURE.is_match(clause).unwrap_or(false)
        {
            return false;
        }
        for rule in RULES.iter() {
            let Ok(Some(found)) = rule.pattern.captures(clause) else {
                continue;
            };
            // Java skips a rule whose DLL name does not clean up and tries the next one.
            if found
                .get(1)
                .or_else(|| found.get(2))
                .is_none_or(|m| !clean_dll_name(m.as_str()))
            {
                continue;
            }
            return rule.load_failure
                && WINDOWS_PATH_LENGTH_ERROR
                    .is_match(&clause.to_lowercase())
                    .unwrap_or(false);
        }
        false
    };
    lines
        .iter()
        .flat_map(|line| line.as_ref().lines())
        .flat_map(|line| line.split(';'))
        .map(str::trim)
        .any(|clause| !clause.is_empty() && path_error(clause))
        .then_some(StartupDiagnosis::PathTooLong)
}

/// `EngineOutputDiagnostic.cleanDllBasename`: whether the matched name, without enclosing quotes or
/// brackets, trailing punctuation and its directories, is a plain `*.dll` file name.
fn clean_dll_name(raw: &str) -> bool {
    let mut s = raw.trim();
    while let (Some(first), Some(last)) = (s.chars().next(), s.chars().last()) {
        let enclosed = matches!(
            (first, last),
            ('\'', '\'') | ('"', '"') | ('‘', '’') | ('“', '”') | ('[', ']') | ('<', '>') | ('(', ')')
        );
        if !enclosed || s.chars().count() < 2 {
            break;
        }
        s = s[first.len_utf8()..s.len() - last.len_utf8()].trim();
    }
    while let Some(last) = s.chars().last() {
        if !matches!(
            last,
            '.' | ',' | ':' | ';' | '!' | '?' | '\'' | '"' | '’' | '”' | ']' | '>'
        ) || s.to_lowercase().ends_with(".dll")
        {
            break;
        }
        s = s[..s.len() - last.len_utf8()].trim();
    }
    let name = s.rsplit(['/', '\\']).next().unwrap_or(s).trim();
    name.to_lowercase().ends_with(".dll") && name.chars().count() > 4 && !name.contains([' ', '\t'])
}
