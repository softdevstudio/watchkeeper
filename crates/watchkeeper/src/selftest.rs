//! `watchkeeper --selftest [dir]`: runs the real app against local test
//! sites and checks it end to end, on whichever engine this OS provides.
//!
//! A first-party site on 127.0.0.1 serves pages that pull in a "tracker"
//! from localhost, a different site. The test servers log every request
//! they receive; those logs are the ground truth.
//!
//! 1. An ordinary page: every request the tracker server saw must be in
//!    Watchkeeper's report (except requests made from inside a worker,
//!    which Watchkeeper says it cannot see), along with the cookies,
//!    storage writes (including a cross-site frame's), fingerprinting
//!    reads, WebSockets and the worker start. Wrapped classes must still
//!    behave like the originals. The layout must tile the window and the
//!    page must not be able to call app commands.
//! 2. A hostile page: it looks for the collector and tries to drain it,
//!    replaces built-in functions the detector could depend on, floods
//!    cookies and requests, then loads a tracker. The tracker must still be
//!    seen, nothing may be drained, and the flood must show as dropped.
//! 3. The Back button: the hostile page replaced history.back; the app's
//!    Back must still go back without calling it.
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

use crate::{history, Shared, PAGE, WINDOW};

const OVERALL_TIMEOUT: Duration = Duration::from_secs(120);

