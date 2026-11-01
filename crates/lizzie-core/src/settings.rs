//! Committed settings and application data owned by this product, and their files.
//!
//! The product keeps its own files in its own per-user directory (resolved by the platform
//! adapter from the application identifier). Java's `config.txt` / `persist` are never read or
//! written here; the explicit import ([`crate::java_import`]) only reads them.
//!
//! - `config.json` (Java `config.txt` role): domain settings and the recent-record list.
//! - `persist.json` (Java `persist` role): window placement and the last opened folder. "重置界面
//!   位置" deletes it and stops it being written again before the next start, like Java
//!   `Config.deletePersist`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// UI language (Java `AppLocale`, `ui.use-language`). Changing it takes effect after a restart.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum Language {
    #[default]
    System,
    ZhCn,
    EnUs,
    KoKr,
    JaJp,
    ZhTw,
    ThTh,
}

impl Language {
    /// `AppLocale.fromSystemLocale`: unknown languages fall back to Simplified Chinese.
    pub fn from_system_locale(tag: Option<&str>) -> Language {
        let Some(tag) = tag else { return Language::ZhCn };
        let lower = tag.replace('_', "-").to_lowercase();
        let mut parts = lower.split('-');
        let lang = parts.next().unwrap_or("");
        let rest: Vec<&str> = parts.collect();
        match lang {
            "zh" if rest.iter().any(|p| matches!(*p, "hant" | "tw" | "hk" | "mo")) => Language::ZhTw,
            "zh" => Language::ZhCn,
            "en" => Language::EnUs,
            "ja" => Language::JaJp,
            "ko" => Language::KoKr,
            "th" => Language::ThTh,
            _ => Language::ZhCn,
        }
    }

    /// Concrete language for a setting, resolving `System` with the OS locale.
    pub fn resolve(self, system_locale: Option<&str>) -> Language {
        match self {
            Language::System => Language::from_system_locale(system_locale),
            other => other,
        }
    }
}

/// Record open/save settings (Java `read-komi` default true, `load-sgf-last` default false,
/// `ui.show-replace-file-hint` default true, `ui.auto-quick-analyze-on-load` default true).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RecordSettings {
    pub read_komi: bool,
    pub load_sgf_last: bool,
    /// Java `ui.append-winrate-to-comment`.
    pub append_winrate_to_comment: bool,
    /// Ask before "保存" overwrites the record's own file.
    pub show_replace_file_hint: bool,
    /// 打开棋谱后自动快速分析 (`ui.auto-quick-analyze-on-load`): a loaded record's main trunk is
    /// analysed in the background where it has no analysis yet.
    pub auto_quick_analyze_on_load: bool,
}

impl Default for RecordSettings {
    fn default() -> Self {
        RecordSettings {
            read_komi: true,
            load_sgf_last: false,
            append_winrate_to_comment: true,
            show_replace_file_hint: true,
            auto_quick_analyze_on_load: true,
        }
    }
}

/// Java `AnalysisEngineCommandHelper.DEFAULT_ANALYSIS_COMMAND`.
pub const DEFAULT_ANALYSIS_COMMAND: &str =
    "katago analysis -model model.bin.gz -config analysis.cfg -quit-without-waiting";

/// 精析设置 (Java `AnalysisSettings`, `ui.analysis-*`): the KataGo analysis engine used by automatic
/// quick analysis and flash analysis. The SSH fields of the dialog belong to remote engines.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct AnalysisEngineSettings {
    /// 分析引擎命令 (`analysis-engine-command`).
    pub command: String,
    /// The command was written by the user rather than generated (`analysis-engine-command-customized`):
    /// a command that is not customized follows a managed engine.
    pub command_customized: bool,
    /// 单步计算量 (`analysis-max-visits`, at least 1): a request searches one more visit.
    pub max_visits: u32,
    /// 使用当前引擎规则 (`analysis-use-current-rules`) versus 使用指定规则 (`analysis-specific-rules`).
    pub use_current_rules: bool,
    pub specific_rules: String,
    /// 总是覆盖已有分析结果 (`analysis-always-override`).
    pub always_override: bool,
    /// 启动Lizzie时预加载引擎 (`analysis-engine-preload`).
    pub preload: bool,
    /// 结束分析后退出引擎 (`analysis-auto-quit`).
    pub auto_quit: bool,
    /// 复用当前前台引擎 (`analysis-reuse-current-engine`): borrow the foreground engine instead of
    /// starting an analysis process.
    pub reuse_current_engine: bool,
    /// 批量精析's 每手总计算量 (`batch-analysis-playouts`, default 100): a batch request searches
    /// at least 2 visits (`AnalysisEngine.targetAnalysisVisits`).
    pub batch_visits: u32,
}

impl Default for AnalysisEngineSettings {
    fn default() -> Self {
        AnalysisEngineSettings {
            command: DEFAULT_ANALYSIS_COMMAND.to_string(),
            command_customized: false,
            max_visits: 1,
            use_current_rules: true,
            specific_rules: String::new(),
            always_override: false,
            preload: false,
            auto_quit: true,
            reuse_current_engine: false,
            batch_visits: 100,
        }
    }
}

