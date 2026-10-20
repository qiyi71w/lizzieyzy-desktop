//! Java `readWeightModelName`, Transformer compatibility, and katagotraining parsers.
use super::{invalid, matches_pattern, validate_sha256, Catalog, ResourceResult, OFFICIAL_MODEL_PREFIX};
use fancy_regex::Regex;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fs::File, io::Read, path::Path};

pub const STRONGEST_URL: &str = "https://katagotraining.org/api/networks/get_strongest/";
pub const NETWORKS_URL: &str = "https://katagotraining.org/networks/";
pub const TRANSFORMER_MINIMUM_VERSION: &str = "1.17.0";

/// Reads no more than 4096 disk bytes and 1024 decompressed bytes, including gzip headers.
/// A renamed file is identified by its model header, never by its filename.
pub fn read_weight_model_name(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?.to_lowercase();
    if name.ends_with(".onnx.gz") || ![".bin", ".txt", ".gz"].iter().any(|ext| name.ends_with(ext)) || !path.is_file() {
        return None;
    }
    let mut prefix = Vec::new();
    File::open(path).ok()?.take(4096).read_to_end(&mut prefix).ok()?;
    let mut header = Vec::new();
    if name.ends_with(".gz") {
        // A truncated compressed prefix is expected; preserve bytes read before its end.
        let _ = flate2::read::GzDecoder::new(prefix.as_slice())
            .take(1024)
            .read_to_end(&mut header);
    } else {
        header.extend_from_slice(&prefix[..prefix.len().min(1024)]);
    }
    parse_weight_header(&header)
}
pub fn parse_weight_header(header: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(&header[..header.len().min(1024)]);
    static HEADER_PATTERN: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
        Regex::new(r"\A[\t\r\n ]*([!-~]{1,256})[\t\r\n ]+[1-9][0-9]{0,2}[\t\r\n ]+[1-9][0-9]{0,5}[\t\r\n ]+[1-9][0-9]{0,5}[\t\r\n ]").expect("model header pattern")
    });
    HEADER_PATTERN
        .captures(&text)
        .ok()
        .flatten()
        .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
}
pub fn is_transformer_weight(name: &str) -> bool {
    let lower = name.to_lowercase();
    let stem = [".bin.gz", ".txt.gz", ".bin", ".txt", ".gz"]
        .iter()
        .find_map(|ext| lower.strip_suffix(ext))
        .unwrap_or(&lower);
    stem.contains("tflrs") || matches_pattern(r"^kata1-tf[23]-b\d+c\d+-.*$", stem)
}
/// Java `B11ModelNotice.isB11`: the official B11 catalog name or its model header.
pub fn is_b11_model(name: &str) -> bool {
    let lower = name.trim().to_lowercase();
    let stem = [".bin.gz", ".txt.gz", ".bin", ".txt"]
        .iter()
        .find_map(|ext| lower.strip_suffix(ext))
        .unwrap_or(&lower);
    matches_pattern(r"^kata1-tf[23]-b11c768-[a-z0-9-]+$", stem)
        || matches_pattern(r"^b11c768h12nbt3tflrs(?:-[a-z0-9-]+)?$", stem)
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct WeightCompatibility {
    pub compatible: bool,
    pub minimum_version: Option<String>,
}
pub fn weight_compatibility(model_name: &str, engine_version: Option<&str>) -> WeightCompatibility {
    if !is_transformer_weight(model_name) {
        return WeightCompatibility {
            compatible: true,
            minimum_version: None,
        };
    }
    let compatible = engine_version.and_then(version_tuple).is_some_and(|v| v >= (1, 17, 0));
    WeightCompatibility {
        compatible,
        minimum_version: Some(TRANSFORMER_MINIMUM_VERSION.into()),
    }
}
/// KataGo's GTP `version` answers `<semver>+<model>`, so each part keeps its leading digits.
fn version_tuple(version: &str) -> Option<(u32, u32, u32)> {
    let mut parts = version.trim().trim_start_matches('v').split('.').map(|part| {
        let digits = part
            .find(|c: char| !c.is_ascii_digit())
            .map_or(part, |end| &part[..end]);
        digits.parse::<u32>().ok()
    });
    Some((parts.next()??, parts.next()??, parts.next().flatten().unwrap_or(0)))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct RemoteWeight {
    pub name: String,
    pub download_url: String,
    pub published_at: String,
    pub elo: String,
    pub sha256: Option<String>,
    pub size_bytes: Option<u64>,
    pub transformer: bool,
    pub recommended: bool,
    pub latest: bool,
}
impl RemoteWeight {
    pub fn file_name(&self) -> String {
        format!("{}.bin.gz", self.name)
    }
}
pub fn pinned_weights(catalog: &Catalog) -> Vec<RemoteWeight> {
    catalog
        .models
        .values()
        .map(|m| RemoteWeight {
            name: m.model_name().into(),
            download_url: catalog.model_url(m),
            published_at: m.published_at.clone(),
            elo: String::new(),
            sha256: Some(m.sha256.clone()),
            size_bytes: Some(m.size_bytes),
            transformer: m.architecture == "transformer",
            recommended: m.id == catalog.default_model_id,
            latest: false,
        })
        .collect()
}
/// Exact media origin/name binding and integrity metadata, as in `parseOfficialStrongest`.
pub fn parse_official_strongest(body: &str) -> ResourceResult<RemoteWeight> {
    let json: serde_json::Value = serde_json::from_str(body)?;
    let text = |key| json.get(key).and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    let name = text("name");
    if !matches_pattern(r"^kata1-(tf\d+-)?b\d+c\d+[A-Za-z0-9]*(?:-[A-Za-z0-9]+)+$", &name) {
        return Err(invalid("Unsupported official strongest network"));
    }
    let download_url = text("model_file");
    if download_url != format!("{OFFICIAL_MODEL_PREFIX}{name}.bin.gz") {
        return Err(invalid("Untrusted official strongest download URL"));
    }
    let sha256 = text("model_file_sha256").to_lowercase();
    validate_sha256(&sha256)?;
    let size_bytes = json
        .get("model_file_bytes")
        .and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
        .filter(|n| *n > 0)
        .ok_or_else(|| invalid("Official strongest network lacks size"))?;
    let date = text("created_at");
    if !matches_pattern(r"^\d{4}-\d{2}-\d{2}(?:T.*)?$", &date) {
        return Err(invalid("Official strongest network lacks creation date"));
    }
    let transformer = name.strip_prefix("kata1-").is_some_and(|s| s.starts_with("tf"));
    Ok(RemoteWeight {
        name,
        download_url,
        published_at: date[..10].into(),
        elo: String::new(),
        sha256: Some(sha256),
        size_bytes: Some(size_bytes),
        transformer,
        recommended: true,
        latest: false,
    })
}
fn captures(pattern: &str, input: &str, group: usize) -> Vec<String> {
    Regex::new(pattern)
        .expect("static HTML pattern")
        .captures_iter(input)
        .filter_map(|c| c.ok().and_then(|c| c.get(group).map(|m| m.as_str().to_string())))
        .collect()
}
fn clean_html(input: &str) -> String {
    let no_tags = Regex::new(r"(?s)<[^>]+>").expect("tag pattern").replace_all(input, " ");
    let mut decoded = no_tags.into_owned();
    for (from, to) in [
        ("&nbsp;", " "),
        ("&amp;", "&"),
        ("&quot;", "\""),
        ("&lt;", "<"),
        ("&gt;", ">"),
        ("&plusmn;", "±"),
        ("&#177;", "±"),
    ] {
        decoded = decoded.replace(from, to);
    }
    let entity = Regex::new(r"&#(x[0-9a-fA-F]+|[0-9]+);").expect("entity pattern");
    let mut out = String::new();
    let mut pos = 0;
    for c in entity.captures_iter(&decoded).flatten() {
        let whole = c.get(0).expect("whole capture");
        let code = c.get(1).expect("code capture").as_str();
        out.push_str(&decoded[pos..whole.start()]);
        let value = if let Some(hex) = code.strip_prefix('x') {
            u32::from_str_radix(hex, 16).ok()
        } else {
            code.parse().ok()
        };
        if let Some(ch) = value.and_then(char::from_u32) {
            out.push(ch);
        } else {
            out.push_str(whole.as_str());
        }
        pos = whole.end();
    }
    out.push_str(&decoded[pos..]);
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}
fn resolve_url(href: &str) -> String {
    let href = href.trim().replace("&amp;", "&");
    if href.starts_with("//") {
        format!("https:{href}")
    } else if href.starts_with('/') {
        format!("https://katagotraining.org{href}")
    } else if href.starts_with("https://") || href.starts_with("http://") {
        href
    } else if href.is_empty() {
        String::new()
    } else {
        format!("https://katagotraining.org/networks/{href}")
    }
}
/// Java's table parser: preserves table order, deduplicates names case-insensitively.
/// HTML-only entries have no invented hash/size and use the >1MiB download rule.
pub fn parse_official_weight_table(html: &str) -> ResourceResult<Vec<RemoteWeight>> {
    let tables = captures(r#"(?is)<table class="table mt-3">(.*?)</table>"#, html, 1);
    if tables.is_empty() {
        return Err(invalid("Unable to parse KataGo weight table"));
    }
    let badge = |pattern: &str| {
        let url = captures(pattern, html, 1)
            .first()
            .map(|s| resolve_url(s))
            .unwrap_or_default();
        let name = captures(pattern, html, 2)
            .first()
            .map(|s| clean_html(s))
            .unwrap_or_default();
        (url, name)
    };
    let strongest = badge(r#"(?is)Strongest confidently-rated network:</span>\s*<a href="([^"]+)">([^<]+)</a>"#);
    let latest = badge(r#"(?is)Latest network:</span>\s*<a href="([^"]+)">([^<]+)</a>"#);
    let mut seen = BTreeSet::new();
    let mut weights = Vec::new();
    for table in tables {
        for row in captures(r"(?is)<tr[^>]*>(.*?)</tr>", &table, 1) {
            let cells = captures(r"(?is)<td[^>]*>(.*?)</td>", &row, 1);
            if cells.len() < 4 {
                continue;
            }
            let name = clean_html(&cells[0]);
            let hrefs = captures(r#"(?is)<a[^>]*href="([^"]+)""#, &cells[3], 1);
            let download_url = resolve_url(hrefs.first().map(String::as_str).unwrap_or(""));
            if name.is_empty() || download_url.is_empty() || !seen.insert(name.to_lowercase()) {
                continue;
            }
            // Never allow table text to become a path outside the weights directory.
            if !matches_pattern(r"^[A-Za-z0-9][A-Za-z0-9._-]*$", &name) {
                continue;
            }
            let matches = |badge: &(String, String)| {
                (!badge.0.is_empty() && badge.0.eq_ignore_ascii_case(&download_url))
                    || (!badge.1.is_empty() && badge.1.eq_ignore_ascii_case(&name))
            };
            weights.push(RemoteWeight {
                transformer: is_transformer_weight(&name),
                recommended: matches(&strongest),
                latest: matches(&latest),
                name,
                download_url,
                published_at: clean_html(&cells[1]),
                elo: clean_html(&cells[2]),
                sha256: None,
                size_bytes: None,
            });
        }
    }
    Ok(weights)
}
