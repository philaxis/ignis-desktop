fn main() {
    // Remote pages can only call commands that are listed here and allowed by a
    // capability, so every command the app registers has to be listed.
    let manifest = tauri_build::AppManifest::new().commands(&[
        "get_settings",
        "save_settings",
        "frame_start_dragging",
        "frame_minimize",
        "frame_toggle_maximize",
        "frame_close",
        "frame_state",
        "frame_snap_rect",
        "popout_frame",
    ]);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))
        .expect("failed to run tauri-build");
}
