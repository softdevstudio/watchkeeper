//! `watchkeeper --selftest [dir]`: runs the real app against two local test
//! sites and checks it end to end, on whichever engine this OS provides.
//!
//! A first-party site on 127.0.0.1 serves a page that pulls in a "tracker"
//! from localhost, a different site. The tracker server logs every request
//! it receives; that log is the ground truth. The test passes only if every
//! request the tracker server saw also shows up in Watchkeeper's report,
//! along with the cookies, storage writes and fingerprinting reads the
//! tracker made. It also checks that the three webviews tile the window,
//! before and after a resize, and that the page cannot call app commands.
//!
//! Results go to `<dir>/selftest.json`; the exit code is 0 only if every
//! check passed.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, LogicalSize, Manager, Url};

use crate::{Shared, PAGE, WINDOW};

const OVERALL_TIMEOUT: Duration = Duration::from_secs(60);

/// `Some(dir)` when the app was started with `--selftest [dir]`.
pub fn requested_dir() -> Option<PathBuf> {
    let args: Vec<String> = std::env::args().collect();
    let i = args.iter().position(|a| a == "--selftest")?;
    Some(
        args.get(i + 1)
            .filter(|a| !a.starts_with("--"))
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("selftest-out")),
    )
}

/// A request a test server received.
#[derive(Debug, Clone, Serialize)]
struct Hit {
    path: String,
    query: String,
}

#[derive(Clone, Copy)]
enum Role {
    Site,
    Tracker,
}

struct Server {
    port: u16,
    hits: Arc<Mutex<Vec<Hit>>>,
}

impl Server {
    fn start(role: Role, other_base: Arc<Mutex<String>>) -> std::io::Result<Server> {
        let v4 = TcpListener::bind("127.0.0.1:0")?;
        let port = v4.local_addr()?.port();
        let hits = Arc::new(Mutex::new(Vec::new()));
        let mut listeners = vec![v4];
        // "localhost" may resolve to ::1 first; listen there too if we can.
        if let Ok(v6) = TcpListener::bind(("::1", port)) {
            listeners.push(v6);
        }
        for listener in listeners {
            let hits = hits.clone();
            let other = other_base.clone();
            std::thread::spawn(move || {
                for stream in listener.incoming().flatten() {
                    let hits = hits.clone();
                    let other = other.clone();
                    std::thread::spawn(move || {
                        let _ = serve(stream, role, &hits, &other);
                    });
                }
            });
        }
        Ok(Server { port, hits })
    }

    fn hits(&self) -> Vec<Hit> {
        self.hits.lock().map(|h| h.clone()).unwrap_or_default()
    }

    fn has(&self, path: &str) -> bool {
        self.hits().iter().any(|h| h.path == path)
    }
}

fn serve(
    stream: TcpStream,
    role: Role,
    hits: &Mutex<Vec<Hit>>,
    other_base: &Mutex<String>,
) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let target = line.split_whitespace().nth(1).unwrap_or("/").to_string();
    let mut length = 0usize;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 || header == "\r\n" || header == "\n" {
            break;
        }
        let lower = header.to_ascii_lowercase();
        if let Some(v) = lower.strip_prefix("content-length:") {
            length = v.trim().parse().unwrap_or(0).min(1 << 20);
        }
    }
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body)?;

    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (target.clone(), String::new()),
    };
    if path != "/favicon.ico" {
        if let Ok(mut h) = hits.lock() {
            h.push(Hit {
                path: path.clone(),
                query,
            });
        }
    }
    let other = other_base.lock().map(|s| s.clone()).unwrap_or_default();
    let (status, kind, extra, content) = respond(role, &path, &other);
    let mut out = stream;
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nCache-Control: no-store\r\nConnection: close\r\n{extra}\r\n",
        content.len()
    );
    out.write_all(head.as_bytes())?;
    out.write_all(&content)?;
    out.flush()
}

const GIF: &[u8] = &[
    0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 1, 0, 1, 0, 0x80, 0, 0, 0, 0, 0, 0xff, 0xff, 0xff, 0x21,
    0xf9, 4, 1, 0, 0, 0, 0, 0x2c, 0, 0, 0, 0, 1, 0, 1, 0, 0, 2, 2, 0x44, 1, 0, 0x3b,
];

