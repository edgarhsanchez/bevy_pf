//! Small cross-platform utilities.

/// Open a URL in the default browser (used by `Hyperlink` and app code).
pub fn open_url(url: &str) {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(window) = web_sys::window() {
            let _ = window.open_with_url_and_target(url, "_blank");
        }
    }
    #[cfg(all(not(target_arch = "wasm32"), target_os = "macos"))]
    {
        let _ = std::process::Command::new("open").arg(url).spawn();
    }
    #[cfg(all(not(target_arch = "wasm32"), target_os = "linux"))]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
    #[cfg(all(not(target_arch = "wasm32"), target_os = "windows"))]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", url])
            .spawn();
    }
    #[cfg(all(
        not(target_arch = "wasm32"),
        not(any(target_os = "macos", target_os = "linux", target_os = "windows"))
    ))]
    {
        bevy::log::warn!("bevy_pf: cannot open URL on this platform: {url}");
    }
}