/// Requests made from inside a worker. Watchkeeper reports the worker
/// starting, not its traffic; the README says so.
const KNOWN_UNSEEN: &[&str] = &["/worker-call"];

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
    /// Fetch metadata and referrer, to tell what made the request.
    how: String,
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

    fn query(&self, path: &str) -> String {
        self.hits()
            .iter()
            .find(|h| h.path == path)
            .map(|h| h.query.clone())
            .unwrap_or_default()
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
    let mut how = Vec::new();
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 || header == "\r\n" || header == "\n" {
            break;
        }
        let lower = header.to_ascii_lowercase();
        if let Some(v) = lower.strip_prefix("content-length:") {
            length = v.trim().parse().unwrap_or(0).min(1 << 20);
        }
        for name in [
            "sec-fetch-dest",
            "sec-fetch-mode",
            "sec-purpose",
            "purpose",
            "referer",
            "upgrade",
        ] {
            if let Some(v) = lower.strip_prefix(&format!("{name}:")) {
                how.push(format!("{name}={}", v.trim()));
            }
        }
    }
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body)?;

    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (target.clone(), String::new()),
    };
    // The hostile page's own flood is not part of the ground truth.
    if path != "/favicon.ico" && !path.starts_with("/flood") {
        if let Ok(mut h) = hits.lock() {
            h.push(Hit {
                path: path.clone(),
                query,
                how: how.join(" "),
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

type Response = (&'static str, &'static str, String, Vec<u8>);

fn html(body: String) -> Response {
    (
        "200 OK",
        "text/html; charset=utf-8",
        String::new(),
        body.into_bytes(),
    )
}

fn respond(role: Role, path: &str, other: &str) -> Response {
    let ok = "200 OK";
    match (role, path) {
        (Role::Site, "/") => {
            let (s, k, _, b) = html(site_page(other));
            (
                s,
                k,
                "Set-Cookie: wk_session=1; Path=/; HttpOnly\r\n".into(),
                b,
            )
        }
        (Role::Site, "/hostile") => html(hostile_page(other)),
        (Role::Tracker, "/tracker.js") => (
            ok,
            "text/javascript",
            String::new(),
            TRACKER_JS.as_bytes().to_vec(),
        ),
        (Role::Tracker, "/h-tracker.js") => (
            ok,
            "text/javascript",
            String::new(),
            HOSTILE_TRACKER_JS.as_bytes().to_vec(),
        ),
        (Role::Tracker, "/frame.html") => html(
            "<!doctype html><title>frame</title><script>\
             localStorage.tp_direct = '1'; localStorage.setItem('tp_set', '1');\
             fetch('/frame-call', { mode: 'no-cors' });</script>"
                .into(),
        ),
        (Role::Tracker, "/style.css") => (
            ok,
            "text/css",
            String::new(),
            b"h1{font-size:20px}".to_vec(),
        ),
        (Role::Tracker, "/pixel.gif") => (
            ok,
            "image/gif",
            "Set-Cookie: wk_track=1; Path=/\r\n".into(),
            GIF.to_vec(),
        ),
        (Role::Tracker, "/h-pixel.gif") => (ok, "image/gif", String::new(), GIF.to_vec()),
        (_, "/favicon.ico") => ("404 Not Found", "text/plain", String::new(), Vec::new()),
        _ => ("204 No Content", "text/plain", String::new(), Vec::new()),
    }
}

fn site_page(tracker: &str) -> String {
    format!(
        r#"<!doctype html>
<html><head><meta charset="utf-8"><title>Watchkeeper self-test page</title>
<link rel="stylesheet" href="{tracker}/style.css"></head>
<body><h1>Watchkeeper self-test page</h1>
<p>This page pulls in a tracker from another site.</p>
<script>document.cookie = 'wk_page=1; path=/'; localStorage.direct_key = 'x';</script>
<script src="{tracker}/tracker.js"></script>
<iframe src="{tracker}/frame.html" width="240" height="80"></iframe>
</body></html>"#
    )
}

const TRACKER_JS: &str = r#"(function () {
  var T = new URL(document.currentScript.src).origin;
  var WS = T.replace('http', 'ws');
  new Image().src = T + '/pixel.gif';
  fetch(T + '/collect?id=1', { mode: 'no-cors' });
  var x = new XMLHttpRequest(); x.open('GET', T + '/xhr'); x.send();
  navigator.sendBeacon(T + '/beacon', 'b');
  try { new WebSocket(WS + '/ws'); } catch (e) { }
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

  // Wrapped classes and functions must still behave like the originals.
  var sub = 0, nonew = 'none', aud = 0;
  try {
    class S extends WebSocket { m() { return 7; } }
    var s = new S(WS + '/ws-sub');
    sub = (s instanceof S && s instanceof WebSocket && s.m() === 7) ? 1 : 0;
  } catch (e) { sub = 0; }
  try { WebSocket(WS + '/ws-nonew'); nonew = 'allowed'; } catch (e) { nonew = 'threw'; }
  try {
    var AC = window.OfflineAudioContext || window.webkitOfflineAudioContext;
    class B extends AC { n() { return 9; } }
    aud = new B(1, 44100, 44100).n() === 9 ? 1 : 0;
  } catch (e) { aud = 0; }
  fetch(T + '/wrap?sub=' + sub + '&nonew=' + nonew + '&aud=' + aud + '&fetchname=' + fetch.name, { mode: 'no-cors' });

  // A worker. Its own request is out of Watchkeeper's sight.
  try {
    var blob = new Blob(["fetch('" + T + "/worker-call', { mode: 'no-cors' });"], { type: 'text/javascript' });
    new Worker(URL.createObjectURL(blob));
  } catch (e) { }
  setTimeout(function () { fetch(T + '/late', { mode: 'no-cors' }); }, 1500);
})();
"#;

fn hostile_page(tracker: &str) -> String {
    format!(
        r#"<!doctype html>
<html><head><meta charset="utf-8"><title>Hostile page</title></head>
<body><h1>Hostile page</h1>
<script>
(function () {{
  // Look for the collector and try to drain it without the secret.
  var found = 0, drained = 0;
  Object.getOwnPropertyNames(window).forEach(function (n) {{
    if (n.indexOf('__wk_') !== 0) return;
    found++;
    var c = window[n];
    [undefined, '', 'guess', n.slice(5)].forEach(function (s) {{
      try {{ var r = c && c.d && c.d(s, '', 0); if (r) drained++; }} catch (e) {{ }}
      try {{ var r2 = c && c.d && c.d(s, 'doc', 999999); if (r2) drained++; }} catch (e) {{ }}
    }});
  }});
  fetch('/h-probe?found=' + found + '&drained=' + drained);
  var t0 = Date.now();

  // Trap the Back button.
  history.back = function () {{ new Image().src = '/trap'; }};
  history.go = function () {{ new Image().src = '/trap'; }};

  // Floods. Real engines write cookies slowly (WebKitGTK takes tens of
  // milliseconds each once a few hundred exist), so the cookie flood is
  // small; the address flood is large enough to overflow the buffer.
  for (var i = 0; i < 300; i++) document.cookie = 'c' + i + '=1';
  for (var j = 0; j < 8000; j++) {{ var x = new XMLHttpRequest(); x.open('GET', '/flood' + j); }}
  fetch('/h-flooded?ms=' + (Date.now() - t0));

  // Replace everything the detector might lean on.
  JSON.stringify = function () {{ return '[]'; }};
  JSON.parse = function () {{ return []; }};
  Object.prototype.toJSON = function () {{ return 1; }};
  window.URL = function () {{ throw new Error('no'); }};
  Array.prototype.push = function () {{ return 0; }};
  Object.defineProperty(Array.prototype, '0', {{ set: function () {{ }}, configurable: true }});
  String.prototype.indexOf = function () {{ return -1; }};
  String.prototype.slice = function () {{ return ''; }};
  String.prototype.charCodeAt = function () {{ return 0; }};
  Reflect.apply = function () {{ }};
  Reflect.construct = function () {{ return {{}}; }};
  Object.prototype.apply = function () {{ }};
  Object.prototype.construct = function () {{ }};

  var s = document.createElement('script');
  s.src = '{tracker}/h-tracker.js';
  document.head.appendChild(s);
}})();
</script>
</body></html>"#
    )
}

// Loaded by the hostile page after it has broken the built-ins above, so it
// avoids URL and the replaced String methods itself.
const HOSTILE_TRACKER_JS: &str = r#"(function () {
  var T = document.currentScript.src.split('/h-tracker.js')[0];
  var WS = 'ws' + T.substring(4);
  new Image().src = T + '/h-pixel.gif';
  fetch(T + '/h-collect', { mode: 'no-cors' });
  navigator.sendBeacon(T + '/h-beacon', 'b');
  try { new WebSocket(WS + '/h-ws'); } catch (e) { }
  setTimeout(function () { fetch(T + '/h-done', { mode: 'no-cors' }); }, 300);
})();
"#;