/// 局部精析's move range (`ui.analysis-start-move`, `ui.analysis-end-move`; -1 leaves an end
/// open): moves from `start_move` up to, not including, `end_move`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct FlashRange {
    pub start_move: i32,
    pub end_move: i32,
}

impl Default for FlashRange {
    fn default() -> Self {
        FlashRange {
            start_move: -1,
            end_move: -1,
        }
    }
}

/// Single-record 自动分析 and 波动过大时加强分析 (Java StartAnaDialog / SetDiffAnalyze).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct AutoAnalysisSettings {
    pub start_move: i32,
    pub end_move: i32,
    pub time_enabled: bool,
    pub time_seconds: i32,
    pub visits_enabled: bool,
    pub visits: i32,
    pub first_visits_enabled: bool,
    pub first_visits: i32,
    pub black: bool,
    pub white: bool,
    pub all_branches: bool,
    pub auto_save: bool,
    pub exit_by_pause: bool,
    pub exit_tip: bool,
    pub diff_enable: bool,
    pub diff_black: bool,
    pub diff_white: bool,
    pub diff_use_win: bool,
    pub diff_use_score: bool,
    pub diff_win_threshold: f64,
    pub diff_score_threshold: f64,
    pub diff_time: i32,
    pub diff_visits: i32,
    pub diff_first_visits: i32,
}

impl Default for AutoAnalysisSettings {
    fn default() -> Self {
        Self {
            start_move: 1, end_move: -1,
            time_enabled: true, time_seconds: 2,
            visits_enabled: false, visits: -1,
            first_visits_enabled: false, first_visits: -1,
            black: true, white: true, all_branches: false, auto_save: true,
            exit_by_pause: true, exit_tip: true,
            diff_enable: false, diff_black: true, diff_white: true,
            diff_use_win: true, diff_use_score: false,
            diff_win_threshold: 10.0, diff_score_threshold: 3.0,
            diff_time: 10, diff_visits: -1, diff_first_visits: -1,
        }
    }
}

/// 整盘精析 (`ui.whole-game-analysis-deep-visits`). Invalid stored values use Java's 1000.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct WholeGameSettings {
    #[serde(deserialize_with = "stored_whole_game_visits")]
    pub deep_visits: u32,
}

impl WholeGameSettings {
    pub fn from_stored(visits: i64) -> Self {
        Self { deep_visits: if (500..=1_000_000).contains(&visits) { visits as u32 } else { 1000 } }
    }
}

impl Default for WholeGameSettings {
    fn default() -> Self { Self { deep_visits: 1000 } }
}

fn stored_whole_game_visits<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    Ok(WholeGameSettings::from_stored(value.as_i64().unwrap_or(1000)).deep_visits)
}

/// 超级鹰眼 分支: which line it reads (`moveList-selected-branch`: 0 主干, 1 当前).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum HawkeyeBranch {
    #[default]
    Main,
    Current,
}

/// 超级鹰眼's 统计 range choice (`applyChkPeriod`; Java keeps it as `move-list-filter-current`
/// with `match-ai-firstmove` / `match-ai-lastmove`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum HawkeyePeriod {
    /// 当前: up to the browsing position.
    Current,
    /// 全局.
    #[default]
    All,
    /// 布局: up to the opening end.
    Opening,
    /// 中盘: after the opening end up to the middle end.
    Middle,
    /// 收官: after the middle end.
    End,
    /// 其他: the custom moves.
    Custom,
}

/// 超级鹰眼 (`MoveListFrame`, `SetThreshold`), Java `ui.*` keys and defaults (`Config.java`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct HawkeyeSettings {
    /// The window is open; reopened at the next start (`show-badmoves-frame`, off).
    pub show_frame: bool,
    /// 总在最前, Q (`badmoves-always-ontop`, off; Java wrote `badmoves-always-on-top` but read this
    /// key, so the choice was lost on restart).
    pub always_on_top: bool,
    /// Top page: 0 统计信息, 1 简易列表, 2 详细列表 (`movelist-selected-indextop`, 0).
    pub top_page: u8,
    /// Chart page as Java numbers it (`movelist-selected-index`, 0): 0 胜率吻合, 1 吻合度走势,
    /// 2 胜率损失分布, 3 目数损失分布, 4 分段吻合度, 5 胜率损失统计, 6 目数损失统计,
    /// 7 翻车现场(胜率), 8 翻车现场(目数).
    pub chart: u8,
    /// 分支 (`moveList-selected-branch`, 主干).
    pub branch: HawkeyeBranch,
    /// The lists and statistics are shown (`show-movelist-matchai`, on).
    pub show_list: bool,
    /// The chart is shown (`show-winrate-matchai`, on).
    pub show_graph: bool,
    /// 列表筛选: rows need this win-rate change, score change (KataGo) and visits on both
    /// positions (`move-list-winrate-threshold`, `-score-`, `-visits-`, all 0).
    pub winrate_threshold: u32,
    pub score_threshold: u32,
    pub visits_threshold: u32,
    /// 置顶当前手(仅详细列表) (`movelist-top-curnode`, off).
    pub top_current_move: bool,
    /// 统计 range (全局).
    pub period: HawkeyePeriod,
    /// 布局结束 / 中盘结束 (`opening-end-move` 60, `middle-end-move` 160).
    pub opening_end_move: i32,
    pub middle_end_move: i32,
    /// 其他 (`game-statistics-custom-start` 10, `-end` 200).
    pub custom_start: i32,
    pub custom_end: i32,
    /// 胜率损失 thresholds of the charts (`winrate-diff-range1` 3, `-range2` 10) and score
    /// (`score-diff-range1` 2, `-range2` 5).
    pub winrate_range1: i32,
    pub winrate_range2: i32,
    pub score_range1: i32,
    pub score_range2: i32,
    /// 损失分档 shows 胜率损失 rather than 目数损失 (`loss-panel-select-winrate`, off).
    pub loss_winrate: bool,
}