fn respond(role: Role, path: &str, other: &str) -> (&'static str, &'static str, String, Vec<u8>) {
    let ok = "200 OK";
    match (role, path) {
        (Role::Site, "/") => (
            ok,
            "text/html; charset=utf-8",
            "Set-Cookie: wk_session=1; Path=/; HttpOnly\r\n".into(),
            site_page(other).into_bytes(),
        ),
        (Role::Tracker, "/tracker.js") => (
            ok,
            "text/javascript",
            String::new(),
            tracker_js().into_bytes(),
        ),
        (Role::Tracker, "/frame.html") => (
            ok,
            "text/html; charset=utf-8",
            String::new(),
            "<!doctype html><title>frame</title><script>fetch('/frame-call',{mode:'no-cors'});</script>"
                .as_bytes()
                .to_vec(),
        ),
        (Role::Tracker, "/style.css") => (ok, "text/css", String::new(), b"h1{font-size:20px}".to_vec()),
        (Role::Tracker, "/pixel.gif") => (
            ok,
            "image/gif",
            "Set-Cookie: wk_track=1; Path=/\r\n".into(),
            GIF.to_vec(),
        ),
        (_, "/favicon.ico") => ("404 Not Found", "text/plain", String::new(), Vec::new()),
        _ => ("204 No Content", "text/plain", String::new(), Vec::new()),
    }
}

fn site_page(tracker: &str) -> String {
    format!(
        r#"<!doctype html>
<html><head><meta charset="utf-8"><title>Watchkeeper self-test</title>
<link rel="stylesheet" href="{tracker}/style.css"></head>
<body><h1>Watchkeeper self-test page</h1>
<p>This page pulls in a tracker from another site.</p>
<script>document.cookie = 'wk_page=1; path=/'; localStorage.direct_key = 'x';</script>
<script src="{tracker}/tracker.js"></script>
<iframe src="{tracker}/frame.html" width="240" height="80"></iframe>
</body></html>"#
    )
}

fn tracker_js() -> String {
    // Relative to the script's own origin, worked out at run time.
    r#"(function () {
  var T = new URL(document.currentScript.src).origin;
  new Image().src = T + '/pixel.gif';
  fetch(T + '/collect?id=1', { mode: 'no-cors' });
  var x = new XMLHttpRequest(); x.open('GET', T + '/xhr'); x.send();
  navigator.sendBeacon(T + '/beacon', 'b');
  document.cookie = 'wk_js=1; path=/';
  localStorage.setItem('wk_ls', '1');
  sessionStorage.setItem('wk_ss', '1');
  var idb = 0; try { indexedDB.open('wk_db'); idb = 1; } catch (e) { }
  var canvas = 0;
  try { var c = document.createElement('canvas'); c.getContext('2d').fillText('wk', 1, 10); c.toDataURL(); canvas = 1; } catch (e) { }
  var webgl = 0;
  try {
    var g = document.createElement('canvas').getContext('webgl');
    if (g) { var ext = g.getExtension('WEBGL_debug_renderer_info'); if (ext) g.getParameter(ext.UNMASKED_VENDOR_WEBGL); webgl = 1; }
  } catch (e) { }
  var audio = 0;
  try { var A = window.OfflineAudioContext || window.webkitOfflineAudioContext; if (A) { new A(1, 44100, 44100); audio = 1; } } catch (e) { }
  fetch(T + '/fp?canvas=' + canvas + '&webgl=' + webgl + '&audio=' + audio + '&idb=' + idb, { mode: 'no-cors' });
  setTimeout(function () { fetch(T + '/late', { mode: 'no-cors' }); }, 1500);
})();
"#
    .to_string()
}

#[derive(Debug, Serialize)]
struct Check {
    name: &'static str,
    pass: bool,
    detail: String,
}

