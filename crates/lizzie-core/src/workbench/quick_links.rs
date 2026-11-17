//! 快速启动: 显示快速启动, the program list edited in 快速启动设置 (`OtherPrograms`), and starting a
//! program from the menu (`FastLink.startProgram`).

use super::Workbench;
use crate::api::{ApiError, CommandResult};
use crate::engine::gtp::prepare_launch;
use crate::quick_links::{self, QuickLinkSettings};

/// Refusal: the chosen program could not be started (`FastLink`'s "运行失败! ").
pub const QUICK_LINK_FAILED: &str = "product.quickLinkFailed";

impl Workbench {
    /// Saves 显示快速启动 and the program list together; the menu follows the saved list.
    pub fn set_quick_links(&mut self, settings: QuickLinkSettings) -> Result<bool, ApiError> {
        let settings = settings.normalized();
        if settings == self.config.quick_links {
            return Ok(false);
        }
        self.commit_config(|c| c.quick_links = settings)?;
        Ok(true)
    }

    /// Starts the saved program at `index` of the list, its command split and resolved like an
    /// engine's (the same candidate folders, working directory and runtime libraries), without
    /// waiting for it. A program that cannot be started is refused with [`QUICK_LINK_FAILED`].
    pub fn launch_quick_link(&mut self, index: usize) -> CommandResult {
        let Some(program) = self.config.quick_links.programs.get(index) else {
            return CommandResult::Unchanged;
        };
        let started = match prepare_launch(&program.command, self.engines.roots()) {
            Some(mut spec) => {
                spec.library_dirs = self.engines.library_dirs().to_vec();
                quick_links::launch(&spec).map_err(|e| e.kind().to_string())
            }
            None => Err("empty command".to_string()),
        };
        match started {
            Ok(()) => CommandResult::Done,
            // The command line may carry credentials: only its position is logged.
            Err(reason) => {
                self.log(true, format!("quick link {} could not start: {reason}", index + 1));
                CommandResult::Refused { message_key: QUICK_LINK_FAILED.into() }
            }
        }
    }
}