impl Default for HawkeyeSettings {
    fn default() -> Self {
        HawkeyeSettings {
            show_frame: false,
            always_on_top: false,
            top_page: 0,
            chart: 0,
            branch: HawkeyeBranch::Main,
            show_list: true,
            show_graph: true,
            winrate_threshold: 0,
            score_threshold: 0,
            visits_threshold: 0,
            top_current_move: false,
            period: HawkeyePeriod::All,
            opening_end_move: 60,
            middle_end_move: 160,
            custom_start: 10,
            custom_end: 200,
            winrate_range1: 3,
            winrate_range2: 10,
            score_range1: 2,
            score_range2: 5,
            loss_winrate: false,
        }
    }
}

impl HawkeyeSettings {
    /// `applyChkPeriod`: the 统计 range (`matchAiFirstMove`, `matchAiLastMove`) of the chosen
    /// period; 当前 ends at `current_move`.
    pub fn range(&self, current_move: u32) -> (i32, i32) {
        match self.period {
            HawkeyePeriod::Current => (-1, i32::try_from(current_move).unwrap_or(i32::MAX)),
            HawkeyePeriod::All => (-1, 1000),
            HawkeyePeriod::Opening => (-1, self.opening_end_move),
            HawkeyePeriod::Middle => (self.opening_end_move, self.middle_end_move),
            HawkeyePeriod::End => (self.middle_end_move, 1000),
            HawkeyePeriod::Custom => (self.custom_start - 1, self.custom_end),
        }
    }

    /// `setFilterStatus`: the period Java shows for a saved range.
    pub fn period_of(filter_current: bool, first: i32, last: i32, opening_end: i32, middle_end: i32) -> HawkeyePeriod {
        if filter_current {
            HawkeyePeriod::Current
        } else if first <= 1 && last == opening_end {
            HawkeyePeriod::Opening
        } else if first == opening_end && last == middle_end {
            HawkeyePeriod::Middle
        } else if first == middle_end && last >= 500 {
            HawkeyePeriod::End
        } else if first <= 1 && last >= 500 {
            HawkeyePeriod::All
        } else {
            HawkeyePeriod::Custom
        }
    }
}

/// 超级鹰眼's bounds, sort column and 详细列表 column widths (Java
/// `persistedUi.badmoves-list-position`: `[sortnum, x, y, width, height, widths...]`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct HawkeyePlacement {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    /// The sort column (`sortnum`).
    pub sort: i32,
    /// The 详细列表's 9 or 12 column widths; empty for Java's preferred widths.
    pub columns: Vec<u32>,
}

/// 快速曲线轻量模型 (`ui.quick-analysis-lightweight-model-enabled`, `katago-quick-analysis-model-path`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct QuickModelSettings {
    /// 自动使用: quick analysis runs the lightweight model when it is installed.
    pub enabled: bool,
    /// A model file kept elsewhere (imported from Java); the resource folder's copy otherwise.
    pub path: Option<String>,
}

/// 问题手 side filter (`ui.problem-list-side-filter`; anything but `white` is Black).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum ProblemSide {
    #[default]
    Black,
    White,
}

/// 问题手排序 (`ui.problem-list-sort`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum ProblemSort {
    /// 失误最大优先: win rate loss, larger first; ties by move number.
    #[default]
    LossDesc,
    /// 手数从小到大.
    MoveAsc,
    /// 手数从大到小.
    MoveDesc,
}

/// The 评论/问题手 panel (Java `SidebarPanel`, `BlunderListPanel`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct ProblemListSettings {
    /// The 问题手 page is shown instead of 评论 (`is-showing-blunder-table`).
    pub showing: bool,
    pub side: ProblemSide,
    pub sort: ProblemSort,
    /// Moves losing less win rate are not listed (`problem-list-winrate-threshold`, 10 points).
    pub winrate_threshold: f64,
    /// Both positions need this many visits (`blunder-playouts-threshold`, 0).
    pub playouts_threshold: u64,
    /// 显示评论/问题手面板顶部控制条 is off (`hide-blunder-table-control-pane`).
    pub hide_control_pane: bool,
}

