use super::{
    data::{Catalog, JosekiLine, Library, Pattern, Problem},
    features::{self, Features, BROAD_TAGS},
    geometry, KnowledgeQuery,
};

struct Match {
    title: String,
    kind: String,
    confidence: i32,
    score: f64,
    reasons: Vec<String>,
    recognition: String,
    correct_idea: String,
    variations: Vec<String>,
    memory_cue: String,
    mistakes: Vec<String>,
    drills: Vec<String>,
    related: Vec<String>,
}
const CONFIDENCE: [&str; 5] = ["", "weak", "partial", "strong", "exact"];
fn confidence(score: f64, exact: bool) -> i32 {
    if exact && score >= 28.0 {
        4
    } else if score >= 21.0 {
        3
    } else if score >= 12.0 {
        2
    } else {
        1
    }
}
fn applicability(confidence: i32, kind: &str) -> String {
    match confidence {
        4 => "本局局部手顺、候选点和区域都高度一致，可以作为同型讲解。".into(),
        3 => "本局棋形和 KataGo 候选点相近，可以作为强相关型讲解，但仍要看全局厚薄。".into(),
        2 => format!(
            "本局只是像这个{}，老师应说“像这个型”，不能硬套结论。",
            if kind == "joseki" { "定式" } else { "棋形" }
        ),
        _ => "弱相关，只适合作为备用训练建议，不应进入主讲。".into(),
    }
}
impl Match {
    fn sort_score(&self) -> i32 {
        self.confidence * 1000
            + self.score as i32
            + if self.reasons.iter().any(|r| r.starts_with("explicit-intent:")) {
                8
            } else {
                0
            }
            + if self
                .reasons
                .iter()
                .any(|r| r.starts_with("answer-overlap") || r.starts_with("sequence-overlap"))
            {
                4
            } else {
                0
            }
            + if self.reasons.iter().any(|r| r.starts_with("geometry:")) {
                6
            } else {
                0
            }
    }
    fn grounded(&self) -> bool {
        if self.confidence == 4 {
            return true;
        }
        let mut signals = 0;
        for reason in &self.reasons {
            if reason == "geometry-strong-local-shape" {
                return true;
            }
            if reason
                .strip_prefix("sequence-overlap:")
                .and_then(|r| r.parse::<f64>().ok())
                .is_some_and(|r| r >= 3.0)
            {
                return true;
            }
            if ["specific-text:", "answer-", "candidate:", "pv:"]
                .iter()
                .any(|prefix| reason.starts_with(prefix))
            {
                signals += 1;
            }
        }
        self.confidence == 3 && signals >= 2
    }
    fn format(&self) -> String {
        let training = if self.related.is_empty() {
            self.drills.join("；")
        } else {
            self.related.join("、")
        };
        format!("匹配类型: {}\n名称: {}\n置信度: {}\n匹配依据: {}\n适用边界: {}\n识别特征: {}\n正确思路: {}\n常见变化: {}\n记忆法: {}\n常见误区: {}\n训练建议: {}\n老师使用边界: exact/strong 可以说“这是某某型”；partial 只能说“像某某型”；weak 不进入主讲。",
            self.kind, self.title, CONFIDENCE[self.confidence as usize], self.reasons.join(", "),
            applicability(self.confidence, &self.kind), self.recognition, self.correct_idea,
            self.variations.join("；"), self.memory_cue, self.mistakes.join("；"), training)
    }
}
fn reasons(values: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for value in values {
        features::insert(&mut out, value);
    }
    out.truncate(8);
    out
}
fn overlap<'a>(
    values: impl IntoIterator<Item = &'a str>,
    tokens: &[String],
    weight: f64,
    prefix: &str,
    reasons: &mut Vec<String>,
) -> f64 {
    let mut score = 0.0;
    for value in values {
        if features::token_has(tokens, value) {
            score += weight;
            reasons.push(format!("{prefix}:{value}"));
        }
    }
    score
}
fn sequence<'a>(values: impl IntoIterator<Item = &'a str>, points: &[String], reasons: &mut Vec<String>) -> f64 {
    let count = values.into_iter().filter(|v| features::has(points, v)).count();
    if count > 0 {
        reasons.push(format!("sequence-overlap:{count}"));
    }
    count as f64
}
fn related(catalog: &Catalog, tags: &[String], limit: usize) -> Vec<String> {
    let tokens: Vec<_> = tags
        .iter()
        .map(|t| t.to_lowercase())
        .filter(|t| !BROAD_TAGS.contains(&t.as_str()))
        .collect();
    if tokens.is_empty() {
        return Vec::new();
    }
    let mut candidates: Vec<_> = catalog
        .life_death_problems
        .iter()
        .chain(&catalog.tesuji_problems)
        .map(|p| {
            let score = p.tags.iter().filter(|t| features::token_has(&tokens, t)).count();
            (p, score)
        })
        .filter(|(_, score)| *score > 0)
        .collect();
    candidates.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| java_title_cmp(&a.0.title, &b.0.title)));
    candidates
        .into_iter()
        .take(limit)
        .map(|(p, _)| problem_label(p))
        .collect()
}
fn problem_label(p: &Problem) -> String {
    format!("{}({})", p.title, p.difficulty)
}
// String.compareTo compares UTF-16 code units, not UTF-8 bytes/scalars.
fn java_title_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

