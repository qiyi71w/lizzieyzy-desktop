//! The ReadBoard line protocol (Java `analysis/ReadBoard.java` `parseLine`, `sendCommand`; helper
//! keywords `readboard/Core/Protocol/ProtocolKeywords.cs` at v3.2.0): one helper line becomes one
//! [`Inbound`], one host command becomes one line through [`Outbound::line`].
//!
//! Java tests every line against a chain of independent prefix checks (`startsWith`, `trim().equals`)
//! in a fixed order, some of which return early. The helper never sends a line that two families
//! would both act on, except the three combinations Java relies on, which are folded into one
//! variant here: `syncPlatform yike` (context and Yike start), `error place failed` (placement
//! failure and console echo) and `playpon …`/`playoutschanged …` (whose extra pass through the
//! `play` check has no effect). A number Java would reject with `Integer.parseInt` drops the rest
//! of the line exactly where Java's exception would; the variants carry `None` for that.

use crate::record::Stone;

/// The lowest helper build this client works with (Java `ReadBoard.version`).
pub const MIN_HELPER_VERSION: i32 = 220430;

/// Where the helper says the last-move marker of a frame comes from (Java `ReadBoardLastMoveSource`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LastMoveSource {
    /// No `lastMoveSource` line in this frame (an older helper).
    LegacyUnknown,
    /// A token this client does not know.
    Unknown,
    None,
    RedBlueMarker,
    FoxCornerFlip,
    Deviation,
    StoneCount,
}

impl LastMoveSource {
    /// `ReadBoardLastMoveSource.parse`.
    pub fn parse(token: &str) -> LastMoveSource {
        match token {
            "none" => LastMoveSource::None,
            "redBlueMarker" => LastMoveSource::RedBlueMarker,
            "foxCornerFlip" => LastMoveSource::FoxCornerFlip,
            "deviation" => LastMoveSource::Deviation,
            "stoneCount" => LastMoveSource::StoneCount,
            _ => LastMoveSource::Unknown,
        }
    }

    /// A marker read from the board image itself, so its colour decides the side to play.
    pub fn is_trusted_visual_marker(self) -> bool {
        matches!(self, LastMoveSource::RedBlueMarker | LastMoveSource::FoxCornerFlip)
    }
}

/// The client window the helper reads (Java `SyncRemoteContext.SyncPlatform`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncPlatform {
    Fox,
    Yike,
    Generic,
}

/// Screen rectangle of the float board (`inboard`), already multiplied by the DPI factor of the
/// `99_<factor>_<type>` form and rounded up like Java's `Math.ceil(value * factor)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FloatBoardGeometry {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub board_type: i32,
    /// The DPI factor of the `99_<factor>_<type>` form (Java `new FloatBoard(…, true)`).
    pub scale: Option<f32>,
}

/// `play>black|white>time playouts firstPlayouts [gma]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayConfig {
    /// `autoPlayColorFromPlayParams`: the trimmed colour word; `None` for anything else.
    pub color: Option<Stone>,
    /// The untrimmed colour word as Java's toolbar branch compares it (`params[1].equals`).
    pub toolbar_side: Option<Stone>,
    pub time: i32,
    pub playouts: i32,
    pub first_playouts: i32,
    /// Fourth token `gma` (`isReadBoardGmaPlayMode`).
    pub gma: bool,
}

/// `start <w> <h>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartSize {
    Size { width: u8, height: u8 },
    /// Fewer than three words.
    Missing,
    /// A size Java could not parse (the rest of the line is dropped).
    Invalid,
}

