//! Contract types of engine performance (see `../contract.rs`).

use lizzie_core::api::*;
use lizzie_core::engine::tuning::{GtpConfiguration, ThreadPolicy, ThreadSource};
use ts_rs::Config;

pub fn decls(cfg: &Config) -> Vec<String> {
    crate::decls![cfg; ThreadSource, ThreadPolicy, GtpConfiguration, TuningCommand, TuningResult, TuningProfileView, BenchmarkTaskView, TuningOutcome, TuningView]
}
