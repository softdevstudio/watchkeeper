//! Shared core for Watchkeeper and, later, Unweb.
//!
//! Two parts live here, neither tied to a window toolkit:
//!
//! * the detection script injected into every frame of a page, with the
//!   small script the app runs to collect what it saw, and
//! * [`SiteReport`], which turns those events and the engine's cookie list
//!   into a per-site picture: which other sites were contacted, cookies,
//!   storage writes and fingerprinting reads.
//!
//! A "site" is a registrable domain, so `news.example.co.uk` and
//! `cdn.example.co.uk` are one site. IP addresses and `localhost` are
//! their own sites.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use url::Url;

const DETECT_TEMPLATE: &str = include_str!("../assets/detect.js");
const TOKEN_MARK: &str = "__WK_TOKEN__";

/// Most events kept per site, so a hostile page cannot grow memory forever.
pub const MAX_EVENTS_PER_SITE: usize = 20_000;

/// The detection script with this launch's token filled in.
///
/// The token must be ASCII letters and digits only; anything else is
/// rejected so it can never break out of the script's string literal.
pub fn detection_script(token: &str) -> Result<String, String> {
    if token.is_empty() || !token.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err("token must be ASCII letters and digits".into());
    }
    Ok(DETECT_TEMPLATE.replace(TOKEN_MARK, token))
}

/// Script that returns the buffered events as a JSON string and clears the
/// buffer. Returns `"[]"` when the detection script is not present.
pub fn drain_script(token: &str) -> Result<String, String> {
    detection_script(token)?;
    Ok(format!(
        "(function(){{try{{var b=window['__wk_{token}'];return b&&b.drain?b.drain():'[]';}}catch(e){{return '[]';}}}})()"
    ))
}

/// One thing the page did, as reported by the detection script.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "k")]
pub enum Event {
    /// A network request.
    #[serde(rename = "res")]
    Request {
        /// Absolute URL.
        u: String,
        /// How it was made: script, img, fetch, beacon, xhr, websocket...
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
    if host.is_empty() || host == "localhost" || host.parse::<std::net::IpAddr>().is_ok() {
        return host;
    }
    if host.starts_with('[') {
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

/// What one site did while you were on it.
#[derive(Debug, Clone, Default, Serialize)]
pub struct SiteReport {
    /// Registrable domain of the page.
    pub site: String,
    /// Last top-level URL seen for this site.
    pub page_url: String,
    /// Distinct request URLs, each with how it was made.
    pub requests: BTreeMap<String, BTreeSet<String>>,
    /// Cookie names written by script.
    pub script_cookies: BTreeSet<String>,
    /// Storage writes as (kind, key).
    pub storage: BTreeSet<(String, String)>,
    /// Fingerprinting reads: canvas, webgl, audio.
    pub fingerprinting: BTreeSet<String>,
    /// Cookies in the engine's store at the last refresh.
    pub cookies: Vec<StoredCookie>,
    /// Events dropped because the per-site cap was reached.
    pub dropped: usize,
    #[serde(skip)]
    events: usize,
}

/// Counts shown in the side panel.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Summary {
    pub site: String,
    pub page_url: String,
    /// Other sites the page contacted.
    pub third_party_sites: Vec<String>,
    /// Requests in total, and how many went to other sites.
    pub requests: usize,
    pub third_party_requests: usize,
    /// Cookies stored for this site, and for the other sites it contacted.
    pub first_party_cookies: usize,
    pub third_party_cookies: usize,
    /// Storage keys written.
    pub storage_writes: usize,
    /// Fingerprinting reads seen.
    pub fingerprinting: Vec<String>,
    pub dropped: usize,
}

impl SiteReport {
    /// A fresh report for a page URL.
    pub fn new(page_url: &str) -> Self {
        SiteReport {
            site: site_of_url(page_url).unwrap_or_default(),
            page_url: page_url.to_string(),
            ..Default::default()
        }
    }

    /// Whether moving to `url` stays on the same site.
    pub fn same_site(&self, url: &str) -> bool {
        site_of_url(url).is_some_and(|s| s == self.site)
    }

    /// Adds the JSON array returned by [`drain_script`]. Malformed input is
    /// rejected as a whole; unknown event kinds are skipped one by one.
    pub fn add_events_json(&mut self, json: &str) -> Result<usize, String> {
        // Some engines hand eval results back JSON-encoded a second time.
        let mut text = json.trim().to_string();
        if text.starts_with('"') {
            text = serde_json::from_str::<String>(&text).map_err(|e| e.to_string())?;
        }
        let values: Vec<serde_json::Value> =
            serde_json::from_str(&text).map_err(|e| e.to_string())?;
        let mut added = 0;
        for v in values {
            if let Ok(ev) = serde_json::from_value::<Event>(v) {
                self.add_event(ev);
                added += 1;
            }
        }
        Ok(added)
    }

    /// Adds one event.
    pub fn add_event(&mut self, ev: Event) {
        if self.events >= MAX_EVENTS_PER_SITE {
            self.dropped += 1;
            return;
        }
        self.events += 1;
        match ev {
            Event::Request { u, t, .. } => {
                if Url::parse(&u).is_ok() {
                    let kind = if t.is_empty() { "other".into() } else { t };
                    self.requests.entry(u).or_default().insert(kind);
                }
            }
            Event::Cookie { n, .. } => {
                self.script_cookies.insert(n);
            }
            Event::Store { s, n, .. } => {
                self.storage.insert((s, n));
            }
            Event::Fingerprint { a, .. } => {
                self.fingerprinting.insert(a);
            }
        }
    }

    /// Replaces the cookie list with the engine's current store.
    pub fn set_cookies(&mut self, cookies: Vec<StoredCookie>) {
        self.cookies = cookies;
    }

    /// Other sites this page contacted, sorted.
    pub fn third_party_sites(&self) -> BTreeSet<String> {
        self.requests
            .keys()
            .filter_map(|u| site_of_url(u))
            .filter(|s| *s != self.site)
            .collect()
    }

    /// The counts for the side panel.
    pub fn summary(&self) -> Summary {
        let third = self.third_party_sites();
        let third_party_requests = self
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
            } else if third.contains(&site) {
                third_party_cookies += 1;
            }
        }
        Summary {
            site: self.site.clone(),
            page_url: self.page_url.clone(),
            third_party_sites: third.into_iter().collect(),
            requests: self.requests.len(),
            third_party_requests,
            first_party_cookies,
            third_party_cookies,
            storage_writes: self.storage.len(),
            fingerprinting: self.fingerprinting.iter().cloned().collect(),
            dropped: self.dropped,
        }
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
    fn token_is_checked() {
        assert!(detection_script("abc123").is_ok());
        assert!(detection_script("").is_err());
        assert!(detection_script("a'b").is_err());
        assert!(drain_script("x;alert(1)").is_err());
        let s = detection_script("tok9").unwrap();
        assert!(!s.contains(TOKEN_MARK));
        assert!(s.contains("'tok9'"));
    }
}
