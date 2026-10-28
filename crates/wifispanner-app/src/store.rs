// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 rubble-za. See the LICENSE and NOTICE files.

//! Where projects and their reports live on disk, and small file helpers.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::SystemTime;

pub const EXTENSION: &str = "wifispanner";

/// `$XDG_DATA_HOME/wifispanner/projects`, falling back to
/// `~/.local/share/wifispanner/projects`.
pub fn projects_dir() -> PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("wifispanner").join("projects")
}

/// `$XDG_CONFIG_HOME/wifispanner`, falling back to `~/.config/wifispanner`.
pub fn config_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("wifispanner")
}

/// A path in the projects folder for a new project, not clashing with an
/// existing file. The name is reduced to safe filename characters.
pub fn new_project_path(name: &str) -> std::io::Result<PathBuf> {
    let dir = projects_dir();
    std::fs::create_dir_all(&dir)?;
    let stem = sanitize(name);
    let mut path = dir.join(format!("{stem}.{EXTENSION}"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{stem}-{n}.{EXTENSION}"));
        n += 1;
    }
    Ok(path)
}

fn sanitize(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || matches!(c, '-' | '_' | ' ') { c } else { '_' })
        .collect();
    let s = s.trim().trim_matches('.').to_string();
    if s.is_empty() { "survey".into() } else { s }
}

pub fn project_files() -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(projects_dir()) else { return Vec::new() };
    rd.filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == EXTENSION))
        .collect()
}

/// Reports for a project live in a folder beside it: `<stem>-reports/`.
pub fn reports_dir(project: &Path) -> PathBuf {
    let stem = project.file_stem().map_or("project".into(), |s| s.to_string_lossy().into_owned());
    project.with_file_name(format!("{stem}-reports"))
}

pub struct ReportFile {
    pub path: PathBuf,
    pub modified: Option<SystemTime>,
}

/// Existing reports for a project, newest first.
pub fn reports_for(project: &Path) -> Vec<ReportFile> {
    let Ok(rd) = std::fs::read_dir(reports_dir(project)) else { return Vec::new() };
    let mut v: Vec<ReportFile> = rd
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "html"))
        .map(|path| {
            let modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
            ReportFile { path, modified }
        })
        .collect();
    v.sort_by(|a, b| b.modified.cmp(&a.modified));
    v
}

/// Open a file or folder with the desktop's default application.
pub fn open_externally(path: &Path) -> Result<(), String> {
    Command::new("xdg-open")
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("could not run xdg-open: {e}"))
}

/// Local time via the system `date`, since the app has no time zone data.
pub fn format_unix(unix_secs: u64, format: &str) -> String {
    Command::new("date")
        .arg("-d")
        .arg(format!("@{unix_secs}"))
        .arg(format)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map_or_else(|| format!("unix {unix_secs}"), |o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

pub fn format_time(t: SystemTime) -> String {
    let secs = t.duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    format_unix(secs, "+%Y-%m-%d %H:%M")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_become_safe_filenames() {
        assert_eq!(sanitize("Ground/Floor: 1"), "Ground_Floor_ 1");
        assert!(!sanitize("../..").contains(['/', '.']));
        assert_eq!(sanitize("  "), "survey");
        assert_eq!(sanitize("Plan-A_2"), "Plan-A_2");
    }

    #[test]
    fn reports_folder_sits_beside_the_project() {
        assert_eq!(reports_dir(Path::new("/a/b/Office.wifispanner")), PathBuf::from("/a/b/Office-reports"));
    }
}
