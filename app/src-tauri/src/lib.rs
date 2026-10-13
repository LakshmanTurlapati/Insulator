mod agents;
mod chats;
mod files;
mod git_ops;
mod scripts;
mod settings;
mod terminal;
mod workspace;

use agents::{spawn_agent, stop_agent, AgentProcess};
use chats::{append_message, due_routines, ensure_chat, new_chat, save_loadout, save_routine, set_running, transcript};
use files::{create_checkpoint, file_tree, list_checkpoints, list_notes, mcp_labels, read_note, restore_checkpoint, search_files, write_note};
use git_ops::{commit_all, create_pr, local_branches, merge_pr, pr_checks, pr_head_branch, pull_latest, push_branch, rename_branch, view_pr, PrInfo};
use scripts::{spawn_script, stop_script, ScriptProcess};
use serde::Serialize;
use settings::{save_project_settings, save_user_git, GitPrefs, SettingsBundle};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;
use terminal::TerminalSession;
use workspace::{
    active_workspace, active_workspace_mut, archive_workspace, create_workspace, default_branch, load_state,
    repo_of, require_repo, restore_workspace, save_state, slug, spotlight_start, spotlight_stop, workspace_diff,
    ChatRecord, DiffReport, Loadout, Persisted, Routine, WorkspaceRecord,
};

struct Runtime {
    persisted: Arc<Mutex<Persisted>>,
    scripts: Arc<Mutex<HashMap<String, ScriptProcess>>>,
    terminal: Arc<Mutex<Option<TerminalSession>>>,
    agents: Arc<Mutex<HashMap<String, AgentProcess>>>,
    agent_text: Arc<Mutex<HashMap<String, String>>>,
}

#[derive(Clone, Serialize)]
struct ScriptEvent {
    stream: String,
    text: String,
}

#[derive(Clone, Serialize)]
struct AgentEvent {
    chat_id: String,
    text: String,
}

#[derive(Serialize)]
struct AppSnapshot {
    repo: Option<String>,
    repo_name: String,
    workspaces: Vec<WorkspaceRecord>,
    active_workspace: Option<String>,
    chats: Vec<ChatRecord>,
    active_chat: Option<String>,
    settings: SettingsBundle,
    harnesses: Vec<agents::Harness>,
    loadouts: Vec<Loadout>,
    routines: Vec<Routine>,
    equipped_harness: String,
    fast: bool,
    spotlight: Option<String>,
    mcp: Vec<String>,
    branches: Vec<String>,
}

#[derive(Serialize)]
struct CommitResult {
    sha: String,
    pushed: bool,
    pr_url: Option<String>,
    note: String,
}

fn default_branch_name(repo: &std::path::Path) -> String {
    git_ops::git(repo, &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"])
        .ok()
        .map(|name| name.trim_start_matches("origin/").to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "main".into())
}

fn runtime(app: &AppHandle) -> State<'_, Runtime> {
    app.state::<Runtime>()
}

fn lock_arc<T>(arc: &Arc<Mutex<T>>) -> Result<MutexGuard<'static, T>, String> {
    let arc = Arc::clone(arc);
    let guard = arc.lock().map_err(|err| err.to_string())?;
    // Runtime keeps this Arc alive for the process, so the guard outlives the local clone.
    Ok(unsafe { std::mem::transmute(guard) })
}

fn lock_state(app: &AppHandle) -> Result<MutexGuard<'static, Persisted>, String> {
    lock_arc(&runtime(app).persisted)
}

#[tauri::command]
fn pick_repo(app: AppHandle) -> Result<Option<String>, String> {
    let Some(folder) = app.dialog().file().blocking_pick_folder() else {
        return Ok(None);
    };
    let path = folder.into_path().map_err(|err| format!("Could not read the chosen folder: {err}"))?;
    let repo = require_repo(&path)?;
    let mut persisted = lock_state(&app)?;
    persisted.repo = Some(repo.to_string_lossy().to_string());
    save_state(&persisted)?;
    Ok(persisted.repo.clone())
}

