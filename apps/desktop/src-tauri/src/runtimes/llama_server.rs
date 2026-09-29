use crate::domain::{
    CompletionRequest, CompletionResponse, LocalModel, ModelRuntime, RuntimeError, RuntimeStatus,
};
use crate::runtimes::lm_studio::LmStudioRuntime;
use reqwest::blocking::Client;
use serde::Serialize;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const MAX_SCAN_DEPTH: usize = 4;
const MAX_MODELS: usize = 500;
const START_TIMEOUT: Duration = Duration::from_secs(300);
const CONTEXT_SIZE: &str = "8192";
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

struct RunningServer {
    child: Child,
    exe: PathBuf,
    model: PathBuf,
    endpoint: String,
}

/// One llama-server process is shared by the whole app and restarted when the model changes.
static SERVER: Mutex<Option<RunningServer>> = Mutex::new(None);

pub struct StandaloneRuntime {
    models_directory: PathBuf,
    server_path: String,
    model_id: String,
}

impl StandaloneRuntime {
    pub fn new(models_directory: &str, server_path: &str, model_id: &str) -> Self {
        Self {
            models_directory: PathBuf::from(models_directory.trim()),
            server_path: server_path.trim().to_string(),
            model_id: model_id.to_string(),
        }
    }

    fn require_models_directory(&self) -> Result<(), RuntimeError> {
        if self.models_directory.as_os_str().is_empty() {
            return Err(RuntimeError::InvalidInput(
                "models folder is not selected".into(),
            ));
        }
        if !self.models_directory.is_dir() {
            return Err(RuntimeError::InvalidInput(format!(
                "models folder does not exist: {}",
                self.models_directory.display()
            )));
        }
        Ok(())
    }
}

impl ModelRuntime for StandaloneRuntime {
    fn status(&self) -> Result<RuntimeStatus, RuntimeError> {
        self.require_models_directory()?;
        let exe = effective_server_path(&self.server_path)?;
        let running = SERVER
            .lock()
            .map_err(|_| RuntimeError::Connection("llama-server lock is poisoned".into()))?
            .as_mut()
            .and_then(|server| {
                matches!(server.child.try_wait(), Ok(None))
                    .then(|| (server.endpoint.clone(), server.model.clone()))
            });
        Ok(match running {
            Some((endpoint, model)) => RuntimeStatus {
                available: true,
                endpoint,
                detail: format!(
                    "Standalone · running {}",
                    model.file_name().unwrap_or_default().to_string_lossy()
                ),
            },
            None => RuntimeStatus {
                available: true,
                endpoint: "standalone".into(),
                detail: format!("Standalone · ready ({})", exe.display()),
            },
        })
    }

    fn list_models(&self) -> Result<Vec<LocalModel>, RuntimeError> {
        self.require_models_directory()?;
        Ok(scan_models(&self.models_directory))
    }

    fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, RuntimeError> {
        self.require_models_directory()?;
        let exe = effective_server_path(&self.server_path)?;
        let model = resolve_model(&self.models_directory, &self.model_id)?;
        let endpoint = ensure_server(&exe, &model)?;
        LmStudioRuntime::new(&endpoint)?.complete(request)
    }
}

pub fn shutdown() {
    if let Ok(mut guard) = SERVER.lock() {
        if let Some(mut server) = guard.take() {
            stop(&mut server.child);
        }
    }
}

