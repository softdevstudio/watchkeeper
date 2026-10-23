use watchkeeper_core::report::{esc, file_stem, utc_stamp, write_files, Report, FORMAT};
use watchkeeper_core::{Session, StoredCookie};

const PAGE: &str = "https://www.news.example.com/story";

fn batch(events: &[String]) -> String {
    let ev: Vec<&str> = events.iter().map(|s| s.as_str()).collect();
    format!(
        r#"{{"doc":"d1","url":"{PAGE}","from":1,"dropped":0,"ev":[{}]}}"#,
        ev.join(",")
    )
}

fn req(u: &str, t: &str, c: &str) -> String {
    format!(r#"{{"k":"res","o":"https://www.news.example.com","u":"{u}","t":"{t}","c":"{c}"}}"#)
}

fn dir(name: &str) -> std::path::PathBuf {
    let mut d = std::env::temp_dir();
    d.push(format!(
        "wk-export-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    d
}

fn session() -> Session {
    let mut s = Session::default();
    let evs = vec![
        req("https://cdn.news.example.com/app.js", "script", "1"),
        req("https://ads.tracker.net/p.gif", "img", "1"),
        req("https://fp.tracker.net/f.js", "script", "1"),
        req("https://w.tracker.net/worker.js", "worker", ""),
        req("https://made.up/x", "xhr", ""),
        r#"{"k":"fp","a":"canvas","o":"https://www.news.example.com","u":"https://fp.tracker.net/f.js"}"#.to_string(),
        r#"{"k":"cookie","n":"wk_js","o":"https://www.news.example.com"}"#.to_string(),
        r#"{"k":"store","s":"localStorage","n":"seen","o":"https://www.news.example.com"}"#.to_string(),
    ];
    s.apply(&batch(&evs)).unwrap();
    s.report.set_cookies(vec![
        StoredCookie {
            name: "sid".into(),
            domain: "news.example.com".into(),
            http_only: true,
        },
        StoredCookie {
            name: "uid".into(),
            domain: ".tracker.net".into(),
            http_only: false,
        },
        StoredCookie {
            name: "old".into(),
            domain: "elsewhere.org".into(),
            http_only: false,
        },
    ]);
    s
}

#[test]
fn report_holds_everything_seen() {
    let s = session();
    let r = Report::new(&s.report, None, "0.3.0", "linux", 1_792_764_485);
    assert_eq!(r.format, FORMAT);
    assert_eq!(r.site, "example.com");
    let sites: Vec<(&str, bool, usize)> = r
        .other_sites
        .iter()
        .map(|o| (o.site.as_str(), o.confirmed, o.addresses.len()))
        .collect();
    assert_eq!(sites, vec![("made.up", false, 1), ("tracker.net", true, 3)]);
    assert_eq!(r.own_addresses.len(), 1);
    assert_eq!(r.workers, vec!["https://w.tracker.net/worker.js"]);
    assert_eq!(r.fingerprinting[0].by, "tracker.net");
    let belongs: Vec<&str> = r.cookies.iter().map(|c| c.belongs_to.as_str()).collect();
    assert_eq!(belongs, vec!["this site", "other site"]);
    assert_eq!(r.earlier_cookies, 1, "counted, not named");
    assert!(!r.to_json().contains("elsewhere.org"));
    assert!(!r.to_html().contains("elsewhere.org"));
    assert_eq!(r.script_cookies[0].name, "wk_js");
    assert_eq!(r.storage_writes[0].key, "seen");
    assert!(!r.cannot_see.is_empty());
    let json: serde_json::Value = serde_json::from_str(&r.to_json()).unwrap();
    assert_eq!(json["format_version"], 1);
    assert_eq!(
        json["summary"]["third_party_sites"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let html = r.to_html();
    for want in [
        "tracker.net",
        "https://ads.tracker.net/p.gif",
        "canvas by tracker.net",
        "sid",
        "seen in localStorage",
    ] {
        assert!(html.contains(want), "html lacks {want}");
    }
}

#[test]
fn hostile_names_stay_text() {
    let mut s = Session::default();
    let nasty = r#"<script>alert(1)</script>"#;
    let evs = vec![
        format!(r#"{{"k":"store","s":"localStorage","n":"{}","o":"https://www.news.example.com"}}"#, nasty.replace('"', "\\\"")),
        r#"{"k":"cookie","n":"a\"><img src=x onerror=alert(1)>","o":"https://www.news.example.com"}"#.to_string(),
        req("https://t.net/<b>x</b>", "img", ""),
    ];
    s.apply(&batch(&evs)).unwrap();
    let html = Report::new(&s.report, None, "0.3.0", "linux", 0).to_html();
    let lower = html.to_ascii_lowercase();
    assert!(!lower.contains("<script"), "no script element");
    assert!(!lower.contains("<img"), "no image element");
    assert!(!lower.contains("<b>x</b>"));
    assert!(!lower.contains("href="), "addresses are not links");
    assert!(lower.contains("default-src 'none'"));
    assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
    assert_eq!(esc("a&b\"'\u{0}"), "a&amp;b&quot;&#39;\u{fffd}");
    assert_eq!(
        esc("\u{202e}gnp.exe\u{200b}\u{85}"),
        "\u{fffd}gnp.exe\u{fffd}\u{fffd}"
    );
    assert_eq!(
        esc("caf\u{e9} \u{65e5}"),
        "caf\u{e9} \u{65e5}",
        "ordinary text kept"
    );
}

#[test]
fn names_and_times() {
    assert_eq!(utc_stamp(0), "1970-01-01 00:00:00 UTC");
    assert_eq!(utc_stamp(1_792_764_485), "2026-10-23 14:08:05 UTC");
    assert_eq!(
        file_stem("Example.com", 1_792_764_485),
        "watchkeeper-example.com-2026-10-23-140805"
    );
    assert_eq!(file_stem("[::1]", 0), "watchkeeper-___1_-1970-01-01-000000");
    assert!(!file_stem("../../etc", 0).contains('/'));
    assert_eq!(file_stem("", 0), "watchkeeper-page-1970-01-01-000000");
    assert!(file_stem(&"x".repeat(500), 0).len() < 140);
}

#[test]
fn files_are_never_overwritten() {
    let d = dir("files");
    let s = session();
    let r = Report::new(&s.report, None, "0.3.0", "linux", 5);
    let stem = file_stem(&r.site, 5);
    let (h1, j1) = write_files(&d, &r, &stem).unwrap();
    let (h2, j2) = write_files(&d, &r, &stem).unwrap();
    assert_ne!(h1, h2);
    assert_ne!(j1, j2);
    assert!(h2.to_string_lossy().ends_with("-2.html"));
    let back: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&j1).unwrap()).unwrap();
    assert_eq!(back["site"], "example.com");
    assert!(std::fs::read_to_string(&h1)
        .unwrap()
        .starts_with("<!doctype html>"));
}

#[test]
fn site_list_matches_the_panel() {
    use watchkeeper_core::MAX_SITES;
    let mut s = Session::default();
    let evs: Vec<String> = (0..MAX_SITES + 40)
        .map(|i| req(&format!("https://s{i}.made.example{i}/x"), "xhr", ""))
        .collect();
    s.apply(&batch(&evs)).unwrap();
    let r = Report::new(&s.report, None, "0.3.0", "linux", 0);
    assert_eq!(r.other_sites.len(), r.summary.third_party_sites.len());
    assert_eq!(r.other_sites.len(), MAX_SITES);
    assert!(r.dropped >= 40);
    assert!(r.to_html().contains(&format!("<b>{MAX_SITES}</b>")));
}
