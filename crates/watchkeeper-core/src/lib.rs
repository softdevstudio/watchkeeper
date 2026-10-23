//! Shared core for Watchkeeper and, later, Unweb.
//!
//! Two parts live here, neither tied to a window toolkit:
//!
//! * the detection script injected into every frame of a page, and the
//!   script the app runs to collect what it saw, and
//! * [`Session`] and [`SiteReport`], which turn collected batches and the
//!   engine's cookie list into a per-site picture.
//!
//! A "site" is a registrable domain, so `news.example.co.uk` and
//! `cdn.example.co.uk` are one site. IP addresses and `localhost` are
//! their own sites.
//!
//! The collection protocol: each page document gets a random id. The app
//! asks for events with the id of the document it last heard from and the
//! number of the last event it received; the script drops only those, and
//! answers with everything newer. If an answer is lost, the same events
//! come again and are skipped by number. Answers from a document the app
//! has already moved past are ignored, so a slow answer from the previous
//! page can never land on the next site.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use url::Url;

const DETECT_TEMPLATE: &str = include_str!("../assets/detect.js");

/// Distinct addresses kept per site.
pub const MAX_ADDRESSES: usize = 5_000;
/// Distinct other sites kept per site. Kept separately from addresses, so
/// a flood of addresses cannot hide a new site.
pub const MAX_SITES: usize = 2_000;
/// Distinct cookie names and storage keys kept per site, each.
pub const MAX_NAMES: usize = 1_000;
/// Longest address or name kept, in bytes.
pub const MAX_TEXT: usize = 512;
/// Largest batch accepted from the page, in bytes.
pub const MAX_BATCH_BYTES: usize = 4 << 20;

/// The random values that tie the script to one launch of the app.
#[derive(Clone, Debug)]
pub struct Keys {
    /// Part of the collector's name on the page.
    pub token: String,
    /// Needed to drain the collector. Never visible to the page.
    pub secret: String,
    /// Marks messages from sub-frames.
    pub frame_key: String,
}

impl Keys {
    /// Builds keys from three random strings. Each must be ASCII letters
    /// and digits only, so it cannot break out of the script's strings.
    pub fn new(token: &str, secret: &str, frame_key: &str) -> Result<Keys, String> {
        for (name, v) in [
            ("token", token),
            ("secret", secret),
            ("frame key", frame_key),
        ] {
            if v.len() < 8 || !v.chars().all(|c| c.is_ascii_alphanumeric()) {
                return Err(format!(
                    "{name} must be at least 8 ASCII letters and digits"
                ));
            }
        }
        Ok(Keys {
            token: token.into(),
            secret: secret.into(),
            frame_key: frame_key.into(),
        })
    }
}

/// The detection script with this launch's keys filled in.
pub fn detection_script(keys: &Keys) -> String {
    DETECT_TEMPLATE
        .replace("__WK_TOKEN__", &keys.token)
        .replace("__WK_SECRET__", &keys.secret)
        .replace("__WK_FRAMEKEY__", &keys.frame_key)
}

/// Script that collects events. It returns a JSON string, or null when the
/// detection script is not on the page.
pub fn drain_script(keys: &Keys, doc: &str, ack: u64) -> String {
    // The document id comes back from the page, so it is passed as a JSON
    // string literal rather than pasted in.
    let doc = serde_json::to_string(doc).unwrap_or_else(|_| "\"\"".into());
    format!(
        "(function(){{try{{var c=window['__wk_{t}'];return c&&c.d?c.d('{s}',{doc},{ack}):null;}}catch(e){{return null;}}}})()",
        t = keys.token,
        s = keys.secret,
    )
}