impl Default for ProblemListSettings {
    fn default() -> Self {
        ProblemListSettings {
            showing: false,
            side: ProblemSide::Black,
            sort: ProblemSort::LossDesc,
            winrate_threshold: 10.0,
            playouts_threshold: 0,
            hide_control_pane: false,
        }
    }
}

/// 布局模式 (Java `ui.extra-mode`, `ExtraMode`): which workbench layout is shown. Only the modes
/// this product has are listed; Java's other layout modes are added by their owners.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum ExtraMode {
    /// 默认模式 (`Normal`, Java value 0).
    #[default]
    Normal,
    /// 双引擎模式 (`Double_Engine`, Java value 2): two boards, the primary engine's on the left and
    /// the secondary engine's on the right.
    DoubleEngine,
}

/// Workbench presentation (`Config.java:1609-1613,1677-1683,1828,1939,1981`).
/// `extra_mode` remains in `UiSettings`; these flags also serve later fixed Java layouts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct PanelSettings {
    pub show_sub_board: bool,
    pub show_winrate_graph: bool,
    pub large_winrate_graph: bool,
    pub show_comment: bool,
    pub show_variation_graph: bool,
    pub show_list_pane: bool,
    pub show_captured: bool,
    pub show_status: bool,
    pub large_sub_board: bool,
    pub show_top_toolbar: bool,
    pub show_double_menu: bool,
    pub always_on_top: bool,
    pub show_winrate_in_title: bool,
    /// Java remembers a sub board hidden by 大胜率图 and restores it when leaving that mode.
    pub hide_sub_board_from_large_winrate: bool,
}

impl Default for PanelSettings {
    fn default() -> Self {
        Self {
            show_sub_board: true,
            show_winrate_graph: true,
            large_winrate_graph: false,
            show_comment: true,
            show_variation_graph: true,
            show_list_pane: true,
            show_captured: true,
            show_status: true,
            large_sub_board: false,
            show_top_toolbar: true,
            show_double_menu: true,
            always_on_top: false,
            show_winrate_in_title: true,
            hide_sub_board_from_large_winrate: false,
        }
    }
}

/// Java's in-frame shares (`LizzieFrame.java:7976-8035`), independent of panel visibility.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct PanelSizes {
    pub leftover_left_share: Option<f64>,
    pub board_position_proportion: u8,
    pub comment_height_share: Option<f64>,
    pub variation_graph_share: Option<f64>,
}

impl Default for PanelSizes {
    fn default() -> Self {
        Self {
            leftover_left_share: None,
            board_position_proportion: 4,
            comment_height_share: None,
            variation_graph_share: None,
        }
    }
}

/// `main-window-other[0]`: Java toolbar heights 0, 26 and 70 (`Menu.java:4532-4588`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum BottomToolbar {
    Hidden,
    #[default]
    Simple,
    Detailed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum WorkbenchPanel {
    SubBoard,
    WinrateGraph,
    LargeWinrateGraph,
    Comment,
    VariationGraph,
    ListPane,
    Captured,
    Status,
    LargeSubBoard,
    AlwaysOnTop,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct UiSettings {
    pub language: Language,
    /// Java `ui.play-sound`, default true.
    pub play_sound: bool,
    /// Java `ui.extra-mode`, default `Normal`.
    pub extra_mode: ExtraMode,
}

impl Default for UiSettings {
    fn default() -> Self {
        UiSettings {
            language: Language::default(),
            play_sound: true,
            extra_mode: ExtraMode::Normal,
        }
    }
}

/// Editing settings (Java `ui.*` keys; defaults from `Config.java`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct EditSettings {
    /// 启用双击找子 (`allow-double-click`, default on).
    pub allow_double_click: bool,
    /// 启用拖动棋子 (`allow-drag`, default off).
    pub allow_drag: bool,
    /// Right click opens the board context menu; off, it goes back one move
    /// (`show-right-menu`, default on).
    pub show_right_menu: bool,
    /// 显示新建棋盘确认对话框 (`show-new-board-hint`, default on).
    pub show_new_board_hint: bool,
    /// 标记工具 shown in the top toolbar (`show-markup-tools`, default off).
    pub show_markup_tools: bool,
    /// 设置棋盘大小 "其他" width and height (`other-size-width` / `-height`, default 21).
    pub other_size_width: u16,
    pub other_size_height: u16,
    /// 五子棋无提子规则 (`no-capture`, default off): stones placed or read while it is on remove
    /// nothing (`Board.place`, `BoardHistoryList.place`).
    pub no_capture: bool,
}

impl Default for EditSettings {
    fn default() -> Self {
        EditSettings {
            allow_double_click: true,
            allow_drag: false,
            show_right_menu: true,
            show_new_board_hint: true,
            show_markup_tools: false,
            other_size_width: 21,
            other_size_height: 21,
            no_capture: false,
        }
    }
}

