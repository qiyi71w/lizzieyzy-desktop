//! GTP text: command lines, response framing, analysis lines and vertices.
//!
//! Every command this client writes carries a numeric id. The framer reports each response under
//! the id of the command that produced it, and `info` lines of an analysis stream under the id of
//! the analysis command that started the stream, so their owner is known exactly.

use std::path::{Path, PathBuf};

use crate::record::{BoardSize, Point};

/// `Utils.splitCommand`: tokens separated by single spaces; `'…'` and `"…"` group text, and a
/// quoted empty string is kept as an empty token.
pub fn split_command(line: &str) -> Vec<String> {
    #[derive(PartialEq)]
    enum State {
        Normal,
        Quote,
        Double,
    }
    let mut out = Vec::new();
    let mut param = String::new();
    let mut quoted = false;
    let mut state = State::Normal;
    for c in line.chars() {
        match state {
            State::Quote if c == '\'' => {
                state = State::Normal;
                quoted = true;
            }
            State::Double if c == '"' => {
                state = State::Normal;
                quoted = true;
            }
            State::Quote | State::Double => param.push(c),
            State::Normal => {
                match c {
                    '\'' => state = State::Quote,
                    '"' => state = State::Double,
                    ' ' => {
                        if quoted || !param.is_empty() {
                            let token = std::mem::take(&mut param);
                            out.push(if out.is_empty() {
                                token.trim().to_string()
                            } else {
                                token
                            });
                        }
                    }
                    _ => param.push(c),
                }
                quoted = false;
            }
        }
    }
    if quoted || !param.is_empty() {
        out.push(param);
    }
    out
}

/// Options whose value is a file path (`CommandLaunchHelper.FILE_PATH_OPTIONS`).
pub(crate) const FILE_PATH_OPTIONS: [&str; 12] = [
    "-model",
    "--model",
    "-config",
    "--config",
    "-human-model",
    "--human-model",
    "-analysis-model",
    "--analysis-model",
    "-evaluation-model",
    "--evaluation-model",
    "-cacerts",
    "--cacerts",
];

/// The program, its arguments and working directory for one engine command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchSpec {
    pub program: String,
    pub args: Vec<String>,
    pub working_dir: Option<PathBuf>,
}

fn looks_relative_file(token: &str) -> bool {
    !token.is_empty() && !Path::new(token).is_absolute() && !token.starts_with('-')
}

/// `CommandLaunchHelper.prepare`: a root where the relative executable exists (scored by how many
/// relative file options also resolve there) becomes the working directory and the relative paths
/// are resolved against it; otherwise the directory of the absolute executable, or the common
/// parent of the executable and absolute file options when that is the executable's parent.
/// `roots` are the candidate roots in Java's order (current directory, then the application
/// location and its ancestors). `None` for an empty command line.
pub fn prepare_launch(command: &str, roots: &[PathBuf]) -> Option<LaunchSpec> {
    let mut parts = split_command(command.trim());
    if parts.is_empty() || parts[0].is_empty() {
        return None;
    }
    let is_option = |t: &str| FILE_PATH_OPTIONS.contains(&t);
    let mut best: Option<(&PathBuf, i32)> = None;
    for root in roots {
        let mut score = 0;
        if looks_relative_file(&parts[0]) {
            if !root.join(&parts[0]).is_file() {
                continue;
            }
            score += 6;
        }
        for w in parts.windows(2) {
            if is_option(&w[0]) && looks_relative_file(&w[1]) && root.join(&w[1]).exists() {
                score += 3;
            }
        }
        if best.is_none_or(|(_, s)| score > s) {
            best = Some((root, score));
        }
    }
    let working_dir = match best {
        Some((root, score)) if score > 0 => {
            if looks_relative_file(&parts[0]) {
                parts[0] = root.join(&parts[0]).to_string_lossy().into_owned();
            }
            for i in 0..parts.len().saturating_sub(1) {
                if is_option(&parts[i]) && looks_relative_file(&parts[i + 1]) && root.join(&parts[i + 1]).exists() {
                    parts[i + 1] = root.join(&parts[i + 1]).to_string_lossy().into_owned();
                }
            }
            Some(root.clone())
        }
        _ => infer_from_absolute(&parts),
    };
    let program = parts.remove(0);
    Some(LaunchSpec {
        program,
        args: parts,
        working_dir,
    })
}

fn absolute_dir(token: &str) -> Option<PathBuf> {
    let p = Path::new(token.trim());
    if !p.is_absolute() {
        return None;
    }
    if p.is_dir() {
        return Some(p.to_path_buf());
    }
    p.parent().filter(|d| d.is_dir()).map(Path::to_path_buf)
}

