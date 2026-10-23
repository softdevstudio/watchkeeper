//! Keeps the per-site history up to date while you browse.
//!
//! The collector thread calls `observe` after each round. A new visit (the
//! report started over for a site) adds an entry; the same visit updates
//! its entry. The file is written every few seconds when something changed,
//! and once more when the app closes. With `--no-history` nothing is kept
//! or shown.

use std::path::PathBuf;

use watchkeeper_core::history::{today, Comparison, History, Visit};
use watchkeeper_core::Summary;

pub struct Recorder {
    history: History,
    /// Where the file lives; None when history is off.
    path: Option<PathBuf>,
    /// The visit being recorded: site and the session's visit number.
    current: Option<(String, u64)>,
    /// A visit the person asked to forget: not recorded until it ends.
    suppressed: Option<(String, u64)>,
    dirty: bool,
    /// Set when the file could not be read or written.
    pub note: Option<String>,
}

impl Recorder {
    pub fn new(path: Option<PathBuf>) -> Recorder {
        let (history, note) = match &path {
            Some(p) => History::load(p),
            None => (History::new(), None),
        };
        Recorder {
            history,
            path,
            current: None,
            suppressed: None,
            dirty: false,
            note,
        }
    }

    pub fn enabled(&self) -> bool {
        self.path.is_some()
    }

    /// Takes in the summary of the site in the window.
    pub fn observe(&mut self, summary: &Summary, visit: u64) {
        if !self.enabled() || summary.site.is_empty() {
            return;
        }
        let key = (summary.site.clone(), visit);
        if self.suppressed.as_ref() == Some(&key) {
            return;
        }
        let entry = Visit::from_summary(summary, &today());
        if self.current.as_ref() == Some(&key) {
            let unchanged = self
                .history
                .sites
                .get(&summary.site)
                .and_then(|v| v.back())
                .is_some_and(|last| *last == entry);
            if !unchanged {
                self.history.update_last(&summary.site, entry);
                self.dirty = true;
            }
        } else {
            self.history.push(&summary.site, entry);
            self.current = Some(key);
            self.dirty = true;
        }
    }

    /// The last visit before this one, for the site in the window.
    pub fn compare(&self, site: &str, visit: u64) -> Option<Comparison> {
        if !self.enabled() || self.current.as_ref() != Some(&(site.to_string(), visit)) {
            return None;
        }
        self.history.compare(site)
    }

    /// When something changed: a copy of the history, trimmed to its
    /// limits, and where to write it. Writing can then happen without
    /// holding this recorder, so the panel never waits on the disk.
    pub fn take_save(&mut self) -> Option<(History, PathBuf)> {
        if !self.dirty {
            return None;
        }
        let path = self.path.clone()?;
        self.history.trim();
        self.dirty = false;
        Some((self.history.clone(), path))
    }

    /// Notes a failed write; the next round tries again.
    pub fn save_failed(&mut self, e: std::io::Error) {
        self.dirty = true;
        self.note = Some(format!("history could not be saved: {e}"));
    }

    /// Writes the file if something changed, holding the recorder.
    pub fn save_if_dirty(&mut self) {
        if let Some((h, p)) = self.take_save() {
            if let Err(e) = h.save(&p) {
                self.save_failed(e);
            }
        }
    }

    /// Forgets one site, including the visit in progress.
    pub fn forget(&mut self, site: &str) {
        self.history.forget(site);
        if let Some(cur) = &self.current {
            if cur.0 == site {
                self.suppressed = self.current.take();
            }
        }
        self.dirty = true;
    }

    /// Forgets everything, including the visit in progress.
    pub fn forget_all(&mut self) {
        self.history.forget_all();
        // Keep an earlier suppression when no visit is being recorded.
        if let Some(cur) = self.current.take() {
            self.suppressed = Some(cur);
        }
        self.dirty = true;
    }

    pub fn path(&self) -> Option<&std::path::Path> {
        self.path.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(site: &str, others: &[&str]) -> Summary {
        Summary {
            site: site.into(),
            third_party_sites: others.iter().map(|s| s.to_string()).collect(),
            confirmed_sites: others.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    /// A file of its own for each test: clock ticks are coarse on Windows,
    /// so the name also carries the test's name.
    fn temp_file(name: &str) -> PathBuf {
        let mut d = std::env::temp_dir();
        d.push(format!(
            "wk-recorder-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        d.join("history.json")
    }

    #[test]
    fn off_keeps_nothing() {
        let mut r = Recorder::new(None);
        r.observe(&summary("a.com", &["t.net"]), 1);
        r.save_if_dirty();
        assert!(!r.enabled());
        assert!(r.compare("a.com", 1).is_none());
        assert!(r.history.sites.is_empty());
    }

    #[test]
    fn visits_and_comparison() {
        let p = temp_file("visits");
        let mut r = Recorder::new(Some(p.clone()));
        r.observe(&summary("", &[]), 1);
        assert!(r.history.sites.is_empty(), "no site, nothing kept");
        r.observe(&summary("a.com", &["t.net"]), 1);
        r.observe(&summary("a.com", &["t.net", "u.net"]), 1);
        assert_eq!(r.history.sites["a.com"].len(), 1, "same visit updates");
        r.observe(&summary("b.com", &[]), 2);
        r.observe(&summary("a.com", &["u.net", "v.net"]), 3);
        assert_eq!(r.history.sites["a.com"].len(), 2);
        let c = r.compare("a.com", 3).unwrap();
        assert_eq!(c.other_site_count, 2);
        assert_eq!(c.new_sites, vec!["v.net"]);
        assert_eq!(c.gone_sites, vec!["t.net"]);
        assert!(
            r.compare("a.com", 1).is_none(),
            "only for the visit in progress"
        );
        r.save_if_dirty();
        assert!(r.note.is_none(), "{:?}", r.note);
        let again = Recorder::new(Some(p.clone()));
        assert_eq!(again.history.sites.len(), 2);
        assert!(again.note.is_none());
    }

    #[test]
    fn forgetting_holds_for_the_visit_in_progress() {
        let p = temp_file("forget");
        let mut r = Recorder::new(Some(p.clone()));
        r.observe(&summary("a.com", &["t.net"]), 1);
        r.observe(&summary("b.com", &["t.net"]), 2);
        r.forget("b.com");
        r.save_if_dirty();
        r.observe(&summary("b.com", &["t.net", "u.net"]), 2);
        assert!(!r.history.sites.contains_key("b.com"));
        assert!(History::load(&p).0.sites.contains_key("a.com"));
        r.forget_all();
        r.observe(&summary("b.com", &["t.net"]), 2);
        r.save_if_dirty();
        assert!(History::load(&p).0.sites.is_empty());
        // A new visit is recorded again.
        r.observe(&summary("c.com", &[]), 3);
        r.save_if_dirty();
        assert_eq!(History::load(&p).0.sites.len(), 1);
    }
}
