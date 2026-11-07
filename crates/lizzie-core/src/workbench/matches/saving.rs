//! Java EngineManager.writeToFile (1840–2011), fillEngineGameSettingStrings (1449–1549),
//! and SGFParser.formatCommentPk (2546–2689). Text comes from the fixed Java bundles.
use super::*;

impl Workbench {
    pub(super) fn match_text(&self, key: &str) -> String {
        use super::super::exporting::java_text;
        let language = self.active_language;
        match key {
            "EngineManager.doublePassFileName" => java_text(language, "EngineManager.doublePassFileName"),
            "EngineManager.outOfMoveFileName" => java_text(language, "EngineManager.outOfMoveFileName"),
            "Leelaz.black" => java_text(language, "Leelaz.black"),
            "Leelaz.white" => java_text(language, "Leelaz.white"),
            "Leelaz.win" => java_text(language, "Leelaz.win"),
            "Leelaz.result" => java_text(language, "Leelaz.result"),
            "SGFParse.seconds" => java_text(language, "SGFParse.seconds"),
            "SGFParse.black" => java_text(language, "SGFParse.black"),
            "SGFParse.white" => java_text(language, "SGFParse.white"),
            "SGFParse.winrate" => java_text(language, "SGFParse.winrate"),
            "SGFParse.leadWithKomi" => java_text(language, "SGFParse.leadWithKomi"),
            "SGFParse.leadJustScore" => java_text(language, "SGFParse.leadJustScore"),
            "SGFParse.stdev" => java_text(language, "SGFParse.stdev"),
            "SGFParse.playouts" => java_text(language, "SGFParse.playouts"),
            "SGFParse.komi" => java_text(language, "SGFParse.komi"),
            "SGFParse.pda" => java_text(language, "SGFParse.pda"),
            "SGFParse.wrn" => java_text(language, "SGFParse.wrn"),
            "SGFParse.moveTime" => java_text(language, "SGFParse.moveTime"),
            "SGFParse.startGameSgf" => java_text(language, "SGFParse.startGameSgf"),
            "SGFParse.blackTotalTime" => java_text(language, "SGFParse.blackTotalTime"),
            "SGFParse.whiteTotalTime" => java_text(language, "SGFParse.whiteTotalTime"),
            "SGFParse.totalVisits" => java_text(language, "SGFParse.totalVisits"),
            "LizzieFrame.currentRules.chinese" => java_text(language, "LizzieFrame.currentRules.chinese"),
            "LizzieFrame.currentRules.chn-ancient" => java_text(language, "LizzieFrame.currentRules.chn-ancient"),
            "LizzieFrame.currentRules.goe" => java_text(language, "LizzieFrame.currentRules.goe"),
            "MatchRules.option.japaneseKorean" => java_text(language, "MatchRules.option.japaneseKorean"),
            "MatchRules.option.agaBga" => java_text(language, "MatchRules.option.agaBga"),
            "MatchRules.option.newZealand" => java_text(language, "MatchRules.option.newZealand"),
            "MatchRules.option.trompTaylor" => java_text(language, "MatchRules.option.trompTaylor"),
            "MatchRules.sgf.begin" => java_text(language, "MatchRules.sgf.begin"),
            "MatchRules.sgf.end" => java_text(language, "MatchRules.sgf.end"),
            "MatchRules.details.target" => java_text(language, "MatchRules.details.target"),
            "MatchRules.details.side" => java_text(language, "MatchRules.details.side"),
            "MatchRules.details.status" => java_text(language, "MatchRules.details.status"),
            "MatchRules.details.actual" => java_text(language, "MatchRules.details.actual"),
            "MatchRules.details.unavailable" => java_text(language, "MatchRules.details.unavailable"),
            "MatchRules.status.CONFIRMED" => java_text(language, "MatchRules.status.CONFIRMED"),
            "MatchRules.unconfirmed" => java_text(language, "MatchRules.unconfirmed"),
            "MatchRules.reason.QUERY_UNSUPPORTED" => java_text(language, "MatchRules.reason.QUERY_UNSUPPORTED"),
            "MatchRules.black" => java_text(language, "MatchRules.black"),
            "MatchRules.white" => java_text(language, "MatchRules.white"),
            _ => java_text(language, key),
        }
    }

