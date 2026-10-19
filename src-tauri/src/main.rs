// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// WebKitGTK's DMA-BUF renderer flickers on animated layers with hybrid graphics (e.g. Intel + AMD)
/// and the NVIDIA driver. Single-GPU machines keep the faster default.
#[cfg(target_os = "linux")]
fn needs_dmabuf_workaround() -> bool {
    let gpus = std::fs::read_dir("/sys/class/drm")
        .map(|dir| {
            dir.flatten()
                .filter(|e| {
                    let name = e.file_name();
                    let name = name.to_string_lossy();
                    // card0, card1… (connectors look like card0-HDMI-A-1).
                    name.starts_with("card") && !name.contains('-')
                })
                .count()
        })
        .unwrap_or(0);
    gpus > 1 || std::path::Path::new("/sys/module/nvidia").exists()
}

fn main() {
    // Must be set before the webview starts; an explicit value always wins.
    #[cfg(target_os = "linux")]
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() && needs_dmabuf_workaround() {
        // SAFETY: single-threaded at this point, before any webview or runtime threads exist.
        unsafe { std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1") };
    }
    nbsp_lib::run()
}