fn joseki(line: &JosekiLine, query: &KnowledgeQuery, f: &Features, catalog: &Catalog) -> Option<Match> {
    let mut score = 0.0;
    let mut r = Vec::new();
    // totalMoves is never supplied by TeacherEvidence: phaseFromMove therefore
    // always returns opening, including when moveNumber is greater than forty.
    if features::has(&line.phase, "opening") {
        score += 5.0;
        r.push("phase:opening".into());
    }
    if f.region == "corner" {
        score += 5.0;
        r.push("region:corner".into());
    }
    if f.joseki_intent {
        score += 10.0;
        r.push("explicit-intent:joseki".into());
    }
    score += overlap(
        line.tags
            .iter()
            .map(String::as_str)
            .chain([line.title.as_str(), line.family.as_str()]),
        &f.tokens,
        4.0,
        "text",
        &mut r,
    );
    for feature in &line.normalized_features {
        if f.any_feature(feature) {
            score += 4.0;
            r.push(format!("shape:{feature}"));
        }
    }
    let count = sequence(line.relative_sequence.iter().map(String::as_str), &f.points, &mut r);
    score += count * 5.0;
    if line.relative_sequence.len() > 1 && query.candidate_moves.contains(&line.relative_sequence[1]) {
        score += 5.0;
        r.push("katago-candidate-prefix".into());
    }
    if query.move_number <= 70 {
        score += 2.0;
        r.push("opening-timing".into());
    }
    if score < 8.0 {
        return None;
    }
    let conf = confidence(score, count >= 3.0 && !f.life_intent && !f.tesuji_intent);
    Some(Match {
        title: line.title.clone(),
        kind: "joseki".into(),
        confidence: conf,
        score,
        reasons: reasons(r),
        recognition: format!("识别为{}相关局部：看角部手顺、挂角/点三三位置和外势方向。", line.title),
        correct_idea: line.decision_rules.join(" "),
        variations: line
            .branches
            .iter()
            .take(3)
            .map(|b| format!("{}: {}", b.name, b.when_to_choose))
            .collect(),
        memory_cue: "定式先问方向，再问先手，最后才背手顺。".into(),
        mistakes: line.common_mistakes.clone(),
        drills: line
            .training_focus
            .iter()
            .map(|focus| format!("{}专项：{focus}", line.title))
            .collect(),
        related: related(catalog, &line.tags, 3),
    })
}
fn problem(
    p: &Problem,
    kind: &str,
    query: &KnowledgeQuery,
    f: &Features,
    geometry: &geometry::Context,
) -> Option<Match> {
    let mut score = 0.0;
    let mut r = Vec::new();
    let explicit = f.intent(kind);
    if p.region == f.region {
        score += 5.0;
        r.push(format!("region:{}", p.region));
    }
    if query.loss_score.unwrap_or(0.0) >= 2.0 {
        score += if kind == "life_death" { 4.0 } else { 3.0 };
        r.push("katago-loss".into());
    }
    if explicit {
        score += 12.0;
        r.push(format!("explicit-intent:{kind}"));
    }
    if f.joseki_intent && !explicit {
        score -= 8.0;
        r.push("penalty:joseki-query".into());
    }
    let text_values = || {
        p.tags
            .iter()
            .map(String::as_str)
            .chain([p.title.as_str(), p.objective.as_str()])
    };
    let specific = features::specific(text_values(), &f.tokens);
    if specific {
        score += 6.0;
        r.push(format!("specific-text:{kind}"));
    }
    score += overlap(text_values(), &f.tokens, 4.0, "text", &mut r);
    for tag in &p.tags {
        if f.any_feature(tag) {
            score += 3.0;
            r.push(format!("shape:{tag}"));
        }
    }
    let answer_overlap = sequence(p.correct_moves.iter().filter_map(|mv| mv.point()), &f.points, &mut r);
    score += answer_overlap * 12.0;
    let geo = geometry.match_problem(p);
    if let Some(geo) = &geo {
        score += geo.score as f64;
        r.push(format!(
            "geometry:{}:{}:{}/{}",
            geo.transform, geo.color_mode, geo.matched, geo.expected
        ));
        r.push(format!("liberties:{}", geo.liberty_score));
        if geo.ratio >= 0.72 {
            score += 6.0;
            r.push("geometry-strong-local-shape".into());
        }
    }
    let answer = |point: &str| p.correct_moves.iter().any(|mv| mv.point() == Some(point));
    if answer(&query.played_move) {
        score += 5.0;
        r.push("answer-played".into());
    }
    if query.candidate_moves.iter().any(|p| answer(p)) {
        score += 7.0;
        r.push("answer-candidate".into());
    }
    if query.principal_variation.iter().any(|p| answer(p)) {
        score += 5.0;
        r.push("answer-pv".into());
    }
    if kind == "life_death" && features::has(&f.moves, "eye-shape") {
        score += 5.0;
        r.push("eye-shape".into());
    }
    if kind == "tesuji" && (features::has(&f.moves, "contact") || features::has(&f.moves, "jump")) {
        score += 3.0;
        r.push("local-tesuji-relation".into());
    }
    if score < 8.0 {
        return None;
    }
    let exact = (answer_overlap >= 1.0 && specific && (explicit || score >= 24.0))
        || geo.as_ref().is_some_and(|geo| geo.ratio >= 0.72 && geo.matched >= 3);
    Some(Match {
        title: p.title.clone(),
        kind: kind.into(),
        confidence: confidence(score, exact),
        score,
        reasons: reasons(r),
        recognition: p
            .teaching
            .recognition
            .as_deref()
            .or(p.teaching.tesuji_idea.as_deref())
            .unwrap_or("先识别局部形状和双方气数。")
            .into(),
        correct_idea: p
            .teaching
            .explanation
            .as_deref()
            .or(p.teaching.tesuji_idea.as_deref())
            .unwrap_or("先找急所，再读失败手。")
            .into(),
        // JsonKnowledgeLoader intentionally never assigns TrainingMove.explanation.
        variations: p
            .correct_moves
            .iter()
            .take(2)
            .filter_map(|mv| mv.point())
            .map(|point| format!("{point}: 正确第一手"))
            .collect(),
        memory_cue: p.teaching.memory_cue.as_deref().unwrap_or("记住急所和次序。").into(),
        mistakes: p
            .failure_moves
            .iter()
            .take(2)
            .filter_map(|mv| mv.failure_point().map(|point| format!("{point}: {}", mv.why())))
            .collect(),
        drills: vec![format!("{}：先看题，不看答案，读清第一手和失败手。", p.title)],
        related: vec![problem_label(p)],
    })
}
fn pattern_matches(patterns: &[Pattern], catalog: &Catalog, query: &KnowledgeQuery, f: &Features) -> Vec<Match> {
    let moves = features::move_features(query, true);
    let candidates = features::from_gtp(query.candidate_moves.iter().map(String::as_str), query.board_size, true);
    let pv = features::from_gtp(
        query.principal_variation.iter().map(String::as_str),
        query.board_size,
        true,
    );
    let query_tokens = features::tokens(&query.text, false);
    let mut matches = Vec::new();
    for p in patterns {
        let mut score = 0.0;
        let mut r = Vec::new();
        if features::has(&p.levels, "intermediate") {
            score += 2.0;
            r.push("level:intermediate".into());
        }
        if features::has(&p.phase, "opening") {
            score += 4.0;
            r.push("phase:opening".into());
        }
        if features::has(&p.regions, f.region) {
            score += 5.0;
            r.push(format!("region:{}", f.region));
        }
        if query.move_number >= p.triggers.min_move_number.unwrap_or(0)
            && query.move_number <= p.triggers.max_move_number.unwrap_or(i32::MAX as u32)
            && (p.triggers.min_move_number.is_some() || p.triggers.max_move_number.is_some())
        {
            score += 2.0;
        }
        if let Some(min) = p.triggers.min_loss_score {
            if query.loss_score.unwrap_or(0.0) >= min {
                score += 3.0;
                r.push(format!("loss>={}", java_double(min)));
            }
        }
        for (values, haystack, weight, label) in [
            (&p.triggers.move_features, &moves, 5.0, "shape"),
            (&p.triggers.candidate_features, &candidates, 6.0, "candidate"),
            (&p.triggers.pv_features, &pv, 3.0, "pv"),
        ] {
            for value in values {
                if features::has(haystack, &value.to_lowercase()) {
                    score += weight;
                    r.push(format!("{label}:{value}"));
                }
            }
        }
        if !query_tokens.is_empty() {
            let values = [p.title.as_str(), p.category.as_str(), p.pattern_type.as_str()]
                .into_iter()
                .chain(p.tags.iter().map(String::as_str))
                .chain(p.aliases.iter().map(String::as_str))
                .chain(p.board_signals.iter().map(String::as_str))
                .chain([
                    p.teaching.recognition.as_str(),
                    p.teaching.correct_idea.as_str(),
                    p.teaching.memory_cue.as_str(),
                ]);
            let card_tokens: Vec<_> = values.flat_map(|s| features::tokens(s, false)).collect();
            for token in &query_tokens {
                if card_tokens
                    .iter()
                    .any(|cand| cand.contains(token) || token.contains(cand))
                {
                    score += 4.0;
                    r.push(format!("text:{token}"));
                }
            }
        }
        if p.category == "joseki" && f.region == "corner" {
            score += 3.0;
            r.push("joseki-opening-corner".into());
        }
        if p.category == "life_death"
            && query.loss_score.unwrap_or(0.0) >= 2.0
            && (f.region == "corner" || f.region == "side")
        {
            score += 3.0;
            r.push("life-death-risk".into());
        }
        if score < 8.0 {
            continue;
        }
        matches.push(Match {
            title: p.title.clone(),
            kind: p.category.clone(),
            score,
            confidence: if score >= 18.0 {
                3
            } else if score >= 10.0 {
                2
            } else {
                1
            },
            reasons: reasons(r),
            recognition: p.teaching.recognition.clone(),
            correct_idea: p.teaching.correct_idea.clone(),
            variations: p
                .variations
                .iter()
                .take(3)
                .map(|v| format!("{}: {}", v.name, v.when_to_choose))
                .collect(),
            memory_cue: p.teaching.memory_cue.clone(),
            mistakes: vec![p.teaching.common_mistake.clone()],
            drills: vec![p.teaching.drill.clone()],
            related: related(catalog, &p.tags, 2),
        });
    }
    matches.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| java_title_cmp(&a.title, &b.title))
    });
    matches.truncate(4);
    matches
}
// Shipped minLossScore values are ordinary finite decimals. Java appends .0 to
// integral doubles; Rust Display does not. No other doubles enter prompt text.
fn java_double(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.1}")
    } else {
        value.to_string()
    }
}