/// One helper line.
#[derive(Clone, Debug, PartialEq)]
pub enum Inbound {
    /// `readboardUpdateReady\t<tag>\t<zip>`: the hosted update request, validated by the updater.
    UpdateReady,
    /// `readboardLoggingV1` / `readboardLoggingSet` / `readboardLoggingObserved`.
    Logging,
    YikeSyncStart,
    YikeSyncStop,
    /// `playpon on|off`; `None` for any other word.
    PlayPonder(Option<bool>),
    /// `re=` row: the cells when every cell is one character `0`..`4`.
    Row(Option<Vec<u8>>),
    /// `foxMoveNumber <n>`; `None` (ignored) for anything but one integer.
    FoxMoveNumber(Option<i32>),
    LastMoveSource(LastMoveSource),
    /// `syncPlatform <p>`; `yike_line` = the whole line is `syncPlatform yike` (any case).
    SyncPlatform { platform: SyncPlatform, yike_line: bool },
    RoomToken(String),
    LiveTitleMove(Option<i32>),
    RecordCurrentMove(Option<i32>),
    RecordTotalMove(Option<i32>),
    RecordAtEnd(bool),
    RecordTitleFingerprint(String),
    ForceRebuild,
    /// `version: <n>`: `number` is `None` when Java's parse would have failed.
    Version { number: Option<i32> },
    /// `error place failed`; `echo` = the line also starts with `error` (console echo).
    PlaceFailed { echo: bool },
    /// Any other `error…` line.
    Error,
    End,
    Clear,
    ClearBoard,
    Start(StartSize),
    Sync,
    BothSync,
    NoBothSync,
    StopAutoPlay,
    EndSync,
    StopSync,
    /// `play>…`; `None` when Java drops the line (wrong shape or a bad number).
    Play(Option<PlayConfig>),
    Pass,
    FirstChanged(Option<i32>),
    PlayoutsChanged(Option<i32>),
    TimeChanged(Option<i32>),
    NoPonder,
    ResumePonder,
    NoInBoard,
    /// `inboard …`; `None` when the line has not six words or a number does not parse.
    InBoard(Option<FloatBoardGeometry>),
    NotInBoard,
    ForeFoxWithInBoard,
    NotForeFoxWithInBoard,
    PlaceComplete,
    /// `ready` (handled after `parseLine` by Java's reader).
    Ready,
    /// Nothing this client acts on.
    Other,
}

/// One host command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outbound {
    Version,
    UpdateSupported,
    UpdatePackageV2Supported,
    AnalysisState { running: bool },
    /// `place x y`: 0-based column and row from the top.
    Place { x: u16, y: u16 },
    Loss,
    Quit,
}

impl Outbound {
    pub fn line(self) -> String {
        match self {
            Outbound::Version => "version".to_owned(),
            Outbound::UpdateSupported => "readboardUpdateSupported".to_owned(),
            Outbound::UpdatePackageV2Supported => "readboardUpdatePackageV2Supported".to_owned(),
            Outbound::AnalysisState { running } => {
                format!("analysisState {}", if running { "running" } else { "paused" })
            }
            Outbound::Place { x, y } => format!("place {x} {y}"),
            Outbound::Loss => "loss".to_owned(),
            Outbound::Quit => "quit".to_owned(),
        }
    }
}

/// Java `String.trim`: strips every character up to U+0020.
pub(crate) fn java_trim(s: &str) -> &str {
    s.trim_matches(|c: char| c <= ' ')
}

/// Java `String.split(sep)`: trailing empty pieces are removed; a string without `sep` is itself.
pub(crate) fn java_split(s: &str, sep: char) -> Vec<&str> {
    if !s.contains(sep) {
        return vec![s];
    }
    let mut parts: Vec<&str> = s.split(sep).collect();
    while parts.last() == Some(&"") {
        parts.pop();
    }
    parts
}

/// Java `Integer.parseInt`.
fn int(s: &str) -> Option<i32> {
    s.parse().ok()
}

/// Java `Float.parseFloat` for the decimal forms the helper writes.
fn float(s: &str) -> Option<f32> {
    let t = java_trim(s);
    let t = t.strip_suffix(['f', 'F', 'd', 'D']).unwrap_or(t);
    t.parse().ok()
}

/// `parseOptionalInt`: the trimmed rest of the line after `prefix`.
fn optional_int(line: &str, prefix: &str) -> Option<i32> {
    int(java_trim(&line[prefix.len()..]))
}

fn color_word(word: &str) -> Option<Stone> {
    match word {
        "black" => Some(Stone::Black),
        "white" => Some(Stone::White),
        _ => None,
    }
}

/// `ReadBoardLoggingProtocol.isControlLine`.
fn is_logging_line(line: &str) -> bool {
    let trimmed = java_trim(line);
    let command = trimmed.split(' ').next().unwrap_or("");
    matches!(command, "readboardLoggingV1" | "readboardLoggingSet" | "readboardLoggingObserved")
}

