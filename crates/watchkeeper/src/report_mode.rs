//! Report mode: `watchkeeper --report <url>` opens the page in a hidden
//! window, watches it for a while, writes the same HTML and JSON report
//! the panel saves, and exits. Made for CI: an exit code says whether the
//! page contacted a site it shouldn't have.

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Url};

use crate::Shared;

pub const EXIT_WRITE: i32 = 1;
pub const EXIT_USAGE: i32 = 2;
pub const EXIT_NO_PAGE: i32 = 3;
pub const EXIT_NOT_ALLOWED: i32 = 4;

pub const USAGE: &str =
    "usage: watchkeeper --report <url> [--out DIR] [--wait SECONDS] [--allow FILE]
  --out DIR       where the HTML and JSON report go (default: this folder)
  --wait SECONDS  how long to watch the page after it has loaded (1 to 300, default 10)
  --allow FILE    the other sites the page may contact, one per line (# comments);
                  exit 4 if it contacts any other
exit codes: 0 report written, 1 report not written, 2 bad arguments,
            3 the page never loaded, 4 a site not on the allow list";

/// How long the page gets to finish loading.
const LOAD_TIMEOUT: Duration = Duration::from_secs(60);
/// The most an allow file may hold.
const MAX_ALLOW: usize = 10_000;

#[derive(Clone, Debug)]
pub struct Args {
    pub url: Url,
    pub out: PathBuf,
    pub wait: Duration,
    /// The sites the page may contact, when an allow file was given.
    pub allow: Option<Vec<String>>,
}

impl Args {
    /// Report mode's arguments, None when `--report` wasn't given.
    pub fn from_env() -> Result<Option<Args>, String> {
        Args::parse(std::env::args().skip(1).collect())
    }

    pub fn parse(args: Vec<String>) -> Result<Option<Args>, String> {
        if !args.iter().any(|a| a == "--report") {
            return Ok(None);
        }
        let mut url = None;
        let mut out = None;
        let mut wait = Duration::from_secs(10);
        let mut allow = None;
        let mut it = args.into_iter();
        while let Some(a) = it.next() {
            let mut value = |name: &str| it.next().ok_or(format!("{name} needs a value"));
            match a.as_str() {
                "--report" => {
                    let v = value("--report")?;
                    url = Some(crate::normalize_url(&v)?);
                }
                "--out" => out = Some(PathBuf::from(value("--out")?)),
                "--wait" => {
                    let v = value("--wait")?;
                    let n: u64 = v
                        .parse()
                        .map_err(|_| format!("--wait takes whole seconds, not {v:?}"))?;
                    if !(1..=300).contains(&n) {
                        return Err("--wait is 1 to 300 seconds".into());
                    }
                    wait = Duration::from_secs(n);
                }
                "--allow" => {
                    let path = value("--allow")?;
                    let text = std::fs::read_to_string(&path)
                        .map_err(|e| format!("can't read the allow file {path}: {e}"))?;
                    allow = Some(parse_allow(&text)?);
                }
                "--no-history" => {}
                other => return Err(format!("{other:?} isn't a report mode option")),
            }
        }
        let url = url.ok_or("--report needs a web address")?;
        let out = match out {
            Some(o) => o,
            None => std::env::current_dir().map_err(|e| format!("no current folder: {e}"))?,
        };
        std::fs::create_dir_all(&out)
            .map_err(|e| format!("can't make the folder {}: {e}", out.display()))?;
        Ok(Some(Args {
            url,
            out,
            wait,
            allow,
        }))
    }
}

/// An allow file: one site per line, `#` starts a comment. Each entry is
/// reduced to its site, so `www.example.com` allows `example.com`.
pub fn parse_allow(text: &str) -> Result<Vec<String>, String> {
    let mut sites = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let entry = line.split('#').next().unwrap_or("").trim();
        if entry.is_empty() {
            continue;
        }
        if entry.contains(char::is_whitespace) || entry.contains('/') {
            return Err(format!(
                "allow file line {}: one site per line, like example.com",
                n + 1
            ));
        }
        let site = watchkeeper_core::site_of_host(&entry.to_ascii_lowercase());
        if site.is_empty() {
            return Err(format!("allow file line {}: not a site", n + 1));
        }
        sites.push(site);
        if sites.len() > MAX_ALLOW {
            return Err(format!("an allow file holds at most {MAX_ALLOW} sites"));
        }
    }
    Ok(sites)
}