    fn match_rule_name(&self, rules: &crate::engine::rules::KataRules) -> String {
        use crate::engine::rules::KataRules;
        for (preset, key) in [
            ("chinese", "LizzieFrame.currentRules.chinese"),
            ("stone-scoring", "LizzieFrame.currentRules.chn-ancient"),
            ("japanese", "MatchRules.option.japaneseKorean"),
            ("aga", "MatchRules.option.agaBga"),
            ("new-zealand", "MatchRules.option.newZealand"),
            ("tromp-taylor", "MatchRules.option.trompTaylor"),
            ("goe", "LizzieFrame.currentRules.goe"),
        ] {
            if KataRules::preset(preset).is_some_and(|preset| preset == *rules) {
                return self.match_text(key);
            }
        }
        self.match_text("NewEngineGameDialog.matchRules.custom")
    }

    /// MatchRulesTexts.replaceSgfComment: one localized, replaceable snapshot block in the root.
    fn match_rules_comment(&self, current: &str) -> String {
        let Some(run) = self.matches.run.as_ref() else {
            return current.to_string();
        };
        let Some(target) = crate::engine::rules::KataRules::parse(&run.settings.rules) else {
            return current.to_string();
        };
        let text = |key| self.match_text(key);
        let confirmed = run.sides.iter().all(|participant| participant.rules_verified);
        let status = |confirmed| {
            text(if confirmed {
                "MatchRules.status.CONFIRMED"
            } else {
                "MatchRules.unconfirmed"
            })
        };
        let mut lines = vec![
            text("MatchRules.sgf.begin"),
            text("MatchRules.details.target").replace("{0}", &self.match_rule_name(&target)),
            status(confirmed),
        ];
        for side in [Stone::Black, Stone::White] {
            let index = side_index(run.first_is_black, side);
            let participant = &run.sides[index];
            let identity = if index == 0 {
                &run.settings.first
            } else {
                &run.settings.second
            };
            let label = if identity.name.is_empty() {
                identity.commands.clone()
            } else if identity.commands.is_empty() {
                identity.name.clone()
            } else {
                format!("{} ({})", identity.name, identity.commands)
            };
            let color = text(if side == Stone::Black {
                "MatchRules.black"
            } else {
                "MatchRules.white"
            });
            lines.push(
                text("MatchRules.details.side")
                    .replace("{0}", &color)
                    .replace("{1}", &label),
            );
            lines.push(text("MatchRules.details.status").replace("{0}", &status(participant.rules_verified)));
            if participant.rules_verified {
                if let Some(observed) = &participant.observed_rules {
                    let named = format!("{} ({})", self.match_rule_name(observed), observed.field_summary());
                    lines.push(text("MatchRules.details.actual").replace("{0}", &named));
                }
            } else {
                lines.push(
                    text("MatchRules.details.unavailable").replace("{0}", &text("MatchRules.reason.QUERY_UNSUPPORTED")),
                );
            }
        }
        lines.push(text("MatchRules.sgf.end"));
        let section = lines.join("\n");
        let begin = text("MatchRules.sgf.begin");
        let end = text("MatchRules.sgf.end");
        if let Some((start, stop)) = current
            .find(&begin)
            .zip(current.find(&end))
            .filter(|(start, stop)| stop >= start)
        {
            let before = current[..start].trim_end();
            let after = current[stop + end.len()..].trim_start();
            [before, &section, after]
                .into_iter()
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join("\n")
        } else if current.trim_end().is_empty() {
            section
        } else {
            format!("{}\n{section}", current.trim_end())
        }
    }

