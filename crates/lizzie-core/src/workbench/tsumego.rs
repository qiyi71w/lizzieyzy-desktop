//! 死活题 and 抓取死活题 in the application flow: the dialog's choices are kept, the current
//! position is framed ([`crate::tsumego::build_frame`]) or a captured problem is set up, and the
//! result replaces the open record through its one write path.
//!
//! Both replace the record like Java's `flattenWithCondition`, so a running game, engine match or
//! AI 陪练 refuses them first (they are editing commands), and 整盘精析 refuses the frame as it
//! refuses other komi changes. Java's `tsumegoNode` (go back before an earlier frame) is not kept:
//! after a frame that node is always the new root, which Java's search never reaches, and after a
//! position that needs no frame it only moved browsing to an unrelated earlier move.

use super::Workbench;
use crate::api::{ApiError, CommandResult, ErrorKind};
use crate::record::{BoardSize, Point, Stone};
use crate::settings::{CaptureSettings, TsumegoFrameSettings};
use crate::tsumego::capture::CapturedProblem;
use crate::tsumego::{build_frame, FrameOptions};

/// The largest board a captured problem opens (`SetBoardSize`'s limit).
const MAX_BOARD: u16 = 52;

impl Workbench {
    /// 死活题 确定.
    pub(super) fn frame_tsumego(&mut self, choices: TsumegoFrameSettings) -> Result<CommandResult, ApiError> {
        if self.whole_game_active() {
            return Ok(CommandResult::Refused {
                message_key: "WholeGameAnalysis.conflict.analysis".into(),
            });
        }
        // A 墙面间距 that is not above 0 (or did not parse) keeps the saved one.
        let saved = self.config.tsumego.frame;
        let choices = TsumegoFrameSettings {
            wall_distance: if choices.wall_distance > 0 {
                choices.wall_distance
            } else {
                saved.wall_distance
            },
            ..choices
        };
        if choices != saved {
            self.commit_config(|c| c.tsumego.frame = choices)?;
        }
        let record = &self.doc.record;
        let position = record.position(self.cursor);
        let black_to_play = record.node(self.cursor).to_play == Stone::Black;
        let options = FrameOptions {
            to_play: choices.to_play,
            attacker: choices.attacker,
            ko_threat: choices.ko_threat,
            wall_distance: choices.wall_distance,
        };
        let Some(frame) = build_frame(record.size, &position.stones, black_to_play, &options).frame else {
            return Ok(CommandResult::Unchanged);
        };
        let to_play = if frame.black_to_play {
            Stone::Black
        } else {
            Stone::White
        };
        self.adopt_position(frame.stones, to_play)?;
        // `setKomi` + `changeKomi`: the frame's komi is kept like one the user typed.
        self.set_komi(frame.komi)?;
        Ok(CommandResult::TsumegoFramed { no_room: frame.no_room })
    }

    /// The finished 抓取死活题.
    pub(super) fn load_captured_problem(&mut self, problem: CapturedProblem) -> Result<CommandResult, ApiError> {
        let (bw, bh) = (problem.width, problem.height);
        if !(1..=4).contains(&problem.direction)
            || bw == 0
            || bh == 0
            || problem.stones.len() != usize::from(bw) * usize::from(bh)
        {
            return Err(ApiError::new(ErrorKind::InvalidArgument, "captured problem"));
        }
        // `reopen(bw, bw)` when the board is smaller than the capture, else `clear`.
        let current = self.doc.record.size;
        let size = if current.width < bw || current.height < bh {
            let side = bw.clamp(2, MAX_BOARD);
            BoardSize {
                width: side,
                height: side,
            }
        } else {
            current
        };
        self.new_board(size);
        let mut stones = vec![None; size.area()];
        let (w, h) = (i32::from(size.width), i32::from(size.height));
        let (cw, ch) = (i32::from(bw), i32::from(bh));
        for y in 0..ch {
            for x in 0..cw {
                let color = match problem.stones[(y * cw + x) as usize] {
                    1 => Stone::Black,
                    2 => Stone::White,
                    _ => continue,
                };
                let (px, py) = match problem.direction {
                    1 => (x, y),
                    2 => (w - (cw - x), y),
                    3 => (x, h - (ch - y)),
                    _ => (w - (cw - x), h - (ch - y)),
                };
                // A capture taller than the square board it opened has no place for its last rows.
                if !(0..w).contains(&px) || !(0..h).contains(&py) {
                    continue;
                }
                let index = size.index(Point {
                    x: px as u16,
                    y: py as u16,
                });
                stones[index].get_or_insert(color);
            }
        }
        self.adopt_position(stones, Stone::Black)?;
        Ok(CommandResult::Done)
    }

    /// 抓取死活题 参数设置 确定: Java keeps whatever integers were typed.
    pub fn set_capture_settings(&mut self, settings: CaptureSettings) -> Result<bool, ApiError> {
        if self.config.tsumego.capture == settings {
            return Ok(false);
        }
        self.commit_config(|c| c.tsumego.capture = settings)?;
        Ok(true)
    }

    /// 自定义工具栏 > 死活.
    pub fn set_tsumego_button_shown(&mut self, shown: bool) -> Result<bool, ApiError> {
        if self.config.tsumego.show_button == shown {
            return Ok(false);
        }
        self.commit_config(|c| c.tsumego.show_button = shown)?;
        Ok(true)
    }

    /// 抓取死活题's thresholds, for the platform capture.
    pub fn capture_settings(&self) -> CaptureSettings {
        self.config.tsumego.capture
    }
}