#[derive(Serialize)]
struct Outcome {
    os: &'static str,
    arch: &'static str,
    user_agent: String,
    passed: bool,
    checks: Vec<Check>,
    summary: watchkeeper_core::Summary,
    tracker_hits: Vec<Hit>,
    site_hits: Vec<Hit>,
    detected_requests: Vec<String>,
    cookies: Vec<watchkeeper_core::StoredCookie>,
    seconds: f64,
}

/// Starts the test servers and the checker. Returns the URL the page
/// should open first.
pub fn start(
    app: &AppHandle,
    shared: Arc<Shared>,
    dir: PathBuf,
) -> Result<Url, Box<dyn std::error::Error>> {
    let site_base = Arc::new(Mutex::new(String::new()));
    let tracker_base = Arc::new(Mutex::new(String::new()));
    let site = Server::start(Role::Site, tracker_base.clone())?;
    let tracker = Server::start(Role::Tracker, site_base.clone())?;
    let site_url = format!("http://127.0.0.1:{}", site.port);
    let tracker_url = format!("http://localhost:{}", tracker.port);
    *site_base.lock().unwrap() = site_url.clone();
    *tracker_base.lock().unwrap() = tracker_url.clone();

    let start_url = site_url.clone();
    let handle = app.clone();
    std::thread::spawn(move || {
        let started = Instant::now();
        let outcome = run_checks(&handle, &shared, &site, &tracker, &site_url, started);
        let passed = outcome.passed;
        let _ = std::fs::create_dir_all(&dir);
        let json = serde_json::to_string_pretty(&outcome).unwrap_or_default();
        let _ = std::fs::write(dir.join("selftest.json"), &json);
        for c in &outcome.checks {
            println!(
                "{} {}: {}",
                if c.pass { "PASS" } else { "FAIL" },
                c.name,
                c.detail
            );
        }
        println!(
            "selftest {} in {:.1}s on {} ({})",
            if passed { "passed" } else { "FAILED" },
            outcome.seconds,
            outcome.os,
            outcome.user_agent
        );
        // WATCHKEEPER_SELFTEST_HOLD=<seconds> keeps the window open after
        // the checks, for a screenshot.
        if let Some(secs) = std::env::var("WATCHKEEPER_SELFTEST_HOLD")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
        {
            std::thread::sleep(Duration::from_secs(secs.min(600)));
        }
        handle.exit(if passed { 0 } else { 1 });
    });
    Ok(Url::parse(&format!("{start_url}/"))?)
}