#[tauri::command]
fn app_snapshot(app: AppHandle) -> Result<AppSnapshot, String> {
    let persisted = lock_state(&app)?.clone();
    let repo = persisted.repo.as_ref().map(PathBuf::from);
    let settings = repo
        .as_ref()
        .map(|path| settings::load_bundle(path))
        .unwrap_or_else(|| SettingsBundle {
            merged: settings::Settings::default(),
            local: settings::Settings::default(),
            shared: settings::Settings::default(),
            user: settings::Settings::default(),
        });
    let harnesses = agents::detect_harnesses(&settings.merged);
    let equipped = persisted
        .equipped_harness
        .clone()
        .or_else(|| harnesses.iter().find(|item| item.installed).map(|item| item.id.clone()))
        .unwrap_or_else(|| "claude".into());
    let branches = repo.as_ref().and_then(|path| local_branches(path).ok()).unwrap_or_default();
    let repo_name = repo
        .as_ref()
        .and_then(|path| path.file_name().map(|name| name.to_string_lossy().to_string()))
        .unwrap_or_else(|| "repository".into());
    Ok(AppSnapshot {
        repo: persisted.repo,
        repo_name,
        workspaces: persisted.workspaces,
        active_workspace: persisted.active_workspace,
        chats: persisted.chats,
        active_chat: persisted.active_chat,
        settings,
        harnesses,
        loadouts: persisted.loadouts,
        routines: persisted.routines,
        equipped_harness: equipped,
        fast: persisted.fast,
        spotlight: persisted.spotlight,
        mcp: mcp_labels(),
        branches,
    })
}

#[tauri::command]
fn create_workspace_cmd(app: AppHandle, name: String, source: String) -> Result<WorkspaceRecord, String> {
    let persisted = lock_state(&app)?;
    let repo = repo_of(&persisted)?;
    drop(persisted);
    let (from_branch, label) = match source.as_str() {
        "branch" => (Some(name.clone()), name.clone()),
        "pr" => {
            let branch = pr_head_branch(&repo, name.trim())?;
            (Some(branch), format!("pr-{name}"))
        }
        _ => (None, name.clone()),
    };
    let record = create_workspace(&repo, &label, from_branch.as_deref())?;
    let setup = settings::load_bundle(&repo).merged.setup.clone();
    {
        let mut persisted = lock_state(&app)?;
        persisted.active_workspace = Some(record.id.clone());
        persisted.workspaces.push(record.clone());
        save_state(&persisted)?;
    }
    if let Some(command) = setup.filter(|item| !item.trim().is_empty()) {
        let _ = start_script(&app, &command, "setup", None);
    }
    Ok(record)
}

#[tauri::command]
fn select_workspace(app: AppHandle, id: String) -> Result<(), String> {
    let mut persisted = lock_state(&app)?;
    if !persisted.workspaces.iter().any(|item| item.id == id) {
        return Err("Workspace not found".into());
    }
    persisted.active_workspace = Some(id.clone());
    persisted.active_chat = persisted
        .chats
        .iter()
        .rev()
        .find(|chat| chat.workspace_id == id)
        .map(|chat| chat.id.clone());
    drop(lock_arc(&runtime(&app).terminal)?.take());
    save_state(&persisted)
}

#[tauri::command]
fn archive_workspace_cmd(app: AppHandle, id: String) -> Result<(), String> {
    let (repo, record, command, delete_branch) = {
        let persisted = lock_state(&app)?;
        let repo = repo_of(&persisted)?;
        let record = persisted
            .workspaces
            .iter()
            .find(|item| item.id == id)
            .cloned()
            .ok_or_else(|| "Workspace not found".to_string())?;
        let settings = settings::load_bundle(&repo).merged;
        (repo, record, settings.archive.clone(), settings.git.delete_branch_on_archive)
    };
    if let Some(command) = command.filter(|item| !item.trim().is_empty()) {
        let _ = std::process::Command::new("zsh")
            .args(["-lc", &command])
            .current_dir(&record.path)
            .env("INSULATOR_WORKSPACE_PATH", &record.path)
            .env("INSULATOR_ROOT_PATH", &repo)
            .env("INSULATOR_PORT", record.port.to_string())
            .env("INSULATOR_IS_LOCAL", "1")
            .status();
    }
    archive_workspace(&repo, &record, delete_branch)?;
    let mut persisted = lock_state(&app)?;
    if let Some(item) = persisted.workspaces.iter_mut().find(|item| item.id == id) {
        item.archived = true;
    }
    if persisted.active_workspace.as_deref() == Some(id.as_str()) {
        persisted.active_workspace = persisted.workspaces.iter().find(|item| !item.archived).map(|item| item.id.clone());
    }
    if persisted.spotlight.as_deref() == Some(id.as_str()) {
        persisted.spotlight = None;
        persisted.spotlight_patch = None;
    }
    save_state(&persisted)
}