/// 存档与读档 check boxes (Java `ui.auto-save-exit` default on, `ui.resume-previous-game` default
/// off, `ui.load-save-zoom` default on).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct RecoverySettings {
    /// 自动存档: the automatic slot is written on exit and every 60 seconds.
    pub auto_save_on_exit: bool,
    /// 启动时自动恢复: start with the automatic slot's record.
    pub resume_previous_game: bool,
    /// 鼠标悬停放大缩略棋盘.
    pub zoom_thumbnails: bool,
}

impl Default for RecoverySettings {
    fn default() -> Self {
        RecoverySettings {
            auto_save_on_exit: true,
            resume_previous_game: false,
            zoom_thumbnails: true,
        }
    }
}

/// How the boards and analysis numbers are shown (Java `ui.*` display keys; defaults are those of
/// a new Java profile, `Config.java` / `createDefaultConfig`). The two viewpoint settings convert
/// every number through [`crate::viewpoint::Viewpoint`], in the 带评论 export too; the others are
/// read by the board projection ([`crate::display`]) and the board painters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct DisplaySettings {
    /// 总是显示黑胜率: win rates and score leads are Black's instead of the side to move's.
    pub winrate_always_black: bool,
    /// 目差显示 > 盘面: score leads are the board difference (KataGo's lead plus the komi for
    /// Black, minus it for White) instead of the lead after komi (目差).
    pub score_lead_with_komi: bool,
    /// 坐标 (`show-coordinates`, on).
    pub coordinates: bool,
    /// Exclusive special-coordinate mode (`ConfigDialog2.SpecialCoordsCbx`).
    pub coordinate_style: CoordinateStyle,
    /// 棋盘下方显示黑白名字 (`show-name-in-board`, on).
    pub names_on_board: bool,
    /// 手数 (`allow-move-number`): 0 none, -1 every move, n the last n moves. A new Java profile
    /// has `show-move-number` off, so none.
    pub move_numbers: i32,
    /// 总是从1开始显示 (`movenumber-from-one`): the last n moves count 1..n.
    pub move_numbers_from_one: bool,
    /// 分支内从1开始显示 (`new-move-number-in-branch`, on) versus 分支内继续总手数. Only branches
    /// created afterwards follow a change (`Menu.moveNumberInBranchTips`).
    pub branch_numbers_from_one: bool,
    /// 分支内总是显示所有手数 (`show-moveall-inbranch`, off).
    pub all_numbers_in_branch: bool,
    /// 显示分支面板上的手数 (`show-var-move`, on).
    pub tree_move_numbers: bool,
    /// 落子评价 (`move-rank-mark-last-move`): -1 none, 0 every move, n the last n moves (1).
    pub move_rank: i32,
    /// 显示最后多手 (`txt-move-rank-mark-last-move`, 3).
    pub move_rank_custom: u32,
    /// 判定依据 (`move-rank-evaluation-mode`).
    pub move_rank_mode: MoveRankMode,
    /// Win rate loss of rank levels 1–5 (`win-loss-threshold-1..5`), percentage points.
    pub win_loss_thresholds: [f64; 5],
    /// Score loss of rank levels 1–5 (`score-loss-threshold-1..5`), points.
    pub score_loss_thresholds: [f64; 5],
    /// 选点信息 > 胜率 / 计算量 / 目差 (`show-winrate-in-suggestion`, `-playouts-`,
    /// `-scoremean-`, all on).
    pub suggestion_winrate: bool,
    pub suggestion_playouts: bool,
    pub suggestion_score: bool,
    /// 目差显示为与最佳选点之差 (`show-score-as-diff`, off).
    pub score_as_diff: bool,
    /// 自定义信息顺序: rows of win rate, playouts and score (`suggestion-info-winrate`,
    /// `-playouts`, `-scorelead`; 1, 2, 3).
    pub suggestion_info_order: [u8; 3],
    /// 延迟显示候选点 (`delay-show-candidates`, off; `delay-candidates-seconds`, 10).
    pub delay_candidates: bool,
    pub delay_candidates_seconds: f64,
    /// 显示选点右上方角标 (`show-suggestion-order`, on).
    pub suggestion_order: bool,
    /// 候选点最高值红色高亮 (`show-suggestion-maxred`, off).
    pub suggestion_max_red: bool,
    /// 轮白下字体显示为白色 (`white-suggestion-white`, off).
    pub white_suggestion_white: bool,
    /// Blue best-candidate ring (`show-blue-ring`, on).
    pub blue_ring: bool,
    /// Candidate colour concentration, 1..=3 (`suggestion-color-ratio`, 2).
    pub suggestion_color_ratio: i32,
    pub white_suggestion_order_white: bool,
    /// Draw forbidden-analysis points (`leelaz.show-nosugg-circle`, off).
    pub no_sugg_circle: bool,
    pub pv_visits: PvVisitsMode,
    pub pv_visits_limit: i32,
    pub remove_dead_in_variation: bool,
    /// 鼠标悬停显示变化图 (`show-suggestion-variations`, on).
    pub suggestion_variations: bool,
    /// 鼠标悬停显示变化图时,变化图不刷新 (`norefresh-onmouse-move`, on).
    pub no_refresh_on_mouse: bool,
    /// 设置变化图回放间隔 (`replay-branch-interval-seconds`, 0.9 in a new profile).
    pub replay_interval_seconds: f64,
    pub auto_replay_branch: bool,
    pub entire_variation_first: bool,
    pub entire_variation_first_seconds: f64,
    pub continue_with_best_move: bool,
    pub directly_with_best_move: bool,
    /// Java `persist.ui-persist.toolbar-parameter[40]`; -1 means no interval entered.
    pub autoplay_main_seconds: f64,
    /// Java `persist.ui-persist.toolbar-parameter[41]`; -1 means no interval entered.
    pub autoplay_sub_millis: i32,
    /// Candidates below this share of the most visits show no numbers
    /// (`min-playout-ratio-for-stats`, 0).
    pub min_playout_ratio: f64,
    /// 下一手 (`show-next-moves`, on; `show-next-move-blunder`, on = 详细).
    pub next_moves: bool,
    pub next_move_info: bool,
    /// 下一手提示需要计算量 (`min-playouts-for-next-move`, 30).
    pub next_move_min_playouts: i32,
    /// 小棋盘设置 (`show-heat`, `show-heat-aftercalc`, `subboard-raw`).
    pub sub_board: SubBoardMode,
    /// The sub board keeps its variation while the pointer is over it (`no-refresh-on-sub`, on).
    pub no_refresh_on_sub: bool,
    /// 纯网络 ownership painter (Java's five `show-pure-estimate-*` exclusive menu choices).
    pub pure_estimate: EstimateStyle,
    /// 胜率图设置 > 显示胜率/目数曲线: the win-rate line (`show-win-rate-line`, on) and the score
    /// lead line (`show-score-lead-line`, on); 胜率 / 目数 / 全部.
    pub winrate_line: bool,
    pub score_lead_line: bool,
    /// 单曲线模式使用面积填充 (`show-winrate-graph-fill`, on): only while one line shows.
    pub winrate_graph_fill: bool,
    /// 显示恶手损失条 (`show-blunder-bar`, off).
    pub blunder_bar: bool,
    /// 显示鼠标悬停信息(仅限黑视角) (`show-mouse-over-winrate-graph`, on).
    pub mouse_over_winrate_graph: bool,
    /// 初始最大目数差 (`initial-max-score-lead`, 15): the score scale before larger leads widen it.
    pub initial_max_score_lead: f64,
    /// Kata形势显示 square style (`show-katago-estimate-small` / `-normal` / `-not-on-live` /
    /// `-big-below` / `-by-size`; small).
    pub kata_estimate_style: EstimateStyle,
    /// 按选点显示评估 (`use-moves-ownership`, on): a hovered candidate shows its own ownership.
    pub use_moves_ownership: bool,
    /// 使用快捷键(大键盘点) (`shortcut-kata-estimate`, off): `.` toggles Kata形势.
    pub kata_estimate_shortcut: bool,
    /// 记忆评估显示状态 (`save-kata-estimate-status`, off).
    pub kata_estimate_save_status: bool,
    /// The Kata形势 state the next start begins with (`show-katago-estimate`,
    /// `-onmainboard`, `-onsubbord`), written as Java writes those keys.
    pub kata_estimate: KataEstimateState,
    /// 终局点目 counts territory (数目, `use-territory-in-score`, off) instead of area (数子).
    pub territory_scoring: bool,
    /// A move matches the AI when it is among the first `match-ai-moves` candidates (3) with at
    /// least `match-ai-percents-playouts` percent (20) of the most visited one's visits.
    pub match_ai_moves: u32,
    pub match_ai_percents_playouts: f64,
    /// 独立选点列表 (U) is open (`show-suggestions-frame`, off); reopened at the next start.
    pub suggestions_frame: bool,
    /// Its 总在最前 (`suggestions-always-ontop`, off; Q).
    pub suggestions_on_top: bool,
    /// Its 列表 / 柱状图 / 当前手 / 鼠标悬停 boxes (`show-bestmoves-list` on,
    /// `show-bestmoves-graph` off, `anaframe-show-next` on, `anaframe-use-mousemove` off).
    pub best_moves_list: bool,
    pub best_moves_graph: bool,
    pub frame_show_next: bool,
    pub frame_mouse_move: bool,
}