    /// Java writes the match formatter into the live search-position node, not just its saved copy.
    pub(super) fn match_record_analysis(&mut self, result: &NodeAnalysisView, seconds: f64) {
        let Some(run) = self.matches.run.as_ref() else { return };
        let index = side_index(run.first_is_black, result.to_play);
        let previous = run.previous[index].as_ref();
        let text = |key| self.match_text(key);
        let side = text(if result.to_play == Stone::Black {
            "SGFParse.black"
        } else {
            "SGFParse.white"
        });
        let difference = previous.map_or_else(String::new, |old| {
            let change = result.winrate - old.winrate;
            format!("({}{:.1}%)", if change > 0.0 { "+" } else { "-" }, change.abs())
        });
        let mut comment = format!(
            "{side} {} {:.1}% {difference}",
            text("SGFParse.winrate"),
            result.winrate
        );
        if let Some(lead) = result.score_lead {
            let mut lead = lead;
            if self.config.display.score_lead_with_komi {
                lead += if result.to_play == Stone::Black {
                    self.komi
                } else {
                    -self.komi
                };
            }
            let difference = previous.and_then(|old| old.score_lead).map_or_else(String::new, |old| {
                let change = result.score_lead.unwrap() - old;
                format!("({}{:.1})", if change > 0.0 { "+" } else { "-" }, change.abs())
            });
            let label = text(if self.config.display.score_lead_with_komi {
                "SGFParse.leadWithKomi"
            } else {
                "SGFParse.leadJustScore"
            });
            comment.push_str(&format!("\n{label} {lead:.1} {difference}"));
            if let Some(stdev) = result.score_stdev {
                comment.push_str(&format!(" {} {stdev:.1}", text("SGFParse.stdev")));
            }
        }
        let engine = result.engine.replace(' ', "");
        let visits = crate::formats::winrate_comment::playouts_text(result.visits);
        comment.push_str(&format!(
            "\n({engine} / {visits} {})\n{} {:.1}",
            text("SGFParse.playouts"),
            text("SGFParse.komi"),
            result.komi
        ));
        if self.engines.is_katago(run.sides[index].instance) && result.score_lead.is_some() {
            let participant = &run.sides[index];
            for (value, key) in [(participant.pda, "SGFParse.pda"), (participant.wrn, "SGFParse.wrn")] {
                if let Some(value) = value.filter(|value| *value != 0.0) {
                    comment.push_str(&format!(" {}{value:?}", text(key)));
                }
            }
        }
        if self.doc.record.node(result.node).move_number >= 3 && seconds > 0.0 {
            comment.push_str(&format!(
                "\n{}{seconds}{}",
                text("SGFParse.moveTime"),
                text("SGFParse.seconds")
            ));
        }
        let _ = self.edit_document(DocumentEdit::SetComment {
            node: result.node,
            comment,
        });
        self.matches.run.as_mut().unwrap().previous[index] = Some(result.clone());
    }

    /// SGFParser.appendGameTimeAndPlayouts writes totals into the live root before saving.
    pub(super) fn match_record_totals(&mut self) {
        let run = self.matches.run.as_ref().unwrap();
        let black = side_index(run.first_is_black, Stone::Black);
        let white = 1 - black;
        let mut comment = self.doc.record.node(ROOT).comment.clone();
        if let Some(index) = run.opening {
            comment.push_str(&format!("{}{index}\n", self.match_text("SGFParse.startGameSgf")));
        }
        let text = |key| self.match_text(key);
        comment.push_str(&format!(
            "{}{}{}\n{}{}\n{}{}{}\n{}{}",
            text("SGFParse.blackTotalTime"),
            run.sides[black].seconds,
            text("SGFParse.seconds"),
            text("SGFParse.totalVisits"),
            run.sides[black].visits,
            text("SGFParse.whiteTotalTime"),
            run.sides[white].seconds,
            text("SGFParse.seconds"),
            text("SGFParse.totalVisits"),
            run.sides[white].visits
        ));
        let katago_black = self.engines.is_katago(run.sides[black].instance);
        let katago_white = self.engines.is_katago(run.sides[white].instance);
        let tag = if katago_white {
            "KW"
        } else if katago_black {
            "KB"
        } else {
            "Y"
        };
        let property = crate::record::Property {
            id: "DZ".into(),
            values: vec![tag.into()],
        };
        let comment = self.match_rules_comment(&comment);
        let _ = self.edit_document(DocumentEdit::SetComment { node: ROOT, comment });
        let _ = self.edit_document(DocumentEdit::SetProperty { node: ROOT, property });
    }

