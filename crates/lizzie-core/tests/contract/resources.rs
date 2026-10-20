//! Contract types of engine resources (see `../contract.rs`).

use lizzie_core::api::*;
use lizzie_core::resources::ManagedProfile;
use ts_rs::Config;

pub fn decls(cfg: &Config) -> Vec<String> {
    crate::decls![cfg; ManagedProfile, ResourceCommand, ResourceResult, ResourcesView]
}
