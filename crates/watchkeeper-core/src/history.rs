//! A short record per site, kept between launches, so the panel can say
//! "12 other sites last visit, 19 today".
//!
//! Each visit keeps its date, a few counts and the names of the other
//! sites the page contacted. Never addresses, cookie names, storage keys
//! or anything typed. The person can forget one site or everything.
//!
//! The file is written whole to a temporary file and then renamed over the
//! old one, so a crash leaves either the old or the new file. A file that
//! cannot be read is moved aside and a fresh history starts.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

use crate::Summary;

/// Visits kept per site.
pub const MAX_VISITS: usize = 20;
/// Sites kept. The ones visited longest ago go first.
pub const MAX_HISTORY_SITES: usize = 2_000;
/// Other-site names kept per visit.
pub const MAX_NAMES_PER_VISIT: usize = 300;
/// Longest site name kept, in bytes (the longest a domain name can be).
pub const MAX_NAME_BYTES: usize = 253;
/// Size the history is kept under when saved, in bytes (roughly). The
/// visits made longest ago go first.
pub const MAX_HISTORY_BYTES: usize = 8 << 20;
/// Largest history file read, in bytes. Well above what is ever saved.
pub const MAX_FILE_BYTES: u64 = 32 << 20;

/// One visit to a site.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Visit {
    /// Day of the visit, YYYY-MM-DD in UTC.
    pub date: String,
    /// How many other sites the page contacted.
    pub other_site_count: usize,
    /// Names of the other sites the engine itself saw contacted (at most
    /// MAX_NAMES_PER_VISIT). Names a page only used in calls that went
    /// nowhere are not kept.
    pub other_sites: Vec<String>,
    pub cookies: usize,
    pub storage_writes: usize,
    /// Fingerprinting reads, as "api by site".
    pub fingerprinting: Vec<String>,
}

impl Visit {
    /// A visit as it stands now, from the panel's summary.
    pub fn from_summary(s: &Summary, date: &str) -> Visit {
        Visit {
            date: date.to_string(),
            other_site_count: s.third_party_sites.len(),
            other_sites: s
                .confirmed_sites
                .iter()
                .filter(|n| n.len() <= MAX_NAME_BYTES)
                .take(MAX_NAMES_PER_VISIT)
                .cloned()
                .collect(),
            cookies: s.first_party_cookies + s.third_party_cookies,
            storage_writes: s.storage_writes,
            fingerprinting: s
                .fingerprinting_by
                .iter()
                .filter(|f| f.api.len() <= 16 && f.by.len() <= MAX_NAME_BYTES)
                .take(64)
                .map(|f| format!("{} by {}", f.api, f.by))
                .collect(),
        }
    }
}

/// What the panel shows about the last visit before this one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Comparison {
    /// Date of the last visit.
    pub date: String,
    pub other_site_count: usize,
    pub cookies: usize,
    /// Sites contacted now that were not contacted then.
    pub new_sites: Vec<String>,
    /// Sites contacted then that were not contacted now.
    pub gone_sites: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct History {
    /// Format version of the file.
    #[serde(default)]
    pub version: u32,
    /// Visits per site, oldest first.
    #[serde(default)]
    pub sites: BTreeMap<String, VecDeque<Visit>>,
}