#[tauri::command]
fn restore_workspace_cmd(app: AppHandle, id: String) -> Result<(), String> {
    let repo = {
        let persisted = lock_state(&app)?;
        repo_of(&persisted)?
    };
    let mut persisted = lock_state(&app)?;
    let record = persisted
        .workspaces
        .iter_mut()
        .find(|item| item.id == id)
        .ok_or_else(|| "Workspace not found".to_string())?;
    restore_workspace(&repo, record)?;
    persisted.active_workspace = Some(id);
    save_state(&persisted)
}

#[tauri::command]
fn save_project_settings_cmd(
    app: AppHandle,
    shared: bool,
    setup: String,
    run: String,
    archive: String,
    globs: String,
    env_text: String,
    review: String,
) -> Result<(), String> {
    let repo = {
        let persisted = lock_state(&app)?;
        repo_of(&persisted)?
    };
    save_project_settings(&repo, shared, &setup, &run, &archive, &globs, &env_text, &review)
}

#[tauri::command]
fn save_user_settings_cmd(
    app: AppHandle,
    rename_branch: bool,
    archive_on_merge: bool,
    delete_branch_on_archive: bool,
    include_workspace_link: bool,
    claude: String,
    codex: String,
    opencode: String,
) -> Result<(), String> {
    let prefs = GitPrefs {
        rename_branch,
        archive_on_merge,
        delete_branch_on_archive,
        include_workspace_link,
    };
    save_user_git(&prefs, &claude, &codex, &opencode)?;
    let _ = app;
    Ok(())
}

#[tauri::command]
fn run_script(app: AppHandle, kind: String, script_id: Option<String>) -> Result<(), String> {
    if kind == "stop" {
        let mut running = lock_arc(&runtime(&app).scripts)?;
        for job in running.values() {
            let _ = stop_script(job);
        }
        running.clear();
        return Ok(());
    }
    let (command, cwd) = {
        let persisted = lock_state(&app)?;
        let repo = repo_of(&persisted)?;
        let settings = settings::load_bundle(&repo).merged;
        if kind == "setup" {
            (settings.setup.unwrap_or_else(|| "echo no setup script".into()), None)
        } else if let Some(id) = script_id.as_deref() {
            let script = settings.run.iter().find(|script| script.id == id && script.local);
            let command = script
                .map(settings::script_line)
                .unwrap_or_else(|| settings::default_run_command(&settings));
            let cwd = script.and_then(|script| script.cwd.clone());
            (command, cwd)
        } else {
            (settings::default_run_command(&settings), settings::script_cwd(&settings, None).map(str::to_string))
        }
    };
    let key = script_id.unwrap_or_else(|| kind.clone());
    start_script(&app, &command, &key, cwd.as_deref())
}

fn start_script(app: &AppHandle, command: &str, key: &str, relative_cwd: Option<&str>) -> Result<(), String> {
    let (workspace_path, repo, port, concurrent, auto_run, env) = {
        let persisted = lock_state(app)?;
        let repo = repo_of(&persisted)?;
        let workspace = active_workspace(&persisted)?.clone();
        let settings = settings::load_bundle(&repo).merged;
        let mut env: Vec<(String, String)> = settings.env.iter().map(|(key, value)| (key.clone(), value.clone())).collect();
        env.push(("INSULATOR_WORKSPACE_NAME".into(), workspace.name.clone()));
        env.push(("INSULATOR_DEFAULT_BRANCH".into(), default_branch_name(&repo)));
        (
            workspace.path,
            repo,
            workspace.port,
            settings.run_mode == "concurrent",
            settings.auto_run_after_setup,
            env,
        )
    };
    let follow = if key == "setup" && auto_run {
        let settings = settings::load_bundle(&repo).merged;
        Some((
            settings::default_run_command(&settings),
            settings::script_cwd(&settings, None).map(str::to_string),
        ))
    } else {
        None
    };
    {
        let mut running = lock_arc(&runtime(app).scripts)?;
        if !concurrent {
            for job in running.values() {
                let _ = stop_script(job);
            }
            running.clear();
        } else if let Some(job) = running.remove(key) {
            let _ = stop_script(&job);
        }
    }
    let handle = app.clone();
    let on_output = Arc::new(move |stream: String, text: String| {
        let _ = handle.emit("script-output", ScriptEvent { stream, text });
    });
    let exit_handle = app.clone();
    let exit_key = key.to_string();
    let on_exit = Arc::new(move || {
        if let Ok(mut jobs) = lock_arc(&runtime(&exit_handle).scripts) {
            jobs.remove(&exit_key);
        }
        let _ = exit_handle.emit("script-exit", ());
        if let Some((command, cwd)) = follow.clone() {
            let _ = start_script(&exit_handle, &command, "run", cwd.as_deref());
        }
    });
    let pairs: Vec<(&str, &str)> = env.iter().map(|(key, value)| (key.as_str(), value.as_str())).collect();
    let relative = relative_cwd.map(str::to_string);
    let job = spawn_script(
        std::path::Path::new(&workspace_path),
        command,
        &repo,
        port,
        &pairs,
        relative.as_deref(),
        on_output,
        on_exit,
    )?;
    lock_arc(&runtime(app).scripts)?.insert(key.to_string(), job);
    Ok(())
}

