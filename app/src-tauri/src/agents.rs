use crate::settings::Settings;
use serde::Serialize;
use std::io::{BufRead, BufReader};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::thread;

#[derive(Debug, Clone, Serialize)]
pub struct Harness {
    pub id: String,
    pub label: String,
    pub path: String,
    pub installed: bool,
}

pub struct AgentProcess {
    pub pgid: i32,
}

pub fn detect_harnesses(settings: &Settings) -> Vec<Harness> {
    vec![
        harness("claude", "Claude Code", settings.claude_code_executable_path.as_deref(), "claude"),
        harness("codex", "Codex", settings.codex_executable_path.as_deref(), "codex"),
        harness("cursor", "Cursor", None, "cursor-agent"),
        harness("opencode", "OpenCode", settings.opencode_executable_path.as_deref(), "opencode"),
    ]
}

fn harness(id: &str, label: &str, configured: Option<&str>, fallback: &str) -> Harness {
    let path = configured
        .filter(|item| !item.is_empty() && Path::new(item).exists())
        .map(str::to_string)
        .or_else(|| which(fallback))
        .unwrap_or_else(|| fallback.to_string());
    let installed = Path::new(&path).exists() || which(&path).is_some();
    Harness {
        id: id.into(),
        label: label.into(),
        path,
        installed,
    }
}

fn which(name: &str) -> Option<String> {
    let output = Command::new("which").arg(name).output().ok()?;
    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if path.is_empty() { None } else { Some(path) }
    } else {
        None
    }
}

pub fn spawn_agent(
    workspace: &Path,
    workspace_name: &str,
    repo: &Path,
    port: u16,
    harness_id: &str,
    settings: &Settings,
    prompt: &str,
    attachments: &[String],
    on_output: Arc<dyn Fn(String) + Send + Sync>,
    on_exit: Arc<dyn Fn() + Send + Sync>,
) -> Result<AgentProcess, String> {
    let found = detect_harnesses(settings)
        .into_iter()
        .find(|item| item.id == harness_id)
        .ok_or_else(|| format!("Unknown harness {harness_id}"))?;
    if !found.installed {
        return Err(format!("{} is not installed on this Mac", found.label));
    }
    let mut command = Command::new(&found.path);
    match harness_id {
        "claude" => {
            command.args(["-p", "--output-format", "text", "--dangerously-skip-permissions", prompt]);
        }
        "codex" => {
            command.arg("exec");
            for path in attachments {
                let lower = path.to_ascii_lowercase();
                if lower.ends_with(".png") || lower.ends_with(".jpg") || lower.ends_with(".jpeg") || lower.ends_with(".webp") || lower.ends_with(".gif") {
                    command.args(["-i", path]);
                }
            }
            command.args(["-s", "workspace-write", "--approve-for-me", prompt]);
        }
        "cursor" => {
            command.args(["-p", "--force", "--output-format", "text", prompt]);
        }
        "opencode" => {
            command.args(["run", "--format", "default", "--dir"]);
            command.arg(workspace);
            for path in attachments {
                command.args(["--file", path]);
            }
            command.arg(prompt);
        }
        other => return Err(format!("Unknown harness {other}")),
    }
    command.current_dir(workspace).env("GIT_TERMINAL_PROMPT", "0");
    for (key, value) in &settings.env {
        command.env(key, value);
    }
    let branch = crate::git_ops::git(repo, &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"])
        .ok()
        .map(|name| name.trim_start_matches("origin/").to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "main".into());
    command
        .env("INSULATOR_WORKSPACE_PATH", workspace)
        .env("INSULATOR_ROOT_PATH", repo)
        .env("INSULATOR_PORT", port.to_string())
        .env("INSULATOR_IS_LOCAL", "1")
        .env("INSULATOR_WORKSPACE_NAME", workspace_name)
        .env("INSULATOR_DEFAULT_BRANCH", branch);
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()
        .map_err(|err| format!("Could not start {}: {err}", found.label))?;
    let pgid = child.id() as i32;
    forward(child.stdout.take(), Arc::clone(&on_output));
    forward(child.stderr.take(), on_output);
    thread::spawn(move || {
        let _ = child.wait();
        on_exit();
    });
    Ok(AgentProcess { pgid })
}

fn forward<R>(stream: Option<R>, on_output: Arc<dyn Fn(String) + Send + Sync>)
where
    R: std::io::Read + Send + 'static,
{
    let Some(stream) = stream else { return };
    thread::spawn(move || {
        let reader = BufReader::new(stream);
        for line in reader.lines().map_while(Result::ok) {
            on_output(line);
        }
    });
}

pub fn stop_agent(job: &AgentProcess) {
    unsafe {
        libc::kill(-job.pgid, libc::SIGTERM);
    }
}
