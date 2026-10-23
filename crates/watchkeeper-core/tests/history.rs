use watchkeeper_core::history::{
    date_from_days, History, Visit, MAX_HISTORY_BYTES, MAX_HISTORY_SITES, MAX_NAMES_PER_VISIT,
    MAX_NAME_BYTES, MAX_VISITS,
};

fn visit(date: &str, sites: &[&str]) -> Visit {
    Visit {
        date: date.into(),
        other_site_count: sites.len(),
        other_sites: sites.iter().map(|s| s.to_string()).collect(),
        cookies: 3,
        storage_writes: 1,
        fingerprinting: vec![],
    }
}

/// A folder of its own for each call: clock ticks are coarse on Windows.
fn dir() -> std::path::PathBuf {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut d = std::env::temp_dir();
    d.push(format!(
        "wk-history-{}-{n}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn dates() {
    assert_eq!(date_from_days(0), "1970-01-01");
    assert_eq!(date_from_days(20_749), "2026-10-23");
    assert_eq!(date_from_days(11_016), "2000-02-29");
    assert_eq!(date_from_days(-1), "1969-12-31");
}

#[test]
fn compare_with_last_visit() {
    let mut h = History::new();
    h.push("news.com", visit("2026-10-21", &["a.net", "b.net"]));
    assert!(
        h.compare("news.com").is_none(),
        "one visit has nothing to compare"
    );
    h.push(
        "news.com",
        visit("2026-10-23", &["b.net", "c.net", "d.net"]),
    );
    let c = h.compare("news.com").unwrap();
    assert_eq!(c.date, "2026-10-21");
    assert_eq!(c.other_site_count, 2);
    assert_eq!(c.new_sites, vec!["c.net", "d.net"]);
    assert_eq!(c.gone_sites, vec!["a.net"]);
    // The current visit grows; the comparison follows.
    h.update_last(
        "news.com",
        visit("2026-10-23", &["b.net", "c.net", "d.net", "e.net"]),
    );
    assert_eq!(h.compare("news.com").unwrap().new_sites.len(), 3);
}

#[test]
fn limits() {
    let mut h = History::new();
    for i in 0..MAX_VISITS + 5 {
        h.push("x.com", visit(&format!("2026-01-{:02}", i % 28 + 1), &[]));
    }
    assert_eq!(h.sites["x.com"].len(), MAX_VISITS);
    let many: Vec<String> = (0..MAX_NAMES_PER_VISIT + 50)
        .map(|i| format!("s{i}.net"))
        .collect();
    let refs: Vec<&str> = many.iter().map(|s| s.as_str()).collect();
    h.push("y.com", visit("2026-01-01", &refs));
    let mut j = History::load(&std::path::PathBuf::from("/nonexistent/wk")).0;
    j.sites = h.sites.clone();
    let d = dir();
    let p = d.join("history.json");
    j.save(&p).unwrap();
    let (back, note) = History::load(&p);
    assert!(note.is_none());
    assert_eq!(
        back.sites["y.com"][0].other_sites.len(),
        MAX_NAMES_PER_VISIT
    );
    let mut k = History::new();
    for i in 0..MAX_HISTORY_SITES + 10 {
        k.push(
            &format!("site{i}.com"),
            visit(&format!("2026-{:02}-01", i % 12 + 1), &[]),
        );
    }
    assert_eq!(k.sites.len(), MAX_HISTORY_SITES);
    h.push("", visit("2026-01-01", &[]));
    assert!(!h.sites.contains_key(""));
}

#[test]
fn save_load_forget() {
    let d = dir();
    let p = d.join("history.json");
    let (empty, note) = History::load(&p);
    assert!(empty.sites.is_empty() && note.is_none());
    let mut h = History::new();
    h.push("a.com", visit("2026-10-22", &["t.net"]));
    h.push("b.com", visit("2026-10-22", &[]));
    h.save(&p).unwrap();
    assert!(!d.join("history.json.tmp").exists());
    let (mut back, _) = History::load(&p);
    assert_eq!(back.sites.len(), 2);
    assert!(back.forget("a.com"));
    assert!(!back.forget("a.com"));
    back.save(&p).unwrap();
    assert_eq!(History::load(&p).0.sites.len(), 1);
    back.forget_all();
    back.save(&p).unwrap();
    assert!(History::load(&p).0.sites.is_empty());
}

#[test]
fn damaged_file_is_set_aside() {
    let d = dir();
    let p = d.join("history.json");
    std::fs::write(&p, "{ not json").unwrap();
    let (h, note) = History::load(&p);
    assert!(h.sites.is_empty());
    assert!(note.unwrap().contains("damaged"));
    let aside: Vec<_> = std::fs::read_dir(&d)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.starts_with("history.json.bad-"))
        .collect();
    assert_eq!(
        aside.len(),
        1,
        "set aside under a name of its own: {aside:?}"
    );
    assert!(!p.exists());
    // A save cut short leaves a temporary file; loading clears it.
    std::fs::write(d.join("history.json.tmp"), "partial").unwrap();
    let _ = History::load(&p);
    assert!(!d.join("history.json.tmp").exists());
}

#[test]
fn size_stays_within_budget() {
    let long = "x".repeat(MAX_NAME_BYTES + 1);
    let mut h = History::new();
    h.push("a.com", visit("2026-01-01", &[&long, "ok.net"]));
    assert_eq!(
        h.sites["a.com"][0].other_sites,
        vec!["ok.net"],
        "overlong names dropped"
    );
    h.push(&long, visit("2026-01-01", &[]));
    assert!(!h.sites.contains_key(&long));
    // Visits full of the longest names, far past the budget.
    let names: Vec<String> = (0..MAX_NAMES_PER_VISIT)
        .map(|i| format!("{i}{}", "n".repeat(240)))
        .collect();
    let refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
    for i in 0..200 {
        let date = format!("2026-{:02}-{:02}", i % 12 + 1, i % 28 + 1);
        h.push(&format!("site{i}.com"), visit(&date, &refs));
    }
    assert!(h.approx_bytes() <= MAX_HISTORY_BYTES);
    assert!(
        h.sites.len() > 50,
        "only the oldest go: {} left",
        h.sites.len()
    );
    let d = dir();
    let p = d.join("history.json");
    h.save(&p).unwrap();
    let size = std::fs::metadata(&p).unwrap().len() as usize;
    assert!(
        size <= MAX_HISTORY_BYTES + MAX_HISTORY_BYTES / 10,
        "file is {size} bytes"
    );
    let (back, note) = History::load(&p);
    assert!(note.is_none());
    assert_eq!(back.sites.len(), h.sites.len());
}
