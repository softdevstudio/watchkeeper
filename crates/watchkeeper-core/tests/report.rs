use watchkeeper_core::{Event, SiteReport, StoredCookie, MAX_EVENTS_PER_SITE};

fn report() -> SiteReport {
    SiteReport::new("https://www.news.example.com/story")
}

#[test]
fn counts_third_party_sites_and_requests() {
    let mut r = report();
    let json = r#"[
        {"k":"res","u":"https://cdn.news.example.com/app.js","t":"script","o":"https://www.news.example.com"},
        {"k":"res","u":"https://ads.tracker.net/p.gif","t":"img","o":"https://www.news.example.com"},
        {"k":"res","u":"https://ads.tracker.net/p.gif","t":"fetch","o":"https://www.news.example.com"},
        {"k":"res","u":"https://collect.analytics.co.uk/b","t":"beacon","o":"https://www.news.example.com"}
    ]"#;
    assert_eq!(r.add_events_json(json).unwrap(), 4);
    let s = r.summary();
    assert_eq!(s.site, "example.com");
    assert_eq!(s.third_party_sites, vec!["analytics.co.uk", "tracker.net"]);
    assert_eq!(s.requests, 3, "same URL counts once");
    assert_eq!(s.third_party_requests, 2);
    let kinds = &r.requests["https://ads.tracker.net/p.gif"];
    assert!(kinds.contains("img") && kinds.contains("fetch"));
}

#[test]
fn double_encoded_json_is_accepted() {
    let mut r = report();
    let inner = r#"[{"k":"fp","a":"canvas","o":""}]"#;
    let outer = serde_json::to_string(inner).unwrap();
    assert_eq!(r.add_events_json(&outer).unwrap(), 1);
    assert_eq!(r.summary().fingerprinting, vec!["canvas"]);
}

#[test]
fn malformed_input_is_rejected_unknown_kinds_skipped() {
    let mut r = report();
    assert!(r.add_events_json("not json").is_err());
    assert!(r.add_events_json("{}").is_err());
    let n = r
        .add_events_json(r#"[{"k":"weird"},{"k":"cookie","n":"sid"},{"k":"res","u":"not a url"}]"#)
        .unwrap();
    assert_eq!(n, 2, "unknown kind skipped");
    assert_eq!(r.summary().requests, 0, "bad URL ignored");
    assert!(r.script_cookies.contains("sid"));
}

#[test]
fn storage_and_cookies() {
    let mut r = report();
    r.add_event(Event::Store {
        s: "localStorage".into(),
        n: "uid".into(),
        o: String::new(),
    });
    r.add_event(Event::Store {
        s: "localStorage".into(),
        n: "uid".into(),
        o: String::new(),
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
    assert_eq!(s.storage_writes, 1);
    assert_eq!(s.first_party_cookies, 1);
    assert_eq!(
        s.third_party_cookies, 1,
        "cookies of sites not contacted here are not counted"
    );
}

#[test]
fn cap_drops_and_counts() {
    let mut r = report();
    for i in 0..MAX_EVENTS_PER_SITE + 10 {
        r.add_event(Event::Cookie {
            n: format!("c{i}"),
            o: String::new(),
        });
    }
    assert_eq!(r.summary().dropped, 10);
    assert_eq!(r.script_cookies.len(), MAX_EVENTS_PER_SITE);
}

#[test]
fn same_site() {
    let r = report();
    assert!(r.same_site("https://shop.example.com/"));
    assert!(!r.same_site("https://example.org/"));
    assert!(!r.same_site("about:blank"));
}