#[tauri::command]
fn workspace_diff_cmd(app: AppHandle) -> Result<DiffReport, String> {
    let persisted = lock_state(&app)?;
    let workspace = active_workspace(&persisted)?;
    workspace_diff(std::path::Path::new(&workspace.path))
}

#[tauri::command]
fn file_tree_cmd(app: AppHandle) -> Result<Vec<files::TreeEntry>, String> {
    let persisted = lock_state(&app)?;
    let workspace = active_workspace(&persisted)?;
    Ok(file_tree(std::path::Path::new(&workspace.path)))
}

#[tauri::command]
fn search_cmd(app: AppHandle, query: String) -> Result<Vec<files::SearchHit>, String> {
    let persisted = lock_state(&app)?;
    let workspace = active_workspace(&persisted)?;
    Ok(search_files(std::path::Path::new(&workspace.path), &query))
}

#[tauri::command]
fn list_notes_cmd(app: AppHandle) -> Result<Vec<String>, String> {
    let persisted = lock_state(&app)?;
    let workspace = active_workspace(&persisted)?;
    Ok(list_notes(std::path::Path::new(&workspace.path)))
}

#[tauri::command]
fn read_note_cmd(app: AppHandle, name: String) -> Result<String, String> {
    let persisted = lock_state(&app)?;
    let workspace = active_workspace(&persisted)?;
    read_note(std::path::Path::new(&workspace.path), &name)
}

#[tauri::command]
fn write_note_cmd(app: AppHandle, name: String, text: String) -> Result<(), String> {
    let persisted = lock_state(&app)?;
    let workspace = active_workspace(&persisted)?;
    write_note(std::path::Path::new(&workspace.path), &name, &text)
}

#[tauri::command]
fn checkpoint_cmd(app: AppHandle) -> Result<String, String> {
    let persisted = lock_state(&app)?;
    let workspace = active_workspace(&persisted)?;
    let id = workspace::nid("ck");
    create_checkpoint(std::path::Path::new(&workspace.path), &id)?;
    Ok(id)
}

#[tauri::command]
fn list_checkpoints_cmd(app: AppHandle) -> Result<Vec<files::Checkpoint>, String> {
    let persisted = lock_state(&app)?;
    let workspace = active_workspace(&persisted)?;
    Ok(list_checkpoints(std::path::Path::new(&workspace.path)))
}

#[tauri::command]
fn restore_checkpoint_cmd(app: AppHandle, id: String) -> Result<(), String> {
    let persisted = lock_state(&app)?;
    let workspace = active_workspace(&persisted)?;
    restore_checkpoint(std::path::Path::new(&workspace.path), &id)
}

#[tauri::command]
fn spotlight_cmd(app: AppHandle, enabled: bool) -> Result<(), String> {
    if !enabled {
        let (repo, patch) = {
            let persisted = lock_state(&app)?;
            let repo = repo_of(&persisted)?;
            (repo, persisted.spotlight_patch.clone().unwrap_or_default())
        };
        spotlight_stop(&repo, &patch)?;
        let mut persisted = lock_state(&app)?;
        persisted.spotlight = None;
        persisted.spotlight_patch = None;
        return save_state(&persisted);
    }
    let (repo, workspace_path, id) = {
        let persisted = lock_state(&app)?;
        let repo = repo_of(&persisted)?;
        let workspace = active_workspace(&persisted)?;
        (repo, workspace.path.clone(), workspace.id.clone())
    };
    let patch = spotlight_start(&repo, std::path::Path::new(&workspace_path))?;
    let mut persisted = lock_state(&app)?;
    persisted.spotlight = Some(id);
    persisted.spotlight_patch = Some(patch);
    save_state(&persisted)
}

