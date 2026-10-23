use watchkeeper_core::{
    Applied, Event, Session, SiteReport, StoredCookie, MAX_ADDRESSES, MAX_NAMES, MAX_TEXT,
};

fn batch(doc: &str, url: &str, from: u64, dropped: u64, events: &[&str]) -> String {
    format!(
        r#"{{"doc":"{doc}","url":"{url}","from":{from},"dropped":{dropped},"ev":[{}]}}"#,
        events.join(",")
    )
}

const PAGE: &str = "https://www.news.example.com/story";

fn req(u: &str, t: &str) -> String {
    format!(r#"{{"k":"res","o":"https://www.news.example.com","u":"{u}","t":"{t}"}}"#)
}

#[test]
fn counts_other_sites_and_addresses() {
    let mut s = Session::default();
    let evs = [
        req("https://cdn.news.example.com/app.js", "script"),
        req("https://ads.tracker.net/p.gif", "img"),
        req("https://ads.tracker.net/p.gif", "fetch"),
        req("https://collect.analytics.co.uk/b", "beacon"),
        req("https://w.tracker.net/worker.js", "worker"),
    ];
    let evs: Vec<&str> = evs.iter().map(|s| s.as_str()).collect();
    assert_eq!(
        s.apply(&batch("d1", PAGE, 1, 0, &evs)).unwrap(),
        Applied::Added(5)
    );
    let sum = s.report.summary();
    assert_eq!(sum.site, "example.com");
    assert_eq!(
        sum.third_party_sites,
        vec!["analytics.co.uk", "tracker.net"]
    );
    assert_eq!(sum.addresses, 4, "same address counts once");
    assert_eq!(sum.third_party_addresses, 3);
    assert_eq!(sum.workers, 1);
    let kinds = &s.report.requests["https://ads.tracker.net/p.gif"];
    assert!(kinds.contains("img") && kinds.contains("fetch"));
}

#[test]
fn acknowledged_events_are_not_counted_twice() {
    let mut s = Session::default();
    let a = req("https://a.tracker.net/1", "img");
    let b = req("https://b.other.org/2", "img");
    s.apply(&batch("d1", PAGE, 1, 0, &[&a])).unwrap();
    assert_eq!(s.drain_args(), ("d1".to_string(), 1));
    // The answer to the next call was lost; the page sends event 1 again.
    s.apply(&batch("d1", PAGE, 1, 0, &[&a, &b])).unwrap();
    assert_eq!(s.drain_args(), ("d1".to_string(), 2));
    assert_eq!(s.report.summary().addresses, 2);
}

#[test]
fn new_site_resets_same_site_carries_on_stale_ignored() {
    let mut s = Session::default();
    let a = req("https://a.tracker.net/1", "img");
    s.apply(&batch("d1", PAGE, 1, 0, &[&a])).unwrap();
    // Same site, new document: the report carries on.
    let b = req("https://b.other.org/2", "img");
    s.apply(&batch("d2", "https://shop.example.com/", 1, 0, &[&b]))
        .unwrap();
    assert_eq!(s.report.summary().third_party_sites.len(), 2);
    // Another site: a fresh report.
    s.apply(&batch("d3", "https://elsewhere.org/", 1, 0, &[]))
        .unwrap();
    assert_eq!(s.report.site, "elsewhere.org");
    assert!(s.report.summary().third_party_sites.is_empty());
    // A late answer from the previous page is ignored.
    let late = req("https://late.tracker.net/x", "beacon");
    assert_eq!(
        s.apply(&batch("d2", "https://shop.example.com/", 2, 0, &[&late]))
            .unwrap(),
        Applied::Stale
    );
    assert!(s.report.summary().third_party_sites.is_empty());
}

#[test]
fn nothing_double_encoded_and_malformed() {
    let mut s = Session::default();
    assert_eq!(s.apply("null").unwrap(), Applied::Nothing);
    assert_eq!(s.apply("").unwrap(), Applied::Nothing);
    let inner = batch("d1", PAGE, 1, 0, &[r#"{"k":"fp","a":"canvas","o":""}"#]);
    let outer = serde_json::to_string(&inner).unwrap();
    assert_eq!(s.apply(&outer).unwrap(), Applied::Added(1));
    assert_eq!(s.report.summary().fingerprinting, vec!["canvas"]);
    assert!(s.apply("not json").is_err());
    assert!(s.apply("{}").is_err());
    // Unknown kinds and bad addresses are skipped, but still acknowledged.
    let r = s
        .apply(&batch(
            "d1",
            PAGE,
            2,
            0,
            &[r#"{"k":"weird"}"#, r#"{"k":"res","u":"not a url"}"#],
        ))
        .unwrap();
    assert_eq!(r, Applied::Added(1));
    assert_eq!(s.drain_args().1, 3);
    assert_eq!(s.report.summary().addresses, 0);
}

#[test]
fn page_dropped_counts_are_reported() {
    let mut s = Session::default();
    s.apply(&batch("d1", PAGE, 1, 40, &[])).unwrap();
    s.apply(&batch("d1", PAGE, 1, 55, &[])).unwrap();
    assert_eq!(s.report.summary().dropped, 55);
}

#[test]
fn storage_cookies_and_origins() {
    let mut r = SiteReport::new(PAGE);
    r.add_event(Event::Store {
        s: "localStorage".into(),
        n: "uid".into(),
        o: "https://www.news.example.com".into(),
    });
    r.add_event(Event::Store {
        s: "localStorage".into(),
        n: "uid".into(),
        o: "https://frame.tracker.net".into(),
    });
    r.add_event(Event::Request {
        u: "https://t.tracker.net/x".into(),
        t: "img".into(),
        o: String::new(),
    });
    r.set_cookies(vec![
        StoredCookie {
            name: "session".into(),
            domain: ".example.com".into(),
            http_only: true,
        },
        StoredCookie {
            name: "uid".into(),
            domain: "t.tracker.net".into(),
            http_only: false,
        },
        StoredCookie {
            name: "other".into(),
            domain: "unrelated.org".into(),
            http_only: false,
        },
    ]);
    let s = r.summary();
    assert_eq!(s.storage_writes, 2);
    assert_eq!(s.third_party_storage_writes, 1);
    assert_eq!(s.first_party_cookies, 1);
    assert_eq!(
        s.third_party_cookies, 1,
        "cookies of sites not contacted here are not counted"
    );
}

#[test]
fn a_cookie_flood_cannot_hide_a_tracker() {
    let mut r = SiteReport::new(PAGE);
    for i in 0..MAX_NAMES + 30_000 {
        r.add_event(Event::Cookie {
            n: format!("c{i}"),
            o: String::new(),
        });
    }
    r.add_event(Event::Request {
        u: "https://t.tracker.net/x".into(),
        t: "img".into(),
        o: String::new(),
    });
    let s = r.summary();
    assert_eq!(s.third_party_sites, vec!["tracker.net"]);
    assert_eq!(s.dropped, 30_000);
}

#[test]
fn an_address_flood_cannot_hide_a_new_site() {
    let mut r = SiteReport::new(PAGE);
    for i in 0..MAX_ADDRESSES + 10 {
        r.add_event(Event::Request {
            u: format!("https://www.news.example.com/{i}"),
            t: "xhr".into(),
            o: String::new(),
        });
    }
    r.add_event(Event::Request {
        u: "https://t.tracker.net/x".into(),
        t: "img".into(),
        o: String::new(),
    });
    let s = r.summary();
    assert_eq!(s.third_party_sites, vec!["tracker.net"]);
    assert_eq!(s.addresses, MAX_ADDRESSES);
    assert!(s.dropped >= 10);
}

#[test]
fn long_text_is_clipped() {
    let mut r = SiteReport::new(PAGE);
    let long = format!("https://t.tracker.net/{}", "é".repeat(2000));
    r.add_event(Event::Request {
        u: long,
        t: "img".into(),
        o: String::new(),
    });
    let key = r.requests.keys().next().unwrap();
    assert!(key.len() <= MAX_TEXT);
}

#[test]
fn same_site() {
    let r = SiteReport::new(PAGE);
    assert!(r.same_site("https://shop.example.com/"));
    assert!(!r.same_site("https://example.org/"));
    assert!(!r.same_site("about:blank"));
}