/// One thing the page did, as reported by the detection script.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "k")]
pub enum Event {
    /// A network request, or a worker started.
    #[serde(rename = "res")]
    Request {
        /// Address without query or fragment.
        u: String,
        /// How it was made: script, img, fetch, beacon, xhr, websocket,
        /// worker...
        #[serde(default)]
        t: String,
        /// Origin of the frame that made it.
        #[serde(default)]
        o: String,
    },
    /// A cookie written by script.
    #[serde(rename = "cookie")]
    Cookie {
        n: String,
        #[serde(default)]
        o: String,
    },
    /// A storage write.
    #[serde(rename = "store")]
    Store {
        /// localStorage, sessionStorage or indexedDB.
        s: String,
        /// Key or database name.
        n: String,
        #[serde(default)]
        o: String,
    },
    /// A read of a feature used for fingerprinting.
    #[serde(rename = "fp")]
    Fingerprint {
        /// canvas, webgl or audio.
        a: String,
        #[serde(default)]
        o: String,
    },
}

/// A cookie as the engine's cookie store reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredCookie {
    pub name: String,
    pub domain: String,
    pub http_only: bool,
}

/// The site a host belongs to.
pub fn site_of_host(host: &str) -> String {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty()
        || host == "localhost"
        || host.starts_with('[')
        || host.parse::<std::net::IpAddr>().is_ok()
    {
        return host;
    }
    match psl::domain_str(&host) {
        Some(d) => d.to_string(),
        None => host,
    }
}

/// The site of a URL, or `None` for URLs without a host.
pub fn site_of_url(url: &str) -> Option<String> {
    let u = Url::parse(url).ok()?;
    let host = u.host_str()?;
    Some(site_of_host(host))
}

fn clip(s: &str) -> String {
    if s.len() <= MAX_TEXT {
        return s.to_string();
    }
    let mut end = MAX_TEXT;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

/// What one site did while you were on it.
#[derive(Debug, Clone, Default, Serialize)]
pub struct SiteReport {
    /// Registrable domain of the page.
    pub site: String,
    /// Last top-level address seen for this site, without query.
    pub page_url: String,
    /// Distinct addresses, each with how it was reached.
    pub requests: BTreeMap<String, BTreeSet<String>>,
    /// Other sites contacted.
    pub other_sites: BTreeSet<String>,
    /// Cookie names written by script, as (site of the frame, name).
    pub script_cookies: BTreeSet<(String, String)>,
    /// Storage writes as (site of the frame, kind, key).
    pub storage: BTreeSet<(String, String, String)>,
    /// Fingerprinting reads: canvas, webgl, audio.
    pub fingerprinting: BTreeSet<String>,
    /// Cookies in the engine's store at the last refresh.
    pub cookies: Vec<StoredCookie>,
    /// Events the page made that could not be counted: the page's buffer
    /// overflowed, or a limit above was reached.
    pub dropped: u64,
}

/// Counts shown in the side panel.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Summary {
    pub site: String,
    pub page_url: String,
    /// Other sites the page contacted.
    pub third_party_sites: Vec<String>,
    /// Distinct addresses requested, and how many on other sites.
    pub addresses: usize,
    pub third_party_addresses: usize,
    /// Cookies in this session's store for this site, and for the other
    /// sites this page contacted (which may include cookies they set on
    /// earlier pages in the same session).
    pub first_party_cookies: usize,
    pub third_party_cookies: usize,
    /// Storage keys written, and how many by frames from other sites.
    pub storage_writes: usize,
    pub third_party_storage_writes: usize,
    /// Fingerprinting reads seen.
    pub fingerprinting: Vec<String>,
    /// Workers started (their own traffic is not visible).
    pub workers: usize,
    /// Events that could not be counted.
    pub dropped: u64,
}

impl SiteReport {
    /// A fresh report for a page address.
    pub fn new(page_url: &str) -> Self {
        SiteReport {
            site: site_of_url(page_url).unwrap_or_default(),
            page_url: clip(page_url),
            ..Default::default()
        }
    }

    /// Whether moving to `url` stays on the same site.
    pub fn same_site(&self, url: &str) -> bool {
        site_of_url(url).is_some_and(|s| s == self.site)
    }