#[tauri::command]
fn terminal_start(app: AppHandle) -> Result<(), String> {
    if lock_arc(&runtime(&app).terminal)?.is_some() {
        return Ok(());
    }
    let persisted = lock_state(&app)?;
    let repo = repo_of(&persisted)?;
    let workspace = active_workspace(&persisted)?.clone();
    drop(persisted);
    let settings = settings::load_bundle(&repo).merged;
    let extra: Vec<(String, String)> = settings.env.into_iter().collect();
    let pairs: Vec<(&str, &str)> = extra.iter().map(|(key, value)| (key.as_str(), value.as_str())).collect();
    let handle = app.clone();
    let session = terminal::start_terminal(
        std::path::Path::new(&workspace.path),
        &repo,
        workspace.port,
        &workspace.name,
        &default_branch_name(&repo),
        &pairs,
        Arc::new(move |text| {
            let _ = handle.emit("terminal-output", text);
        }),
    )?;
    *lock_arc(&runtime(&app).terminal)? = Some(session);
    Ok(())
}

#[tauri::command]
fn terminal_write(app: AppHandle, data: String) -> Result<(), String> {
    let terminal = lock_arc(&runtime(&app).terminal)?;
    let session = terminal.as_ref().ok_or_else(|| "Terminal is not running".to_string())?;
    session.write(&data)
}

#[tauri::command]
fn terminal_resize(app: AppHandle, rows: u16, cols: u16) -> Result<(), String> {
    let terminal = lock_arc(&runtime(&app).terminal)?;
    let session = terminal.as_ref().ok_or_else(|| "Terminal is not running".to_string())?;
    session.resize(rows, cols)
}

#[tauri::command]
fn new_chat_cmd(app: AppHandle) -> Result<ChatRecord, String> {
    let mut persisted = lock_state(&app)?;
    let workspace_id = active_workspace(&persisted)?.id.clone();
    let harness = persisted.equipped_harness.clone().unwrap_or_else(|| "claude".into());
    let chat = new_chat(&mut persisted, &workspace_id, &harness);
    save_state(&persisted)?;
    Ok(chat)
}

#[tauri::command]
fn select_chat(app: AppHandle, id: String) -> Result<(), String> {
    let mut persisted = lock_state(&app)?;
    let workspace_id = persisted
        .chats
        .iter()
        .find(|chat| chat.id == id)
        .map(|chat| chat.workspace_id.clone())
        .ok_or_else(|| "Chat not found".to_string())?;
    persisted.active_chat = Some(id);
    persisted.active_workspace = Some(workspace_id);
    save_state(&persisted)
}

#[tauri::command]
fn set_harness(app: AppHandle, harness: String, fast: bool) -> Result<(), String> {
    let mut persisted = lock_state(&app)?;
    persisted.equipped_harness = Some(harness);
    persisted.fast = fast;
    save_state(&persisted)
}

#[tauri::command]
fn pick_attachments(app: AppHandle) -> Result<Vec<String>, String> {
    let Some(files) = app.dialog().file().blocking_pick_files() else {
        return Ok(Vec::new());
    };
    let mut paths = Vec::new();
    for file in files {
        if let Ok(path) = file.into_path() {
            paths.push(path.to_string_lossy().to_string());
        }
    }
    Ok(paths)
}

