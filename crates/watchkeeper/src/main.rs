#![cfg_attr(all(not(debug_assertions), windows), windows_subsystem = "windows")]

//! Watchkeeper: a tiny browser that shows you who's watching.
//!
//! One window holds three webviews: the toolbar across the top, the web
//! page below it, and the side panel on the right. The page runs in a
//! private session that starts empty and is gone when the app closes. The
//! detection script from watchkeeper-core runs in every frame of the page;
//! a background thread collects its events and the engine's cookie list.
//! The page webview has no access to any app command.

#[cfg(target_os = "linux")]
mod linux_layout;
mod selftest;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::webview::{NewWindowResponse, PageLoadEvent, WebviewBuilder};
use tauri::window::WindowBuilder;
#[cfg(not(target_os = "linux"))]
use tauri::WindowEvent;
use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, Url, WebviewUrl};
use watchkeeper_core::{detection_script, drain_script, SiteReport, StoredCookie, Summary};

pub const WINDOW: &str = "main";
pub const TOOLBAR: &str = "toolbar";
pub const PAGE: &str = "page";
pub const PANEL: &str = "panel";

pub const TOOLBAR_HEIGHT: f64 = 48.0;
pub const PANEL_WIDTH: f64 = 320.0;

/// How often the page's buffered events are collected.
const DRAIN_EVERY: Duration = Duration::from_millis(400);
/// Cookies are read every this many collections.
const COOKIES_EVERY: u64 = 3;

/// State shared by commands, the collector thread and the self-test.
pub struct Shared {
    pub report: Mutex<SiteReport>,
    pub token: String,
}

impl Shared {
    fn summary(&self) -> Summary {
        self.report.lock().map(|r| r.summary()).unwrap_or_default()
    }
}

/// A rectangle in logical pixels: x, y, width, height.
pub type Rect = (f64, f64, f64, f64);

