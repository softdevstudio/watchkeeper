//! Who runs a site: Watchkeeper's own list (assets/trackers.json), written
//! for this project from public facts and licensed with it under the
//! Apache License 2.0. A site not on the list has no name.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

const LIST: &str = include_str!("../assets/trackers.json");

/// The categories a service can have.
pub const CATEGORIES: &[&str] = &[
    "advertising",
    "analytics",
    "social",
    "session recording",
    "tag manager",
    "consent",
    "customer data",
    "marketing",
    "performance",
    "content",
    "hosting",
    "other",
];

/// What the list says about a site.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Known {
    pub name: String,
    pub company: String,
    pub category: String,
}

#[derive(Deserialize)]
struct File {
    format: String,
    format_version: u32,
    services: Vec<Service>,
}

#[derive(Deserialize)]
struct Service {
    name: String,
    company: String,
    category: String,
    sites: Vec<String>,
}

fn table() -> &'static HashMap<String, Known> {
    static TABLE: OnceLock<HashMap<String, Known>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let file: File = serde_json::from_str(LIST).expect("assets/trackers.json is valid");
        assert!(
            file.format == "watchkeeper-trackers" && file.format_version == 1,
            "assets/trackers.json has an unknown format"
        );
        let mut map = HashMap::new();
        for s in file.services {
            for site in s.sites {
                map.insert(
                    site,
                    Known {
                        name: s.name.clone(),
                        company: s.company.clone(),
                        category: s.category.clone(),
                    },
                );
            }
        }
        map
    })
}

/// Who runs `site` (a site as site_of_host gives it), if the list knows.
pub fn known(site: &str) -> Option<Known> {
    table().get(site).cloned()
}

/// How many sites the list names.
pub fn len() -> usize {
    table().len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_is_sound() {
        let file: File = serde_json::from_str(LIST).unwrap();
        assert_eq!(file.format, "watchkeeper-trackers");
        assert_eq!(file.format_version, 1);
        let mut seen = std::collections::HashSet::new();
        let mut not_sites = Vec::new();
        for s in &file.services {
            for site in &s.sites {
                if crate::site_of_host(site) != *site
                    || crate::site_of_host(&format!("www.{site}")) != *site
                {
                    not_sites.push(site.clone());
                }
            }
        }
        // Each entry must be a site as Watchkeeper reduces hosts, or it
        // could never match (googleapis.com, for one, is a public suffix).
        assert!(
            not_sites.is_empty(),
            "not sites as Watchkeeper reduces hosts: {not_sites:?}"
        );
        for s in &file.services {
            assert!(!s.name.is_empty() && !s.company.is_empty(), "{}", s.name);
            assert!(
                CATEGORIES.contains(&s.category.as_str()),
                "{}: {}",
                s.name,
                s.category
            );
            assert!(!s.sites.is_empty(), "{}", s.name);
            for site in &s.sites {
                assert!(seen.insert(site.clone()), "{site} is listed twice");
            }
        }
        assert_eq!(len(), seen.len());
        assert!(len() >= 150, "{}", len());
        let ga = known("google-analytics.com").unwrap();
        assert_eq!(
            (ga.company.as_str(), ga.category.as_str()),
            ("Google", "analytics")
        );
        assert_eq!(known("example.com"), None);
        assert_eq!(
            known(&crate::site_of_host("region1.google-analytics.com")),
            Some(ga)
        );
    }
}
