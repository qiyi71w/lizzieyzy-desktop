use super::KnowledgeQuery;

pub(super) const BROAD_TAGS: &[&str] = &[
    "角部",
    "边上",
    "中腹",
    "中心",
    "定式",
    "布局",
    "方向",
    "大场",
    "变化",
    "价值判断",
    "局部轻重",
    "先手价值",
    "实地",
    "外势",
    "问题手",
    "急所",
    "死活",
    "手筋",
    "形",
    "形状",
    "先手",
    "后手",
    "ai定式",
];
const JOSEKI_WORDS: &[&str] = &[
    "定式",
    "joseki",
    "星位",
    "小目",
    "点三三",
    "三三",
    "挂角",
    "守角",
    "夹攻",
    "低挂",
    "高挂",
    "大雪崩",
    "雪崩",
    "大斜",
    "太斜",
    "双飞燕",
    "妖刀",
    "托退",
    "中国流",
    "三连星",
    "小林流",
    "布局",
    "开局",
];
const LIFE_WORDS: &[&str] = &[
    "死活",
    "做活",
    "杀棋",
    "活棋",
    "真眼",
    "假眼",
    "眼形",
    "急所",
    "对杀",
    "劫活",
    "双活",
    "曲四",
    "刀把五",
    "梅花六",
    "葡萄六",
    "板六",
    "金鸡独立",
    "直三",
    "弯三",
    "中手",
    "破眼",
    "扑入",
];
const TESUJI_WORDS: &[&str] = &[
    "手筋",
    "倒扑",
    "吃回",
    "接不归",
    "老鼠偷油",
    "征子",
    "枷",
    "滚打包收",
    "挖",
    "扭十字",
    "断",
    "窥",
    "扳头",
    "双打",
    "弃子",
    "扑",
];

pub(super) fn insert(values: &mut Vec<String>, value: impl Into<String>) {
    let value = value.into();
    if !values.contains(&value) {
        values.push(value);
    }
}
pub(super) fn has(values: &[String], value: &str) -> bool {
    values.iter().any(|v| v == value)
}

pub(super) fn tokens(text: &str, engine: bool) -> Vec<String> {
    let lower = text.to_lowercase();
    let mut out = Vec::new();
    for token in lower.split(|c: char| {
        "，。！？、；：,.!?;:()[]【】/".contains(c)
            || matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{000b}' | '\u{000c}')
            || (engine && matches!(c, '_' | '-'))
    }) {
        let token = token.trim_matches(|c: char| c <= '\u{0020}');
        if !token.is_empty() {
            insert(&mut out, token);
        }
    }
    out
}

// MatchEngine deliberately does not reject a column beyond a smaller board's width.
// PatternSearchEngine and geometry do, so keep their parsing policies separate.
pub(super) fn point(value: &str, size: u32, pattern: bool) -> Option<(i32, i32)> {
    let value = value.trim_matches(|c: char| c <= '\u{0020}').to_uppercase();
    let bytes = value.as_bytes();
    if !(2..=3).contains(&bytes.len()) || !bytes[1..].iter().all(u8::is_ascii_digit) {
        return None;
    }
    let letters = if pattern {
        "ABCDEFGHJKLMNOPQRSTUVWXYZ"
    } else {
        "ABCDEFGHJKLMNOPQRST"
    };
    let col = letters.find(bytes[0] as char)? as i32;
    let number: u32 = value[1..].parse().ok()?;
    if number < 1 || number > size || (pattern && col >= size as i32) {
        return None;
    }
    Some((size as i32 - number as i32, col))
}

fn add_point(out: &mut Vec<String>, (row, col): (i32, i32), size: u32) {
    let size = size as i32;
    let x = col.min(size - 1 - col);
    let y = row.min(size - 1 - row);
    let min = x.min(y);
    let max = x.max(y);
    insert(
        out,
        if x <= 5 && y <= 5 {
            "corner"
        } else if min <= 3 {
            "side"
        } else {
            "center"
        },
    );
    match min {
        0 => insert(out, "first-line"),
        1 => insert(out, "second-line"),
        2 => insert(out, "third-line"),
        3 => insert(out, "fourth-line"),
        _ => (),
    }
    if x == 3 && y == 3 {
        insert(out, "4-4");
    }
    if x == 2 && y == 2 {
        insert(out, "3-3");
    }
    if (x == 2 && y == 3) || (x == 3 && y == 2) {
        insert(out, "3-4");
    }
    if min <= 3 && (4..=6).contains(&max) {
        insert(out, "approach");
    }
    if min <= 2 && max <= 5 {
        insert(out, "eye-shape");
    }
}

pub(super) fn from_gtp<'a>(points: impl IntoIterator<Item = &'a str>, size: u32, pattern: bool) -> Vec<String> {
    let mut out = Vec::new();
    for p in points {
        if let Some(p) = point(p, size, pattern) {
            add_point(&mut out, p, size);
        }
    }
    out
}