#[tauri::command]
fn send_message(app: AppHandle, text: String, attachments: Vec<String>) -> Result<String, String> {
    let text = text.trim().to_string();
    if text.is_empty() && attachments.is_empty() {
        return Err("Write a message first".into());
    }
    let (chat_id, prompt, harness, workspace, repo, port, settings) = {
        let mut persisted = lock_state(&app)?;
        let repo = repo_of(&persisted)?;
        let settings = settings::load_bundle(&repo).merged;
        let harness = persisted.equipped_harness.clone().unwrap_or_else(|| "claude".into());
        let workspace = active_workspace(&persisted)?.clone();
        if settings.git.rename_branch && workspace.branch.starts_with("insulator/") {
            let prior = persisted
                .chats
                .iter()
                .filter(|chat| chat.workspace_id == workspace.id)
                .any(|chat| chat.messages.iter().any(|message| message.role == "user"));
            if !prior {
                let next = format!("insulator/{}", slug(&text));
                if next != workspace.branch && rename_branch(std::path::Path::new(&workspace.path), &next).is_ok() {
                    if let Ok(record) = active_workspace_mut(&mut persisted) {
                        record.branch = next;
                    }
                }
            }
        }
        let workspace = active_workspace(&persisted)?.clone();
        let chat_id = ensure_chat(&mut persisted, &workspace.id, &harness);
        append_message(&mut persisted, &chat_id, "user", &text, attachments.clone());
        set_running(&mut persisted, &chat_id, true);
        let mut prompt = transcript(&persisted, &chat_id);
        if let Some(review) = settings.review_prompt.clone() {
            if !review.trim().is_empty() {
                prompt = format!("Review preferences:\n{review}\n\n{prompt}");
            }
        }
        if !attachments.is_empty() {
            prompt.push_str("\nAttached files:\n");
            for path in &attachments {
                prompt.push_str(path);
                prompt.push('\n');
            }
        }
        save_state(&persisted)?;
        let port = workspace.port;
        (chat_id, prompt, harness, workspace, repo, port, settings)
    };
    let handle = app.clone();
    let chat_for_output = chat_id.clone();
    let on_output = Arc::new(move |line: String| {
        if let Ok(mut buffer) = lock_arc(&runtime(&handle).agent_text) {
            let entry = buffer.entry(chat_for_output.clone()).or_default();
            entry.push_str(&line);
            entry.push('\n');
        }
        let _ = handle.emit(
            "agent-output",
            AgentEvent {
                chat_id: chat_for_output.clone(),
                text: line,
            },
        );
    });
    let exit_handle = app.clone();
    let exit_chat = chat_id.clone();
    let on_exit = Arc::new(move || {
        let text = lock_arc(&runtime(&exit_handle).agent_text)
            .ok()
            .and_then(|mut buffer| buffer.remove(&exit_chat))
            .unwrap_or_default();
        if let Ok(mut persisted) = lock_state(&exit_handle) {
            append_message(&mut persisted, &exit_chat, "assistant", text.trim(), Vec::new());
            set_running(&mut persisted, &exit_chat, false);
            let _ = save_state(&persisted);
        }
        if let Ok(mut jobs) = lock_arc(&runtime(&exit_handle).agents) {
            jobs.remove(&exit_chat);
        }
        let _ = exit_handle.emit(
            "agent-exit",
            AgentEvent {
                chat_id: exit_chat.clone(),
                text: String::new(),
            },
        );
    });
    let job = spawn_agent(
        std::path::Path::new(&workspace.path),
        &workspace.name,
        &repo,
        port,
        &harness,
        &settings,
        &prompt,
        &attachments,
        on_output,
        on_exit,
    )?;
    lock_arc(&runtime(&app).agents)?.insert(chat_id.clone(), job);
    Ok(chat_id)
}

#[tauri::command]
fn stop_chat(app: AppHandle, id: String) -> Result<(), String> {
    let mut jobs = lock_arc(&runtime(&app).agents)?;
    if let Some(job) = jobs.remove(&id) {
        stop_agent(&job);
    }
    Ok(())
}

#[tauri::command]
fn add_comment(app: AppHandle, path: String, text: String) -> Result<(), String> {
    let mut persisted = lock_state(&app)?;
    let workspace = active_workspace_mut(&mut persisted)?;
    workspace.comments.push(workspace::ReviewComment {
        id: workspace::nid("comment"),
        path,
        text,
        resolved: false,
    });
    save_state(&persisted)
}

#[tauri::command]
fn resolve_comment(app: AppHandle, id: String) -> Result<(), String> {
    let mut persisted = lock_state(&app)?;
    let workspace = active_workspace_mut(&mut persisted)?;
    if let Some(comment) = workspace.comments.iter_mut().find(|item| item.id == id) {
        comment.resolved = true;
    } else if let Some(comment) = workspace.comments.iter_mut().rev().find(|item| !item.resolved) {
        comment.resolved = true;
    }
    save_state(&persisted)
}

