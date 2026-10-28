// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 rubble-za. See the LICENSE and NOTICE files.

//! Checking GitHub for a newer release.
//!
//! The releases are read from GitHub's API. For a private repository that is
//! refused to anonymous requests, so the check falls back to the `gh` CLI
//! when it is installed and signed in.

use std::process::Command;
use std::time::Duration;

use serde::Deserialize;

const REPO: &str = "rubble-za/WiFi-Spanner";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateInfo {
    /// Release tag, e.g. `v0.3.1`.
    pub tag: String,
    /// The release's page, where the AppImage can be downloaded.
    pub url: String,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
}

/// Look for a release newer than `current` (a version like `0.3.0`).
/// `Ok(None)` means this is the newest.
pub fn check(current: &str) -> Result<Option<UpdateInfo>, String> {
    let json = fetch_public().or_else(|public_err| {
        fetch_with_gh().map_err(|gh_err| {
            format!("GitHub did not share the release list ({public_err}), and the GitHub CLI could not ({gh_err})")
        })
    })?;
    newest_after(&json, current)
}

fn fetch_public() -> Result<String, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(8))).build().into();
    let mut response = agent
        .get(format!("https://api.github.com/repos/{REPO}/releases?per_page=30"))
        .header("User-Agent", concat!("wifispanner/", env!("CARGO_PKG_VERSION")))
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| e.to_string())?;
    response.body_mut().read_to_string().map_err(|e| e.to_string())
}

fn fetch_with_gh() -> Result<String, String> {
    let out = Command::new("gh")
        .args(["api", &format!("repos/{REPO}/releases?per_page=30")])
        .output()
        .map_err(|e| format!("gh not available: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// `0.3.1`, `v0.3.1` or `v0.3.1-rc1` as comparable numbers.
fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let core = s.trim().trim_start_matches('v').split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let mut next = || parts.next()?.parse::<u64>().ok();
    let v = (next()?, next()?, next()?);
    parts.next().is_none().then_some(v)
}

/// The highest published release, if it is newer than `current`. Drafts are
/// ignored; pre-releases count, since every release so far is one.
fn newest_after(json: &str, current: &str) -> Result<Option<UpdateInfo>, String> {
    let current = parse_version(current).ok_or_else(|| format!("unrecognised app version {current:?}"))?;
    let releases: Vec<Release> = serde_json::from_str(json).map_err(|e| format!("unexpected reply from GitHub: {e}"))?;
    Ok(releases
        .into_iter()
        .filter(|r| !r.draft)
        .filter_map(|r| Some((parse_version(&r.tag_name)?, r)))
        .max_by_key(|(v, _)| *v)
        .filter(|(v, _)| *v > current)
        .map(|(_, r)| UpdateInfo { tag: r.tag_name, url: r.html_url }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const JSON: &str = r#"[
        {"tag_name":"v0.2.0","html_url":"https://example.test/0.2.0","draft":false,"prerelease":true},
        {"tag_name":"v0.10.0","html_url":"https://example.test/0.10.0","draft":false,"prerelease":true},
        {"tag_name":"v9.9.9","html_url":"https://example.test/draft","draft":true,"prerelease":false},
        {"tag_name":"nightly","html_url":"https://example.test/nightly","draft":false,"prerelease":true},
        {"tag_name":"v0.1.1","html_url":"https://example.test/0.1.1","draft":false,"prerelease":true}
    ]"#;

    #[test]
    fn picks_the_highest_release_comparing_numbers_not_text() {
        let got = newest_after(JSON, "0.3.0").unwrap().unwrap();
        assert_eq!(got, UpdateInfo { tag: "v0.10.0".into(), url: "https://example.test/0.10.0".into() });
    }

    #[test]
    fn nothing_newer_means_up_to_date() {
        assert_eq!(newest_after(JSON, "0.10.0"), Ok(None));
        assert_eq!(newest_after(JSON, "1.0.0"), Ok(None));
    }

    #[test]
    fn drafts_and_odd_tags_are_ignored() {
        // v9.9.9 is a draft and "nightly" is not a version.
        assert!(newest_after(JSON, "0.0.1").unwrap().is_some_and(|u| u.tag == "v0.10.0"));
    }

    #[test]
    fn version_parsing() {
        assert_eq!(parse_version("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("1.2.3-rc1"), Some((1, 2, 3)));
        assert_eq!(parse_version("1.2"), None);
        assert_eq!(parse_version("1.2.3.4"), None);
        assert_eq!(parse_version("nightly"), None);
    }

    #[test]
    fn bad_replies_are_errors() {
        assert!(newest_after("{\"message\":\"Not Found\"}", "0.1.0").is_err());
        assert!(newest_after("[]", "bogus").is_err());
    }

    /// Talks to GitHub: run with `cargo test -- --ignored live_check`.
    #[test]
    #[ignore]
    fn live_check() {
        eprintln!("public: {:?}", fetch_public().map(|b| b.len()));
        eprintln!("gh: {:?}", fetch_with_gh().map(|b| b.len()));
        let old = check("0.1.0").unwrap().expect("an older version should see a newer release");
        eprintln!("from 0.1.0 the newest is {old:?}");
        eprintln!("from 99.0.0: {:?}", check("99.0.0"));
    }
}
