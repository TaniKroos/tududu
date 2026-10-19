//! Runs text models in llama.cpp's `llama-server`, which ships inside the app
//! in its `llama/` folder (staged by build.rs).
//!
//! Tududu starts the server when a text task needs it, with the selected text
//! model, on a random port that only this computer can reach, and stops it
//! after the same idle time as the speech model (the model-unload setting), so
//! it holds no memory while unused. It runs as a separate process, like the
//! speech engine's worker, so its copy of ggml can't clash with
//! transcribe-cpp's.

use crate::settings::get_settings;
use anyhow::{anyhow, Context, Result};
use log::{debug, info, warn};
use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};
use tokio::sync::Mutex;

/// How long a model gets to load before the start counts as failed.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(90);
/// Context window, in tokens: room for a long selection and its translation.
const CONTEXT_TOKENS: &str = "4096";

/// A running `llama-server`.
struct Server {
    child: Child,
    port: u16,
    model: PathBuf,
}

impl Drop for Server {
    fn drop(&mut self) {
        stop_process(&mut self.child);
    }
}

/// A command that starts the engine so it can't outlive Tududu, even if
/// Tududu crashes or is force-quit (a normal quit stops it explicitly).
///
/// macOS has no "kill my child when I die", so a shell watchdog starts the
/// engine and stops it once Tududu's process is gone. Both run in their own
/// process group, which [`stop_process`] signals as a whole.
#[cfg(unix)]
fn engine_command(binary: &Path) -> Command {
    use std::os::unix::process::CommandExt;
    // $1: Tududu's pid, $2: the engine, the rest: its arguments.
    const WATCHDOG: &str = r#"parent=$1; shift; engine=$1; shift
"$engine" "$@" &
child=$!
trap 'kill "$child" 2>/dev/null; wait "$child"; exit 0' TERM INT HUP
while kill -0 "$parent" 2>/dev/null && kill -0 "$child" 2>/dev/null; do sleep 1; done
kill "$child" 2>/dev/null
wait "$child""#;
    let mut command = Command::new("/bin/sh");
    command
        .arg("-c")
        .arg(WATCHDOG)
        .arg("tududu-text-engine") // $0
        .arg(std::process::id().to_string())
        .arg(binary);
    command.process_group(0);
    command
}

/// See the unix variant; on Windows the engine joins a kill-on-close job
/// after it starts ([`kill_with_tududu`]).
#[cfg(windows)]
fn engine_command(binary: &Path) -> Command {
    use std::os::windows::process::CommandExt;
    let mut command = Command::new(binary);
    // CREATE_NO_WINDOW: no console window flashing up.
    command.creation_flags(0x0800_0000);
    command
}

/// Windows: put the engine in a job that Windows closes, killing the engine,
/// when Tududu exits for any reason. The job's handle is deliberately never
/// closed.
#[cfg(windows)]
fn kill_with_tududu(child: &Child) {
    use std::os::windows::io::AsRawHandle;
    use std::sync::OnceLock;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    // One job for the app's lifetime, kept as an address (HANDLE isn't Sync).
    static JOB: OnceLock<Option<usize>> = OnceLock::new();
    let job = JOB.get_or_init(|| {
        // SAFETY: plain Win32 calls on a job object this function owns; `info`
        // outlives the call that reads it.
        unsafe {
            let job = CreateJobObjectW(None, windows::core::PCWSTR::null()).ok()?;
            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const std::ffi::c_void,
                std::mem::size_of_val(&info) as u32,
            )
            .ok()?;
            Some(job.0 as usize)
        }
    });
    let Some(job) = job else {
        warn!("Couldn't create a job object; the text engine may outlive a crash");
        return;
    };
    // SAFETY: both handles are valid: the job lives forever and the child was
    // just spawned.
    let assigned =
        unsafe { AssignProcessToJobObject(HANDLE(*job as *mut _), HANDLE(child.as_raw_handle())) };
    if let Err(e) = assigned {
        warn!("Couldn't tie the text engine to Tududu's lifetime: {e}");
    }
}