pub(super) fn move_features(query: &KnowledgeQuery, pattern: bool) -> Vec<String> {
    let limit = if pattern { 8 } else { 10 };
    let moves = &query.recent_moves[query.recent_moves.len().saturating_sub(limit)..];
    let coordinates: Vec<_> = moves
        .iter()
        .map(|mv| {
            if pattern {
                point(&mv.gtp, query.board_size, true)
            } else {
                Some((mv.row, mv.col))
            }
        })
        .collect();
    let mut out = Vec::new();
    for p in coordinates.iter().flatten() {
        add_point(&mut out, *p, query.board_size);
    }
    if coordinates.len() >= 2 {
        if let (Some(last), Some(previous)) = (coordinates[coordinates.len() - 1], coordinates[coordinates.len() - 2]) {
            let dx = (last.1 - previous.1).abs();
            let dy = (last.0 - previous.0).abs();
            if dx + dy == 1 {
                insert(&mut out, "contact");
            }
            if (dx == 1 && dy == 2) || (dx == 2 && dy == 1) {
                insert(&mut out, "knight-move");
            }
            if (dx == 0 && dy == 2) || (dx == 2 && dy == 0) {
                insert(&mut out, "jump");
            }
        }
    }
    for feature in from_gtp([query.played_move.as_str()], query.board_size, pattern) {
        insert(&mut out, feature);
    }
    out
}

pub(super) struct Features {
    pub tokens: Vec<String>,
    pub moves: Vec<String>,
    pub candidates: Vec<String>,
    pub pv: Vec<String>,
    pub points: Vec<String>,
    pub region: &'static str,
    pub joseki_intent: bool,
    pub life_intent: bool,
    pub tesuji_intent: bool,
}
impl Features {
    pub fn new(query: &KnowledgeQuery) -> Self {
        let tokens = tokens(&query.text, true);
        let intent = |words: &[&str]| words.iter().any(|word| token_has(&tokens, word));
        let joseki_intent = intent(JOSEKI_WORDS);
        let life_intent = intent(LIFE_WORDS);
        let tesuji_intent = intent(TESUJI_WORDS);
        let mut points = Vec::new();
        insert(&mut points, query.played_move.clone());
        for p in query.candidate_moves.iter().chain(&query.principal_variation) {
            insert(&mut points, p.clone());
        }
        for mv in &query.recent_moves {
            insert(&mut points, mv.gtp.clone());
        }
        for stone in &query.board_snapshot {
            insert(&mut points, stone.point.clone());
        }
        let mut region_counts = [0; 3];
        for p in std::iter::once(&query.played_move)
            .chain(query.candidate_moves.iter().take(3))
            .chain(query.principal_variation.iter().take(5))
            .chain(query.recent_moves.iter().map(|mv| &mv.gtp))
        {
            if let Some((row, col)) = point(p, query.board_size, false) {
                let x = col.min(query.board_size as i32 - 1 - col);
                let y = row.min(query.board_size as i32 - 1 - row);
                region_counts[if x <= 5 && y <= 5 {
                    0
                } else if x.min(y) <= 3 {
                    1
                } else {
                    2
                }] += 1;
            }
        }
        let region = if region_counts[0] >= region_counts[1] && region_counts[0] >= region_counts[2] {
            "corner"
        } else if region_counts[1] >= region_counts[2] {
            "side"
        } else {
            "center"
        };
        Self {
            tokens,
            moves: move_features(query, false),
            candidates: from_gtp(
                query.candidate_moves.iter().map(String::as_str),
                query.board_size,
                false,
            ),
            pv: from_gtp(
                query.principal_variation.iter().map(String::as_str),
                query.board_size,
                false,
            ),
            points,
            region,
            joseki_intent,
            life_intent,
            tesuji_intent,
        }
    }
    pub fn intent(&self, kind: &str) -> bool {
        match kind {
            "joseki" => self.joseki_intent,
            "life_death" => self.life_intent,
            "tesuji" => self.tesuji_intent,
            _ => false,
        }
    }
    pub fn any_feature(&self, value: &str) -> bool {
        let value = value.to_lowercase();
        self.moves
            .iter()
            .chain(&self.candidates)
            .chain(&self.pv)
            .any(|f| value.contains(f) || f.contains(&value))
    }
}
pub(super) fn token_has(tokens: &[String], value: &str) -> bool {
    let value = value.to_lowercase();
    let value = value.trim_matches(|c: char| c <= '\u{0020}');
    tokens.iter().any(|t| value.contains(t) || t.contains(value))
}
pub(super) fn specific<'a>(values: impl IntoIterator<Item = &'a str>, tokens: &[String]) -> bool {
    values
        .into_iter()
        .any(|value| !BROAD_TAGS.contains(&value.to_lowercase().as_str()) && token_has(tokens, value))
}