/// The sites contacted that the allow list doesn't name.
pub fn not_allowed(contacted: &[String], allow: &[String]) -> Vec<String> {
    contacted
        .iter()
        .filter(|s| !allow.contains(s))
        .cloned()
        .collect()
}

/// Watches the page off the main thread, writes the report and exits.
pub fn run(app: AppHandle, shared: Arc<Shared>, args: Args) {
    std::thread::spawn(move || {
        let code = watch(&shared, &args);
        app.exit(code);
    });
}

fn watch(shared: &Shared, args: &Args) -> i32 {
    let started = Instant::now();
    let loaded = loop {
        if shared
            .loaded_at
            .lock()
            .map(|l| l.is_some())
            .unwrap_or(false)
        {
            break true;
        }
        if started.elapsed() > LOAD_TIMEOUT {
            break false;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    if !loaded {
        eprintln!(
            "watchkeeper: the page didn't finish loading in {} s; reporting what was seen",
            LOAD_TIMEOUT.as_secs()
        );
    }
    std::thread::sleep(args.wait);
    // Three more collections, so the last events and the cookie store
    // (read every third round) are in.
    let from = shared.drains.load(Ordering::SeqCst);
    let until = Instant::now() + Duration::from_secs(15);
    while shared.drains.load(Ordering::SeqCst) < from + 3 && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(100));
    }
    let contacted = shared
        .session
        .lock()
        .map(|s| s.report.summary().third_party_sites)
        .unwrap_or_default();
    let saved = match shared.save_report() {
        Ok(s) => s,
        Err(e) if e.contains("no page") => {
            eprintln!("watchkeeper: {e}");
            return EXIT_NO_PAGE;
        }
        Err(e) => {
            eprintln!("watchkeeper: {e}");
            return EXIT_WRITE;
        }
    };
    let folder = PathBuf::from(&saved.folder);
    println!("{}", folder.join(&saved.html).display());
    println!("{}", folder.join(&saved.json).display());
    if let Some(allow) = &args.allow {
        let extra = not_allowed(&contacted, allow);
        if !extra.is_empty() {
            for s in &extra {
                eprintln!("watchkeeper: not on the allow list: {s}");
            }
            return EXIT_NOT_ALLOWED;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Result<Option<Args>, String> {
        Args::parse(list.iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn arguments() {
        assert!(args(&["example.com"]).unwrap().is_none());
        let dir = std::env::temp_dir().join(format!("wk-report-args-{}", std::process::id()));
        let d = dir.to_string_lossy().to_string();
        let a = args(&["--report", "example.com", "--out", &d, "--wait", "3"])
            .unwrap()
            .unwrap();
        assert_eq!(a.url.as_str(), "https://example.com/");
        assert_eq!(a.wait, Duration::from_secs(3));
        assert!(dir.is_dir());
        for bad in [
            vec!["--report"],
            vec!["--report", "file:///etc/passwd"],
            vec!["--report", "example.com", "--wait", "0"],
            vec!["--report", "example.com", "--wait", "301"],
            vec!["--report", "example.com", "--wait", "2.5"],
            vec!["--report", "example.com", "--colour"],
            vec!["--report", "example.com", "--allow", "/no/such/file"],
        ] {
            assert!(args(&bad).is_err(), "{bad:?}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn allow_files() {
        let a = parse_allow(
            "# trackers we accept\nwww.Google-Analytics.com\n\ncdn.example.co.uk # our CDN\n",
        )
        .unwrap();
        assert_eq!(a, vec!["google-analytics.com", "example.co.uk"]);
        assert!(parse_allow("two sites.com here.com").is_err());
        assert!(parse_allow("https://x.com/").is_err());
        assert_eq!(
            not_allowed(&["a.com".into(), "b.com".into()], &["a.com".into()]),
            vec!["b.com".to_string()]
        );
    }
}
