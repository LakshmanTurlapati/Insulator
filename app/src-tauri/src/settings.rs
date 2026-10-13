use crate::git_ops::git;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunScript {
    pub id: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub default: bool,
    #[serde(default)]
    pub hide: bool,
    #[serde(default = "yes")]
    pub local: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitPrefs {
    #[serde(default = "yes")]
    pub rename_branch: bool,
    #[serde(default = "yes")]
    pub archive_on_merge: bool,
    #[serde(default)]
    pub delete_branch_on_archive: bool,
    #[serde(default)]
    pub include_workspace_link: bool,
}

fn yes() -> bool {
    true
}

impl Default for GitPrefs {
    fn default() -> Self {
        Self {
            rename_branch: true,
            archive_on_merge: true,
            delete_branch_on_archive: false,
            include_workspace_link: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub setup: Option<String>,
    pub archive: Option<String>,
    #[serde(default)]
    pub auto_run_after_setup: bool,
    pub run_mode: String,
    pub run: Vec<RunScript>,
    pub file_include_globs: String,
    pub env: BTreeMap<String, String>,
    pub claude_code_executable_path: Option<String>,
    pub codex_executable_path: Option<String>,
    pub opencode_executable_path: Option<String>,
    pub review_prompt: Option<String>,
    pub git: GitPrefs,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            setup: None,
            archive: None,
            auto_run_after_setup: false,
            run_mode: "nonconcurrent".into(),
            run: Vec::new(),
            file_include_globs: String::new(),
            env: BTreeMap::new(),
            claude_code_executable_path: None,
            codex_executable_path: None,
            opencode_executable_path: None,
            review_prompt: None,
            git: GitPrefs::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SettingsBundle {
    pub merged: Settings,
    pub local: Settings,
    pub shared: Settings,
    pub user: Settings,
}

pub fn insulator_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home).join(".insulator")
}

pub fn load_bundle(repo: &Path) -> SettingsBundle {
    let user_path = insulator_dir().join("settings.toml");
    let managed_path = insulator_dir().join("settings.managed.toml");
    let shared_path = repo.join(".insulator/settings.toml");
    let local_path = repo.join(".insulator/settings.local.toml");
    let user = parse_settings_value(&read_toml(&user_path));
    let shared = parse_settings_value(&read_toml(&shared_path));
    let local = parse_settings_value(&read_toml(&local_path));
    let mut merged_value = read_toml(&user_path);
    overlay(&mut merged_value, &read_toml(&shared_path));
    overlay(&mut merged_value, &read_toml(&local_path));
    overlay(&mut merged_value, &read_toml(&managed_path));
    let mut merged = parse_settings_value(&merged_value);
    if let Some(patterns) = worktreeinclude(repo) {
        merged.file_include_globs = patterns;
    }
    SettingsBundle {
        merged,
        local,
        shared,
        user,
    }
}

pub fn default_run_command(settings: &Settings) -> String {
    settings
        .run
        .iter()
        .find(|script| script.default && !script.hide && script.local)
        .or_else(|| settings.run.iter().find(|script| !script.hide && script.local))
        .map(script_line)
        .unwrap_or_else(|| "echo no run script".into())
}

pub fn save_project_settings(
    repo: &Path,
    shared: bool,
    setup: &str,
    run: &str,
    archive: &str,
    globs: &str,
    env_text: &str,
    review: &str,
) -> Result<(), String> {
    let path = if shared {
        repo.join(".insulator/settings.toml")
    } else {
        repo.join(".insulator/settings.local.toml")
    };
    let mut value = read_toml(&path);
    let table = value.as_table_mut().ok_or("settings root must be a table")?;
    let scripts = table.entry("scripts").or_insert(toml::Value::Table(toml::map::Map::new()));
    let scripts = scripts.as_table_mut().ok_or("scripts must be a table")?;
    scripts.insert("setup".into(), toml::Value::String(setup.trim().to_string()));
    scripts.insert("archive".into(), toml::Value::String(archive.trim().to_string()));
    scripts.insert(
        "run".into(),
        toml::Value::Table({
            let mut run_table = toml::map::Map::new();
            let mut dev = toml::map::Map::new();
            dev.insert("command".into(), toml::Value::String(run.trim().to_string()));
            dev.insert("default".into(), toml::Value::Boolean(true));
            run_table.insert("dev".into(), toml::Value::Table(dev));
            run_table
        }),
    );
    table.insert("file_include_globs".into(), toml::Value::String(globs.trim().to_string()));
    table.insert("review_prompt".into(), toml::Value::String(review.trim().to_string()));
    table.insert("environment_variables".into(), toml::Value::Table(env_table(env_text)));
    write_toml(&path, &value)
}

pub fn save_user_git(prefs: &GitPrefs, claude: &str, codex: &str, opencode: &str) -> Result<(), String> {
    let path = insulator_dir().join("settings.toml");
    let mut value = read_toml(&path);
    let table = value.as_table_mut().ok_or("settings root must be a table")?;
    table.insert(
        "git".into(),
        toml::Value::Table({
            let mut git = toml::map::Map::new();
            git.insert("rename_branch".into(), toml::Value::Boolean(prefs.rename_branch));
            git.insert("archive_on_merge".into(), toml::Value::Boolean(prefs.archive_on_merge));
            git.insert(
                "delete_branch_on_archive".into(),
                toml::Value::Boolean(prefs.delete_branch_on_archive),
            );
            git.insert(
                "include_workspace_link".into(),
                toml::Value::Boolean(prefs.include_workspace_link),
            );
            git
        }),
    );
    if !claude.trim().is_empty() {
        table.insert(
            "claude_code_executable_path".into(),
            toml::Value::String(claude.trim().into()),
        );
    }
    if !codex.trim().is_empty() {
        table.insert("codex_executable_path".into(), toml::Value::String(codex.trim().into()));
    }
    if !opencode.trim().is_empty() {
        table.insert(
            "opencode_executable_path".into(),
            toml::Value::String(opencode.trim().into()),
        );
    }
    write_toml(&path, &value)
}

pub fn copy_included_files(repo: &Path, dest: &Path, settings: &Settings) -> Result<usize, String> {
    let patterns = include_patterns(repo, settings);
    if patterns.is_empty() {
        return Ok(0);
    }
    let listed = git(repo, &["ls-files", "--others", "-i", "--exclude-standard"]).unwrap_or_default();
    let mut copied = 0usize;
    for line in listed.lines() {
        if line.is_empty() || !patterns.iter().any(|pattern| glob_match(pattern, line)) {
            continue;
        }
        let from = repo.join(line);
        if !from.is_file() {
            continue;
        }
        let bytes = from.metadata().map(|meta| meta.len()).unwrap_or(0);
        if bytes > 2_000_000 {
            continue;
        }
        let to = dest.join(line);
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        fs::copy(&from, &to).map_err(|err| err.to_string())?;
        copied += 1;
    }
    Ok(copied)
}

fn include_patterns(repo: &Path, settings: &Settings) -> Vec<String> {
    if let Some(text) = worktreeinclude(repo) {
        return split_patterns(&text);
    }
    split_patterns(&settings.file_include_globs)
}

fn worktreeinclude(repo: &Path) -> Option<String> {
    fs::read_to_string(repo.join(".worktreeinclude")).ok()
}

fn split_patterns(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect()
}

pub fn glob_match(pattern: &str, path: &str) -> bool {
    let pattern = pattern.trim().trim_start_matches('/');
    if pattern.is_empty() || pattern.starts_with('#') {
        return false;
    }
    let path = path.trim_start_matches("./");
    if pattern.contains('/') {
        wild(pattern.as_bytes(), path.as_bytes())
    } else {
        path.split('/').any(|segment| wild(pattern.as_bytes(), segment.as_bytes()))
    }
}

fn wild(pattern: &[u8], text: &[u8]) -> bool {
    let mut i = 0;
    let mut j = 0;
    while i < pattern.len() {
        if pattern[i] == b'*' && i + 1 < pattern.len() && pattern[i + 1] == b'*' {
            let rest = if i + 2 < pattern.len() && pattern[i + 2] == b'/' {
                &pattern[i + 3..]
            } else {
                &pattern[i + 2..]
            };
            let mut k = j;
            loop {
                if wild(rest, &text[k..]) {
                    return true;
                }
                if k >= text.len() {
                    return false;
                }
                k += 1;
            }
        }
        if pattern[i] == b'*' {
            i += 1;
            while j <= text.len() {
                if j < text.len() && text[j] == b'/' {
                    break;
                }
                if wild(&pattern[i..], &text[j..]) {
                    return true;
                }
                if j == text.len() {
                    return false;
                }
                j += 1;
            }
            return wild(&pattern[i..], &text[j..]);
        }
        if j >= text.len() {
            return false;
        }
        if pattern[i] == b'?' {
            if text[j] == b'/' {
                return false;
            }
            i += 1;
            j += 1;
            continue;
        }
        if pattern[i] != text[j] {
            return false;
        }
        i += 1;
        j += 1;
    }
    j == text.len()
}

pub fn script_line(script: &RunScript) -> String {
    if script.args.is_empty() {
        script.command.clone()
    } else {
        let args = script.args.iter().map(|arg| shell_quote(arg)).collect::<Vec<_>>().join(" ");
        format!("{} {args}", script.command)
    }
}

pub fn script_cwd<'a>(settings: &'a Settings, script_id: Option<&str>) -> Option<&'a str> {
    let script = script_id
        .and_then(|id| settings.run.iter().find(|script| script.id == id))
        .or_else(|| settings.run.iter().find(|script| script.default && script.local))
        .or_else(|| settings.run.iter().find(|script| script.local));
    script.and_then(|script| script.cwd.as_deref())
}

fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

fn script_available_locally(entry: &toml::Value) -> bool {
    match entry.get("available_in") {
        None => true,
        Some(value) => match value.as_str() {
            Some("cloud") => false,
            Some(_) => true,
            None => value
                .as_array()
                .map(|items| items.iter().any(|item| item.as_str() == Some("local")))
                .unwrap_or(true),
        },
    }
}

fn insert_env(env: &mut BTreeMap<String, String>, table: &toml::map::Map<String, toml::Value>) {
    for (name, item) in table {
        if name == "cloud" {
            continue;
        }
        if name == "local" {
            if let Some(inner) = item.as_table() {
                insert_env(env, inner);
            }
            continue;
        }
        if let Some(text) = item.as_str() {
            env.insert(name.clone(), text.to_string());
        }
    }
}

fn env_table(text: &str) -> toml::map::Map<String, toml::Value> {
    let mut map = toml::map::Map::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            map.insert(key.trim().into(), toml::Value::String(value.trim().into()));
        }
    }
    map
}

fn read_toml(path: &Path) -> toml::Value {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or(toml::Value::Table(toml::map::Map::new()))
}

fn write_toml(path: &Path, value: &toml::Value) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let text = toml::to_string_pretty(value).map_err(|err| err.to_string())?;
    fs::write(path, text).map_err(|err| err.to_string())
}

