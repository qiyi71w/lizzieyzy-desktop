//! Platform adapter for engine resources: detection probes, downloads, archive installs,
//! choosers and verification processes run here, off the UI thread and outside the workbench
//! lock; their results go back through the core workbench.

use lizzie_core::api::{ApiError, CommandResult, ResourceCommand, ResourceResult};
use tauri::AppHandle;

use crate::State;

pub async fn dispatch(_app: AppHandle, _shared: State, op: ResourceCommand) -> Result<CommandResult, ApiError> {
    match op {
        ResourceCommand::Inspect { .. } => Ok(CommandResult::Resource {
            result: ResourceResult::Done,
        }),
    }
}