fn infer_from_absolute(parts: &[String]) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut add = |d: Option<PathBuf>| {
        if let Some(d) = d {
            if !dirs.contains(&d) {
                dirs.push(d);
            }
        }
    };
    add(absolute_dir(&parts[0]));
    for w in parts.windows(2) {
        if FILE_PATH_OPTIONS.contains(&w[0].as_str()) || w[0] == "-jar" {
            add(absolute_dir(&w[1]));
        }
    }
    let first = dirs.first()?.clone();
    let mut common = Some(first.clone());
    for d in &dirs[1..] {
        common = common.and_then(|c| c.ancestors().find(|a| d.starts_with(a)).map(Path::to_path_buf));
    }
    match common {
        Some(c) if c.parent().is_some() && first.parent() == Some(c.as_path()) => Some(c),
        _ => Some(first),
    }
}

// ------------------------------------------------------------------------ vertices

const COLUMNS: &[u8] = b"ABCDEFGHJKLMNOPQRSTUVWXYZ";

/// GTP vertex of a point: column letter without `I`, row counted from the bottom.
pub fn vertex(size: BoardSize, p: Point) -> String {
    let col = COLUMNS.get(p.x as usize).map(|&c| c as char).unwrap_or('?');
    format!("{col}{}", size.height - p.y)
}

/// Text that is neither `pass` nor a point of this board.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InvalidVertex;

/// A GTP vertex on this board; `Ok(None)` for `pass`.
pub fn parse_vertex(size: BoardSize, text: &str) -> Result<Option<Point>, InvalidVertex> {
    let t = text.trim();
    if t.eq_ignore_ascii_case("pass") {
        return Ok(None);
    }
    let mut chars = t.chars();
    let letter = chars.next().ok_or(InvalidVertex)?.to_ascii_uppercase();
    let x = COLUMNS.iter().position(|&c| c as char == letter).ok_or(InvalidVertex)? as u16;
    let row: u16 = chars.as_str().parse().map_err(|_| InvalidVertex)?;
    if row == 0 || row > size.height || x >= size.width {
        return Err(InvalidVertex);
    }
    Ok(Some(Point {
        x,
        y: size.height - row,
    }))
}

// ------------------------------------------------------------------------ framing

#[derive(Clone, Debug, PartialEq)]
pub enum Frame {
    /// The engine began answering command `id` (everything written before it is consumed).
    Header { id: u64 },
    /// One `info …` line of the analysis stream answering command `id`.
    Info { id: u64, line: String },
    /// A complete response (or the end of an analysis stream).
    Response { id: u64, ok: bool, text: String },
}

/// Splits engine stdout into frames attributed to command ids.
#[derive(Default)]
pub struct Framer {
    current: Option<(u64, bool)>,
    text: String,
}

impl Framer {
    pub fn feed(&mut self, line: &str, out: &mut Vec<Frame>) {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() {
            self.finish(out);
            return;
        }
        let Some((id, _)) = self.current else {
            let (ok, rest) = match line.as_bytes()[0] {
                b'=' => (true, &line[1..]),
                b'?' => (false, &line[1..]),
                _ => return, // output outside a response is not attributable
            };
            let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
            let id = rest[..digits].parse().unwrap_or(0);
            self.current = Some((id, ok));
            self.text = rest[digits..].trim().to_string();
            out.push(Frame::Header { id });
            return;
        };
        if line.starts_with("info ") {
            out.push(Frame::Info {
                id,
                line: line.to_string(),
            });
            return;
        }
        if !self.text.is_empty() {
            self.text.push('\n');
        }
        self.text.push_str(line);
    }

    /// Ends the open response, if any (blank line or end of output).
    pub fn finish(&mut self, out: &mut Vec<Frame>) {
        if let Some((id, ok)) = self.current.take() {
            out.push(Frame::Response {
                id,
                ok,
                text: std::mem::take(&mut self.text),
            });
        }
    }
}

// ------------------------------------------------------------------------ analysis lines

/// The engine's analysis wire format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dialect {
    /// KataGo: win rate and prior as fractions, score fields present.
    Kata,
    /// Leela Zero family: win rate and prior in hundredths of a percent.
    Leela,
    /// Sai: Leela percentages and an `areas` score in ten-thousandths of a point.
    Sai,
    /// Sayuri: Leela percentages and a side-to-move `scoreLead`.
    Sayuri,
    /// Leela 0.11: stderr summary tables, with percentages already in 0–100.
    Leela011,
}

/// One candidate move (`MoveData`). Percentages are 0–100 for the side to move.
#[derive(Clone, Debug, PartialEq)]
pub struct CandidateLine {
    /// `None` for a pass.
    pub point: Option<Point>,
    pub visits: u64,
    /// Root allocation (`edgeVisits`), else `visits` (`MoveData.allocationVisits`).
    pub allocation_visits: u64,
    pub winrate: f64,
    pub score_lead: Option<f64>,
    pub score_stdev: Option<f64>,
    pub prior: f64,
    pub lcb: Option<f64>,
    pub order: u32,
    pub is_symmetry: bool,
    pub pv: Vec<Option<Point>>,
}

/// Candidates of one analysis line, sorted by `order`, and the `rootInfo visits` if present.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InfoLine {
    pub candidates: Vec<CandidateLine>,
    pub root_visits: Option<u64>,
}

const STREAM_TAGS: [&str; 5] = [
    "ownership",
    "ownershipStdev",
    "pvVisits",
    "pvEdgeVisits",
    "movesOwnership",
];