impl Default for DisplaySettings {
    fn default() -> Self {
        DisplaySettings {
            winrate_always_black: false,
            score_lead_with_komi: false,
            coordinates: true,
            coordinate_style: CoordinateStyle::Normal,
            names_on_board: true,
            move_numbers: 0,
            move_numbers_from_one: false,
            branch_numbers_from_one: true,
            all_numbers_in_branch: false,
            tree_move_numbers: true,
            move_rank: 1,
            move_rank_custom: 3,
            move_rank_mode: MoveRankMode::Auto,
            win_loss_thresholds: [-1.0, -3.0, -6.0, -12.0, -24.0],
            score_loss_thresholds: [-0.5, -1.5, -3.0, -6.0, -12.0],
            suggestion_winrate: true,
            suggestion_playouts: true,
            suggestion_score: true,
            score_as_diff: false,
            suggestion_info_order: [1, 2, 3],
            delay_candidates: false,
            delay_candidates_seconds: 10.0,
            suggestion_order: true,
            suggestion_max_red: false,
            white_suggestion_white: false,
            blue_ring: true,
            suggestion_color_ratio: 2,
            white_suggestion_order_white: false,
            no_sugg_circle: false,
            pv_visits: PvVisitsMode::Off,
            pv_visits_limit: 0,
            remove_dead_in_variation: true,
            suggestion_variations: true,
            no_refresh_on_mouse: true,
            replay_interval_seconds: 0.9,
            auto_replay_branch: false,
            entire_variation_first: false,
            entire_variation_first_seconds: 3.0,
            continue_with_best_move: false,
            directly_with_best_move: false,
            autoplay_main_seconds: -1.0,
            autoplay_sub_millis: -1,
            min_playout_ratio: 0.0,
            next_moves: true,
            next_move_info: true,
            next_move_min_playouts: 30,
            sub_board: SubBoardMode::Variation,
            no_refresh_on_sub: true,
            pure_estimate: EstimateStyle::BigBelow,
            winrate_line: true,
            score_lead_line: true,
            winrate_graph_fill: true,
            blunder_bar: false,
            mouse_over_winrate_graph: true,
            initial_max_score_lead: 15.0,
            kata_estimate_style: EstimateStyle::Small,
            use_moves_ownership: true,
            kata_estimate_shortcut: false,
            kata_estimate_save_status: false,
            kata_estimate: KataEstimateState::default(),
            territory_scoring: false,
            match_ai_moves: 3,
            match_ai_percents_playouts: 20.0,
            suggestions_frame: false,
            suggestions_on_top: false,
            best_moves_list: true,
            best_moves_graph: false,
            frame_show_next: true,
            frame_mouse_move: false,
        }
    }
}

