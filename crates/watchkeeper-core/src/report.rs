//! A saved report of everything Watchkeeper saw on one site: a JSON file
//! for tools and a single HTML file for people.
//!
//! Every name and address in a report came from the page, so the HTML
//! escapes all of it, carries no script and forbids loading anything
//! (Content-Security-Policy default-src 'none'). Addresses are shown as
//! text, never as links. Files are created new, never overwritten.

use serde::Serialize;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::history::{date_from_days, Comparison};
use crate::{site_of_host, site_of_url, FingerprintRead, SiteReport, Summary};

/// Name of the JSON format, and its version.
pub const FORMAT: &str = "watchkeeper-report";
pub const FORMAT_VERSION: u32 = 1;

/// What Watchkeeper cannot see, said in every report.
pub const LIMITS: &[&str] = &[
    "Requests made from inside workers and service workers are not seen; a worker starting is.",
    "A beacon sent while leaving a page can be missed.",
    "An engine can hold back some details of frames from other sites.",
    "A page can add false events about itself, and a flood of activity can push some addresses out; the dropped count says how many.",
    "The site named for a fingerprinting read is the nearest script on the call stack, not proof that script chose to read.",
    "Each engine blocks some third-party cookies on its own, so counts differ between Windows, macOS and Linux.",
];

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Address {
    pub address: String,
    /// How it was reached: script, img, fetch, xhr, beacon, websocket...
    pub kinds: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct OtherSite {
    pub site: String,
    /// The engine's own resource list showed this site being contacted.
    pub confirmed: bool,
    pub addresses: Vec<Address>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Cookie {
    pub name: String,
    pub domain: String,
    pub http_only: bool,
    /// "this site" or "other site" (one the engine saw this page contact).
    pub belongs_to: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Named {
    /// Site of the frame that did it.
    pub site: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct StorageWrite {
    pub site: String,
    /// localStorage, sessionStorage or indexedDB.
    pub kind: String,
    pub key: String,
}

/// Everything seen on one site.
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub format: &'static str,
    pub format_version: u32,
    /// Watchkeeper's version and the system it ran on.
    pub watchkeeper: String,
    pub system: String,
    /// When the report was saved, in UTC.
    pub saved: String,
    pub site: String,
    /// The page's address without query or fragment.
    pub page_address: String,
    /// The counts the side panel shows.
    pub summary: Summary,
    pub other_sites: Vec<OtherSite>,
    /// Addresses on the site itself.
    pub own_addresses: Vec<Address>,
    /// Addresses on sites past the site limit, counted as dropped and not
    /// listed.
    pub unlisted_addresses: usize,
    /// Cookies in the session's store for this site and the other sites
    /// the engine saw it contact.
    pub cookies: Vec<Cookie>,
    /// Every other cookie in the store, left by earlier sites in the
    /// session: counted, never named, so a report says nothing about
    /// other sites the person visited. Naming a site in a call that goes
    /// nowhere does not bring its cookies into a report.
    pub earlier_cookies: usize,
    pub script_cookies: Vec<Named>,
    pub storage_writes: Vec<StorageWrite>,
    pub fingerprinting: Vec<FingerprintRead>,
    /// Addresses of workers started.
    pub workers: Vec<String>,
    /// Events the page made that could not be counted.
    pub dropped: u64,
    /// The last visit before this one, when history is kept.
    pub last_visit: Option<Comparison>,
    pub cannot_see: Vec<&'static str>,
}

impl Report {
    /// A report of what `r` holds, saved at `secs` (Unix time).
    pub fn new(
        r: &SiteReport,
        last_visit: Option<Comparison>,
        watchkeeper: &str,
        system: &str,
        secs: u64,
    ) -> Report {
        let mut by_site: BTreeMap<String, Vec<Address>> = BTreeMap::new();
        let mut own = Vec::new();
        let mut workers = Vec::new();
        for (u, kinds) in &r.requests {
            let a = Address {
                address: u.clone(),
                kinds: kinds.iter().cloned().collect(),
            };
            if kinds.contains("worker") {
                workers.push(u.clone());
            }
            match site_of_url(u) {
                Some(s) if s == r.site => own.push(a),
                Some(s) => by_site.entry(s).or_default().push(a),
                None => own.push(a),
            }
        }
        // The same sites the panel lists. Addresses on sites past the
        // site limit were counted as dropped and are left out here too.
        let other_sites = r
            .other_sites
            .iter()
            .filter(|s| **s != r.site)
            .map(|site| OtherSite {
                confirmed: r.confirmed_sites.contains(site),
                addresses: by_site.remove(site).unwrap_or_default(),
                site: site.clone(),
            })
            .collect();
        let unlisted_addresses = by_site.values().map(|v| v.len()).sum();
        let mut cookies = Vec::new();
        let mut earlier_cookies = 0;
        for c in &r.cookies {
            let site = site_of_host(c.domain.trim_start_matches('.'));
            let belongs_to = if site == r.site {
                "this site"
            } else if r.confirmed_sites.contains(&site) {
                "other site"
            } else {
                earlier_cookies += 1;
                continue;
            };
            cookies.push(Cookie {
                name: c.name.clone(),
                domain: c.domain.clone(),
                http_only: c.http_only,
                belongs_to: belongs_to.to_string(),
            });
        }
        Report {
            format: FORMAT,
            format_version: FORMAT_VERSION,
            watchkeeper: watchkeeper.to_string(),
            system: system.to_string(),
            saved: utc_stamp(secs),
            site: r.site.clone(),
            page_address: r.page_url.clone(),
            summary: r.summary(),
            other_sites,
            own_addresses: own,
            unlisted_addresses,
            cookies,
            earlier_cookies,
            script_cookies: r
                .script_cookies
                .iter()
                .map(|(site, name)| Named {
                    site: site.clone(),
                    name: name.clone(),
                })
                .collect(),
            storage_writes: r
                .storage
                .iter()
                .map(|(site, kind, key)| StorageWrite {
                    site: site.clone(),
                    kind: kind.clone(),
                    key: key.clone(),
                })
                .collect(),
            fingerprinting: r.fingerprinting_by(),
            workers,
            dropped: r.dropped,
            last_visit,
            cannot_see: LIMITS.to_vec(),
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }

    pub fn to_html(&self) -> String {
        let mut h = String::with_capacity(16 * 1024);
        let site = if self.site.is_empty() {
            "no site"
        } else {
            &self.site
        };
        h.push_str("<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n");
        h.push_str("<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'\">\n");
        h.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
        h.push_str(&format!(
            "<title>Watchkeeper report for {}</title>\n",
            esc(site)
        ));
        h.push_str(STYLE);
        h.push_str("</head>\n<body>\n<main>\n");
        h.push_str(&format!(
            "<p class=\"kicker\">Watchkeeper report</p>\n<h1>{}</h1>\n",
            esc(site)
        ));
        h.push_str(&format!(
            "<p class=\"meta\">Page {}<br>Saved {} by Watchkeeper {} on {}</p>\n",
            esc(&self.page_address),
            esc(&self.saved),
            esc(&self.watchkeeper),
            esc(&self.system)
        ));

        let s = &self.summary;
        h.push_str("<section class=\"counts\">\n");
        count(
            &mut h,
            s.third_party_sites.len(),
            "other sites told about the visit",
        );
        count(&mut h, s.addresses, "addresses requested");
        count(&mut h, s.first_party_cookies, "cookies for this site");
        count(&mut h, s.third_party_cookies, "cookies for other sites");
        count(&mut h, s.storage_writes, "storage writes");
        count(
            &mut h,
            s.fingerprinting.len(),
            "kinds of fingerprinting read",
        );
        h.push_str("</section>\n");
        if self.dropped > 0 {
            h.push_str(&format!(
                "<p class=\"warn\">The page did {} more things than Watchkeeper could count. The list of other sites is kept apart and stays complete.</p>\n",
                self.dropped
            ));
        }
        if let Some(lv) = &self.last_visit {
            h.push_str(&format!(
                "<p>Last visit, {}: {} other sites. This visit: {}.{}</p>\n",
                esc(&lv.date),
                lv.other_site_count,
                s.third_party_sites.len(),
                if lv.new_sites.is_empty() {
                    String::new()
                } else {
                    format!(" New this time: {}.", esc(&lv.new_sites.join(", ")))
                }
            ));
        }

        h.push_str("<h2>Other sites told about the visit</h2>\n");
        if self.other_sites.is_empty() {
            h.push_str("<p>None.</p>\n");
        }
        for o in &self.other_sites {
            h.push_str(&format!(
                "<details><summary><b>{}</b> <span class=\"muted\">{} address{}{}</span></summary>\n<ul>\n",
                esc(&o.site),
                o.addresses.len(),
                if o.addresses.len() == 1 { "" } else { "es" },
                if o.confirmed {
                    ""
                } else {
                    ", named in calls the engine did not list"
                }
            ));
            for a in &o.addresses {
                address(&mut h, a);
            }
            h.push_str("</ul></details>\n");
        }

        if self.unlisted_addresses > 0 {
            h.push_str(&format!(
                "<p class=\"muted\">{} more addresses were on sites past the limit of {} other sites. They are counted above but not listed.</p>\n",
                self.unlisted_addresses,
                crate::MAX_SITES
            ));
        }

        h.push_str("<h2>Fingerprinting reads</h2>\n");
        list(
            &mut h,
            self.fingerprinting
                .iter()
                .map(|f| format!("{} by {}", f.api, f.by)),
        );

        h.push_str("<h2>Cookies in the session's store</h2>\n");
        if self.cookies.is_empty() {
            h.push_str("<p>None.</p>\n");
        } else {
            h.push_str("<table><tr><th>Name</th><th>Domain</th><th>Belongs to</th><th>Hidden from scripts</th></tr>\n");
            for c in &self.cookies {
                h.push_str(&format!(
                    "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
                    esc(&c.name),
                    esc(&c.domain),
                    esc(&c.belongs_to),
                    if c.http_only { "yes" } else { "no" }
                ));
            }
            h.push_str("</table>\n");
        }
        if self.earlier_cookies > 0 {
            h.push_str(&format!(
                "<p class=\"muted\">{} more cookies in the store were left by earlier sites in the session. They are not named here.</p>\n",
                self.earlier_cookies
            ));
        }

        h.push_str("<h2>Cookies written by scripts</h2>\n");
        list(
            &mut h,
            self.script_cookies
                .iter()
                .map(|c| format!("{} (by {})", c.name, c.site)),
        );

        h.push_str("<h2>Storage writes</h2>\n");
        list(
            &mut h,
            self.storage_writes
                .iter()
                .map(|w| format!("{} in {} (by {})", w.key, w.kind, w.site)),
        );

        h.push_str("<h2>Workers started</h2>\n");
        list(&mut h, self.workers.iter().cloned());

        h.push_str("<h2>Addresses on the site itself</h2>\n");
        if self.own_addresses.is_empty() {
            h.push_str("<p>None.</p>\n");
        } else {
            h.push_str("<ul>\n");
            for a in &self.own_addresses {
                address(&mut h, a);
            }
            h.push_str("</ul>\n");
        }

        h.push_str("<h2>What Watchkeeper cannot see</h2>\n");
        list(&mut h, self.cannot_see.iter().map(|s| s.to_string()));
        h.push_str("<p class=\"muted\">Addresses are kept without query strings. Names only, never cookie or storage values.</p>\n");
        h.push_str("</main>\n</body>\n</html>\n");
        h
    }
}

const STYLE: &str = "<style>
body { margin: 0; font: 15px/1.5 system-ui, sans-serif; color: #1d1f23; background: #fff; }
main { max-width: 860px; margin: 0 auto; padding: 24px 16px 48px; }
h1 { margin: 0 0 6px; font-size: 28px; word-break: break-all; }
h2 { margin: 28px 0 8px; font-size: 18px; }
.kicker { margin: 0; text-transform: uppercase; letter-spacing: .06em; font-size: 12px; color: #6b7078; }
.meta, .muted { color: #6b7078; font-size: 13px; word-break: break-all; }
.counts { display: flex; flex-wrap: wrap; gap: 10px; margin: 16px 0; }
.counts div { border: 1px solid #e3e5e8; border-radius: 8px; padding: 8px 12px; min-width: 120px; }
.counts b { display: block; font-size: 24px; color: #b4321f; }
.warn { color: #b4321f; }
ul { padding-left: 20px; margin: 6px 0; }
li { word-break: break-all; }
details { border-bottom: 1px solid #e3e5e8; padding: 6px 0; }
table { border-collapse: collapse; width: 100%; font-size: 13px; }
th, td { text-align: left; border-bottom: 1px solid #e3e5e8; padding: 4px 6px; word-break: break-all; }
@media (prefers-color-scheme: dark) {
  body { color: #e6e7e9; background: #17181b; }
  .counts div, details, th, td { border-color: #33363c; }
  .counts b, .warn { color: #ff8a73; }
}
</style>
";

fn count(h: &mut String, n: usize, label: &str) {
    h.push_str(&format!("<div><b>{n}</b>{}</div>\n", esc(label)));
}

fn address(h: &mut String, a: &Address) {
    h.push_str(&format!(
        "<li>{} <span class=\"muted\">{}</span></li>\n",
        esc(&a.address),
        esc(&a.kinds.join(", "))
    ));
}

fn list(h: &mut String, items: impl Iterator<Item = String>) {
    let items: Vec<String> = items.collect();
    if items.is_empty() {
        h.push_str("<p>None.</p>\n");
        return;
    }
    h.push_str("<ul>\n");
    for i in items {
        h.push_str(&format!("<li>{}</li>\n", esc(&i)));
    }
    h.push_str("</ul>\n");
}

/// Escapes text for HTML element content and attribute values.
pub fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c if (c as u32) < 0x20 && c != '\n' && c != '\t' => out.push('\u{fffd}'),
            // C1 controls, and characters that reorder or hide text
            // (direction overrides, zero-width marks), so a page cannot
            // make its names read misleadingly.
            '\u{7f}'..='\u{9f}'
            | '\u{200b}'..='\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2060}'..='\u{206f}'
            | '\u{061c}'
            | '\u{180e}'
            | '\u{2028}'..='\u{2029}'
            | '\u{feff}'
            | '\u{fff9}'..='\u{fffb}'
            | '\u{e0000}'..='\u{e007f}' => out.push('\u{fffd}'),
            c => out.push(c),
        }
    }
    out
}

/// Unix time as "2026-10-23 14:08:05 UTC".
pub fn utc_stamp(secs: u64) -> String {
    let t = secs % 86_400;
    format!(
        "{} {:02}:{:02}:{:02} UTC",
        date_from_days((secs / 86_400) as i64),
        t / 3600,
        t % 3600 / 60,
        t % 60
    )
}

/// The start of a report's file names, from the site and the time:
/// "watchkeeper-example.com-2026-10-23-140805". Only letters, digits,
/// dots and hyphens survive from the site.
pub fn file_stem(site: &str, secs: u64) -> String {
    let mut clean: String = site
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .take(100)
        .collect();
    clean = clean.trim_matches('.').to_string();
    if clean.is_empty() {
        clean = "page".to_string();
    }
    let t = secs % 86_400;
    format!(
        "watchkeeper-{clean}-{}-{:02}{:02}{:02}",
        date_from_days((secs / 86_400) as i64),
        t / 3600,
        t % 3600 / 60,
        t % 60
    )
}

/// Writes `<stem>.html` and `<stem>.json` into `dir`, creating it if
/// needed. Never overwrites: a stem already taken gets "-2", "-3"...
pub fn write_files(dir: &Path, report: &Report, stem: &str) -> std::io::Result<(PathBuf, PathBuf)> {
    std::fs::create_dir_all(dir)?;
    let html = report.to_html();
    let json = report.to_json();
    for n in 1..1000 {
        let base = if n == 1 {
            stem.to_string()
        } else {
            format!("{stem}-{n}")
        };
        let hp = dir.join(format!("{base}.html"));
        let jp = dir.join(format!("{base}.json"));
        if hp.exists() || jp.exists() {
            continue;
        }
        let mut hf = match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&hp)
        {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        };
        let mut jf = match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&jp)
        {
            Ok(f) => f,
            Err(e) => {
                drop(hf);
                let _ = std::fs::remove_file(&hp);
                if e.kind() == std::io::ErrorKind::AlreadyExists {
                    continue;
                }
                return Err(e);
            }
        };
        let written = hf
            .write_all(html.as_bytes())
            .and_then(|_| hf.sync_all())
            .and_then(|_| jf.write_all(json.as_bytes()))
            .and_then(|_| jf.sync_all());
        if let Err(e) = written {
            // No half-written report is left behind.
            drop(hf);
            drop(jf);
            let _ = std::fs::remove_file(&hp);
            let _ = std::fs::remove_file(&jp);
            return Err(e);
        }
        return Ok((hp, jp));
    }
    Err(std::io::Error::other("no free file name"))
}
