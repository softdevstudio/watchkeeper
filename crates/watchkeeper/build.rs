fn main() {
    // App commands get generated allow-/deny- permissions, so each one must
    // be granted explicitly in capabilities/. The page webview is granted
    // none of them.
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "navigate",
            "go_back",
            "go_forward",
            "reload",
            "snapshot",
            "forget_site",
            "forget_all",
        ]),
    ))
    .expect("tauri build step failed");
}