/// Stop the engine: SIGTERM to its process group (watchdog and engine), then
/// SIGKILL if it hasn't exited after 3 s.
#[cfg(unix)]
fn stop_process(child: &mut Child) {
    let group = -(child.id() as i32);
    // SAFETY: signalling our own child's process group.
    unsafe { libc::kill(group, libc::SIGTERM) };
    for _ in 0..30 {
        if let Ok(Some(_)) = child.try_wait() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    // SAFETY: as above.
    unsafe { libc::kill(group, libc::SIGKILL) };
    let _ = child.wait();
}

#[cfg(windows)]
fn stop_process(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

pub struct TextEngine {
    app_handle: AppHandle,
    server: Mutex<Option<Server>>,
    last_used: Arc<StdMutex<Instant>>,
    http: reqwest::Client,
}

impl TextEngine {
    pub fn new(app_handle: &AppHandle) -> Arc<Self> {
        let engine = Arc::new(Self {
            app_handle: app_handle.clone(),
            server: Mutex::new(None),
            last_used: Arc::new(StdMutex::new(Instant::now())),
            http: reqwest::Client::new(),
        });
        Self::spawn_idle_watcher(&engine);
        engine
    }

    /// The bundled `llama-server`, if this build includes it.
    fn server_binary(&self) -> Result<PathBuf> {
        let name = if cfg!(windows) {
            "llama/llama-server.exe"
        } else {
            "llama/llama-server"
        };
        let path = self
            .app_handle
            .path()
            .resolve(name, tauri::path::BaseDirectory::Resource)
            .context("Couldn't locate the app's resources")?;
        if path.exists() {
            Ok(path)
        } else {
            Err(anyhow!(
                "This build of Tududu doesn't include the text engine ({} is missing)",
                path.display()
            ))
        }
    }

    /// Translate `text` into English with the text model at `model`.
    pub async fn translate_to_english(&self, model: &Path, text: &str) -> Result<String> {
        let port = self.ensure_server(model).await?;
        self.touch();
        let started = Instant::now();
        let body = serde_json::json!({
            "messages": [{ "role": "user", "content": english_translation_prompt(text) }],
            "temperature": 0,
            // Translations run about as long as the source; leave headroom.
            "max_tokens": (text.chars().count() * 2).clamp(64, 2048),
        });
        let response: serde_json::Value = self
            .http
            .post(format!("http://127.0.0.1:{port}/v1/chat/completions"))
            .json(&body)
            .timeout(Duration::from_secs(120))
            .send()
            .await
            .context("The text engine didn't respond")?
            .error_for_status()
            .context("The text engine rejected the request")?
            .json()
            .await
            .context("The text engine sent an unreadable reply")?;
        self.touch();
        let content = response["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| anyhow!("The text engine's reply had no text"))?;
        let english = clean_translation(content);
        info!(
            "Translated {} chars to English in {:.2}s",
            text.chars().count(),
            started.elapsed().as_secs_f64()
        );
        Ok(english)
    }

    /// Start the server with `model` unless it's already running with it.
    async fn ensure_server(&self, model: &Path) -> Result<u16> {
        let mut server = self.server.lock().await;
        if let Some(running) = server.as_mut() {
            let alive = matches!(running.child.try_wait(), Ok(None));
            if alive && running.model == model {
                return Ok(running.port);
            }
            if !alive {
                warn!("The text engine had exited; restarting it");
            }
        }
        // A different model, or a dead server: replace it.
        *server = None;

        let binary = self.server_binary()?;
        let port = free_port()?;
        info!(
            "Starting the text engine with {} on port {}",
            model.display(),
            port
        );
        let mut command = engine_command(&binary);
        command
            .arg("-m")
            .arg(model)
            .args(["--host", "127.0.0.1", "--port", &port.to_string()])
            .args(["-c", CONTEXT_TOKENS, "-np", "1", "-ngl", "99"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        if let Some(dir) = binary.parent() {
            command.current_dir(dir);
        }
        let mut child = command
            .spawn()
            .with_context(|| format!("Couldn't start {}", binary.display()))?;
        #[cfg(windows)]
        kill_with_tududu(&child);
        if let Some(stderr) = child.stderr.take() {
            std::thread::spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(|l| l.ok()) {
                    debug!("[llama-server] {line}");
                }
            });
        }
        let mut started = Server {
            child,
            port,
            model: model.to_path_buf(),
        };
        self.wait_until_healthy(&mut started).await?;
        info!("Text engine ready on port {}", port);
        *server = Some(started);
        Ok(port)
    }

    async fn wait_until_healthy(&self, server: &mut Server) -> Result<()> {
        let started = Instant::now();
        let url = format!("http://127.0.0.1:{}/health", server.port);
        loop {
            if let Ok(Some(status)) = server.child.try_wait() {
                return Err(anyhow!("The text engine stopped while starting ({status})"));
            }
            let healthy = self
                .http
                .get(&url)
                .timeout(Duration::from_secs(2))
                .send()
                .await
                .map(|r| r.status().is_success())
                .unwrap_or(false);
            if healthy {
                return Ok(());
            }
            if started.elapsed() > STARTUP_TIMEOUT {
                return Err(anyhow!(
                    "The text engine didn't start within {}s",
                    STARTUP_TIMEOUT.as_secs()
                ));
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    fn touch(&self) {
        *self.last_used.lock().unwrap() = Instant::now();
    }

    /// Stop the server, freeing the model's memory.
    pub async fn stop(&self) {
        if self.server.lock().await.take().is_some() {
            info!("Stopped the text engine");
        }
    }

    /// Stop the server from a non-async context, e.g. on app exit.
    pub fn stop_blocking(&self) {
        if let Ok(mut server) = self.server.try_lock() {
            if server.take().is_some() {
                info!("Stopped the text engine");
            }
        }
    }

    /// Stop an idle server after the model-unload timeout.
    fn spawn_idle_watcher(engine: &Arc<Self>) {
        let weak = Arc::downgrade(engine);
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(10));
            let Some(engine) = weak.upgrade() else {
                break;
            };
            let Some(limit) = get_settings(&engine.app_handle)
                .model_unload_timeout
                .to_seconds()
            else {
                continue; // Never unload.
            };
            let idle = engine.last_used.lock().unwrap().elapsed();
            // "Immediately" (0) still gives a follow-up translation a moment.
            if idle >= Duration::from_secs(limit.max(10)) {
                if let Ok(mut server) = engine.server.try_lock() {
                    if server.take().is_some() {
                        info!("Stopped the idle text engine after {}s", idle.as_secs());
                    }
                }
            }
        });
    }
}

/// A free localhost port for the server.
fn free_port() -> Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0").context("No free local port")?;
    Ok(listener.local_addr()?.port())
}

/// The request for an English translation. Gemma has no system role, so the
/// instruction and the text go in one user turn; the tags keep instructions
/// and text apart. Tested on Gemma 3 1B: without "do not add anything" it
/// decorates translations with emoji.
fn english_translation_prompt(text: &str) -> String {
    format!(
        "Translate the text between <text> tags into English. Reply with only the English \
         translation. Keep names, numbers and links exactly as written, and do not add \
         anything.\n\n<text>\n{text}\n</text>"
    )
}

/// Trim the model's reply to the bare translation.
fn clean_translation(reply: &str) -> String {
    let mut text = reply.trim();
    for tag in ["<text>", "</text>"] {
        text = text.trim_start_matches(tag).trim_end_matches(tag).trim();
    }
    text.to_string()
}

#[cfg(test)]
mod tests {
    use super::{clean_translation, english_translation_prompt};

    #[test]
    fn prompt_wraps_the_text_in_tags() {
        let prompt = english_translation_prompt("Hola");
        assert!(prompt.ends_with("<text>\nHola\n</text>"));
        assert!(prompt.contains("into English"));
    }

    #[test]
    fn cleaning_strips_whitespace_and_echoed_tags() {
        assert_eq!(clean_translation("  Hello!\n"), "Hello!");
        assert_eq!(clean_translation("<text>\nHello!\n</text>"), "Hello!");
        assert_eq!(
            clean_translation("Line one\nLine two"),
            "Line one\nLine two"
        );
    }
}