    fn match_side_settings(&self, index: usize) -> String {
        let run = self.matches.run.as_ref().unwrap();
        let values = &run.values;
        let settings = &run.settings;
        let text = |key| self.match_text(key);
        let mut line = text(if index == 0 {
            "EngineGameInfo.settingFirst"
        } else {
            "EngineGameInfo.settingSecond"
        });
        let seconds = if index == 0 {
            values.first_seconds
        } else {
            values.second_seconds
        };
        let mode = if index == 0 {
            settings.first_time_mode
        } else {
            settings.second_time_mode
        };
        if values.kind == GameKind::Genmove && mode == MatchTimeMode::RawAdvanced {
            line.push_str(&text("EngineGameInfo.time"));
            line.push_str(if index == 0 {
                &settings.first_time_command
            } else {
                &settings.second_time_command
            });
        } else if values.time_enabled && seconds > 0.0 {
            line.push_str(&format!(
                "{}{seconds}{}",
                text("EngineGameInfo.time"),
                text("SGFParse.seconds")
            ));
        }
        if values.kind == GameKind::Analysis {
            let visits = if index == 0 {
                values.first_visits
            } else {
                values.second_visits
            };
            let first = if index == 0 {
                values.first_move_visits
            } else {
                values.second_move_visits
            };
            if values.visits_enabled && visits > 0 {
                line.push_str(&format!("{}{visits}", text("EngineGameInfo.totalVisits")));
            }
            if values.first_move_visits_enabled && first > 0 {
                line.push_str(&format!("{}{first}", text("EngineGameInfo.firstVisits")));
            }
            let policy = if index == 0 {
                settings.first_resign
            } else {
                settings.second_resign
            };
            line.push_str(&format!(
                "\r\n{}{}{}{}{}{}",
                text("EngineGameInfo.resignThreshold"),
                policy.min_move,
                text("EngineGameInfo.resignThreshold2"),
                policy.consecutive_moves,
                text("EngineGameInfo.resignThreshold3"),
                policy.winrate
            ));
        }
        let identity = if index == 0 { &settings.first } else { &settings.second };
        line.push_str(&format!("\r\n{}{}", text("EngineGameInfo.command"), identity.commands));
        if let Some(rules) = crate::engine::rules::KataRules::parse(&settings.rules) {
            line.push_str(&format!(
                "\r\n{}: {}",
                text("EngineGameInfo.rules"),
                self.match_rule_name(&rules)
            ));
        }
        line
    }