pub(super) fn match_text(library: &Library, query: &KnowledgeQuery) -> String {
    let f = Features::new(query);
    let geometry = geometry::Context::new(query);
    let mut matches = Vec::new();
    for line in &library.catalog.joseki_lines {
        if let Some(m) = joseki(line, query, &f, &library.catalog) {
            matches.push(m);
        }
    }
    for (problems, kind) in [
        (&library.catalog.life_death_problems, "life_death"),
        (&library.catalog.tesuji_problems, "tesuji"),
    ] {
        for p in problems {
            if let Some(m) = problem(p, kind, query, &f, &geometry) {
                matches.push(m);
            }
        }
    }
    matches.extend(pattern_matches(&library.patterns, &library.catalog, query, &f));
    matches.sort_by(|a, b| {
        b.sort_score()
            .cmp(&a.sort_score())
            .then_with(|| b.score.total_cmp(&a.score))
            .then_with(|| java_title_cmp(&a.title, &b.title))
    });
    // Java truncates to maxResults=8 before TeacherEvidence removes ungrounded
    // matches. Filtering first would expose matches Java never shows.
    matches
        .iter()
        .take(8)
        .filter(|m| !m.title.is_empty() && m.grounded())
        .take(2)
        .map(Match::format)
        .collect::<Vec<_>>()
        .join("\n\n")
}
