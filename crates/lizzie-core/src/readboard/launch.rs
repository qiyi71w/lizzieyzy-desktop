//! Finding and starting the helper (Java `ReadBoard.legacyNativeReadBoardDirectory`,
//! `nativeReadBoardDirectoryCandidatesForBase`, `isNativeReadBoardExeAvailable`, `startEngine` and
//! `completeLaunchArguments`).
//!
//! The helper lives in a `readboard` directory next to the program: Java looks below the working
//! directory, the directory of its jar and the root of its bundled runtime, each directly and
//! below `app/`. The app passes its own equivalents as base directories (absolute paths, in order
//! of preference), for example the working directory, the executable's directory and the
//! resource directory where the Windows package installs `readboard/readboard.exe`.

use std::fs;
use std::path::{Path, PathBuf};

use super::logging::{named_launch_arguments, LoggingLaunch};
use super::HelperLaunch;

/// The helper executable (Java `LEGACY_NATIVE_READBOARD_EXE`). Only the pipe transport is
/// reachable in Java (`new ReadBoard(true, false)`), so the TCP `readboard.bat` is never used.
pub const HELPER_EXE: &str = "readboard.exe";
const HELPER_DIR: &str = "readboard";

/// Java `nativeReadBoardDirectoryCandidatesForBase`: `<base>/readboard`, then
/// `<base>/app/readboard`. (Java also adds `<base>/readboard` again when the base itself is named
/// `app`; that is the first candidate repeated, which its ordered set drops.)
fn candidates_for_base(base: &Path) -> [PathBuf; 2] {
    [base.join(HELPER_DIR), base.join("app").join(HELPER_DIR)]
}

/// Java `defaultNativeReadBoardDirectoryCandidates`: the candidates of every base in order,
/// without repeats.
pub fn candidates(base_dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut all: Vec<PathBuf> = Vec::new();
    for candidate in base_dirs.iter().flat_map(|base| candidates_for_base(base)) {
        if !all.contains(&candidate) {
            all.push(candidate);
        }
    }
    all
}

/// Java `canRead()` on `readboard.exe`: the file exists and can be opened.
fn helper_readable(dir: &Path) -> bool {
    let exe = dir.join(HELPER_EXE);
    exe.is_file() && fs::File::open(&exe).is_ok()
}

/// Java `resolveNativeReadBoardDirectory`: the first candidate with a readable `readboard.exe`.
/// Without one, Java falls back to `readboard` below the working directory, which is the first
/// candidate of the first base; the helper then counts as unavailable there.
pub fn helper_dir(candidates: &[PathBuf]) -> PathBuf {
    candidates
        .iter()
        .find(|dir| helper_readable(dir))
        .or_else(|| candidates.first())
        .cloned()
        .unwrap_or_else(|| PathBuf::from(HELPER_DIR))
}

/// Java `isNativeReadBoardExeAvailable` (the menu, shortcut and function-search gate): the
/// directory holds a readable `readboard.exe`.
pub fn available(dir: &Path) -> bool {
    helper_readable(dir)
}

/// The values a launch snapshots from the host (Java reads the main toolbar's auto-play fields
/// and the `ReadBoard.language` resource string).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LaunchParams {
    /// Auto-play time (Java `txtAutoPlayTime`): the field text when its box is checked, else
    /// `None`.
    pub time: Option<String>,
    /// Auto-play playouts (Java `txtAutoPlayPlayouts`), as `time`.
    pub playouts: Option<String>,
    /// Auto-play first-move playouts (Java `txtAutoPlayFirstPlayouts`), as `time`.
    pub first_playouts: Option<String>,
    /// The helper's UI language (`ReadBoard.language`: `cn`, `en`, `jp` or `kr`).
    pub language: String,
    /// Logging inputs; `None` launches without the logging contract.
    pub logging: Option<LoggingLaunch>,
}