#[tauri::command]
fn send_comments(app: AppHandle) -> Result<String, String> {
    let text = {
        let persisted = lock_state(&app)?;
        let workspace = active_workspace(&persisted)?;
        let notes: Vec<_> = workspace
            .comments
            .iter()
            .filter(|item| !item.resolved)
            .map(|item| format!("{}: {}", item.path, item.text))
            .collect();
        if notes.is_empty() {
            return Err("No open review comments".into());
        }
        format!("Please address these review comments:\n{}", notes.join("\n"))
    };
    send_message(app, text, Vec::new())
}

#[tauri::command]
fn commit_and_push(app: AppHandle, message: String) -> Result<CommitResult, String> {
    let (workspace_path, repo, branch, base, body, link) = {
        let persisted = lock_state(&app)?;
        let repo = repo_of(&persisted)?;
        let workspace = active_workspace(&persisted)?.clone();
        let settings = settings::load_bundle(&repo).merged;
        let base = default_branch(&repo);
        let mut body = message.clone();
        if settings.git.include_workspace_link {
            body.push_str(&format!("\n\nWorkspace: {}\nBranch: {}", workspace.path, workspace.branch));
        }
        (
            workspace.path.clone(),
            repo,
            workspace.branch.clone(),
            base,
            body,
            settings.git.include_workspace_link,
        )
    };
    let _ = link;
    let sha = commit_all(std::path::Path::new(&workspace_path), &message)?;
    match push_branch(std::path::Path::new(&workspace_path), &branch) {
        Ok(_) => match create_pr(&repo, &branch, &base, &message, &body) {
            Ok(pr) => {
                let mut persisted = lock_state(&app)?;
                if let Ok(record) = active_workspace_mut(&mut persisted) {
                    record.pr_url = Some(pr.url.clone());
                }
                let _ = save_state(&persisted);
                Ok(CommitResult {
                    sha,
                    pushed: true,
                    pr_url: Some(pr.url),
                    note: "Committed, pushed, and opened a pull request".into(),
                })
            }
            Err(err) => Ok(CommitResult {
                sha,
                pushed: true,
                pr_url: None,
                note: format!("Committed and pushed. Pull request skipped: {err}"),
            }),
        },
        Err(err) => Ok(CommitResult {
            sha,
            pushed: false,
            pr_url: None,
            note: format!("Committed. Push skipped: {err}"),
        }),
    }
}

#[tauri::command]
fn pull_cmd(app: AppHandle) -> Result<String, String> {
    let persisted = lock_state(&app)?;
    let workspace = active_workspace(&persisted)?;
    pull_latest(std::path::Path::new(&workspace.path))
}

#[tauri::command]
fn pr_status(app: AppHandle) -> Result<Option<PrInfo>, String> {
    let persisted = lock_state(&app)?;
    let repo = repo_of(&persisted)?;
    let workspace = active_workspace(&persisted)?;
    view_pr(&repo, &workspace.branch)
}

#[tauri::command]
fn checks_cmd(app: AppHandle) -> Result<String, String> {
    let persisted = lock_state(&app)?;
    let repo = repo_of(&persisted)?;
    let workspace = active_workspace(&persisted)?;
    pr_checks(&repo, &workspace.branch)
}

#[tauri::command]
fn merge_cmd(app: AppHandle) -> Result<String, String> {
    let (repo, branch, archive_after, id) = {
        let persisted = lock_state(&app)?;
        let repo = repo_of(&persisted)?;
        let workspace = active_workspace(&persisted)?;
        let settings = settings::load_bundle(&repo).merged;
        (repo, workspace.branch.clone(), settings.git.archive_on_merge, workspace.id.clone())
    };
    let text = merge_pr(&repo, &branch)?;
    if archive_after {
        let _ = archive_workspace_cmd(app, id);
    }
    Ok(text)
}

#[tauri::command]
fn copy_link(app: AppHandle) -> Result<String, String> {
    let persisted = lock_state(&app)?;
    let workspace = active_workspace(&persisted)?;
    Ok(format!("{}\n{}", workspace.branch, workspace.path))
}

#[tauri::command]
fn save_loadout_cmd(app: AppHandle, name: String) -> Result<Loadout, String> {
    let mut persisted = lock_state(&app)?;
    let harness = persisted.equipped_harness.clone().unwrap_or_else(|| "claude".into());
    let fast = persisted.fast;
    let loadout = save_loadout(&mut persisted, &name, &harness, fast);
    save_state(&persisted)?;
    Ok(loadout)
}

