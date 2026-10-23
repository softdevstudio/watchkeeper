//! Back, forward and reload through each engine's own API.
//!
//! Running `history.back()` inside the page would call whatever the page
//! put there, so a site could trap the Back button. The native calls below
//! cannot be intercepted by page code.

/// Which way to move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Back,
    Forward,
}

/// Moves the page one step through its history, if it can.
pub fn step(view: &tauri::Webview, step: Step) -> Result<(), String> {
    view.with_webview(move |platform| {
        #[cfg(target_os = "linux")]
        {
            use webkit2gtk::WebViewExt;
            let wv = platform.inner();
            match step {
                Step::Back if wv.can_go_back() => wv.go_back(),
                Step::Forward if wv.can_go_forward() => wv.go_forward(),
                _ => {}
            }
        }
        #[cfg(target_os = "windows")]
        unsafe {
            if let Ok(core) = platform.controller().CoreWebView2() {
                let _ = match step {
                    Step::Back => core.GoBack(),
                    Step::Forward => core.GoForward(),
                };
            }
        }
        #[cfg(target_os = "macos")]
        unsafe {
            let ptr = platform.inner() as *const objc2_web_kit::WKWebView;
            if let Some(wv) = ptr.as_ref() {
                let _ = match step {
                    Step::Back => wv.goBack(),
                    Step::Forward => wv.goForward(),
                };
            }
        }
        #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
        let _ = (platform, step);
    })
    .map_err(|e| e.to_string())
}