/// `ReadBoardUpdateRequest.tryParse` shape: three tab-separated fields led by the command. The
/// version tag and path are validated by the updater; Java's `parseLine` acts on no other family
/// for such a line, so routing it whole keeps Java's behaviour.
fn is_update_line(line: &str) -> bool {
    let parts: Vec<&str> = line.split('\t').collect();
    parts.len() == 3 && parts[0] == "readboardUpdateReady"
}

fn parse_play(line: &str) -> Option<PlayConfig> {
    let params = java_split(java_trim(line), '>');
    if params.len() != 3 {
        return None;
    }
    let play = java_split(java_trim(params[2]), ' ');
    if play.len() < 3 {
        return None;
    }
    let playouts = int(play[1])?;
    let first_playouts = int(play[2])?;
    let time = int(play[0])?;
    Some(PlayConfig {
        color: color_word(java_trim(params[1])),
        toolbar_side: color_word(params[1]),
        time,
        playouts,
        first_playouts,
        gma: play.len() >= 4 && java_trim(play[3]) == "gma",
    })
}

fn parse_inboard(line: &str) -> Option<FloatBoardGeometry> {
    let params = java_split(java_trim(line), ' ');
    if params.len() != 6 {
        return None;
    }
    if params[5].starts_with("99") {
        let parts = java_split(params[5], '_');
        let factor = float(parts.get(1)?)?;
        let scaled = |s: &str| int(s).map(|v| (v as f32 * factor).ceil() as i32);
        Some(FloatBoardGeometry {
            x: scaled(params[1])?,
            y: scaled(params[2])?,
            width: scaled(params[3])?,
            height: scaled(params[4])?,
            board_type: int(parts.get(2)?)?,
            scale: Some(factor),
        })
    } else {
        Some(FloatBoardGeometry {
            x: int(params[1])?,
            y: int(params[2])?,
            width: int(params[3])?,
            height: int(params[4])?,
            board_type: int(params[5])?,
            scale: None,
        })
    }
}

/// `firstchanged`/`playoutschanged`/`timechanged <n>`: Java acts only on exactly two words.
fn changed_value(line: &str) -> Option<i32> {
    let params = java_split(java_trim(line), ' ');
    if params.len() == 2 {
        int(params[1])
    } else {
        None
    }
}