/// Java's exclusive special-coordinate choices.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum CoordinateStyle {
    #[default]
    Normal,
    WithI,
    Fox,
    NumbersFromTop,
    NumbersFromBottom,
}

/// Variation visit labels (`show-pv-visits-last-move` / `-all-move`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum PvVisitsMode {
    #[default]
    Off,
    LastMove,
    AllMoves,
}

/// Ownership square style of the 纯网络 and the Kata形势 estimates (Java's five exclusive choices).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum EstimateStyle {
    Small,
    Transparent,
    TransparentNotOnLive,
    #[default]
    BigBelow,
    BySize,
}

/// Kata形势显示: the search ownership is requested (`shown`) and drawn on the main and/or sub
/// board. Shown with neither board is 隐藏 (`isHiddenKataEstimate`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct KataEstimateState {
    pub shown: bool,
    pub on_main: bool,
    pub on_sub: bool,
}

impl Default for KataEstimateState {
    fn default() -> Self {
        KataEstimateState {
            shown: false,
            on_main: true,
            on_sub: true,
        }
    }
}

impl KataEstimateState {
    /// `isHiddenKataEstimate` (`Config.java:1759-1760`).
    pub fn hidden(&self) -> bool {
        self.shown && !self.on_main && !self.on_sub
    }
}

/// 落子评价 > 判定依据 (`MoveRankEvaluationMode`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum MoveRankMode {
    /// 智能平衡: the score loss, and the win rate loss from 恶手 up; the win rate alone without a
    /// score.
    #[default]
    Auto,
    /// 仅目差 (no mark without a score).
    Score,
    /// 仅胜率.
    Winrate,
    /// 综合: whichever reaches the level.
    Combined,
}

/// 小棋盘设置: what the sub board shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum SubBoardMode {
    /// 变化图: the selected candidate's variation.
    #[default]
    Variation,
    /// 仅棋盘棋子: the position only.
    Raw,
    /// 第一感热点图: candidates sized by their policy prior.
    Policy,
    /// 计算后热点图: candidates sized by their search visits.
    Visits,
}

pub const RECENT_LIMIT: usize = 5;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    pub record: RecordSettings,
    pub ui: UiSettings,
    /// Oldest first, at most [`RECENT_LIMIT`] (Java `ui.recent-file-paths`).
    pub recent_files: Vec<String>,
    /// Engines and analysis (Java `leelaz.engine-settings-list` and analysis keys).
    pub engine: crate::engine::profile::EngineSettings,
    /// Editing (Java `ui.allow-*`, `show-new-board-hint`, ...).
    pub edit: EditSettings,
    /// Network access (Java `ui.network-proxy-*`).
    pub network: NetworkSettings,
    /// 存档与读档 (Java `ui.auto-save-exit`, `resume-previous-game`, `load-save-zoom`).
    pub recovery: RecoverySettings,
    /// Analysis numbers (Java `ui.win-rate-always-black`, `show-katago-score-lead-with-komi`).
    pub display: DisplaySettings,
    /// 精析设置 (Java `ui.analysis-*`).
    pub analysis_engine: AnalysisEngineSettings,
    /// 快速曲线轻量模型.
    pub quick_model: QuickModelSettings,
    /// 评论/问题手 panel.
    pub problems: ProblemListSettings,
    /// 局部精析's move range.
    pub flash_range: FlashRange,
    pub auto_analysis: AutoAnalysisSettings,
    pub whole_game: WholeGameSettings,
    /// 超级鹰眼.
    pub hawkeye: HawkeyeSettings,
    /// Fixed Java workbench panel and toolbar presentation.
    pub panels: PanelSettings,
    pub appearance: crate::appearance::AppearanceSettings,
    /// Default theme edits live in config; named themes have their own theme.txt.
    pub appearance_defaults: crate::appearance::ThemeSettings,
}