pub fn parse_info(line: &str, dialect: Dialect, size: BoardSize) -> InfoLine {
    let (infos, root) = match line.split_once(" rootInfo ") {
        Some((a, b)) => (a, Some(b)),
        None => (line, None),
    };
    let root_visits = root.and_then(|r| {
        let t: Vec<&str> = r.split_whitespace().collect();
        t.iter()
            .position(|x| *x == "visits")
            .and_then(|i| t.get(i + 1)?.parse().ok())
    });
    let scale = match dialect {
        Dialect::Kata => 100.0,
        Dialect::Leela | Dialect::Sai | Dialect::Sayuri => 0.01,
        Dialect::Leela011 => 1.0,
    };
    let mut candidates = Vec::new();
    for chunk in infos.split("info ").filter(|c| !c.trim().is_empty()) {
        let toks: Vec<&str> = chunk.split_whitespace().collect();
        let mut point = Err(InvalidVertex);
        let mut c = CandidateLine {
            point: None,
            visits: 0,
            allocation_visits: 0,
            winrate: 0.0,
            score_lead: None,
            score_stdev: None,
            prior: 0.0,
            lcb: None,
            order: 0,
            is_symmetry: false,
            pv: Vec::new(),
        };
        let mut edge = None;
        let mut i = 0;
        while i + 1 < toks.len() {
            let (key, v) = (toks[i], toks[i + 1]);
            if key == "pv" {
                // `Branch`: the variation ends at the first move that is not on the board
                // (`resign`, a coordinate outside it).
                c.pv = toks[i + 1..]
                    .iter()
                    .take_while(|t| !STREAM_TAGS.contains(t))
                    .map_while(|t| parse_vertex(size, t).ok())
                    .collect();
                break;
            }
            let num = || v.parse::<f64>().ok();
            match key {
                "move" => point = parse_vertex(size, v),
                "visits" => c.visits = v.parse().unwrap_or(0),
                "edgeVisits" => edge = v.parse().ok(),
                "winrate" => c.winrate = num().unwrap_or(0.0) * scale,
                "scoreLead" => c.score_lead = num(),
                "scoreMean" if c.score_lead.is_none() => c.score_lead = num(),
                "areas" if dialect == Dialect::Sai => c.score_lead = num().map(|x| x / 10000.0),
                "scoreStdev" => c.score_stdev = num(),
                "prior" => c.prior = num().unwrap_or(0.0) * scale,
                "lcb" => c.lcb = num().map(|x| x * scale),
                "order" => c.order = v.parse().unwrap_or(0),
                "isSymmetryOf" => c.is_symmetry = true,
                _ => {}
            }
            i += 2;
        }
        let Ok(p) = point else { continue };
        c.point = p;
        c.allocation_visits = edge.unwrap_or(c.visits);
        candidates.push(c);
    }
    candidates.sort_by_key(|c| c.order);
    InfoLine {
        candidates,
        root_visits,
    }
}

/// `MoveData.fromSummaryLeela0110`: a candidate in a pondering summary table.
/// Other diagnostic lines (including malformed candidates) are not moves.
pub fn parse_leela011_summary(line: &str, size: BoardSize) -> Option<CandidateLine> {
    let (coordinate, rest) = line.trim_start().split_once(" -> ")?;
    let point = parse_vertex(size, coordinate).ok()?;
    let (visits, rest) = rest.trim_start().split_once(" (W: ")?;
    let visits = visits.parse().ok()?;
    let (winrate, rest) = rest.split_once("%)")?;
    let (_, rest) = rest.rsplit_once(" (N: ")?;
    let (prior, pv) = rest.split_once("%) PV: ")?;
    let pv: Vec<_> = pv
        .split_whitespace()
        .map_while(|v| parse_vertex(size, v).ok())
        .collect();
    if pv.is_empty() {
        return None;
    }
    Some(CandidateLine {
        point,
        visits,
        allocation_visits: visits,
        winrate: winrate.trim().parse().ok()?,
        score_lead: None,
        score_stdev: None,
        prior: prior.trim().parse().ok()?,
        lcb: None,
        order: 0,
        is_symmetry: false,
        pv,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_follows_java_quoting() {
        assert_eq!(
            split_command(r#""/a b/katago" gtp -model "/m x.bin.gz" -config c.cfg"#),
            vec!["/a b/katago", "gtp", "-model", "/m x.bin.gz", "-config", "c.cfg"]
        );
        assert_eq!(split_command("a  'b c'd \"\" e"), vec!["a", "b cd", "", "e"]);
    }

    #[test]
    fn vertices_skip_i_and_count_rows_from_the_bottom() {
        let s = BoardSize::STANDARD;
        assert_eq!(vertex(s, Point { x: 8, y: 0 }), "J19");
        assert_eq!(parse_vertex(s, "q16"), Ok(Some(Point { x: 15, y: 3 })));
        assert_eq!(parse_vertex(s, "pass"), Ok(None));
        assert!(parse_vertex(BoardSize { width: 9, height: 9 }, "K5").is_err());
    }
}