/// A measured layout: content width, content height, then each webview's
/// rectangle.
pub type Measured = (f64, f64, Vec<(&'static str, Rect)>);

/// Where the toolbar, page and panel go in a window of the given size.
/// The panel never takes more than 40 percent of the width.
pub fn layout(width: f64, height: f64) -> [(&'static str, Rect); 3] {
    let w = width.max(1.0);
    let h = height.max(1.0);
    let bar = TOOLBAR_HEIGHT.min(h);
    let body = (h - bar).max(1.0);
    let panel = PANEL_WIDTH.min(w * 0.4).floor().max(1.0);
    let page = (w - panel).max(1.0);
    [
        (TOOLBAR, (0.0, 0.0, w, bar)),
        (PAGE, (0.0, bar, page, body)),
        (PANEL, (page, bar, panel, body)),
    ]
}

#[cfg_attr(target_os = "linux", allow(dead_code))]
fn apply_layout(window: &tauri::Window) -> tauri::Result<()> {
    let scale = window.scale_factor()?;
    let size = window.inner_size()?.to_logical::<f64>(scale);
    for (label, (x, y, w, h)) in layout(size.width, size.height) {
        if let Some(view) = window.get_webview(label) {
            view.set_position(LogicalPosition::new(x, y))?;
            view.set_size(LogicalSize::new(w, h))?;
        }
    }
    Ok(())
}

/// Where the toolbar, page and panel really are: the content area's width
/// and height, then each webview's rectangle, all in logical pixels.
pub fn measure_layout(window: &tauri::Window) -> Result<Measured, String> {
    #[cfg(target_os = "linux")]
    {
        linux_layout::measure(window)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let scale = window.scale_factor().map_err(|e| e.to_string())?;
        let inner = window
            .inner_size()
            .map_err(|e| e.to_string())?
            .to_logical::<f64>(scale);
        let mut rects = Vec::new();
        for label in [TOOLBAR, PAGE, PANEL] {
            let view = window
                .get_webview(label)
                .ok_or(format!("no {label} webview"))?;
            let p = view
                .position()
                .map_err(|e| e.to_string())?
                .to_logical::<f64>(scale);
            let s = view
                .size()
                .map_err(|e| e.to_string())?
                .to_logical::<f64>(scale);
            rects.push((label, (p.x, p.y, s.width, s.height)));
        }
        Ok((inner.width, inner.height, rects))
    }
}

/// Turns what was typed in the address bar into a URL. Only http and https
/// are allowed.
pub fn normalize_url(input: &str) -> Result<Url, String> {
    let text = input.trim();
    if text.is_empty() {
        return Err("Type a web address".into());
    }
    let candidate = if text.contains("://") {
        text.to_string()
    } else if !text.contains(char::is_whitespace)
        && (text.contains('.') || text.starts_with("localhost"))
    {
        format!("https://{text}")
    } else {
        return Err("Type a full web address, such as example.com".into());
    };
    let url = Url::parse(&candidate).map_err(|_| "That is not a web address".to_string())?;
    match url.scheme() {
        "http" | "https" if url.host_str().is_some() => Ok(url),
        _ => Err("Only http and https addresses can be opened".into()),
    }
}

fn page(app: &AppHandle) -> Result<tauri::Webview, String> {
    app.get_webview(PAGE)
        .ok_or_else(|| "The page is not ready".to_string())
}

#[tauri::command]
fn navigate(app: AppHandle, url: String) -> Result<(), String> {
    let url = normalize_url(&url)?;
    page(&app)?.navigate(url).map_err(|e| e.to_string())
}

#[tauri::command]
fn go_back(app: AppHandle) -> Result<(), String> {
    page(&app)?
        .eval("history.back()")
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn go_forward(app: AppHandle) -> Result<(), String> {
    page(&app)?
        .eval("history.forward()")
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn reload(app: AppHandle) -> Result<(), String> {
    page(&app)?
        .eval("location.reload()")
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn snapshot(shared: tauri::State<'_, Arc<Shared>>) -> Summary {
    shared.summary()
}

fn random_token() -> String {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("no system randomness");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Starts a fresh report when the page moves to another site.
fn page_started(shared: &Shared, url: &Url) {
    let Ok(mut report) = shared.report.lock() else {
        return;
    };
    let url = url.as_str();
    if report.site.is_empty() || !report.same_site(url) {
        *report = SiteReport::new(url);
    } else {
        report.page_url = url.to_string();
    }
}

/// Collects the page's events and the cookie store in the background.
fn spawn_collector(app: AppHandle, shared: Arc<Shared>) {
    let drain = drain_script(&shared.token).expect("token checked at startup");
    std::thread::spawn(move || {
        let mut tick: u64 = 0;
        loop {
            std::thread::sleep(DRAIN_EVERY);
            let Some(view) = app.get_webview(PAGE) else {
                continue;
            };
            let target = shared.clone();
            let _ = view.eval_with_callback(drain.clone(), move |json| {
                if let Ok(mut report) = target.report.lock() {
                    let _ = report.add_events_json(&json);
                }
            });
            tick += 1;
            if tick.is_multiple_of(COOKIES_EVERY) {
                // Reading cookies must stay off the main thread (WebView2
                // deadlocks otherwise); this thread is fine.
                if let Ok(cookies) = view.cookies() {
                    let list = cookies
                        .iter()
                        .map(|c| StoredCookie {
                            name: c.name().to_string(),
                            domain: c.domain().unwrap_or_default().to_string(),
                            http_only: c.http_only().unwrap_or(false),
                        })
                        .collect();
                    if let Ok(mut report) = shared.report.lock() {
                        report.set_cookies(list);
                    }
                }
            }
        }
    });
}

fn build_window(app: &AppHandle, shared: Arc<Shared>, start: Url) -> tauri::Result<()> {
    let (w, h) = (1200.0, 800.0);
    let window = WindowBuilder::new(app, WINDOW)
        .title("Watchkeeper")
        .inner_size(w, h)
        .min_inner_size(640.0, 400.0)
        .build()?;

    let script = detection_script(&shared.token).expect("token checked at startup");
    let rects = layout(w, h);
    let at = |i: usize| {
        let (_, (x, y, w, h)) = rects[i];
        (LogicalPosition::new(x, y), LogicalSize::new(w, h))
    };

    let (pos, size) = at(0);
    window.add_child(
        WebviewBuilder::new(TOOLBAR, WebviewUrl::App("toolbar.html".into())),
        pos,
        size,
    )?;

    let on_load = shared.clone();
    let opener = app.clone();
    let page_builder = WebviewBuilder::new(PAGE, WebviewUrl::External(start))
        .incognito(true)
        .initialization_script_for_all_frames(script)
        .on_page_load(move |_view, payload| {
            if payload.event() == PageLoadEvent::Started {
                page_started(&on_load, payload.url());
            }
        })
        // Links that ask for a new window open in the page instead, so the
        // detection script is always present.
        .on_new_window(move |url, _features| {
            if matches!(url.scheme(), "http" | "https") {
                if let Some(view) = opener.get_webview(PAGE) {
                    let _ = view.navigate(url);
                }
            }
            NewWindowResponse::Deny
        })
        // Downloads are not supported yet.
        .on_download(|_view, _event| false);
    let (pos, size) = at(1);
    window.add_child(page_builder, pos, size)?;

    let (pos, size) = at(2);
    window.add_child(
        WebviewBuilder::new(PANEL, WebviewUrl::App("panel.html".into())),
        pos,
        size,
    )?;

    #[cfg(target_os = "linux")]
    linux_layout::arrange(&window).map_err(|e| tauri::Error::Io(std::io::Error::other(e)))?;
    #[cfg(not(target_os = "linux"))]
    {
        let resized = window.clone();
        window.on_window_event(move |event| {
            if matches!(
                event,
                WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. }
            ) {
                let _ = apply_layout(&resized);
            }
        });
        apply_layout(&window)?;
    }
    Ok(())
}

fn main() {
    let selftest_dir = selftest::requested_dir();
    let token = random_token();
    detection_script(&token).expect("random token is hex");
    let shared = Arc::new(Shared {
        report: Mutex::new(SiteReport::default()),
        token,
    });

    let state = shared.clone();
    let result = tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            navigate, go_back, go_forward, reload, snapshot
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            let start = match &selftest_dir {
                Some(dir) => selftest::start(&handle, shared.clone(), dir.clone())?,
                None => Url::parse("about:blank").expect("valid"),
            };
            build_window(&handle, shared.clone(), start)?;
            spawn_collector(handle, shared.clone());
            Ok(())
        })
        .run(tauri::generate_context!());
    if let Err(e) = result {
        eprintln!("watchkeeper: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_tiles_the_window() {
        for &(w, h) in &[(1200.0, 800.0), (640.0, 400.0), (3840.0, 2160.0)] {
            let r = layout(w, h);
            let (_, bar) = r[0];
            let (_, page) = r[1];
            let (_, panel) = r[2];
            let (w, h) = (w.max(1.0), h.max(1.0));
            assert_eq!(bar.2, w);
            assert!(page.0 + page.2 <= panel.0 + 0.001, "page and panel overlap");
            assert!(panel.0 + panel.2 <= w + 0.001);
            assert!(panel.2 <= (PANEL_WIDTH).min(w * 0.4).max(1.0));
            assert!(page.1 >= bar.1 + bar.3 - 0.001);
            assert!(page.3 >= 1.0 && panel.3 >= 1.0 && h >= 1.0);
        }
    }

    #[test]
    fn urls() {
        assert_eq!(
            normalize_url("example.com").unwrap().as_str(),
            "https://example.com/"
        );
        assert_eq!(
            normalize_url(" http://a.org/x ").unwrap().as_str(),
            "http://a.org/x"
        );
        assert!(normalize_url("localhost:8080").is_ok());
        assert!(normalize_url("").is_err());
        assert!(normalize_url("two words").is_err());
        assert!(normalize_url("file:///etc/passwd").is_err());
        assert!(normalize_url("javascript://alert(1)").is_err());
        assert!(normalize_url("ftp://x.org").is_err());
    }
}