/// How this client reaches the network (Java `NetworkProxy`, `ui.network-proxy-mode`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum ProxyMode {
    /// `direct` (Java default): no proxy.
    #[default]
    Direct,
    /// `system`: the operating system's proxy settings.
    System,
    /// `manual`: an HTTP proxy at `proxy_host:proxy_port`.
    Manual,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct NetworkSettings {
    pub proxy_mode: ProxyMode,
    /// `network-proxy-host` (manual mode; non-empty when manual).
    pub proxy_host: String,
    /// `network-proxy-port` (manual mode; 1–65535 when manual).
    pub proxy_port: u16,
}

impl Config {
    /// `Config.saveRecentFilePaths`: an existing entry moves to the newest position.
    pub fn remember_recent(&mut self, path: &str) {
        self.recent_files.retain(|p| p != path);
        if self.recent_files.len() >= RECENT_LIMIT {
            self.recent_files.remove(0);
        }
        self.recent_files.push(path.to_string());
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WindowPlacement {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub maximized: bool,
}

/// A tool window's bounds and column widths (Java `persistedUi.suggestions-list-position-7/9`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FramePlacement {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub columns: Vec<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Persisted {
    pub last_folder: Option<String>,
    /// Where the picture chooser starts (Java `filesystem.last-image-folder`, else `last-folder`).
    pub last_image_folder: Option<String>,
    pub window: Option<WindowPlacement>,
    /// 独立选点列表 with 7 columns (no score) and with 9 columns (KataGo / Sai).
    pub suggestions_frame_7: Option<FramePlacement>,
    pub suggestions_frame_9: Option<FramePlacement>,
    /// Column widths of the main window's 选点列表 (`persistedUi.main-window-list`, the five
    /// shown columns).
    pub main_list_columns: Option<Vec<u32>>,
    /// 超级鹰眼's last placement (primary window only, like Java).
    pub hawkeye: Option<HawkeyePlacement>,
    pub panel_sizes: PanelSizes,
    pub bottom_toolbar: BottomToolbar,
}

/// File locations inside the product's per-user data directories.
#[derive(Clone, Debug)]
pub struct Storage {
    pub config_file: PathBuf,
    pub persist_file: PathBuf,
    pub logs_dir: PathBuf,
    /// Downloaded and imported engine resources (packages, weights, runtimes, partial downloads).
    pub resources_dir: PathBuf,
    /// 存档与读档 slots (Java `save/`).
    pub save_dir: PathBuf,
}

impl Storage {
    pub fn in_dirs(config_dir: &Path, data_dir: &Path) -> Storage {
        Storage {
            config_file: config_dir.join("config.json"),
            persist_file: config_dir.join("persist.json"),
            logs_dir: data_dir.join("logs"),
            resources_dir: data_dir.join("resources"),
            save_dir: data_dir.join("save"),
        }
    }
}

/// Reads a JSON file; a missing or unreadable file yields defaults (Java falls back to its
/// default config the same way).
pub fn load_json<T: Default + for<'de> Deserialize<'de>>(path: &Path) -> T {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// Writes through a temporary file and rename so a crash never leaves a partial file.
pub fn save_json<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    write_atomic(path, &serde_json::to_vec_pretty(value)?)
}

/// Replaces `path` with `bytes` through a temporary file and rename; on failure the temporary
/// file is removed and `path` is unchanged.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    let result = std::fs::write(&tmp, bytes).and_then(|()| std::fs::rename(&tmp, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Writes `config` and, if given, `persisted` as one change: when the second write fails, the
/// first file is put back as it was (or removed if it did not exist). An existing settings file
/// that cannot be read (and so could not be put back) is not replaced at all.
pub fn save_both(storage: &Storage, config: &Config, persisted: Option<&Persisted>) -> std::io::Result<()> {
    let previous = match std::fs::read(&storage.config_file) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };
    save_json(&storage.config_file, config)?;
    if let Some(p) = persisted {
        if let Err(e) = save_json(&storage.persist_file, p) {
            let restored = match previous {
                Some(bytes) => write_atomic(&storage.config_file, &bytes),
                None => std::fs::remove_file(&storage.config_file),
            };
            return Err(match restored {
                Ok(()) => e,
                Err(r) => std::io::Error::new(
                    e.kind(),
                    format!("{e}; putting {} back failed: {r}", storage.config_file.display()),
                ),
            });
        }
    }
    Ok(())
}