#[tauri::command]
fn equip_loadout(app: AppHandle, id: String) -> Result<(), String> {
    let mut persisted = lock_state(&app)?;
    let loadout = persisted
        .loadouts
        .iter()
        .find(|item| item.id == id)
        .cloned()
        .ok_or_else(|| "Loadout not found".to_string())?;
    persisted.equipped_harness = Some(loadout.harness);
    persisted.fast = loadout.fast;
    save_state(&persisted)
}

#[tauri::command]
fn save_routine_cmd(app: AppHandle, name: String, prompt: String, schedule: String) -> Result<Routine, String> {
    let mut persisted = lock_state(&app)?;
    let harness = persisted.equipped_harness.clone().unwrap_or_else(|| "claude".into());
    let routine = save_routine(&mut persisted, &name, &prompt, &harness, &schedule);
    save_state(&persisted)?;
    Ok(routine)
}

#[tauri::command]
fn set_routine_active(app: AppHandle, id: String, active: bool) -> Result<(), String> {
    let mut persisted = lock_state(&app)?;
    if let Some(routine) = persisted.routines.iter_mut().find(|item| item.id == id) {
        routine.active = active;
    }
    save_state(&persisted)
}

#[tauri::command]
fn run_routine(app: AppHandle, id: String) -> Result<String, String> {
    let (name, prompt, harness) = {
        let persisted = lock_state(&app)?;
        let routine = persisted
            .routines
            .iter()
            .find(|item| item.id == id)
            .ok_or_else(|| "Routine not found".to_string())?;
        (routine.name.clone(), routine.prompt.clone(), routine.harness.clone())
    };
    let record = create_workspace_cmd(app.clone(), name, "new".into())?;
    {
        let mut persisted = lock_state(&app)?;
        persisted.equipped_harness = Some(harness);
        persisted.active_workspace = Some(record.id);
        save_state(&persisted)?;
    }
    send_message(app, prompt, Vec::new())
}

#[tauri::command]
fn tick_routines(app: AppHandle) -> Result<Vec<String>, String> {
    let due = {
        let persisted = lock_state(&app)?;
        due_routines(&persisted)
    };
    let mut fired = Vec::new();
    for id in due {
        let today = std::process::Command::new("date")
            .arg("+%F")
            .output()
            .ok()
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
            .unwrap_or_default();
        {
            let mut persisted = lock_state(&app)?;
            if let Some(routine) = persisted.routines.iter_mut().find(|item| item.id == id) {
                routine.last_fired = Some(today);
            }
            save_state(&persisted)?;
        }
        let _ = run_routine(app.clone(), id.clone());
        fired.push(id);
    }
    Ok(fired)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let persisted = load_state();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Runtime {
            persisted: Arc::new(Mutex::new(persisted)),
            scripts: Arc::new(Mutex::new(HashMap::new())),
            terminal: Arc::new(Mutex::new(None)),
            agents: Arc::new(Mutex::new(HashMap::new())),
            agent_text: Arc::new(Mutex::new(HashMap::new())),
        })
        .invoke_handler(tauri::generate_handler![
            pick_repo,
            app_snapshot,
            create_workspace_cmd,
            select_workspace,
            archive_workspace_cmd,
            restore_workspace_cmd,
            save_project_settings_cmd,
            save_user_settings_cmd,
            run_script,
            workspace_diff_cmd,
            file_tree_cmd,
            search_cmd,
            list_notes_cmd,
            read_note_cmd,
            write_note_cmd,
            checkpoint_cmd,
            list_checkpoints_cmd,
            restore_checkpoint_cmd,
            spotlight_cmd,
            terminal_start,
            terminal_write,
            terminal_resize,
            new_chat_cmd,
            select_chat,
            set_harness,
            pick_attachments,
            send_message,
            stop_chat,
            add_comment,
            resolve_comment,
            send_comments,
            commit_and_push,
            pull_cmd,
            pr_status,
            checks_cmd,
            merge_cmd,
            copy_link,
            save_loadout_cmd,
            equip_loadout,
            save_routine_cmd,
            set_routine_active,
            run_routine,
            tick_routines
        ])
        .run(tauri::generate_context!())
        .expect("error while running Insulator");
}
