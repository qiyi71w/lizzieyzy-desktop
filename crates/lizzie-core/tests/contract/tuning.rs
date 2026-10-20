//! Contract types of engine performance (see `../contract.rs`).

use lizzie_core::api::*;
use lizzie_core::engine::tuning::{GtpConfiguration, ThreadPolicy};
use ts_rs::Config;

pub fn decls(cfg: &Config) -> Vec<String> {
    crate::decls![cfg; ThreadPolicy, GtpConfiguration, TuningCommand, TuningResult, TuningView]
}
