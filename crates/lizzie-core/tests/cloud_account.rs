use lizzie_core::cloud::{Catalog, CloudCommand, CloudSettings, DiscoverySource, Provider};
use serde_json::json;

#[test]
fn server_catalog_is_authoritative_and_canonicalizes_only_advertised_weights() {
    let catalog = Catalog::from_server(&json!({
        "serverVersion":"  service\n 1.17  ", "defaultKataWeight":"UNKNOWN",
        "supportKataWeights":["18bnbt",{"name":"custom-model","description":"  server\nweight  "},"bad;command","custom-model"]
    })).unwrap();
    assert_eq!(catalog.server_version,"service 1.17");
    assert_eq!(catalog.weights.len(),2);
    assert_eq!(catalog.default_weight,"18bnbt");
    assert!(catalog.server_authoritative());
    assert_eq!(catalog.resolve_weight("CUSTOM-MODEL"),"custom-model");
    assert_eq!(catalog.resolve_weight("10b512t"),"18bnbt");
    assert_eq!(catalog.clone().with_documented_weights(),catalog);
}

#[test]
fn importing_legacy_catalog_replaces_obsolete_baseline_but_keeps_user_weights() {
    let catalog = Catalog::from_java(&json!({
        "serverVersion":"old", "defaultKataWeight":"60b",
        "supportKataWeights":["60b",{"name":"my-model","description":"kept","source":"OFFICIAL_DOCUMENTED"}]
    })).unwrap();
    assert_eq!(catalog.default_weight,"10b512t");
    assert_eq!(catalog.weights.iter().map(|w|w.name.as_str()).collect::<Vec<_>>(),["20b","10b384t","10b512t","11b768t","my-model"]);
    assert_eq!(catalog.weights.last().unwrap().source,DiscoverySource::CachedLegacy);
    assert!(!catalog.server_authoritative());
}

#[test]
fn cloud_settings_defaults_and_diagnostics_never_contain_account_credentials() {
    let mut settings=CloudSettings::default();
    assert_eq!(settings.provider,Provider::Local);
    assert!(!settings.remember_token && !settings.remember_password);
    assert_eq!(settings.catalog.default_weight,"10b512t");
    settings.custom_url="wss://private-user:private-password@host.invalid/?token=private-token".into();
    settings.args="private-argument".into();
    let debug=format!("{settings:?}");
    assert!(!debug.contains("private-"));
    let command=CloudCommand::PasswordLogin {identifier:"user".into(),password:"secret-password".into(),remember_token:false,remember_password:false};
    assert!(!format!("{command:?}").contains("secret-password"));
    let serialized=serde_json::to_value(CloudSettings::default()).unwrap();
    assert!(serialized.get("password").is_none() && serialized.get("token").is_none());
}

#[test]
fn refreshed_catalog_replaces_retired_draft_weight_without_changing_other_options() {
    let catalog=Catalog::from_server(&json!({"defaultKataWeight":"11b768t","supportKataWeights":["11b768t","10b512t"]})).unwrap();
    assert_eq!(catalog.resolve_args("--gpu-type 1x --kata-name katago-CUDA --kata-weight retired-model --visits 500"),
        "--gpu-type 1x --kata-name katago-CUDA --kata-weight 11b768t --visits 500");
    assert_eq!(catalog.resolve_args("--gpu-type vip-share --kata-weight 10B512T"),
        "--gpu-type vip-share --kata-weight 10b512t");
    assert_eq!(catalog.resolve_args("--gpu-type 1x"),"--gpu-type 1x --kata-weight 10b512t");
    let default_draft=Catalog::from_server(&json!({"defaultKataWeight":"server-new","supportKataWeights":["server-new"]})).unwrap();
    assert_eq!(default_draft.resolve_args(""),"--platform all --engine-type go --gpu-type vip-share --kata-name katago-TENSORRT --kata-weight server-new");
}

#[test]
fn account_results_only_commit_catalog_args_when_the_original_saved_args_are_current() {
    use lizzie_core::engine::gtp::LaunchSpec;
    use lizzie_core::engine::runtime::{EngineIo,InstanceId,Launcher};
    use lizzie_core::settings::Storage;
    use lizzie_core::workbench::Workbench;
    struct NoEngine;
    impl Launcher for NoEngine {
        fn launch(&self,_:InstanceId,_:&LaunchSpec)->std::io::Result<Box<dyn EngineIo>> {
            Err(std::io::Error::other("no engine started by account-settings tests"))
        }
    }
    let temp=tempfile::tempdir().unwrap();
    let storage=Storage::in_dirs(&temp.path().join("config"),&temp.path().join("data"));
    let mut wb=Workbench::new(storage,None,Box::new(NoEngine),Vec::new());
    let original=wb.config().cloud.clone();
    let catalog=Catalog::from_server(&json!({"defaultKataWeight":"server-current","supportKataWeights":["server-current"]})).unwrap();
    let mut refreshed=original.clone();
    refreshed.catalog=catalog;
    refreshed.args=refreshed.catalog.resolve_args("--gpu-type 1x --kata-weight retired");
    wb.cloud_account_settings(&refreshed,None).unwrap();
    assert_eq!(wb.config().cloud.args,original.args,"ordinary account completion cannot overwrite launch args");
    wb.cloud_account_settings(&refreshed,Some(&original.args)).unwrap();
    assert_eq!(wb.config().cloud.args,"--gpu-type 1x --kata-weight server-current");
    let mut later=refreshed.clone();
    later.args="--gpu-type vip-share --kata-weight later-selection".into();
    wb.cloud_account_settings(&later,Some(&refreshed.args)).unwrap();
    refreshed.catalog_updated_at=12345;
    wb.cloud_account_settings(&refreshed,Some(&original.args)).unwrap();
    assert_eq!(wb.config().cloud.args,later.args,"stale refresh cannot undo a later launch selection");
    assert_eq!(wb.config().cloud.catalog_updated_at,12345,"fresh catalog data remains account-owned");
    wb.cloud_account_settings(&original,None).unwrap();
    assert_eq!(wb.config().cloud.catalog,refreshed.catalog,"late ordinary account reply cannot revert the catalog");
    assert_eq!(wb.config().cloud.catalog_updated_at,12345);
    let persisted: lizzie_core::settings::Config = serde_json::from_slice(
        &std::fs::read(&wb.storage().config_file).unwrap()).unwrap();
    assert_eq!(persisted.cloud.catalog,refreshed.catalog);
    assert_eq!(persisted.cloud.catalog_updated_at,12345);
    assert_eq!(wb.config().cloud.provider,original.provider);
}