    pub(super) fn match_summary(&self) -> String {
        let run = self.matches.run.as_ref().unwrap();
        let score = &run.score;
        let values = &run.values;
        let text = |key| self.match_text(key);
        let yes_no = |value| {
            text(if value {
                "EngineGameInfo.yes"
            } else {
                "EngineGameInfo.no"
            })
        };
        let mut all = text("EngineGameInfo.otherSettings");
        all.push_str(&text(if values.kind == GameKind::Genmove {
            "EngineGameInfo.genmoveMode"
        } else {
            "EngineGameInfo.analyzeMode"
        }));
        all.push_str(&format!("{}{}", text("EngineGameInfo.komi"), self.komi));
        if values.batch_enabled {
            all.push_str(&format!("{}{}", text("EngineGameInfo.totalGames"), run.limit));
        }
        all.push_str(&format!(
            "{}{}{}{}{}{}",
            text("EngineGameInfo.continueGame"),
            yes_no(values.continue_position),
            text("EngineGameInfo.exchange"),
            yes_no(values.exchange_colors),
            text("EngineGameInfo.maxMoves"),
            self.match_max_moves()
        ));
        if values.kind == GameKind::Analysis && values.random_moves_enabled {
            all.push_str(&format!(
                "{}{}{}{}%",
                text("EngineGameInfo.randomPlay1"),
                values.random_moves,
                text("EngineGameInfo.randomPlay2"),
                values.random_winrate_diff
            ));
            if run.settings.check_random_visits {
                all.push_str(&format!(
                    "{}{:.1}%",
                    text("EngineGameInfo.randomPlay3"),
                    run.settings.random_visits_percent
                ));
            }
        }
        all.push_str(&format!(
            "{}{}",
            text("EngineGameInfo.backgroundPonder"),
            yes_no(run.settings.ponder)
        ));
        let first = score.first_black_wins + score.first_white_wins;
        let second = score.second_black_wins + score.second_white_wins;
        let games = f64::from(run.ordinal);
        let rate = if first + second == 0 {
            0.0
        } else {
            f64::from(first) / f64::from(first + second)
        };
        let mut lines = vec![
            all,
            String::new(),
            self.match_side_settings(0),
            String::new(),
            self.match_side_settings(1),
            String::new(),
            format!(
                "{}{}{}{first}:{second}{}{:.2}%",
                text("EngineGameInfo.totalGameResults"),
                run.ordinal,
                text("EngineGameInfo.gameScore"),
                text("EngineGameInfo.gameWinrate"),
                rate * 100.0
            ),
            String::new(),
        ];
        for (index, participant) in run.sides.iter().enumerate() {
            let (wins, black, white, visits) = if index == 0 {
                (
                    first,
                    score.first_black_wins,
                    score.first_white_wins,
                    score.first_visits,
                )
            } else {
                (
                    second,
                    score.second_black_wins,
                    score.second_white_wins,
                    score.second_visits,
                )
            };
            lines.push(format!(
                "{}({}):\n{}: {wins} {}: {black} {}: {white}{}{}{}{}{}",
                text(if index == 0 {
                    "EngineGameInfo.engine1"
                } else {
                    "EngineGameInfo.engine2"
                }),
                participant.name,
                text("EngineGameInfo.allWins"),
                text("EngineGameInfo.sgfStartBlackWin"),
                text("EngineGameInfo.sgfStartWhiteWin"),
                text("EngineGameInfo.totalTime"),
                run.total_seconds[index],
                text("SGFParse.seconds"),
                text("EngineGameInfo.result.totalVisits"),
                visits
            ));
        }
        lines.extend([
            String::new(),
            text("EngineGameInfo.timeVisitsTips"),
            String::new(),
            format!(
                "{}{} {}{}",
                text("EngineGameInfo.doublePassGame"),
                score.double_passes,
                text("EngineGameInfo.outOfMoveGame"),
                score.max_moves
            ),
            String::new(),
        ]);
        let elo = (1.0 / rate - 1.0).log10() * 400.0;
        let interval = |sigma: f64| {
            let squared = sigma * sigma;
            let center = (rate + squared / (2.0 * games)) / (1.0 + squared / games);
            let radius = sigma * (rate * (1.0 - rate) / games + squared / (4.0 * games * games)).sqrt()
                / (1.0 + squared / games);
            (center, radius)
        };
        let (center, radius) = interval(2.0);
        let edge = if center > 0.5 { center + radius } else { center - radius };
        let confidence_elo = (1.0 / edge - 1.0).log10() * 400.0;
        let mut elo_line = text("EngineGameInfo.secondEngineElo");
        if first == 0 {
            elo_line.push_str(&text("EngineGameInfo.elo100Wr"));
        } else {
            let java_number = |value: f64| {
                if value == f64::NEG_INFINITY {
                    "-Infinity".into()
                } else if value == f64::INFINITY {
                    "Infinity".into()
                } else {
                    format!("{value:.2}")
                }
            };
            elo_line.push_str(&format!("{}{} ± ", if elo > 0.0 { "+" } else { "" }, java_number(elo)));
            if (0.0..1.0).contains(&(center + radius)) {
                elo_line.push_str(&java_number((confidence_elo - elo).abs()));
            }
            if games < 50.0 {
                elo_line.push_str(&format!("?({})", text("EngineGameInfo.notEnoughGames")));
            }
        }
        lines.push(elo_line);
        for (sigma, key) in [
            (1.0, "EngineGameInfo.oneStdev"),
            (2.0, "EngineGameInfo.twoStdev"),
            (3.0, "EngineGameInfo.threeStdev"),
        ] {
            let (center, radius) = interval(sigma);
            lines.push(format!("{}{:.2}% ± {:.2}%", text(key), center * 100.0, radius * 100.0));
        }
        if run.settings.sgf_start {
            lines.push(String::new());
            lines.push(format!(
                "{}{}",
                text("EngineGameInfo.sgfStartNumber"),
                self.matches.openings.len()
            ));
            for (index, score) in &run.opening_scores {
                lines.push(format!(
                    "{}{}{}:\n{}: {}{}{}{}{}{}   {}: {}{}{}{}{}{}",
                    text("EngineGameInfo.sgfStartOpen"),
                    if matches!(
                        self.active_language,
                        crate::settings::Language::ZhCn | crate::settings::Language::ZhTw
                    ) {
                        ""
                    } else {
                        " "
                    },
                    index,
                    text("EngineGameInfo.engine1"),
                    text("EngineGameInfo.allWins"),
                    score.first_black_wins + score.first_white_wins,
                    text("EngineGameInfo.sgfStartBlackWin"),
                    score.first_black_wins,
                    text("EngineGameInfo.sgfStartWhiteWin"),
                    score.first_white_wins,
                    text("EngineGameInfo.engine2"),
                    text("EngineGameInfo.allWins"),
                    score.second_black_wins + score.second_white_wins,
                    text("EngineGameInfo.sgfStartBlackWin"),
                    score.second_black_wins,
                    text("EngineGameInfo.sgfStartWhiteWin"),
                    score.second_white_wins
                ));
            }
        }
        lines.push(String::new());
        lines.join("\r\n")
    }
}