    fn site_of_origin(&self, origin: &str) -> String {
        site_of_url(origin).unwrap_or_else(|| self.site.clone())
    }

    /// Adds one event.
    pub fn add_event(&mut self, ev: Event) {
        match ev {
            Event::Request { u, t, .. } => {
                let Some(site) = site_of_url(&u) else {
                    return;
                };
                if site != self.site && !self.other_sites.contains(&site) {
                    if self.other_sites.len() < MAX_SITES {
                        self.other_sites.insert(site);
                    } else {
                        self.dropped += 1;
                    }
                }
                let kind = if t.is_empty() {
                    "other".to_string()
                } else {
                    clip(&t)
                };
                let u = clip(&u);
                if let Some(kinds) = self.requests.get_mut(&u) {
                    if kinds.len() < 16 {
                        kinds.insert(kind);
                    }
                } else if self.requests.len() < MAX_ADDRESSES {
                    self.requests.insert(u, BTreeSet::from([kind]));
                } else {
                    self.dropped += 1;
                }
            }
            Event::Cookie { n, o } => {
                let key = (self.site_of_origin(&o), clip(&n));
                if !self.script_cookies.contains(&key) {
                    if self.script_cookies.len() < MAX_NAMES {
                        self.script_cookies.insert(key);
                    } else {
                        self.dropped += 1;
                    }
                }
            }
            Event::Store { s, n, o } => {
                let key = (self.site_of_origin(&o), clip(&s), clip(&n));
                if !self.storage.contains(&key) {
                    if self.storage.len() < MAX_NAMES {
                        self.storage.insert(key);
                    } else {
                        self.dropped += 1;
                    }
                }
            }
            Event::Fingerprint { a, .. } => {
                if self.fingerprinting.len() < 16 {
                    self.fingerprinting.insert(clip(&a));
                }
            }
        }
    }

    /// Whether a script cookie with this name was seen.
    pub fn has_script_cookie(&self, name: &str) -> bool {
        self.script_cookies.iter().any(|(_, n)| n == name)
    }

    /// Whether a storage write of this kind and key was seen.
    pub fn has_storage(&self, kind: &str, key: &str) -> bool {
        self.storage.iter().any(|(_, k, n)| k == kind && n == key)
    }

    /// Replaces the cookie list with the engine's current store.
    pub fn set_cookies(&mut self, cookies: Vec<StoredCookie>) {
        self.cookies = cookies;
    }

    /// The counts for the side panel.
    pub fn summary(&self) -> Summary {
        let third_party_addresses = self
            .requests
            .keys()
            .filter(|u| site_of_url(u).is_some_and(|s| s != self.site))
            .count();
        let mut first_party_cookies = 0;
        let mut third_party_cookies = 0;
        for c in &self.cookies {
            let site = site_of_host(c.domain.trim_start_matches('.'));
            if site == self.site {
                first_party_cookies += 1;
            } else if self.other_sites.contains(&site) {
                third_party_cookies += 1;
            }
        }
        Summary {
            site: self.site.clone(),
            page_url: self.page_url.clone(),
            third_party_sites: self.other_sites.iter().cloned().collect(),
            addresses: self.requests.len(),
            third_party_addresses,
            first_party_cookies,
            third_party_cookies,
            storage_writes: self.storage.len(),
            third_party_storage_writes: self
                .storage
                .iter()
                .filter(|(s, _, _)| *s != self.site)
                .count(),
            fingerprinting: self.fingerprinting.iter().cloned().collect(),
            workers: self
                .requests
                .values()
                .filter(|k| k.contains("worker"))
                .count(),
            dropped: self.dropped,
        }
    }
}

#[derive(Deserialize)]
struct Batch {
    doc: String,
    url: String,
    from: u64,
    #[serde(default)]
    dropped: u64,
    ev: Vec<serde_json::Value>,
}