fn wait_until(deadline: Instant, mut f: impl FnMut() -> bool) -> bool {
    while Instant::now() < deadline {
        if f() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    f()
}

/// Runs a script in the page and returns its JSON result.
fn eval_json(app: &AppHandle, label: &str, js: &str) -> Option<String> {
    let view = app.get_webview(label)?;
    let (tx, rx) = std::sync::mpsc::channel();
    view.eval_with_callback(js, move |out| {
        let _ = tx.send(out);
    })
    .ok()?;
    rx.recv_timeout(Duration::from_secs(5)).ok()
}

fn unquote(s: &str) -> String {
    serde_json::from_str::<String>(s).unwrap_or_else(|_| s.to_string())
}

/// Checks that toolbar, page and panel tile the window's content area.
fn check_layout(app: &AppHandle, name: &'static str) -> Check {
    let result = (|| -> Result<String, String> {
        let window = app.get_window(WINDOW).ok_or("no window")?;
        let (w, h, rects) = crate::measure_layout(&window)?;
        if rects.len() != 3 {
            return Err(format!("expected 3 webviews, found {}", rects.len()));
        }
        // Two logical pixels of slack for rounding at fractional scales.
        let near = |a: f64, b: f64| (a - b).abs() <= 2.0;
        let (_, bar) = rects[0];
        let (_, page) = rects[1];
        let (_, panel) = rects[2];
        let mut problems = Vec::new();
        if !(near(bar.0, 0.0) && near(bar.1, 0.0) && near(bar.2, w)) {
            problems.push("toolbar does not span the top");
        }
        if !near(bar.3, crate::TOOLBAR_HEIGHT) {
            problems.push("toolbar height is wrong");
        }
        if !(near(page.0, 0.0) && near(page.1, bar.1 + bar.3)) {
            problems.push("page is not below the toolbar on the left");
        }
        if !(near(page.0 + page.2, panel.0) && near(panel.1, page.1)) {
            problems.push("page and panel do not meet");
        }
        if !near(panel.0 + panel.2, w) {
            problems.push("panel does not reach the right edge");
        }
        if !(near(page.1 + page.3, h) && near(panel.1 + panel.3, h)) {
            problems.push("page or panel does not reach the bottom");
        }
        if panel.2 < 50.0 || page.2 < 50.0 || page.3 < 50.0 {
            problems.push("page or panel is too small");
        }
        let desc = format!(
            "content {w}x{h}; {}",
            rects
                .iter()
                .map(|(l, (x, y, w, h))| format!("{l} {w}x{h} at ({x},{y})"))
                .collect::<Vec<_>>()
                .join(", ")
        );
        if problems.is_empty() {
            Ok(desc)
        } else {
            Err(format!("{}: {desc}", problems.join("; ")))
        }
    })();
    let pass = result.is_ok();
    Check {
        name,
        pass,
        detail: result.unwrap_or_else(|e| e),
    }
}

fn run_checks(
    app: &AppHandle,
    shared: &Shared,
    site: &Server,
    tracker: &Server,
    site_url: &str,
    started: Instant,
) -> Outcome {
    let deadline = started + OVERALL_TIMEOUT;
    let mut checks = Vec::new();

    // The page has loaded and the tracker's delayed call has gone out.
    let loaded = wait_until(deadline, || {
        tracker.has("/late") && tracker.has("/frame-call")
    });
    // Give the collector two more rounds, and the cookie reader one.
    std::thread::sleep(Duration::from_millis(1600));
    checks.push(Check {
        name: "page loaded",
        pass: loaded,
        detail: format!("tracker received {} requests", tracker.hits().len()),
    });

    let user_agent = eval_json(app, PAGE, "navigator.userAgent")
        .map(|s| unquote(&s))
        .unwrap_or_default();

    checks.push(check_layout(app, "layout at start"));

    let report = shared.report.lock().map(|r| r.clone()).unwrap_or_default();
    let summary = report.summary();
    let tracker_host = format!("localhost:{}", tracker.port);

    // Every request the tracker server received was seen by Watchkeeper.
    let seen_paths: Vec<String> = report
        .requests
        .keys()
        .filter_map(|u| Url::parse(u).ok())
        .filter(|u| {
            format!("{}:{}", u.host_str().unwrap_or(""), u.port().unwrap_or(0)) == tracker_host
        })
        .map(|u| u.path().to_string())
        .collect();
    let mut missed: Vec<String> = tracker
        .hits()
        .iter()
        .map(|h| h.path.clone())
        .filter(|p| !seen_paths.contains(p))
        .collect();
    missed.sort();
    missed.dedup();
    checks.push(Check {
        name: "every tracker request seen",
        pass: loaded && missed.is_empty(),
        detail: if missed.is_empty() {
            format!(
                "{} distinct tracker URLs seen: {}",
                seen_paths.len(),
                seen_paths.join(" ")
            )
        } else {
            format!("missed: {}", missed.join(" "))
        },
    });

    checks.push(Check {
        name: "sites told apart",
        pass: summary.site == "127.0.0.1" && summary.third_party_sites == vec!["localhost"],
        detail: format!(
            "site {} contacted {:?}",
            summary.site, summary.third_party_sites
        ),
    });

    let want_cookies = ["wk_page", "wk_js"];
    let missing: Vec<&str> = want_cookies
        .iter()
        .copied()
        .filter(|c| !report.script_cookies.contains(*c))
        .collect();
    checks.push(Check {
        name: "script cookies seen",
        pass: missing.is_empty(),
        detail: format!(
            "seen {:?}{}",
            report.script_cookies,
            if missing.is_empty() {
                String::new()
            } else {
                format!(", missing {missing:?}")
            }
        ),
    });

    let fp_query = tracker
        .hits()
        .iter()
        .find(|h| h.path == "/fp")
        .map(|h| h.query.clone())
        .unwrap_or_default();
    let flag = |k: &str| fp_query.split('&').any(|p| p == format!("{k}=1"));

    let mut want_store = vec![
        ("localStorage", "wk_ls"),
        ("localStorage", "direct_key"),
        ("sessionStorage", "wk_ss"),
    ];
    if flag("idb") {
        want_store.push(("indexedDB", "wk_db"));
    }
    let missing: Vec<String> = want_store
        .iter()
        .filter(|(k, n)| !report.storage.contains(&(k.to_string(), n.to_string())))
        .map(|(k, n)| format!("{k}:{n}"))
        .collect();
    checks.push(Check {
        name: "storage writes seen",
        pass: missing.is_empty(),
        detail: format!(
            "seen {:?}{}",
            report.storage,
            if missing.is_empty() {
                String::new()
            } else {
                format!(", missing {missing:?}")
            }
        ),
    });

    let mut want_fp = Vec::new();
    for api in ["canvas", "webgl", "audio"] {
        if flag(api) {
            want_fp.push(api);
        }
    }
    let missing: Vec<&str> = want_fp
        .iter()
        .copied()
        .filter(|a| !report.fingerprinting.contains(*a))
        .collect();
    checks.push(Check {
        name: "fingerprinting reads seen",
        pass: !fp_query.is_empty() && missing.is_empty(),
        detail: format!(
            "tracker reported {fp_query:?}; seen {:?}{}",
            report.fingerprinting,
            if missing.is_empty() {
                String::new()
            } else {
                format!(", missing {missing:?}")
            }
        ),
    });

    let httponly = report
        .cookies
        .iter()
        .any(|c| c.name == "wk_session" && c.http_only);
    checks.push(Check {
        name: "server cookie read from the engine",
        pass: httponly,
        detail: format!(
            "{} cookies in the store: {}",
            report.cookies.len(),
            report
                .cookies
                .iter()
                .map(|c| format!(
                    "{}@{}{}",
                    c.name,
                    c.domain,
                    if c.http_only { " (HttpOnly)" } else { "" }
                ))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    });

    // The page must not be able to call app commands.
    let probe = format!(
        "(function(){{var i=window.__TAURI_INTERNALS__;if(!i||!i.invoke){{fetch('{site_url}/ipc?absent=1');return;}}\
         i.invoke('snapshot').then(function(){{fetch('{site_url}/ipc?allowed=1');}},function(){{fetch('{site_url}/ipc?denied=1');}});}})()"
    );
    if let Some(v) = app.get_webview(PAGE) {
        let _ = v.eval(probe);
    }
    let answered = wait_until(Instant::now() + Duration::from_secs(10), || {
        site.has("/ipc")
    });
    let ipc = site
        .hits()
        .iter()
        .find(|h| h.path == "/ipc")
        .map(|h| h.query.clone())
        .unwrap_or_default();
    checks.push(Check {
        name: "page cannot call app commands",
        pass: answered && ipc != "allowed=1",
        detail: match ipc.as_str() {
            "denied=1" => "IPC bridge present, snapshot command refused".into(),
            "absent=1" => "no IPC bridge in the page".into(),
            "allowed=1" => "the page called the snapshot command".into(),
            _ => "probe did not answer".into(),
        },
    });

    // Resize the window and check the layout follows.
    if let Some(w) = app.get_window(WINDOW) {
        let _ = w.set_size(LogicalSize::new(900.0, 600.0));
    }
    std::thread::sleep(Duration::from_millis(1500));
    checks.push(check_layout(app, "layout after resize"));

    let passed = checks.iter().all(|c| c.pass);
    let report = shared.report.lock().map(|r| r.clone()).unwrap_or_default();
    Outcome {
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        user_agent,
        passed,
        checks,
        summary: report.summary(),
        tracker_hits: tracker.hits(),
        site_hits: site.hits(),
        detected_requests: report.requests.keys().cloned().collect(),
        cookies: report.cookies.clone(),
        seconds: started.elapsed().as_secs_f64(),
    }
}