/// Classifies one helper line (without its `\n`; a trailing `\r` is ignored).
pub fn parse_line(raw: &str) -> Inbound {
    let line = raw.strip_suffix('\r').unwrap_or(raw);
    let trimmed = java_trim(line);
    if is_update_line(line) {
        return Inbound::UpdateReady;
    }
    if is_logging_line(line) {
        return Inbound::Logging;
    }
    match trimmed {
        "yikeSyncStart" => return Inbound::YikeSyncStart,
        "yikeSyncStop" => return Inbound::YikeSyncStop,
        _ => {}
    }
    if line.starts_with("playpon") {
        let params = java_split(line, ' ');
        let on = match params.as_slice() {
            [_, word] if word.starts_with("on") => Some(true),
            [_, word] if word.starts_with("off") => Some(false),
            _ => None,
        };
        return Inbound::PlayPonder(on);
    }
    if let Some(row) = line.strip_prefix("re=") {
        let cells: Option<Vec<u8>> = row
            .split(',')
            .map(|cell| match cell.as_bytes() {
                [c @ b'0'..=b'4'] => Some(c - b'0'),
                _ => None,
            })
            .collect();
        return Inbound::Row(cells);
    }
    if line.starts_with("foxMoveNumber") {
        let params = java_split(trimmed, ' ');
        return Inbound::FoxMoveNumber(if params.len() == 2 { int(params[1]) } else { None });
    }
    if let Some(token) = line.strip_prefix("lastMoveSource ") {
        return Inbound::LastMoveSource(LastMoveSource::parse(java_trim(token)));
    }
    if let Some(token) = line.strip_prefix("syncPlatform ") {
        let t = java_trim(token);
        let platform = if t.eq_ignore_ascii_case("fox") {
            SyncPlatform::Fox
        } else if t.eq_ignore_ascii_case("yike") {
            SyncPlatform::Yike
        } else {
            SyncPlatform::Generic
        };
        return Inbound::SyncPlatform { platform, yike_line: trimmed.eq_ignore_ascii_case("syncPlatform yike") };
    }
    if let Some(token) = line.strip_prefix("roomToken ") {
        return Inbound::RoomToken(java_trim(token).to_owned());
    }
    if line.starts_with("liveTitleMove ") {
        return Inbound::LiveTitleMove(optional_int(line, "liveTitleMove "));
    }
    if line.starts_with("recordCurrentMove ") {
        return Inbound::RecordCurrentMove(optional_int(line, "recordCurrentMove "));
    }
    if line.starts_with("recordTotalMove ") {
        return Inbound::RecordTotalMove(optional_int(line, "recordTotalMove "));
    }
    if line.starts_with("recordAtEnd ") {
        return Inbound::RecordAtEnd(trimmed.ends_with('1'));
    }
    if let Some(token) = line.strip_prefix("recordTitleFingerprint ") {
        return Inbound::RecordTitleFingerprint(java_trim(token).to_owned());
    }
    if trimmed == "forceRebuild" {
        return Inbound::ForceRebuild;
    }
    if line.starts_with("version") {
        let params = java_split(trimmed, ' ');
        return Inbound::Version { number: params.get(1).and_then(|p| int(p)) };
    }
    if trimmed.starts_with("error place failed") {
        return Inbound::PlaceFailed { echo: line.starts_with("error") };
    }
    if line.starts_with("error") {
        return Inbound::Error;
    }
    match trimmed {
        "end" => return Inbound::End,
        "clear" => return Inbound::Clear,
        "clearBoard" => return Inbound::ClearBoard,
        _ => {}
    }
    if line.starts_with("start") {
        let params = java_split(trimmed, ' ');
        if params.len() < 3 {
            return Inbound::Start(StartSize::Missing);
        }
        let size = |s: &str| int(s).and_then(|v| u8::try_from(v).ok()).filter(|&v| v > 0);
        return Inbound::Start(match (size(params[1]), size(params[2])) {
            (Some(width), Some(height)) => StartSize::Size { width, height },
            _ => StartSize::Invalid,
        });
    }
    if trimmed == "sync" {
        return Inbound::Sync;
    }
    if line.starts_with("both") {
        return Inbound::BothSync;
    }
    if line.starts_with("noboth") {
        return Inbound::NoBothSync;
    }
    if line.starts_with("stopAutoPlay") {
        return Inbound::StopAutoPlay;
    }
    if line.starts_with("endsync") {
        return Inbound::EndSync;
    }
    if line.starts_with("stopsync") {
        return Inbound::StopSync;
    }
    if line.starts_with("playoutschanged") {
        return Inbound::PlayoutsChanged(changed_value(line));
    }
    if line.starts_with("play") {
        return Inbound::Play(parse_play(line));
    }
    if line.starts_with("pass") {
        return Inbound::Pass;
    }
    if line.starts_with("firstchanged") {
        return Inbound::FirstChanged(changed_value(line));
    }
    if line.starts_with("timechanged") {
        return Inbound::TimeChanged(changed_value(line));
    }
    match trimmed {
        "noponder" => return Inbound::NoPonder,
        "resumeponder" => return Inbound::ResumePonder,
        _ => {}
    }
    if line.starts_with("noinboard") {
        return Inbound::NoInBoard;
    }
    if line.starts_with("inboard") {
        return Inbound::InBoard(parse_inboard(line));
    }
    if line.starts_with("notinboard") {
        return Inbound::NotInBoard;
    }
    if line.starts_with("foreFoxWithInBoard") {
        return Inbound::ForeFoxWithInBoard;
    }
    if line.starts_with("notForeFoxWithInBoard") {
        return Inbound::NotForeFoxWithInBoard;
    }
    if line.starts_with("placeComplete") {
        return Inbound::PlaceComplete;
    }
    if trimmed == "ready" {
        return Inbound::Ready;
    }
    Inbound::Other
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_need_single_cells_zero_to_four() {
        assert_eq!(parse_line("re=0,1,2,3,4\r"), Inbound::Row(Some(vec![0, 1, 2, 3, 4])));
        assert_eq!(parse_line("re=0,5,0"), Inbound::Row(None));
        assert_eq!(parse_line("re=0,,0"), Inbound::Row(None));
        assert_eq!(parse_line("re=00,1"), Inbound::Row(None));
    }

    #[test]
    fn malformed_numbers_drop_the_line_instead_of_panicking() {
        assert_eq!(parse_line("start x 19"), Inbound::Start(StartSize::Invalid));
        assert_eq!(parse_line("start 19"), Inbound::Start(StartSize::Missing));
        assert_eq!(parse_line("foxMoveNumber 12x"), Inbound::FoxMoveNumber(None));
        assert_eq!(parse_line("play>black>a 100 0"), Inbound::Play(None));
        assert_eq!(parse_line("play>black>5 100"), Inbound::Play(None));
        assert_eq!(parse_line("version"), Inbound::Version { number: None });
        assert_eq!(parse_line("timechanged 1 2"), Inbound::TimeChanged(None));
        assert_eq!(parse_line("inboard 1 2 x 4 5"), Inbound::InBoard(None));
        assert_eq!(parse_line("inboard 1 2 3 4 99_x_1"), Inbound::InBoard(None));
        assert_eq!(parse_line("liveTitleMove abc"), Inbound::LiveTitleMove(None));
    }

    #[test]
    fn play_lines_carry_the_auto_play_configuration() {
        assert_eq!(
            parse_line("play>white>5 200 50 gma"),
            Inbound::Play(Some(PlayConfig {
                color: Some(Stone::White),
                toolbar_side: Some(Stone::White),
                time: 5,
                playouts: 200,
                first_playouts: 50,
                gma: true,
            }))
        );
        // `playpon` and `playoutschanged` pass Java's `play` check without effect.
        assert_eq!(parse_line("playpon on"), Inbound::PlayPonder(Some(true)));
        assert_eq!(parse_line("playponder off"), Inbound::PlayPonder(Some(false)));
        assert_eq!(parse_line("playoutschanged 300"), Inbound::PlayoutsChanged(Some(300)));
    }

    #[test]
    fn inboard_has_a_plain_and_a_dpi_scaled_form() {
        assert_eq!(
            parse_line("inboard 10 20 300 400 1"),
            Inbound::InBoard(Some(FloatBoardGeometry {
                x: 10,
                y: 20,
                width: 300,
                height: 400,
                board_type: 1,
                scale: None,
            }))
        );
        assert_eq!(
            parse_line("inboard 10 21 300 401 99_1.5_2"),
            Inbound::InBoard(Some(FloatBoardGeometry {
                x: 15,
                y: 32,
                width: 450,
                height: 602,
                board_type: 2,
                scale: Some(1.5),
            }))
        );
        assert_eq!(parse_line("noinboard"), Inbound::NoInBoard);
        assert_eq!(parse_line("notinboard"), Inbound::NotInBoard);
    }

    #[test]
    fn routed_and_context_lines() {
        assert_eq!(parse_line("readboardUpdateReady\tv1.2.3\tC:\\a.zip"), Inbound::UpdateReady);
        assert_eq!(parse_line("readboardLoggingV1 a on off"), Inbound::Logging);
        assert_eq!(parse_line(" yikeSyncStart "), Inbound::YikeSyncStart);
        assert_eq!(
            parse_line("syncPlatform YIKE"),
            Inbound::SyncPlatform { platform: SyncPlatform::Yike, yike_line: true }
        );
        assert_eq!(parse_line("lastMoveSource foxCornerFlip"), Inbound::LastMoveSource(LastMoveSource::FoxCornerFlip));
        assert_eq!(parse_line("lastMoveSource bogus"), Inbound::LastMoveSource(LastMoveSource::Unknown));
        assert_eq!(parse_line("lastMoveSource"), Inbound::Other);
        assert_eq!(parse_line("error place failed"), Inbound::PlaceFailed { echo: true });
        assert_eq!(parse_line("error something"), Inbound::Error);
        assert_eq!(parse_line("bothSync"), Inbound::BothSync);
        assert_eq!(parse_line("nobothSync"), Inbound::NoBothSync);
        assert_eq!(parse_line("endsync"), Inbound::EndSync);
        assert_eq!(parse_line("end"), Inbound::End);
        assert_eq!(parse_line("ready\r"), Inbound::Ready);
    }

    #[test]
    fn outbound_lines() {
        assert_eq!(Outbound::Place { x: 3, y: 15 }.line(), "place 3 15");
        assert_eq!(Outbound::AnalysisState { running: false }.line(), "analysisState paused");
        assert_eq!(Outbound::UpdatePackageV2Supported.line(), "readboardUpdatePackageV2Supported");
    }
}
