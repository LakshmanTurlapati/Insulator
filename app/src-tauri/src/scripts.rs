use std::io::{BufRead, BufReader};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::thread;

pub struct ScriptProcess {
    pub pgid: i32,
}

pub fn spawn_script(
    workspace: &Path,
    command: &str,
    repo: &Path,
    port: u16,
    extra_env: &[(&str, &str)],
    relative_cwd: Option<&str>,
    on_output: Arc<dyn Fn(String, String) + Send + Sync>,
    on_exit: Arc<dyn Fn() + Send + Sync>,
) -> Result<ScriptProcess, String> {
    let cwd = relative_cwd
        .filter(|path| !path.is_empty() && !path.contains(".."))
        .map(|path| workspace.join(path))
        .unwrap_or_else(|| workspace.to_path_buf());
    let mut child = Command::new("zsh");
    child
        .arg("-lc")
        .arg(command)
        .current_dir(cwd)
        .env("GIT_TERMINAL_PROMPT", "0");
    for (key, value) in extra_env {
        child.env(key, value);
    }
    child
        .env("INSULATOR_WORKSPACE_PATH", workspace)
        .env("INSULATOR_ROOT_PATH", repo)
        .env("INSULATOR_PORT", port.to_string())
        .env("INSULATOR_IS_LOCAL", "1");
    let mut child = child
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()
        .map_err(|err| format!("Could not start script: {err}"))?;
    let pgid = child.id() as i32;
    forward(child.stdout.take(), "stdout", Arc::clone(&on_output));
    forward(child.stderr.take(), "stderr", on_output);
    thread::spawn(move || {
        let _ = child.wait();
        on_exit();
    });
    Ok(ScriptProcess { pgid })
}

fn forward<R>(stream: Option<R>, name: &str, on_output: Arc<dyn Fn(String, String) + Send + Sync>)
where
    R: std::io::Read + Send + 'static,
{
    let Some(stream) = stream else { return };
    let name = name.to_string();
    thread::spawn(move || {
        let reader = BufReader::new(stream);
        for line in reader.lines().map_while(Result::ok) {
            on_output(name.clone(), line);
        }
    });
}

pub fn stop_script(job: &ScriptProcess) -> Result<(), String> {
    let pgid = job.pgid;
    unsafe {
        libc::kill(-pgid, libc::SIGTERM);
    }
    thread::spawn(move || {
        thread::sleep(std::time::Duration::from_secs(5));
        let still_alive = unsafe { libc::kill(-pgid, 0) } == 0;
        if still_alive {
            unsafe {
                libc::kill(-pgid, libc::SIGKILL);
            }
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::time::Duration;

    #[test]
    fn stop_kills_process_group() {
        let dir = std::env::temp_dir();
        let lines = Arc::new(Mutex::new(Vec::new()));
        let capture = Arc::clone(&lines);
        let exited = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = Arc::clone(&exited);
        let job = spawn_script(
            &dir,
            "echo started; sleep 30",
            &dir,
            5173,
            &[],
            None,
            Arc::new(move |stream, text| {
                capture.lock().unwrap().push(format!("{stream}:{text}"));
            }),
            Arc::new(move || flag.store(true, std::sync::atomic::Ordering::SeqCst)),
        )
        .unwrap();
        thread::sleep(Duration::from_millis(150));
        stop_script(&job).unwrap();
        for _ in 0..20 {
            if exited.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }
        assert!(exited.load(std::sync::atomic::Ordering::SeqCst));
        let status = Command::new("ps")
            .args(["-p", &job.pgid.to_string()])
            .status()
            .unwrap();
        assert!(!status.success());
        let logged = lines.lock().unwrap().join("\n");
        assert!(logged.contains("started"));
    }
}