/// What happened to a batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applied {
    /// Events from the current document were added.
    Added(usize),
    /// The batch came from a document the app has moved past.
    Stale,
    /// No detection script answered (blank page, or the page is loading).
    Nothing,
}

/// Follows the page from document to document and keeps the report for
/// the site in the window.
#[derive(Debug, Default)]
pub struct Session {
    pub report: SiteReport,
    doc: String,
    acked: u64,
    js_dropped: u64,
    retired: VecDeque<String>,
}

impl Session {
    /// The document id and acknowledgement to send with the next drain.
    pub fn drain_args(&self) -> (String, u64) {
        (self.doc.clone(), self.acked)
    }

    /// Adds the answer to a [`drain_script`] call. Some engines hand the
    /// result back JSON-encoded a second time; both forms are accepted.
    pub fn apply(&mut self, answer: &str) -> Result<Applied, String> {
        if answer.len() > MAX_BATCH_BYTES {
            return Err("batch too large".into());
        }
        let mut text = answer.trim().to_string();
        if text.is_empty() || text == "null" || text == "undefined" {
            return Ok(Applied::Nothing);
        }
        if text.starts_with('"') {
            text = serde_json::from_str::<String>(&text).map_err(|e| e.to_string())?;
        }
        let batch: Batch = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        if batch.doc.is_empty() || batch.doc.len() > 128 {
            return Err("bad document id".into());
        }
        if self.retired.contains(&batch.doc) {
            return Ok(Applied::Stale);
        }
        if batch.doc != self.doc {
            // A new document. Same site: the report carries on.
            if !self.doc.is_empty() {
                self.retired.push_back(std::mem::take(&mut self.doc));
                while self.retired.len() > 64 {
                    self.retired.pop_front();
                }
            }
            let cookies = std::mem::take(&mut self.report.cookies);
            if self.report.site.is_empty() || !self.report.same_site(&batch.url) {
                self.report = SiteReport::new(&batch.url);
            }
            self.report.cookies = cookies;
            self.doc = batch.doc.clone();
            self.acked = 0;
            self.js_dropped = 0;
        }
        self.report.page_url = clip(&batch.url);
        if batch.dropped > self.js_dropped {
            self.report.dropped += batch.dropped - self.js_dropped;
            self.js_dropped = batch.dropped;
        }
        let mut added = 0;
        for (i, v) in batch.ev.into_iter().enumerate() {
            let seq = batch.from.saturating_add(i as u64);
            if seq <= self.acked {
                continue;
            }
            self.acked = seq;
            if let Ok(ev) = serde_json::from_value::<Event>(v) {
                self.report.add_event(ev);
                added += 1;
            }
        }
        Ok(Applied::Added(added))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sites() {
        assert_eq!(site_of_host("news.bbc.co.uk"), "bbc.co.uk");
        assert_eq!(site_of_host("WWW.Example.COM."), "example.com");
        assert_eq!(site_of_host("localhost"), "localhost");
        assert_eq!(site_of_host("127.0.0.1"), "127.0.0.1");
        assert_eq!(
            site_of_url("https://cdn.example.com/x.js").as_deref(),
            Some("example.com")
        );
        assert_eq!(site_of_url("about:blank"), None);
    }

    #[test]
    fn keys_are_checked() {
        assert!(Keys::new("abcd1234", "efgh5678", "ijkl9012").is_ok());
        assert!(Keys::new("short", "efgh5678", "ijkl9012").is_err());
        assert!(Keys::new("abcd1234", "efgh'678", "ijkl9012").is_err());
        let k = Keys::new("tok12345", "sec12345", "frm12345").unwrap();
        let s = detection_script(&k);
        assert!(!s.contains("__WK_"));
        assert!(s.contains("'__wk_tok12345'") && s.contains("'sec12345'"));
        let d = drain_script(&k, "doc\"x", 7);
        assert!(d.contains("\"doc\\\"x\"") && d.contains(",7)"));
    }
}