fn overlay(base: &mut toml::Value, top: &toml::Value) {
    match (base, top) {
        (toml::Value::Table(left), toml::Value::Table(right)) => {
            for (key, value) in right {
                if let Some(existing) = left.get_mut(key) {
                    overlay(existing, value);
                } else {
                    left.insert(key.clone(), value.clone());
                }
            }
        }
        (slot, value) => *slot = value.clone(),
    }
}

fn parse_settings_value(value: &toml::Value) -> Settings {
    let mut settings = Settings::default();
    let scripts = value.get("scripts");
    settings.setup = scripts
        .and_then(|item| item.get("setup"))
        .and_then(|item| item.as_str())
        .map(str::to_string);
    settings.archive = scripts
        .and_then(|item| item.get("archive"))
        .and_then(|item| item.as_str())
        .map(str::to_string);
    if let Some(mode) = scripts.and_then(|item| item.get("run_mode")).and_then(|item| item.as_str()) {
        settings.run_mode = mode.to_string();
    }
    if let Some(legacy) = scripts.and_then(|item| item.get("run")).and_then(|item| item.as_str()) {
        settings.run.push(RunScript {
            id: "run".into(),
            command: legacy.to_string(),
            args: Vec::new(),
            cwd: None,
            default: true,
            hide: false,
            local: true,
        });
    }
    settings.auto_run_after_setup = scripts
        .and_then(|item| item.get("auto_run_after_setup"))
        .and_then(|item| item.as_bool())
        .unwrap_or(false);
    if let Some(table) = scripts.and_then(|item| item.get("run")).and_then(|item| item.as_table()) {
        for (id, entry) in table {
            let command = entry.get("command").and_then(|item| item.as_str()).unwrap_or("").to_string();
            if command.is_empty() || !script_available_locally(entry) {
                continue;
            }
            let args = entry
                .get("args")
                .and_then(|item| item.as_array())
                .map(|items| items.iter().filter_map(|item| item.as_str().map(str::to_string)).collect())
                .unwrap_or_default();
            let cwd = entry
                .get("options")
                .and_then(|item| item.get("cwd"))
                .and_then(|item| item.as_str())
                .or_else(|| entry.get("cwd").and_then(|item| item.as_str()))
                .map(str::to_string);
            settings.run.push(RunScript {
                id: id.clone(),
                command,
                args,
                cwd,
                default: entry.get("default").and_then(|item| item.as_bool()).unwrap_or(false),
                hide: entry.get("hide").and_then(|item| item.as_bool()).unwrap_or(false),
                local: true,
            });
        }
    }
    if let Some(globs) = value.get("file_include_globs").and_then(|item| item.as_str()) {
        settings.file_include_globs = globs.to_string();
    } else if value.get("file_include_globs").is_none() {
        settings.file_include_globs = ".env*\n".into();
    }
    for key in ["env", "environment", "environment_variables"] {
        if let Some(table) = value.get(key).and_then(|item| item.as_table()) {
            insert_env(&mut settings.env, table);
        }
    }
    settings.claude_code_executable_path = value
        .get("claude_code_executable_path")
        .and_then(|item| item.as_str())
        .map(str::to_string);
    settings.codex_executable_path = value
        .get("codex_executable_path")
        .and_then(|item| item.as_str())
        .map(str::to_string);
    settings.opencode_executable_path = value
        .get("opencode_executable_path")
        .and_then(|item| item.as_str())
        .map(str::to_string);
    settings.review_prompt = value.get("review_prompt").and_then(|item| item.as_str()).map(str::to_string);
    if let Some(git) = value.get("git") {
        settings.git.rename_branch = git.get("rename_branch").and_then(|item| item.as_bool()).unwrap_or(true);
        settings.git.archive_on_merge = git
            .get("archive_on_merge")
            .and_then(|item| item.as_bool())
            .unwrap_or(true);
        settings.git.delete_branch_on_archive = git
            .get("delete_branch_on_archive")
            .and_then(|item| item.as_bool())
            .unwrap_or(false);
        settings.git.include_workspace_link = git
            .get("include_workspace_link")
            .and_then(|item| item.as_bool())
            .unwrap_or(false);
    }
    settings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn higher_layer_overrides_setup() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("insulator-settings-{nanos}"));
        let repo = root.join("repo");
        fs::create_dir_all(repo.join(".insulator")).unwrap();
        fs::write(
            repo.join(".insulator/settings.toml"),
            "[scripts]\nsetup = \"shared\"\n",
        )
        .unwrap();
        fs::write(
            repo.join(".insulator/settings.local.toml"),
            "[scripts]\nsetup = \"local\"\n\n[scripts.run.dev]\ncommand = \"echo dev\"\ndefault = true\n",
        )
        .unwrap();
        let bundle = load_bundle(&repo);
        assert_eq!(bundle.merged.setup.as_deref(), Some("local"));
        assert_eq!(default_run_command(&bundle.merged), "echo dev");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn reads_insulator_environment_and_local_scripts() {
        let value: toml::Value = toml::from_str(
            r#"
[environment_variables]
SHARED = "1"

[environment_variables.local]
LOCAL = "2"

[environment_variables.cloud]
CLOUD = "3"

[scripts.run.hidden]
command = "echo cloud"
available_in = "cloud"

[scripts.run.dev]
command = "echo dev"
default = true
args = ["--port", "1"]

[scripts.run.web]
command = "echo web"

[scripts.run.web.options]
cwd = "apps/web"
"#,
        )
        .unwrap();
        let settings = parse_settings_value(&value);
        assert_eq!(settings.env.get("SHARED").map(String::as_str), Some("1"));
        assert_eq!(settings.env.get("LOCAL").map(String::as_str), Some("2"));
        assert!(settings.env.get("CLOUD").is_none());
        assert!(settings.run.iter().all(|script| script.id != "hidden"));
        assert_eq!(default_run_command(&settings), "echo dev '--port' '1'");
        assert_eq!(
            settings.run.iter().find(|script| script.id == "web").and_then(|script| script.cwd.as_deref()),
            Some("apps/web")
        );
        assert_eq!(settings.file_include_globs, ".env*\n");
    }

    #[test]
    fn matches_env_and_nested_glob() {
        assert!(glob_match(".env*", ".env.local"));
        assert!(glob_match(".env*", "src/.env.local"));
        assert!(glob_match("config/*.local.json", "config/app.local.json"));
        assert!(!glob_match("config/*.local.json", ".env.local"));
    }

    #[test]
    fn copies_gitignored_env() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let repo = std::env::temp_dir().join(format!("insulator-copy-{nanos}"));
        fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-b", "main"]).unwrap();
        fs::write(repo.join(".gitignore"), ".env*\n").unwrap();
        fs::write(repo.join(".env.local"), "TOKEN=1\n").unwrap();
        fs::write(repo.join("README.md"), "hi\n").unwrap();
        git(&repo, &["add", ".gitignore", "README.md"]).unwrap();
        git(&repo, &["config", "user.email", "test@example.com"]).unwrap();
        git(&repo, &["config", "user.name", "Insulator Test"]).unwrap();
        git(&repo, &["commit", "-m", "init"]).unwrap();
        let dest = std::env::temp_dir().join(format!("insulator-copy-dest-{nanos}"));
        fs::create_dir_all(&dest).unwrap();
        let mut settings = Settings::default();
        settings.file_include_globs = ".env*\n".into();
        let copied = copy_included_files(&repo, &dest, &settings).unwrap();
        assert_eq!(copied, 1);
        assert_eq!(fs::read_to_string(dest.join(".env.local")).unwrap(), "TOKEN=1\n");
        let _ = fs::remove_dir_all(repo);
        let _ = fs::remove_dir_all(dest);
    }
}
