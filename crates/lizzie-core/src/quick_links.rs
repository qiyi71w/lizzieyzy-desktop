//! 快速启动 (Java `Menu.quickLinks`, `OtherPrograms`, `FastLink`): a list of outside programs,
//! each a name and a command line, shown as a menu while 显示快速启动 is on. Choosing one starts
//! its command at once, split and resolved like an engine command, without waiting for it.

use crate::engine::gtp::{join_command, LaunchSpec};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::{Command, Stdio};
use ts_rs::TS;

/// One program of the list (`leelaz.program-name-list` / `program-command-list` at one index).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct QuickLinkProgram {
    pub name: String,
    pub command: String,
}

impl QuickLinkProgram {
    /// 自动生成 (`OtherPrograms.getEngineLine`): the entry for the file picked in the chooser,
    /// named after the file. Java puts the bare path in the command, which the shared command
    /// syntax then splits at a space; here the path is one token of that syntax
    /// ([`join_command`]), so the picked file is the program that starts. A path holding both
    /// kinds of quote, which [`join_command`] does not quote, is given as it is.
    pub fn chosen(path: &Path) -> QuickLinkProgram {
        let text = path.to_string_lossy().into_owned();
        QuickLinkProgram {
            name: path.file_name().map_or_else(|| text.clone(), |name| name.to_string_lossy().into_owned()),
            command: join_command(std::slice::from_ref(&text)).unwrap_or(text),
        }
    }
}

/// 显示快速启动 (`ui.show-quick-links`, default off) and the program list (empty by default).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct QuickLinkSettings {
    pub show: bool,
    pub programs: Vec<QuickLinkProgram>,
}

impl QuickLinkSettings {
    /// The list as Java keeps it (`OtherPrograms.saveEngineConfig`, `getProgramData`): commands
    /// are trimmed and an entry without a command is no entry. Names stay as typed.
    pub fn normalized(mut self) -> QuickLinkSettings {
        for program in &mut self.programs {
            program.command = program.command.trim().to_string();
        }
        self.programs.retain(|p| !p.command.is_empty());
        self
    }
}

/// `FastLink.startProgram`: starts `spec` and leaves it running on its own. Nothing is read from
/// or written to it; it is reaped when it ends. An error is a program that could not be started.
pub fn launch(spec: &LaunchSpec) -> std::io::Result<()> {
    let mut command = Command::new(&spec.program);
    command.args(&spec.args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    if let Some(dir) = &spec.working_dir {
        command.current_dir(dir);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        if !spec.library_dirs.is_empty() {
            let mut paths = spec.library_dirs.clone();
            paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()));
            if let Ok(path) = std::env::join_paths(paths) {
                command.env("PATH", path);
            }
        }
        // Java's ProcessBuilder starts processes without a console window.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command.spawn()?;
    // Waiting only collects the exit status; the program runs on its own.
    let _ = std::thread::Builder::new().name("quick-link".into()).spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}