/// Runs `llama-server --version` so the settings screen can validate the chosen executable.
pub fn check_server(path: &str) -> Result<String, RuntimeError> {
    let exe = effective_server_path(path)?;
    let mut command = Command::new(&exe);
    command.arg("--version");
    hide_window(&mut command);
    let output = command
        .output()
        .map_err(|error| RuntimeError::Connection(format!("cannot run llama-server: {error}")))?;
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let version = text
        .lines()
        .map(str::trim)
        .find(|line| line.to_ascii_lowercase().contains("version"))
        .map(str::to_string);
    match version {
        Some(line) => Ok(line),
        None if output.status.success() => Ok(format!("OK · {}", exe.display())),
        None => Err(RuntimeError::InvalidInput(
            "the selected file does not look like llama-server".into(),
        )),
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LlamaDevice {
    pub id: String,
    pub name: String,
    pub memory_mib: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct LlamaDevices {
    pub devices: Vec<LlamaDevice>,
    pub active: Option<LlamaDevice>,
}

/// Parses `llama-server --list-devices` lines such as
/// `  CUDA0: NVIDIA GeForce RTX 3060 (12287 MiB, 11255 MiB free)`.
fn parse_devices(text: &str) -> Vec<LlamaDevice> {
    text.lines()
        .filter_map(|line| {
            let (id, rest) = line.trim().split_once(": ")?;
            if id.contains(char::is_whitespace) || !id.ends_with(|c: char| c.is_ascii_digit()) {
                return None;
            }
            let (name, memory) = rest.rsplit_once(" (")?;
            let memory_mib = memory.split_whitespace().next()?.parse().ok()?;
            Some(LlamaDevice {
                id: id.to_string(),
                name: name.trim().to_string(),
                memory_mib,
            })
        })
        .collect()
}

fn is_integrated(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [
        "uhd graphics",
        "hd graphics",
        "iris",
        "intel(r) graphics",
        "radeon(tm) graphics",
        "radeon graphics",
        "vega",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

/// Picks the strongest device: CUDA/ROCm cards first, then discrete Vulkan cards, then
/// integrated ones; the larger memory wins ties. Integrated GPUs report shared system
/// memory, so memory alone would wrongly favor them.
pub fn best_device(devices: &[LlamaDevice]) -> Option<&LlamaDevice> {
    let score = |device: &LlamaDevice| {
        let id = device.id.to_ascii_lowercase();
        if id.starts_with("cuda") || id.starts_with("rocm") || id.starts_with("hip") {
            3
        } else if is_integrated(&device.name) {
            1
        } else {
            2
        }
    };
    devices
        .iter()
        .max_by_key(|device| (score(device), device.memory_mib))
}

pub fn list_devices(exe: &Path) -> Vec<LlamaDevice> {
    let mut command = Command::new(exe);
    command.arg("--list-devices");
    hide_window(&mut command);
    command
        .output()
        .map(|output| {
            parse_devices(&format!(
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ))
        })
        .unwrap_or_default()
}

pub fn describe_devices(path: &str) -> Result<LlamaDevices, RuntimeError> {
    let exe = effective_server_path(path)?;
    let devices = list_devices(&exe);
    let active = best_device(&devices).cloned();
    Ok(LlamaDevices { devices, active })
}

#[cfg(windows)]
const PATH_SCRIPT: &str = r#"
$dir = $env:LINGVOLOC_DIR.TrimEnd('\')
$key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)
$raw = $key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
$parts = @($raw.Split(';') | Where-Object { $_ })
$present = ($parts | ForEach-Object { $_.TrimEnd('\') }) -contains $dir
if ($env:LINGVOLOC_MODE -eq 'add' -and -not $present) {
  $key.SetValue('Path', (($parts + $dir) -join ';'), [Microsoft.Win32.RegistryValueKind]::ExpandString)
  Add-Type -Namespace LingvoLoc -Name Native -MemberDefinition '[DllImport("user32.dll", CharSet=CharSet.Auto)] public static extern IntPtr SendMessageTimeout(IntPtr h, uint m, UIntPtr w, string l, uint f, uint t, out UIntPtr r);'
  $result = [UIntPtr]::Zero
  [LingvoLoc.Native]::SendMessageTimeout([IntPtr]0xffff, 0x1A, [UIntPtr]::Zero, 'Environment', 2, 5000, [ref]$result) | Out-Null
  'added'
} elseif ($present) { 'present' } else { 'absent' }
"#;

/// Checks or adds the folder of `llama-server.exe` in the user's PATH (no admin rights).
/// Returns `present`, `absent` or `added`.
pub fn user_path(path: &str, add: bool) -> Result<String, RuntimeError> {
    let exe = effective_server_path(path)?;
    let dir = exe
        .parent()
        .ok_or_else(|| RuntimeError::InvalidInput("llama-server has no folder".into()))?;
    #[cfg(windows)]
    {
        let mut command = Command::new("powershell.exe");
        command
            .args(["-NoProfile", "-NonInteractive", "-Command", PATH_SCRIPT])
            .env("LINGVOLOC_DIR", dir)
            .env("LINGVOLOC_MODE", if add { "add" } else { "check" });
        hide_window(&mut command);
        let output = command
            .output()
            .map_err(|error| RuntimeError::Connection(format!("cannot run PowerShell: {error}")))?;
        if !output.status.success() {
            return Err(RuntimeError::Connection(
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
    #[cfg(not(windows))]
    {
        let _ = (dir, add);
        Err(RuntimeError::InvalidInput(
            "PATH editing is only supported on Windows".into(),
        ))
    }
}

pub fn find_on_path() -> Option<PathBuf> {
    let name = if cfg!(windows) {
        "llama-server.exe"
    } else {
        "llama-server"
    };
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(name))
            .find(|candidate| candidate.is_file())
    })
}

/// Finds llama-server in a folder the user unzipped llama.cpp into (or a subfolder of it).
pub fn find_in_directory(directory: &Path) -> Option<PathBuf> {
    let name = if cfg!(windows) {
        "llama-server.exe"
    } else {
        "llama-server"
    };
    fn search(directory: &Path, name: &str, depth: usize) -> Option<PathBuf> {
        let direct = directory.join(name);
        if direct.is_file() {
            return Some(direct);
        }
        if depth == 0 {
            return None;
        }
        std::fs::read_dir(directory)
            .ok()?
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .find_map(|path| search(&path, name, depth - 1))
    }
    search(directory, name, 2)
}

fn effective_server_path(configured: &str) -> Result<PathBuf, RuntimeError> {
    if configured.trim().is_empty() {
        return find_on_path().ok_or_else(|| {
            RuntimeError::InvalidInput(
                "llama.cpp is not installed; open Settings and press Download llama.cpp".into(),
            )
        });
    }
    let path = PathBuf::from(configured.trim());
    if path.is_file() {
        Ok(path)
    } else {
        Err(RuntimeError::InvalidInput(format!(
            "llama-server not found: {}",
            path.display()
        )))
    }
}

fn hide_window(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = command;
}

fn stop(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn log_path() -> PathBuf {
    std::env::temp_dir().join("lingvoloc-llama-server.log")
}

fn strip_ansi(text: &str) -> String {
    let mut clean = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            for next in chars.by_ref() {
                if next.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            clean.push(c);
        }
    }
    clean
}

fn log_tail() -> String {
    let text = strip_ansi(&std::fs::read_to_string(log_path()).unwrap_or_default());
    let lines: Vec<&str> = text.lines().rev().take(6).collect();
    lines.into_iter().rev().collect::<Vec<_>>().join(" | ")
}

fn free_port() -> Result<u16, RuntimeError> {
    std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .map(|address| address.port())
        .map_err(|error| RuntimeError::Connection(format!("no free local port: {error}")))
}

fn ensure_server(exe: &Path, model: &Path) -> Result<String, RuntimeError> {
    let mut guard = SERVER
        .lock()
        .map_err(|_| RuntimeError::Connection("llama-server lock is poisoned".into()))?;
    if let Some(server) = guard.as_mut() {
        let alive = matches!(server.child.try_wait(), Ok(None));
        if alive && server.exe == exe && server.model == model {
            return Ok(server.endpoint.clone());
        }
    }
    if let Some(mut old) = guard.take() {
        stop(&mut old.child);
    }

    let port = free_port()?;
    // With several GPUs (e.g. discrete + integrated) pin the strongest one explicitly.
    let devices = list_devices(exe);
    let device = if devices.len() > 1 {
        best_device(&devices).map(|device| device.id.clone())
    } else {
        None
    };
    let log = File::create(log_path())
        .map(Stdio::from)
        .unwrap_or_else(|_| Stdio::null());
    let mut command = Command::new(exe);
    command
        .arg("-m")
        .arg(model)
        .args(["--host", "127.0.0.1", "--port", &port.to_string()])
        // --no-jinja: the app sends plain text, but TranslateGemma's embedded template
        // demands structured content and makes llama-server refuse to start.
        // --chat-template gemma: some conversions are not recognised by the legacy matcher
        // and fall back to ChatML, which leaks `<|im_start|>` tokens into the translation.
        .args(["-c", CONTEXT_SIZE, "-np", "1", "-ngl", "99", "--no-jinja"])
        .args(["--chat-template", "gemma"]);
    if let Some(device) = &device {
        command.args(["--device", device]);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log);
    hide_window(&mut command);
    let mut child = command
        .spawn()
        .map_err(|error| RuntimeError::Connection(format!("cannot start llama-server: {error}")))?;

    let endpoint = format!("http://127.0.0.1:{port}/v1");
    let health = format!("http://127.0.0.1:{port}/health");
    let client = Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .map_err(|error| RuntimeError::Connection(error.to_string()))?;
    let started = Instant::now();
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            return Err(RuntimeError::Connection(format!(
                "llama-server exited ({status}): {}",
                log_tail()
            )));
        }
        if client
            .get(&health)
            .send()
            .is_ok_and(|response| response.status().is_success())
        {
            break;
        }
        if started.elapsed() > START_TIMEOUT {
            stop(&mut child);
            return Err(RuntimeError::Timeout(format!(
                "llama-server did not become ready: {}",
                log_tail()
            )));
        }
        std::thread::sleep(Duration::from_millis(300));
    }

    *guard = Some(RunningServer {
        child,
        exe: exe.to_path_buf(),
        model: model.to_path_buf(),
        endpoint: endpoint.clone(),
    });
    Ok(endpoint)
}

fn resolve_model(directory: &Path, id: &str) -> Result<PathBuf, RuntimeError> {
    if id.trim().is_empty() {
        return Err(RuntimeError::InvalidInput(
            "a model must be selected".into(),
        ));
    }
    let candidate = directory.join(id);
    let root = directory
        .canonicalize()
        .map_err(|error| RuntimeError::InvalidInput(error.to_string()))?;
    let resolved = candidate
        .canonicalize()
        .map_err(|_| RuntimeError::InvalidInput(format!("model file not found: {id}")))?;
    if !resolved.starts_with(&root) || !is_gguf(&resolved) {
        return Err(RuntimeError::InvalidInput(format!(
            "model is outside the models folder: {id}"
        )));
    }
    Ok(resolved)
}

fn is_gguf(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("gguf"))
}

pub fn scan_models(directory: &Path) -> Vec<LocalModel> {
    let mut models = Vec::new();
    collect_models(directory, directory, 0, &mut models);
    models.sort_by_key(|model| model.id.to_lowercase());
    models
}

fn collect_models(root: &Path, directory: &Path, depth: usize, models: &mut Vec<LocalModel>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        if models.len() >= MAX_MODELS {
            return;
        }
        let path = entry.path();
        if path.is_dir() {
            if depth < MAX_SCAN_DEPTH {
                collect_models(root, &path, depth + 1, models);
            }
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !is_gguf(&path) || is_auxiliary_file(name) {
            continue;
        }
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        let id = relative
            .components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        let owned_by = relative
            .parent()
            .and_then(|parent| parent.file_name())
            .map(|folder| folder.to_string_lossy().into_owned());
        models.push(LocalModel {
            quantization: quantization_from_file_name(name),
            id,
            owned_by,
        });
    }
}

/// Skips vision projectors and every shard of a split model except the first.
fn is_auxiliary_file(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if lower.contains("mmproj") {
        return true;
    }
    let stem = lower.trim_end_matches(".gguf");
    match stem.rsplit_once("-of-") {
        Some((head, total)) if total.len() == 5 && total.bytes().all(|b| b.is_ascii_digit()) => {
            !head.ends_with("-00001")
        }
        _ => false,
    }
}

fn quantization_from_file_name(name: &str) -> Option<String> {
    let stem = name.trim_end_matches(".gguf").trim_end_matches(".GGUF");
    stem.split(['-', '.'])
        .find(|token| {
            let lower = token.to_ascii_lowercase();
            let digits = lower
                .strip_prefix("iq")
                .or_else(|| lower.strip_prefix('q'))
                .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_digit()));
            digits || matches!(lower.as_str(), "f16" | "bf16" | "f32")
        })
        .map(str::to_uppercase)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(label: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("lingvoloc-{label}-{unique}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn scans_gguf_files_and_skips_auxiliary_ones() {
        let dir = temp_dir("scan");
        std::fs::create_dir_all(dir.join("google")).unwrap();
        for file in [
            "translategemma-4b-it.Q8_0.gguf",
            "google/gemma-3-4b-it-Q4_K_M.gguf",
            "google/mmproj-gemma.gguf",
            "big-00001-of-00002.gguf",
            "big-00002-of-00002.gguf",
            "notes.txt",
        ] {
            std::fs::write(dir.join(file), b"x").unwrap();
        }
        let models = scan_models(&dir);
        let ids: Vec<&str> = models.iter().map(|model| model.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "big-00001-of-00002.gguf",
                "google/gemma-3-4b-it-Q4_K_M.gguf",
                "translategemma-4b-it.Q8_0.gguf"
            ]
        );
        assert_eq!(models[1].owned_by.as_deref(), Some("google"));
        assert_eq!(models[1].quantization.as_deref(), Some("Q4_K_M"));
        assert_eq!(models[2].quantization.as_deref(), Some("Q8_0"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn resolves_models_only_inside_the_folder() {
        let dir = temp_dir("resolve");
        let outside = temp_dir("outside");
        std::fs::write(dir.join("a.gguf"), b"x").unwrap();
        std::fs::write(outside.join("b.gguf"), b"x").unwrap();
        assert!(resolve_model(&dir, "a.gguf").is_ok());
        let escape = format!(
            "../{}/b.gguf",
            outside.file_name().unwrap().to_string_lossy()
        );
        assert!(resolve_model(&dir, &escape).is_err());
        assert!(resolve_model(&dir, "missing.gguf").is_err());
        assert!(resolve_model(&dir, "").is_err());
        std::fs::remove_dir_all(dir).unwrap();
        std::fs::remove_dir_all(outside).unwrap();
    }

    #[test]
    fn reports_missing_configuration() {
        let runtime = StandaloneRuntime::new("", "", "");
        assert!(matches!(
            runtime.status(),
            Err(RuntimeError::InvalidInput(_))
        ));
        let dir = temp_dir("status");
        let runtime =
            StandaloneRuntime::new(&dir.to_string_lossy(), "Z:\\nope\\llama-server.exe", "");
        assert!(matches!(
            runtime.status(),
            Err(RuntimeError::InvalidInput(_))
        ));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn finds_llama_server_in_a_nested_folder() {
        let dir = temp_dir("locate");
        assert!(find_in_directory(&dir).is_none());
        let nested = dir.join("llama-b1-bin-win-cuda").join("bin");
        std::fs::create_dir_all(&nested).unwrap();
        let name = if cfg!(windows) {
            "llama-server.exe"
        } else {
            "llama-server"
        };
        std::fs::write(nested.join(name), b"x").unwrap();
        assert_eq!(find_in_directory(&dir), Some(nested.join(name)));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn parses_and_ranks_devices() {
        let text = "Available devices:\n  Vulkan0: Intel(R) UHD Graphics 750 (16000 MiB, 15000 MiB free)\n  Vulkan1: NVIDIA GeForce RTX 3060 (12288 MiB, 11000 MiB free)\n";
        let devices = parse_devices(text);
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[1].name, "NVIDIA GeForce RTX 3060");
        assert_eq!(best_device(&devices).unwrap().id, "Vulkan1");
        let cuda = parse_devices("  CUDA0: NVIDIA GeForce RTX 3060 (12287 MiB, 11255 MiB free)");
        assert_eq!(best_device(&cuda).unwrap().id, "CUDA0");
        assert!(parse_devices("Available devices:\nrandom text").is_empty());
    }

    #[test]
    fn strips_ansi_colors_from_log_output() {
        assert_eq!(strip_ansi("\u{1b}[31mE\u{1b}[0m srv"), "E srv");
    }

    #[test]
    fn finds_quantization_in_file_names() {
        assert_eq!(
            quantization_from_file_name("model-F16.gguf").as_deref(),
            Some("F16")
        );
        assert_eq!(quantization_from_file_name("qwen2.5-7b.gguf"), None);
    }
}