impl History {
    /// Reads the history file. A missing file gives an empty history. A
    /// file that cannot be read is renamed to `<name>.bad` and an empty
    /// history is returned along with a note saying so.
    pub fn load(path: &Path) -> (History, Option<String>) {
        // A save cut short leaves its temporary file behind.
        let mut tmp = path.as_os_str().to_owned();
        tmp.push(".tmp");
        let _ = std::fs::remove_file(PathBuf::from(tmp));
        let meta = match std::fs::metadata(path) {
            Ok(m) => m,
            Err(_) => return (History::new(), None),
        };
        let bad = |why: String| {
            // A name of its own, so an earlier copy set aside is kept.
            let mut aside = path.as_os_str().to_owned();
            let secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            aside.push(format!(".bad-{secs}"));
            let _ = std::fs::rename(path, PathBuf::from(aside));
            (History::new(), Some(why))
        };
        if meta.len() > MAX_FILE_BYTES {
            return bad("history file too large; set aside".into());
        }
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => return bad(format!("history file unreadable ({e}); set aside")),
        };
        match serde_json::from_str::<History>(&text) {
            Ok(mut h) => {
                h.trim();
                (h, None)
            }
            Err(e) => bad(format!("history file damaged ({e}); set aside")),
        }
    }

    pub fn new() -> History {
        History {
            version: 1,
            sites: BTreeMap::new(),
        }
    }

    /// Writes the file: to `<name>.tmp` first, then renamed over the old.
    /// Call `trim` first to keep it within MAX_HISTORY_BYTES.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut tmp = path.as_os_str().to_owned();
        tmp.push(".tmp");
        let tmp = PathBuf::from(tmp);
        let text = serde_json::to_string(self).map_err(std::io::Error::other)?;
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, path)
    }

    /// Starts a new visit to `site`.
    pub fn push(&mut self, site: &str, visit: Visit) {
        if site.is_empty() {
            return;
        }
        let visits = self.sites.entry(site.to_string()).or_default();
        visits.push_back(visit);
        while visits.len() > MAX_VISITS {
            visits.pop_front();
        }
        self.trim();
    }

    /// Replaces the latest visit to `site`, or starts one.
    pub fn update_last(&mut self, site: &str, visit: Visit) {
        match self.sites.get_mut(site).and_then(|v| v.back_mut()) {
            Some(last) => *last = visit,
            None => self.push(site, visit),
        }
    }

    /// The visit before the latest one.
    pub fn previous(&self, site: &str) -> Option<&Visit> {
        let v = self.sites.get(site)?;
        if v.len() < 2 {
            return None;
        }
        v.get(v.len() - 2)
    }

    /// Compares the latest visit with the one before.
    pub fn compare(&self, site: &str) -> Option<Comparison> {
        let prev = self.previous(site)?;
        let now = self.sites.get(site)?.back()?;
        let then: BTreeSet<&String> = prev.other_sites.iter().collect();
        let current: BTreeSet<&String> = now.other_sites.iter().collect();
        Some(Comparison {
            date: prev.date.clone(),
            other_site_count: prev.other_site_count,
            cookies: prev.cookies,
            new_sites: current.difference(&then).map(|s| s.to_string()).collect(),
            gone_sites: then.difference(&current).map(|s| s.to_string()).collect(),
        })
    }

    /// Forgets one site.
    pub fn forget(&mut self, site: &str) -> bool {
        self.sites.remove(site).is_some()
    }

    /// Forgets everything.
    pub fn forget_all(&mut self) {
        self.sites.clear();
    }

    /// Roughly how many bytes the saved file takes.
    pub fn approx_bytes(&self) -> usize {
        self.sites
            .iter()
            .map(|(k, v)| {
                k.len()
                    + 8
                    + v.iter()
                        .map(|x| {
                            120 + x.other_sites.iter().map(|n| n.len() + 3).sum::<usize>()
                                + x.fingerprinting.iter().map(|n| n.len() + 3).sum::<usize>()
                        })
                        .sum::<usize>()
            })
            .sum()
    }

    /// Keeps within the limits: visits per site, names per visit, sites,
    /// and the size budget, dropping what was visited longest ago.
    pub fn trim(&mut self) {
        for v in self.sites.values_mut() {
            while v.len() > MAX_VISITS {
                v.pop_front();
            }
            for visit in v.iter_mut() {
                visit.other_sites.retain(|n| n.len() <= MAX_NAME_BYTES);
                visit.other_sites.truncate(MAX_NAMES_PER_VISIT);
                visit
                    .fingerprinting
                    .retain(|n| n.len() <= MAX_NAME_BYTES + 20);
                visit.fingerprinting.truncate(64);
            }
        }
        self.sites.retain(|k, _| k.len() <= MAX_NAME_BYTES);
        let mut size = self.approx_bytes();
        while size > MAX_HISTORY_BYTES {
            // The site whose oldest visit is oldest gives that visit up.
            let Some(key) = self
                .sites
                .iter()
                .filter_map(|(k, v)| v.front().map(|x| (x.date.clone(), k.clone())))
                .min()
                .map(|(_, k)| k)
            else {
                break;
            };
            let Some(v) = self.sites.get_mut(&key) else {
                break;
            };
            if let Some(x) = v.pop_front() {
                size = size.saturating_sub(
                    120 + x.other_sites.iter().map(|n| n.len() + 3).sum::<usize>()
                        + x.fingerprinting.iter().map(|n| n.len() + 3).sum::<usize>(),
                );
            }
            if v.is_empty() {
                self.sites.remove(&key);
                size = size.saturating_sub(key.len() + 8);
            }
        }
        self.sites.retain(|k, v| !k.is_empty() && !v.is_empty());
        if self.sites.len() > MAX_HISTORY_SITES {
            let mut by_date: Vec<(String, String)> = self
                .sites
                .iter()
                .map(|(k, v)| {
                    (
                        v.back().map(|x| x.date.clone()).unwrap_or_default(),
                        k.clone(),
                    )
                })
                .collect();
            by_date.sort();
            let excess = self.sites.len() - MAX_HISTORY_SITES;
            for (_, k) in by_date.into_iter().take(excess) {
                self.sites.remove(&k);
            }
        }
    }
}

/// Today's date as YYYY-MM-DD in UTC.
pub fn today() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    date_from_days((secs / 86_400) as i64)
}

/// Days since 1970-01-01 to a calendar date (proleptic Gregorian).
pub fn date_from_days(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}