/// Java `startEngine`: `<dir>/readboard.exe` started in `dir` with
/// `yzy <time> <playouts> <firstPlayouts> 0 <language> -1` and, for a logging-contract launch,
/// the named logging arguments. An unchecked or empty auto-play value is passed as `" "`; `0`
/// and `-1` select the pipe transport (the helper's `LaunchOptions`: `args[4] == "1"` would be
/// TCP, `args[6]` the TCP port).
pub fn launch_spec(dir: &Path, params: &LaunchParams) -> HelperLaunch {
    let value = |v: &Option<String>| match v.as_deref() {
        Some(text) if !text.is_empty() => text.to_string(),
        _ => " ".to_string(),
    };
    let mut args = vec![
        "yzy".to_string(),
        value(&params.time),
        value(&params.playouts),
        value(&params.first_playouts),
        "0".to_string(),
        params.language.clone(),
        "-1".to_string(),
    ];
    if let Some(logging) = &params.logging {
        args.extend(named_launch_arguments(logging));
    }
    HelperLaunch { program: dir.join(HELPER_EXE), dir: dir.to_path_buf(), args }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidates_keep_base_order_and_drop_repeats() {
        let work = PathBuf::from("/opt/work");
        let exe = PathBuf::from("/opt/work/app");
        let resources = PathBuf::from("/usr/lib/lizzie");
        assert_eq!(
            candidates(&[work.clone(), exe.clone(), resources.clone(), work.clone()]),
            vec![
                work.join("readboard"),
                work.join("app/readboard"),
                exe.join("app/readboard"),
                resources.join("readboard"),
                resources.join("app/readboard"),
            ]
        );
    }

    #[test]
    fn a_base_named_app_adds_no_extra_candidate() {
        let app = PathBuf::from("/opt/lizzie/app");
        assert_eq!(candidates(std::slice::from_ref(&app)), vec![app.join("readboard"), app.join("app/readboard")]);
        assert_eq!(candidates(&[PathBuf::from("/opt/lizzie/APP")]).len(), 2);
        assert!(candidates(&[]).is_empty());
    }

    #[test]
    fn helper_dir_is_first_candidate_with_the_executable_else_the_first_candidate() {
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("work");
        let second = temp.path().join("install");
        let all = candidates(&[first.clone(), second.clone()]);
        assert_eq!(helper_dir(&all), first.join("readboard"));
        assert!(!available(&helper_dir(&all)));

        // A directory named readboard.exe is not the helper.
        fs::create_dir_all(first.join("app/readboard/readboard.exe")).unwrap();
        fs::create_dir_all(second.join("app/readboard")).unwrap();
        fs::write(second.join("app/readboard/readboard.exe"), b"MZ").unwrap();
        assert_eq!(helper_dir(&all), second.join("app/readboard"));
        assert!(available(&helper_dir(&all)));

        fs::create_dir_all(first.join("readboard")).unwrap();
        fs::write(first.join("readboard/readboard.exe"), b"MZ").unwrap();
        assert_eq!(helper_dir(&all), first.join("readboard"));

        assert_eq!(helper_dir(&[]), PathBuf::from("readboard"));
    }

    #[test]
    fn launch_arguments_use_space_placeholders_and_pipe_transport() {
        let dir = PathBuf::from("/opt/lizzie/readboard");
        let spec = launch_spec(
            &dir,
            &LaunchParams {
                time: None,
                playouts: Some(String::new()),
                first_playouts: None,
                language: "cn".to_string(),
                logging: None,
            },
        );
        assert_eq!(spec.program, dir.join("readboard.exe"));
        assert_eq!(spec.dir, dir);
        assert_eq!(spec.args, ["yzy", " ", " ", " ", "0", "cn", "-1"]);
    }

    #[test]
    fn launch_arguments_carry_values_and_logging_suffix() {
        let logging = LoggingLaunch {
            log_dir: "C:\\Users\\u\\AppData\\logs\\readboard".to_string(),
            host_session_id: "c2Vzc2lvbg".to_string(),
            diagnostics: false,
        };
        let params = LaunchParams {
            time: Some("10".to_string()),
            playouts: Some("2000".to_string()),
            first_playouts: Some("500".to_string()),
            language: "en".to_string(),
            logging: Some(logging.clone()),
        };
        let spec = launch_spec(Path::new("readboard"), &params);
        assert_eq!(
            spec.args,
            [
                "yzy",
                "10",
                "2000",
                "500",
                "0",
                "en",
                "-1",
                "--log-dir",
                "C:\\Users\\u\\AppData\\logs\\readboard",
                "--host-session-id",
                "c2Vzc2lvbg",
                "--logging-contract",
                "1",
                "--diagnostics",
                "off",
                "--capture",
                "off"
            ]
        );

        // Without a usable Windows log directory the helper starts like an old host launched it.
        let unix = LaunchParams {
            logging: Some(LoggingLaunch { log_dir: "/home/u/logs/readboard".to_string(), ..logging }),
            ..params
        };
        assert_eq!(launch_spec(Path::new("readboard"), &unix).args, ["yzy", "10", "2000", "500", "0", "en", "-1"]);
    }
}
