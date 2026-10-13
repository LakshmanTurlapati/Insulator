use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;

pub struct TerminalSession {
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    master: Arc<Mutex<Box<dyn portable_pty::MasterPty + Send>>>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
}

pub fn start_terminal(
    workspace: &Path,
    repo: &Path,
    port: u16,
    name: &str,
    branch: &str,
    extra_env: &[(&str, &str)],
    on_output: Arc<dyn Fn(String) + Send + Sync>,
) -> Result<TerminalSession, String> {
    let system = native_pty_system();
    let pair = system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|err| err.to_string())?;
    let mut command = CommandBuilder::new("zsh");
    command.arg("-l");
    command.cwd(workspace);
    for (key, value) in extra_env {
        command.env(*key, *value);
    }
    command.env("INSULATOR_WORKSPACE_PATH", workspace);
    command.env("INSULATOR_ROOT_PATH", repo);
    command.env("INSULATOR_PORT", port.to_string());
    command.env("INSULATOR_IS_LOCAL", "1");
    command.env("INSULATOR_WORKSPACE_NAME", name);
    command.env("INSULATOR_DEFAULT_BRANCH", branch);
    let child = pair
        .slave
        .spawn_command(command)
        .map_err(|err| err.to_string())?;
    let reader = pair.master.try_clone_reader().map_err(|err| err.to_string())?;
    let writer = pair.master.take_writer().map_err(|err| err.to_string())?;
    thread::spawn(move || {
        let mut reader = reader;
        let mut buffer = [0u8; 4096];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(count) => {
                    let text = String::from_utf8_lossy(&buffer[..count]).to_string();
                    on_output(text);
                }
            }
        }
    });
    Ok(TerminalSession {
        writer: Arc::new(Mutex::new(writer)),
        master: Arc::new(Mutex::new(pair.master)),
        child,
    })
}

impl TerminalSession {
    pub fn write(&self, data: &str) -> Result<(), String> {
        let mut writer = self.writer.lock().map_err(|err| err.to_string())?;
        writer
            .write_all(data.as_bytes())
            .map_err(|err| err.to_string())?;
        writer.flush().map_err(|err| err.to_string())
    }

    pub fn resize(&self, rows: u16, cols: u16) -> Result<(), String> {
        let master = self.master.lock().map_err(|err| err.to_string())?;
        master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|err| err.to_string())
    }

}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}
