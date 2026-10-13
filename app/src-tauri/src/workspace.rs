use crate::git_ops::{self, git};
use crate::settings::{self, Settings};
use serde::{Deserialize, Serialize};
use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewComment {
    pub id: String,
    pub path: String,
    pub text: String,
    #[serde(default)]
    pub resolved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceRecord {
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub path: String,
    pub branch: String,
    pub port: u16,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub pr_url: Option<String>,
    #[serde(default)]
    pub comments: Vec<ReviewComment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub text: String,
    #[serde(default)]
    pub attachments: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRecord {
    pub id: String,
    pub workspace_id: String,
    pub title: String,
    pub harness: String,
    #[serde(default)]
    pub fast: bool,
    #[serde(default)]
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub running: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Loadout {
    pub id: String,
    pub name: String,
    pub harness: String,
    #[serde(default)]
    pub fast: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Routine {
    pub id: String,
    pub name: String,
    pub prompt: String,
    pub harness: String,
    pub schedule: String,
    #[serde(default = "truthy")]
    pub active: bool,
    #[serde(default)]
    pub last_fired: Option<String>,
}

fn truthy() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Persisted {
    pub repo: Option<String>,
    #[serde(default)]
    pub workspaces: Vec<WorkspaceRecord>,
    #[serde(default)]
    pub active_workspace: Option<String>,
    #[serde(default)]
    pub active_chat: Option<String>,
    #[serde(default)]
    pub chats: Vec<ChatRecord>,
    #[serde(default)]
    pub loadouts: Vec<Loadout>,
    #[serde(default)]
    pub routines: Vec<Routine>,
    #[serde(default)]
    pub equipped_harness: Option<String>,
    #[serde(default)]
    pub fast: bool,
    #[serde(default)]
    pub spotlight: Option<String>,
    #[serde(default)]
    pub spotlight_patch: Option<String>,
    #[serde(default)]
    pub workspace: Option<WorkspaceRecord>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiffFile {
    pub path: String,
    pub added: u32,
    pub removed: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiffReport {
    pub files: Vec<DiffFile>,
    pub patch: String,
}

pub fn nid(prefix: &str) -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    format!("{prefix}-{nanos}")
}

pub fn support_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home).join("Library/Application Support/Insulator")
}

pub fn state_path() -> PathBuf {
    support_dir().join("state.json")
}

fn workspaces_root() -> PathBuf {
    std::env::var("INSULATOR_WORKSPACES")
        .map(PathBuf::from)
        .unwrap_or_else(|_| support_dir().join("workspaces"))
}

pub fn load_state() -> Persisted {
    let mut state = fs::read_to_string(state_path())
        .ok()
        .and_then(|text| serde_json::from_str::<Persisted>(&text).ok())
        .unwrap_or_default();
    if state.workspaces.is_empty() {
        if let Some(legacy) = state.workspace.clone() {
            state.workspaces.push(legacy);
        }
    }
    state.workspace = None;
    for workspace in &mut state.workspaces {
        if workspace.id.is_empty() {
            workspace.id = nid("ws");
        }
    }
    for chat in &mut state.chats {
        chat.running = false;
    }
    if state.active_workspace.is_none() {
        state.active_workspace = state.workspaces.iter().find(|item| !item.archived).map(|item| item.id.clone());
    }
    state
}

pub fn save_state(state: &Persisted) -> Result<(), String> {
    let path = state_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let mut copy = state.clone();
    copy.workspace = None;
    let text = serde_json::to_string_pretty(&copy).map_err(|err| err.to_string())?;
    fs::write(path, text).map_err(|err| err.to_string())
}

pub fn require_repo(path: &Path) -> Result<PathBuf, String> {
    let repo = path.canonicalize().map_err(|err| err.to_string())?;
    if git(&repo, &["rev-parse", "--git-dir"]).is_err() {
        return Err("Choose a folder that contains a git repository".into());
    }
    Ok(repo)
}

pub fn default_branch(repo: &Path) -> String {
    let _ = git(repo, &["fetch", "origin"]);
    if let Ok(remote) = git(repo, &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"]) {
        return remote.trim_start_matches("origin/").to_string();
    }
    git(repo, &["rev-parse", "--abbrev-ref", "HEAD"])
        .ok()
        .filter(|branch| !branch.is_empty() && branch != "HEAD")
        .unwrap_or_else(|| "main".into())
}

pub fn slug(input: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
        if out.len() >= 40 {
            break;
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "workspace".into()
    } else {
        trimmed.to_string()
    }
}

pub fn allocate_port(start: u16) -> Result<u16, String> {
    for port in start..start.saturating_add(200) {
        if TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return Ok(port);
        }
    }
    Err("No free local port".into())
}

pub fn active_workspace(state: &Persisted) -> Result<&WorkspaceRecord, String> {
    let id = state
        .active_workspace
        .as_deref()
        .ok_or_else(|| "Create a workspace first".to_string())?;
    state
        .workspaces
        .iter()
        .find(|item| item.id == id)
        .ok_or_else(|| "Create a workspace first".to_string())
}

pub fn active_workspace_mut(state: &mut Persisted) -> Result<&mut WorkspaceRecord, String> {
    let id = state
        .active_workspace
        .clone()
        .ok_or_else(|| "Create a workspace first".to_string())?;
    state
        .workspaces
        .iter_mut()
        .find(|item| item.id == id)
        .ok_or_else(|| "Create a workspace first".to_string())
}

pub fn create_workspace(repo: &Path, name: &str, from_branch: Option<&str>) -> Result<WorkspaceRecord, String> {
    let repo_slug = slug(repo.file_name().and_then(|item| item.to_str()).unwrap_or("repo"));
    let parent = workspaces_root().join(repo_slug);
    fs::create_dir_all(&parent).map_err(|err| err.to_string())?;
    let requested = slug(name);
    let mut n = 1u32;
    let (path, branch, created_branch) = loop {
        let suffix = if n == 1 { requested.clone() } else { format!("{requested}-{n}") };
        let path = parent.join(&suffix);
        if let Some(existing) = from_branch {
            if !path.exists() {
                break (path, existing.to_string(), false);
            }
        } else {
            let branch = format!("insulator/{suffix}");
            let taken = path.exists()
                || git(repo, &["show-ref", "--verify", "--quiet", &format!("refs/heads/{branch}")]).is_ok();
            if !taken {
                break (path, branch, true);
            }
        }
        n += 1;
        if n > 50 {
            return Err("Could not find a free workspace path".into());
        }
    };
    let path_text = path.to_str().ok_or("Workspace path is not utf-8")?.to_string();
    if created_branch {
        let base = default_branch(repo);
        let remote = format!("origin/{base}");
        let start = if git(repo, &["rev-parse", "--verify", "--quiet", &remote]).is_ok() {
            remote
        } else {
            base
        };
        git(repo, &["worktree", "add", "-b", &branch, &path_text, &start])?;
    } else {
        git(repo, &["worktree", "add", &path_text, &branch])?;
    }
    let settings = settings::load_bundle(repo).merged;
    let _ = settings::copy_included_files(repo, &path, &settings);
    let port = allocate_port(5173)?;
    Ok(WorkspaceRecord {
        id: nid("ws"),
        name: name.trim().to_string(),
        path: path_text,
        branch,
        port,
        archived: false,
        pr_url: None,
        comments: Vec::new(),
    })
}

pub fn archive_workspace(repo: &Path, record: &WorkspaceRecord, delete_branch: bool) -> Result<(), String> {
    let path = PathBuf::from(&record.path);
    if path.exists() {
        git(repo, &["worktree", "remove", "--force", record.path.as_str()])?;
    }
    if delete_branch {
        let _ = git(repo, &["branch", "-D", &record.branch]);
    }
    Ok(())
}

pub fn restore_workspace(repo: &Path, record: &mut WorkspaceRecord) -> Result<(), String> {
    let path = PathBuf::from(&record.path);
    if !path.exists() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        git(repo, &["worktree", "add", &record.path, &record.branch])?;
    }
    record.archived = false;
    record.port = allocate_port(5173)?;
    Ok(())
}

pub fn spotlight_start(repo: &Path, workspace: &Path) -> Result<String, String> {
    let dirty = git(repo, &["status", "--porcelain"])?;
    if !dirty.is_empty() {
        return Err("Root checkout has uncommitted changes".into());
    }
    let patch = git_ops::git_raw(workspace, &["diff", "HEAD"])?;
    git_ops::git_apply(repo, &patch, false)?;
    Ok(patch)
}

pub fn spotlight_stop(repo: &Path, patch: &str) -> Result<(), String> {
    git_ops::git_apply(repo, patch, true)
}

pub fn workspace_diff(workspace: &Path) -> Result<DiffReport, String> {
    let numstat = git(workspace, &["diff", "HEAD", "--numstat"]).unwrap_or_default();
    let mut files = Vec::new();
    for line in numstat.lines() {
        let mut parts = line.split('\t');
        let added = parts.next().unwrap_or("0");
        let removed = parts.next().unwrap_or("0");
        let path = parts.next().unwrap_or("").to_string();
        if path.is_empty() {
            continue;
        }
        files.push(DiffFile {
            added: added.parse().unwrap_or(0),
            removed: removed.parse().unwrap_or(0),
            path,
        });
    }
    if let Ok(porcelain) = git(workspace, &["status", "--porcelain"]) {
        for line in porcelain.lines() {
            if let Some(path) = line.strip_prefix("?? ") {
                if !files.iter().any(|file| file.path == path) {
                    files.push(DiffFile {
                        path: path.to_string(),
                        added: 1,
                        removed: 0,
                    });
                }
            }
        }
    }
    let patch = git_ops::git_raw(workspace, &["diff", "HEAD", "--patch"]).unwrap_or_default();
    Ok(DiffReport { files, patch })
}

pub fn repo_of(state: &Persisted) -> Result<PathBuf, String> {
    let repo = state.repo.as_deref().ok_or_else(|| "Choose a git repository first".to_string())?;
    Ok(PathBuf::from(repo))
}

#[allow(dead_code)]
pub fn settings_for(state: &Persisted) -> Result<Settings, String> {
    Ok(settings::load_bundle(&repo_of(state)?).merged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static LOCK: Mutex<()> = Mutex::new(());

    struct EnvGuard {
        prev: Option<String>,
    }

    impl EnvGuard {
        fn set(value: &Path) -> Self {
            let prev = std::env::var("INSULATOR_WORKSPACES").ok();
            std::env::set_var("INSULATOR_WORKSPACES", value);
            Self { prev }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.prev {
                Some(value) => std::env::set_var("INSULATOR_WORKSPACES", value),
                None => std::env::remove_var("INSULATOR_WORKSPACES"),
            }
        }
    }

    fn temp_repo() -> PathBuf {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("insulator-repo-{nanos}"));
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
    fn creates_worktree_archives_and_spotlights() {
        let _lock = LOCK.lock().unwrap();
        let repo = temp_repo();
        let root = std::env::temp_dir().join(format!("insulator-ws-{}", nid("t")));
        let _env = EnvGuard::set(&root);
        let workspace = create_workspace(&repo, "Feature One", None).unwrap();
        let path = PathBuf::from(&workspace.path);
        assert!(path.join("README.md").exists());
        fs::write(path.join("README.md"), "hello\nchanged\n").unwrap();
        let diff = workspace_diff(&path).unwrap();
        assert!(diff.files.iter().any(|file| file.path == "README.md"));
        let patch = spotlight_start(&repo, &path).unwrap();
        assert!(fs::read_to_string(repo.join("README.md")).unwrap().contains("changed"));
        spotlight_stop(&repo, &patch).unwrap();
        assert_eq!(fs::read_to_string(repo.join("README.md")).unwrap(), "hello\n");
        archive_workspace(&repo, &workspace, false).unwrap();
        assert!(!path.exists());
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(repo);
    }
}
