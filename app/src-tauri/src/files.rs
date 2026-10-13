use crate::git_ops::git;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize)]
pub struct TreeEntry {
    pub path: String,
    pub dir: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    pub path: String,
    pub line: u32,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Checkpoint {
    pub id: String,
    pub sha: String,
}

const SKIP: &[&str] = &[".git", "node_modules", "target", "dist", ".next", "build"];

pub fn file_tree(workspace: &Path) -> Vec<TreeEntry> {
    let mut out = Vec::new();
    walk(workspace, workspace, 0, &mut out);
    out
}

fn walk(root: &Path, dir: &Path, depth: usize, out: &mut Vec<TreeEntry>) {
    if out.len() >= 400 || depth > 4 {
        return;
    }
    let Ok(read) = fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = read.filter_map(Result::ok).collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        if out.len() >= 400 {
            break;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if SKIP.contains(&name.as_str()) {
            continue;
        }
        let path = entry.path();
        let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().to_string();
        let dir = path.is_dir();
        out.push(TreeEntry { path: rel, dir });
        if dir {
            walk(root, &path, depth + 1, out);
        }
    }
}

pub fn search_files(workspace: &Path, query: &str) -> Vec<SearchHit> {
    if query.trim().is_empty() {
        return Vec::new();
    }
    if let Some(hits) = search_rg(workspace, query) {
        return hits;
    }
    let mut hits = Vec::new();
    search_walk(workspace, workspace, query, &mut hits);
    hits
}

fn search_rg(workspace: &Path, query: &str) -> Option<Vec<SearchHit>> {
    let output = Command::new("rg")
        .args(["-n", "--max-count", "40", "--max-filesize", "200K", "-g", "!.git", query])
        .current_dir(workspace)
        .output()
        .ok()?;
    if !output.status.success() && output.stdout.is_empty() {
        return Some(Vec::new());
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut hits = Vec::new();
    for line in text.lines() {
        let mut parts = line.splitn(3, ':');
        let path = parts.next().unwrap_or("").to_string();
        let line_no = parts.next().unwrap_or("0").parse().unwrap_or(0);
        let body = parts.next().unwrap_or("").to_string();
        if path.is_empty() {
            continue;
        }
        hits.push(SearchHit {
            path,
            line: line_no,
            text: body,
        });
    }
    Some(hits)
}

fn search_walk(root: &Path, dir: &Path, query: &str, hits: &mut Vec<SearchHit>) {
    if hits.len() >= 40 {
        return;
    }
    let Ok(read) = fs::read_dir(dir) else { return };
    for entry in read.filter_map(Result::ok) {
        if hits.len() >= 40 {
            break;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if SKIP.contains(&name.as_str()) {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            search_walk(root, &path, query, hits);
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else { continue };
        let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().to_string();
        for (index, line) in text.lines().enumerate() {
            if line.contains(query) {
                hits.push(SearchHit {
                    path: rel.clone(),
                    line: (index + 1) as u32,
                    text: line.to_string(),
                });
                if hits.len() >= 40 {
                    break;
                }
            }
        }
    }
}

pub fn context_dir(workspace: &Path) -> PathBuf {
    workspace.join(".context")
}

pub fn list_notes(workspace: &Path) -> Vec<String> {
    let dir = context_dir(workspace);
    let Ok(read) = fs::read_dir(dir) else { return Vec::new() };
    let mut names: Vec<_> = read
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_file())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect();
    names.sort();
    names
}

pub fn read_note(workspace: &Path, name: &str) -> Result<String, String> {
    let path = note_path(workspace, name)?;
    fs::read_to_string(path).or_else(|_| Ok(String::new()))
}

pub fn write_note(workspace: &Path, name: &str, text: &str) -> Result<(), String> {
    let path = note_path(workspace, name)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    fs::write(path, text).map_err(|err| err.to_string())
}

fn note_path(workspace: &Path, name: &str) -> Result<PathBuf, String> {
    let name = name.trim();
    if name.is_empty() || name.contains('/') || name.contains("..") {
        return Err("Note name is not valid".into());
    }
    Ok(context_dir(workspace).join(name))
}

pub fn create_checkpoint(workspace: &Path, id: &str) -> Result<String, String> {
    let sha = match git(workspace, &["stash", "create"]) {
        Ok(sha) if !sha.is_empty() => sha,
        _ => git(workspace, &["rev-parse", "HEAD"])?,
    };
    git(
        workspace,
        &["update-ref", &format!("refs/insulator/checkpoints/{id}"), &sha],
    )?;
    Ok(sha)
}

pub fn restore_checkpoint(workspace: &Path, id: &str) -> Result<(), String> {
    let sha = git(workspace, &["rev-parse", &format!("refs/insulator/checkpoints/{id}")])?;
    git(workspace, &["checkout", &sha, "--", "."])?;
    Ok(())
}

pub fn list_checkpoints(workspace: &Path) -> Vec<Checkpoint> {
    let text = git(workspace, &["for-each-ref", "--format=%(refname:short) %(objectname:short)", "refs/insulator/checkpoints"]).unwrap_or_default();
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let name = parts.next()?.trim_start_matches("insulator/checkpoints/").to_string();
            let sha = parts.next()?.to_string();
            Some(Checkpoint { id: name, sha })
        })
        .collect()
}

pub fn mcp_labels() -> Vec<String> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    let path = PathBuf::from(home).join(".claude.json");
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    value
        .get("mcpServers")
        .and_then(|item| item.as_object())
        .map(|map| map.keys().cloned().collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git_ops::git;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn checkpoint_restores_file() {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("insulator-ck-{nanos}"));
        fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "-b", "main"]).unwrap();
        git(&dir, &["config", "user.email", "test@example.com"]).unwrap();
        git(&dir, &["config", "user.name", "Insulator Test"]).unwrap();
        fs::write(dir.join("README.md"), "one\n").unwrap();
        git(&dir, &["add", "README.md"]).unwrap();
        git(&dir, &["commit", "-m", "init"]).unwrap();
        fs::write(dir.join("README.md"), "two\n").unwrap();
        create_checkpoint(&dir, "snap").unwrap();
        fs::write(dir.join("README.md"), "three\n").unwrap();
        restore_checkpoint(&dir, "snap").unwrap();
        assert_eq!(fs::read_to_string(dir.join("README.md")).unwrap(), "two\n");
        write_note(&dir, "handoff.md", "note").unwrap();
        assert_eq!(read_note(&dir, "handoff.md").unwrap(), "note");
        let _ = fs::remove_dir_all(dir);
    }
}