#[derive(Debug, Serialize)]
struct Check {
    name: &'static str,
    pass: bool,
    detail: String,
}

fn check(name: &'static str, pass: bool, detail: String) -> Check {
    Check { name, pass, detail }
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

fn report(shared: &Shared) -> watchkeeper_core::SiteReport {
    shared
        .session
        .lock()
        .map(|s| s.report.clone())
        .unwrap_or_default()
}

/// Paths on the tracker host that the report contains.
fn seen_tracker_paths(r: &watchkeeper_core::SiteReport, tracker: &Server) -> Vec<String> {
    let host = format!("localhost:{}", tracker.port);
    let mut paths: Vec<String> = r
        .requests
        .keys()
        .filter_map(|u| Url::parse(u).ok())
        .filter(|u| format!("{}:{}", u.host_str().unwrap_or(""), u.port().unwrap_or(0)) == host)
        .map(|u| u.path().to_string())
        .collect();
    paths.sort();
    paths.dedup();
    paths
}

fn flag(q: &str, k: &str) -> bool {
    q.split('&').any(|p| p == k)
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

    // ---- 1. An ordinary page ---------------------------------------------
    let loaded = wait_until(deadline, || {
        tracker.has("/late") && tracker.has("/frame-call") && tracker.has("/wrap")
    });
    // Two more collection rounds, a cookie read and a frame storage sweep.
    std::thread::sleep(Duration::from_millis(2000));
    checks.push(check(
        "page loaded",
        loaded,
        format!("tracker received {} requests", tracker.hits().len()),
    ));

    let user_agent = eval_json(app, PAGE, "navigator.userAgent")
        .map(|s| unquote(&s))
        .unwrap_or_default();

    checks.push(check_layout(app, "layout at start"));

    let r = report(shared);
    let summary = r.summary();
    let seen = seen_tracker_paths(&r, tracker);
    // Engines that mark page requests with Sec-Fetch-Mode mark every one.
    // An unmarked request on such an engine was made by the engine itself
    // (Edge's WebView2 sends one to the site root), not by the page; it is
    // listed below, not counted as a miss.
    let marks = tracker
        .hits()
        .iter()
        .any(|h| h.how.contains("sec-fetch-mode="));
    let engine_own: Vec<String> = tracker
        .hits()
        .iter()
        .filter(|h| marks && !h.how.contains("sec-fetch-mode=") && !h.how.contains("upgrade="))
        .map(|h| h.path.clone())
        .collect();
    let mut missed: Vec<String> = tracker
        .hits()
        .iter()
        .map(|h| h.path.clone())
        .filter(|p| {
            !seen.contains(p) && !KNOWN_UNSEEN.contains(&p.as_str()) && !engine_own.contains(p)
        })
        .collect();
    missed.sort();
    missed.dedup();
    checks.push(check(
        "every tracker request seen",
        loaded && missed.is_empty(),
        if missed.is_empty() {
            format!(
                "{} distinct tracker addresses seen: {}; known out of sight: {}; made by the engine itself: {}",
                seen.len(),
                seen.join(" "),
                KNOWN_UNSEEN.join(" "),
                if engine_own.is_empty() { "none".to_string() } else { engine_own.join(" ") }
            )
        } else {
            let detail: Vec<String> = tracker
                .hits()
                .iter()
                .filter(|h| missed.contains(&h.path))
                .map(|h| format!("{} [{}]", h.path, h.how))
                .collect();
            format!("missed: {}", detail.join("; "))
        },
    ));

    checks.push(check(
        "sites told apart",
        summary.site == "127.0.0.1" && summary.third_party_sites == vec!["localhost"],
        format!(
            "site {} contacted {:?}",
            summary.site, summary.third_party_sites
        ),
    ));

    let has = |p: &str| seen.iter().any(|s| s == p);
    checks.push(check(
        "WebSockets seen",
        has("/ws") && has("/ws-sub"),
        format!(
            "tracker server saw {:?}; report has /ws {} and /ws-sub {}",
            tracker
                .hits()
                .iter()
                .filter(|h| h.path.starts_with("/ws"))
                .map(|h| h.path.clone())
                .collect::<Vec<_>>(),
            has("/ws"),
            has("/ws-sub")
        ),
    ));

    let wrap = tracker.query("/wrap");
    checks.push(check(
        "wrapped classes behave like the originals",
        flag(&wrap, "sub=1")
            && flag(&wrap, "nonew=threw")
            && flag(&wrap, "aud=1")
            && flag(&wrap, "fetchname=fetch"),
        format!("page reported {wrap:?}"),
    ));

    let workers = summary.workers;
    checks.push(check(
        "worker start seen",
        workers >= 1,
        format!("{workers} worker(s) reported"),
    ));

    let missing: Vec<&str> = ["wk_page", "wk_js"]
        .into_iter()
        .filter(|c| !r.has_script_cookie(c))
        .collect();
    checks.push(check(
        "script cookies seen",
        missing.is_empty(),
        format!("seen {:?}, missing {missing:?}", r.script_cookies),
    ));

    let fp_query = tracker.query("/fp");
    let mut want_store = vec![
        ("localStorage", "wk_ls"),
        ("localStorage", "direct_key"),
        ("sessionStorage", "wk_ss"),
        ("localStorage", "tp_set"),
        ("localStorage", "tp_direct"),
    ];
    if flag(&fp_query, "idb=1") {
        want_store.push(("indexedDB", "wk_db"));
    }
    let missing: Vec<String> = want_store
        .iter()
        .filter(|(k, n)| !r.has_storage(k, n))
        .map(|(k, n)| format!("{k}:{n}"))
        .collect();
    let frame_site_ok = r
        .storage
        .iter()
        .any(|(s, _, n)| s == "localhost" && n == "tp_direct");
    checks.push(check(
        "storage writes seen, with the writing site",
        missing.is_empty() && frame_site_ok,
        format!(
            "seen {:?}, missing {missing:?}; frame write credited to localhost: {frame_site_ok}",
            r.storage
        ),
    ));

    let want_fp: Vec<&str> = ["canvas", "webgl", "audio"]
        .into_iter()
        .filter(|a| flag(&fp_query, &format!("{a}=1")))
        .collect();
    let missing: Vec<&str> = want_fp
        .iter()
        .copied()
        .filter(|a| !r.fingerprinting.contains(*a))
        .collect();
    checks.push(check(
        "fingerprinting reads seen",
        !fp_query.is_empty() && missing.is_empty(),
        format!(
            "tracker reported {fp_query:?}; seen {:?}, missing {missing:?}",
            r.fingerprinting
        ),
    ));

    let httponly = r
        .cookies
        .iter()
        .any(|c| c.name == "wk_session" && c.http_only);
    checks.push(check(
        "server cookie read from the engine",
        httponly,
        format!(
            "{} cookies in the store: {}",
            r.cookies.len(),
            r.cookies
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
    ));

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
    let ipc = site.query("/ipc");
    checks.push(check(
        "page cannot call app commands",
        answered && ipc != "allowed=1",
        match ipc.as_str() {
            "denied=1" => "IPC bridge present, snapshot command refused".into(),
            "absent=1" => "no IPC bridge in the page".into(),
            "allowed=1" => "the page called the snapshot command".into(),
            _ => "probe did not answer".into(),
        },
    ));

    // Resize the window and check the layout follows.
    if let Some(w) = app.get_window(WINDOW) {
        let _ = w.set_size(LogicalSize::new(900.0, 600.0));
    }
    std::thread::sleep(Duration::from_millis(1500));
    checks.push(check_layout(app, "layout after resize"));

    // ---- 2. A hostile page ---------------------------------------------------
    let dropped_before = report(shared).dropped;
    if let Some(v) = app.get_webview(PAGE) {
        let _ = v.navigate(Url::parse(&format!("{site_url}/hostile")).expect("valid"));
    }
    let done = wait_until(Instant::now() + Duration::from_secs(40), || {
        tracker.has("/h-done")
    });
    std::thread::sleep(Duration::from_millis(2500));
    let r = report(shared);
    let seen = seen_tracker_paths(&r, tracker);
    let want = [
        "/h-tracker.js",
        "/h-pixel.gif",
        "/h-collect",
        "/h-beacon",
        "/h-ws",
    ];
    let missing: Vec<&str> = want
        .into_iter()
        .filter(|p| !seen.iter().any(|s| s == p))
        .collect();
    checks.push(check(
        "hostile page cannot blind the detector",
        done && missing.is_empty(),
        if missing.is_empty() {
            format!(
                "after the page replaced built-ins and flooded, still seen: {}",
                want.join(" ")
            )
        } else {
            format!(
                "missed {missing:?}; tracker saw {:?}",
                tracker
                    .hits()
                    .iter()
                    .map(|h| h.path.clone())
                    .collect::<Vec<_>>()
            )
        },
    ));

    let probe = site.query("/h-probe");
    checks.push(check(
        "hostile page cannot drain the collector",
        site.has("/h-probe") && flag(&probe, "drained=0"),
        format!("page reported {probe:?} (found = collectors it could name)"),
    ));

    let dropped = r.dropped.saturating_sub(dropped_before);
    checks.push(check(
        "floods are reported, not hidden",
        dropped > 0 && r.summary().dropped > 0,
        format!(
            "{dropped} events from the flood reported as not counted; {} distinct addresses kept",
            r.requests.len()
        ),
    ));

    // ---- 3. Back ignores the page's history.back -----------------------------
    let before = shared.address.lock().map(|a| a.clone()).unwrap_or_default();
    let back = match app.get_webview(PAGE) {
        Some(v) => history::step(&v, history::Step::Back),
        None => Err("no page".into()),
    };
    let returned = wait_until(Instant::now() + Duration::from_secs(10), || {
        shared
            .address
            .lock()
            .map(|a| a.trim_end_matches('/') == site_url)
            .unwrap_or(false)
    });
    std::thread::sleep(Duration::from_millis(500));
    let trapped = site.has("/trap");
    let after = shared.address.lock().map(|a| a.clone()).unwrap_or_default();
    checks.push(check(
        "Back cannot be trapped by the page",
        back.is_ok() && returned && !trapped,
        format!(
            "from {before} back to {after}; page's history.back called: {trapped}{}",
            back.err()
                .map(|e| format!("; error {e}"))
                .unwrap_or_default()
        ),
    ));

    let passed = checks.iter().all(|c| c.pass);
    let r = report(shared);
    Outcome {
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        user_agent,
        passed,
        checks,
        summary: r.summary(),
        tracker_hits: tracker.hits(),
        site_hits: site.hits(),
        detected_requests: r.requests.keys().cloned().collect(),
        cookies: r.cookies.clone(),
        seconds: started.elapsed().as_secs_f64(),
    }
}
