fn main() {
    // The 弈客 page's only bridge command (`plugin:yike-page|report`); granted to the remote
    // 弈客 page by capabilities/yike-page.json. No app manifest: app commands stay local-only.
    let attributes = tauri_build::Attributes::new()
        .plugin("yike-page", tauri_build::InlinedPlugin::new().commands(&["report"]));
    if let Err(error) = tauri_build::try_build(attributes) {
        println!("{error:#}");
        std::process::exit(1);
    }
}
