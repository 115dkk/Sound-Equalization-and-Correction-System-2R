#![forbid(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

fn main() {
    const COMMANDS: &[&str] = &[
        "host_status",
        "host_log",
        "host_install",
        "host_uninstall",
        "host_stop_managing",
        "open_app",
        "tray_labels",
    ];
    let attributes = tauri_build::Attributes::new()
        .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS));
    tauri_build::try_build(attributes).expect("failed to build the host shell");
}
