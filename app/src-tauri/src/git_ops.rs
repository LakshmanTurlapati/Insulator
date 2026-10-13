use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PrInfo {
    pub number: Option<u64>,
    pub url: String,
    pub title: String,
    pub body: String,
    pub state: String,
    pub mergeable: String,
    pub head: String,
    pub base: String,
}

pub fn git(repo: &Path, args: &[&str]) -> Result<String, String> {
    git_raw(repo, args).map(|text| text.trim().to_string())
}

pub fn git_raw(repo: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|err| format!("git {}: {err}", args.join(" ")))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if stderr.is_empty() {
            format!("git {} failed", args.join(" "))
        } else {
            stderr
        })
    }
}

pub fn git_apply(repo: &Path, patch: &str, reverse: bool) -> Result<(), String> {
    if patch.trim().is_empty() {
        return Ok(());
    }
    let mut command = Command::new("git");
    command.arg("apply");
    if reverse {
        command.arg("-R");
    }
    let mut child = command
        .current_dir(repo)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("git apply: {err}"))?;
    child
        .stdin
        .take()
        .ok_or("git apply stdin")?
        .write_all(patch.as_bytes())
        .map_err(|err| err.to_string())?;
    let output = child.wait_with_output().map_err(|err| err.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

pub fn commit_all(workspace: &Path, message: &str) -> Result<String, String> {
    git(workspace, &["add", "-A"])?;
    let status = git(workspace, &["status", "--porcelain"])?;
    if status.is_empty() {
        return Err("Nothing to commit".into());
    }
    let message = if message.trim().is_empty() {
        "Update workspace"
    } else {
        message.trim()
    };
    git(workspace, &["commit", "-m", message])?;
    git(workspace, &["rev-parse", "--short", "HEAD"])
}

pub fn push_branch(workspace: &Path, branch: &str) -> Result<String, String> {
    git(workspace, &["push", "-u", "origin", branch])
}

pub fn pull_latest(workspace: &Path) -> Result<String, String> {
    git(workspace, &["pull", "--ff-only"])
}

pub fn local_branches(repo: &Path) -> Result<Vec<String>, String> {
    let text = git(repo, &["branch", "--format=%(refname:short)"])?;
    Ok(text.lines().map(str::trim).filter(|line| !line.is_empty()).map(str::to_string).collect())
}

pub fn rename_branch(workspace: &Path, next: &str) -> Result<(), String> {
    git(workspace, &["branch", "-m", next])?;
    Ok(())
}

fn gh(repo: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("gh")
        .args(args)
        .current_dir(repo)
        .env("GH_PROMPT_DISABLED", "1")
        .output()
        .map_err(|err| format!("gh is not available: {err}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if output.status.success() {
        Ok(stdout)
    } else {
        Err(if stderr.is_empty() { stdout } else { stderr })
    }
}

pub fn view_pr(repo: &Path, branch: &str) -> Result<Option<PrInfo>, String> {
    match gh(
        repo,
        &[
            "pr",
            "view",
            branch,
            "--json",
            "number,url,title,body,state,mergeable,headRefName,baseRefName",
        ],
    ) {
        Ok(text) => Ok(Some(parse_pr(&text))),
        Err(err) if err.to_lowercase().contains("no pull request") || err.contains("no pull requests") => {
            Ok(None)
        }
        Err(err) => Err(err),
    }
}

fn parse_pr(text: &str) -> PrInfo {
    let value: Value = serde_json::from_str(text).unwrap_or(Value::Null);
    PrInfo {
        number: value.get("number").and_then(|item| item.as_u64()),
        url: value.get("url").and_then(|item| item.as_str()).unwrap_or("").into(),
        title: value.get("title").and_then(|item| item.as_str()).unwrap_or("").into(),
        body: value.get("body").and_then(|item| item.as_str()).unwrap_or("").into(),
        state: value.get("state").and_then(|item| item.as_str()).unwrap_or("").into(),
        mergeable: value.get("mergeable").and_then(|item| item.as_str()).unwrap_or("").into(),
        head: value.get("headRefName").and_then(|item| item.as_str()).unwrap_or("").into(),
        base: value.get("baseRefName").and_then(|item| item.as_str()).unwrap_or("").into(),
    }
}

pub fn create_pr(repo: &Path, branch: &str, base: &str, title: &str, body: &str) -> Result<PrInfo, String> {
    if let Some(existing) = view_pr(repo, branch)? {
        return Ok(existing);
    }
    let url = gh(
        repo,
        &["pr", "create", "--head", branch, "--base", base, "--title", title, "--body", body],
    )?;
    view_pr(repo, branch)?.ok_or(url)
}

pub fn pr_head_branch(repo: &Path, number: &str) -> Result<String, String> {
    let text = gh(repo, &["pr", "view", number, "--json", "headRefName,title"])?;
    let value: Value = serde_json::from_str(&text).map_err(|err| err.to_string())?;
    value
        .get("headRefName")
        .and_then(|item| item.as_str())
        .map(str::to_string)
        .ok_or_else(|| "Pull request has no head branch".into())
}

pub fn pr_checks(repo: &Path, branch: &str) -> Result<String, String> {
    let output = Command::new("gh")
        .args(["pr", "checks", branch])
        .current_dir(repo)
        .env("GH_PROMPT_DISABLED", "1")
        .output()
        .map_err(|err| format!("gh is not available: {err}"))?;
    let mut text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if text.is_empty() {
        text = stderr;
    }
    if text.is_empty() {
        text = "No checks reported".into();
    }
    Ok(text)
}

pub fn merge_pr(repo: &Path, branch: &str) -> Result<String, String> {
    gh(repo, &["pr", "merge", branch, "--merge", "--yes"])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_repo() -> std::path::PathBuf {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("insulator-git-{nanos}"));
        fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "-b", "main"]).unwrap();
        git(&dir, &["config", "user.email", "test@example.com"]).unwrap();
        git(&dir, &["config", "user.name", "Insulator Test"]).unwrap();
        fs::write(dir.join("README.md"), "hello\n").unwrap();
        git(&dir, &["add", "README.md"]).unwrap();
        git(&dir, &["commit", "-m", "init"]).unwrap();
        dir
    }

    #[test]
    fn commits_tracked_change() {
        let repo = temp_repo();
        fs::write(repo.join("README.md"), "hello\nchanged\n").unwrap();
        let sha = commit_all(&repo, "update readme").unwrap();
        assert!(!sha.is_empty());
        assert!(git(&repo, &["status", "--porcelain"]).unwrap().is_empty());
        assert!(git(&repo, &["log", "-1", "--pretty=%s"]).unwrap().contains("update readme"));
        let _ = fs::remove_dir_all(repo);
    }
}
