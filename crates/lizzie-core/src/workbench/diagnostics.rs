//! 帮助 › 诊断与日志: the saved diagnostic switches and the 完整日志 session
//! (`DiagnosticsDialog.applyCurrentPlan`, `startFullTraceFromUi`, `stopFullTraceFromUi`).

use super::Workbench;
use crate::api::{ApiError, ErrorKind};
use crate::diagnostics::{Diagnostics, LoggingSettings, TraceScope};

impl Workbench {
    /// Records through `diagnostics`, the application's recorder that the network adapters
    /// share, with the saved switches.
    pub fn set_diagnostics(&mut self, diagnostics: Diagnostics) {
        diagnostics.apply(&self.config.logging);
        self.engines.set_diagnostics(diagnostics.clone());
        self.diagnostics = diagnostics;
    }

    pub fn diagnostics(&self) -> &Diagnostics {
        &self.diagnostics
    }

    /// 应用: 诊断记录, the modules and the scope preference take effect at once and are saved.
    /// A running 完整日志 session keeps its scopes.
    pub fn set_logging(&mut self, settings: LoggingSettings) -> Result<bool, ApiError> {
        let settings = settings.normalized();
        if settings == self.config.logging {
            return Ok(false);
        }
        self.commit_config(|c| c.logging = settings.clone())?;
        self.diagnostics.apply(&settings);
        Ok(true)
    }

    /// The saved switches changed elsewhere (a Java import).
    pub(super) fn logging_replaced(&mut self) {
        self.diagnostics.apply(&self.config.logging);
    }

    /// 完整日志 on or off. Turning it on starts a session for `scopes` (none chosen means all,
    /// like the dialog), saving them as the preference first; without `scopes` the saved
    /// preference is used. A running session is left as it is: its scopes change only after it
    /// stops. Off ends the session (停止完整日志).
    pub fn set_full_trace(&mut self, enabled: bool, scopes: Option<Vec<TraceScope>>) -> Result<bool, ApiError> {
        if !enabled {
            let stopped = self.diagnostics.stop_full_trace();
            if stopped {
                self.log(false, "full trace stopped".into());
            }
            return Ok(stopped);
        }
        if !self.diagnostics.full_trace().is_empty() {
            return Ok(false);
        }
        let scopes = match scopes {
            Some(chosen) => {
                let chosen = if chosen.is_empty() { TraceScope::ALL.to_vec() } else { chosen };
                let next = LoggingSettings { preferred_full_trace_scopes: chosen, ..self.config.logging.clone() };
                self.set_logging(next)?;
                self.config.logging.preferred_full_trace_scopes.clone()
            }
            None => self.config.logging.preferred_full_trace_scopes.clone(),
        };
        self.diagnostics
            .start_full_trace(&scopes)
            .map_err(|e| ApiError::new(ErrorKind::Io, format!("full trace: {e}")))?;
        let names: Vec<&str> = self.diagnostics.full_trace().into_iter().map(TraceScope::wire_name).collect();
        self.log(false, format!("full trace started ({})", names.join(", ")));
        Ok(true)
    }
}
