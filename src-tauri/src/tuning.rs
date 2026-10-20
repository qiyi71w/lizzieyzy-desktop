//! Platform adapter for engine performance: benchmark processes and file choosers run here, off
//! the UI thread and outside the workbench lock; their results go back through the core
//! workbench.

use lizzie_core::api::{ApiError, CommandResult, TuningCommand, TuningResult};
use tauri::AppHandle;

use crate::State;

pub async fn dispatch(_app: AppHandle, _shared: State, op: TuningCommand) -> Result<CommandResult, ApiError> {
    match op {
        TuningCommand::Refresh => Ok(CommandResult::Tuning {
            result: TuningResult::Done,
        }),
    }
}
