use crate::domain::RuntimeError;
use crate::runtimes::llama_server::{find_in_directory, shutdown};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

// `releases/latest` points at a tooling tag without binaries; real `bNNNN` builds are prereleases.
const RELEASES_API: &str = "https://api.github.com/repos/ggml-org/llama.cpp/releases?per_page=20";
const USER_AGENT: &str = "LingvoLoc";
const VERSION_FILE: &str = "version.txt";

#[derive(Debug, Clone, Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Debug, Clone, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuKind {
    Nvidia,
    Other,
    None,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadedLlama {
    pub path: String,
    pub version: String,
    pub variant: String,
    pub up_to_date: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadProgress {
    pub percent: u8,
    pub stage: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GpuInfo {
    pub names: Vec<String>,
    /// Build that will be downloaded: CUDA, Vulkan or CPU.
    pub backend: String,
}

fn video_controller_names() -> Vec<String> {
    let mut command = std::process::Command::new("powershell.exe");
    command.args([
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        "(Get-CimInstance Win32_VideoController).Name",
    ]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command
        .output()
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn is_virtual_adapter(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [
        "basic render",
        "remote display",
        "virtual",
        "parsec",
        "displaylink",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

fn driver_libraries() -> (bool, bool) {
    let Some(root) = std::env::var_os("SystemRoot") else {
        return (false, false);
    };
    let system32 = PathBuf::from(root).join("System32");
    (
        system32.join("nvcuda.dll").is_file(),
        system32.join("vulkan-1.dll").is_file(),
    )
}

fn classify_gpu(names: &[String], cuda_driver: bool, vulkan_driver: bool) -> GpuKind {
    let real: Vec<&String> = names.iter().filter(|n| !is_virtual_adapter(n)).collect();
    let has_nvidia = real
        .iter()
        .any(|name| name.to_ascii_lowercase().contains("nvidia"));
    if cuda_driver && (has_nvidia || real.is_empty()) {
        GpuKind::Nvidia
    } else if vulkan_driver && (!real.is_empty() || names.is_empty()) {
        GpuKind::Other
    } else {
        GpuKind::None
    }
}

/// Detects the GPUs (by name) and which llama.cpp build fits them.
pub fn detect_gpu_info() -> (GpuKind, GpuInfo) {
    let names: Vec<String> = video_controller_names()
        .into_iter()
        .filter(|name| !is_virtual_adapter(name))
        .collect();
    let (cuda, vulkan) = driver_libraries();
    let kind = classify_gpu(&names, cuda, vulkan);
    let backend = match kind {
        GpuKind::Nvidia => "CUDA",
        GpuKind::Other => "Vulkan",
        GpuKind::None => "CPU",
    };
    (
        kind,
        GpuInfo {
            names,
            backend: backend.into(),
        },
    )
}

fn is_windows_zip(name: &str) -> bool {
    name.contains("-bin-win-") && name.ends_with("-x64.zip")
}

fn find_asset<'a>(assets: &'a [Asset], variant: &str) -> Option<&'a Asset> {
    let marker = format!("-bin-win-{variant}");
    assets.iter().find(|asset| {
        asset.name.starts_with("llama-")
            && is_windows_zip(&asset.name)
            && asset.name.contains(&marker)
    })
}

/// Picks the release archives for this machine, most capable first.
/// Returns the variant label and the archives to unpack into one folder.
fn select_assets(gpu: GpuKind, assets: &[Asset]) -> Option<(String, Vec<Asset>)> {
    let cuda = || {
        let candidates: Vec<&Asset> = assets
            .iter()
            .filter(|asset| {
                asset.name.starts_with("llama-")
                    && is_windows_zip(&asset.name)
                    && asset.name.contains("-bin-win-cuda-")
            })
            .collect();
        // CUDA 12.x runs on a wider range of NVIDIA drivers than the newest major version.
        let main = candidates
            .iter()
            .find(|asset| asset.name.contains("-cuda-12"))
            .or(candidates.first())?;
        let suffix = &main.name[main.name.find("cuda-")?..];
        let runtime = assets
            .iter()
            .find(|asset| asset.name.starts_with("cudart-") && asset.name.ends_with(suffix))?;
        Some(("CUDA".to_string(), vec![(*main).clone(), runtime.clone()]))
    };
    let single = |variant: &str, label: &str| {
        find_asset(assets, variant).map(|asset| (label.to_string(), vec![asset.clone()]))
    };
    match gpu {
        GpuKind::Nvidia => cuda()
            .or_else(|| single("vulkan", "Vulkan"))
            .or_else(|| single("cpu", "CPU")),
        GpuKind::Other => single("vulkan", "Vulkan").or_else(|| single("cpu", "CPU")),
        GpuKind::None => single("cpu", "CPU").or_else(|| single("vulkan", "Vulkan")),
    }
}

fn http_error(error: reqwest::Error) -> RuntimeError {
    RuntimeError::Connection(format!("download failed: {error}"))
}

fn io_error(error: std::io::Error) -> RuntimeError {
    RuntimeError::Connection(format!("file error: {error}"))
}

fn percent_of(done: u64, total: u64) -> u8 {
    (done * 100).checked_div(total).unwrap_or(0).min(100) as u8
}

fn download_asset(
    client: &Client,
    asset: &Asset,
    destination: &Path,
    done: &mut u64,
    total: u64,
    report: &mut dyn FnMut(DownloadProgress),
) -> Result<(), RuntimeError> {
    let mut response = client
        .get(&asset.browser_download_url)
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(http_error)?;
    let mut file = std::fs::File::create(destination).map_err(io_error)?;
    let mut buffer = [0u8; 64 * 1024];
    let mut last_percent = 255u8;
    loop {
        let read = response.read(&mut buffer).map_err(io_error)?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read]).map_err(io_error)?;
        *done += read as u64;
        let percent = percent_of(*done, total);
        if percent != last_percent {
            last_percent = percent;
            report(DownloadProgress {
                percent,
                stage: format!("Downloading {}", asset.name),
            });
        }
    }
    Ok(())
}

fn extract_zip(archive: &Path, destination: &Path) -> Result<(), RuntimeError> {
    let file = std::fs::File::open(archive).map_err(io_error)?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|error| RuntimeError::MalformedResponse(format!("bad archive: {error}")))?;
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|error| RuntimeError::MalformedResponse(format!("bad archive: {error}")))?;
        // enclosed_name rejects entries that would escape the destination folder.
        let Some(relative) = entry.enclosed_name() else {
            continue;
        };
        let target = destination.join(relative);
        if entry.is_dir() {
            std::fs::create_dir_all(&target).map_err(io_error)?;
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(io_error)?;
        }
        let mut output = std::fs::File::create(&target).map_err(io_error)?;
        std::io::copy(&mut entry, &mut output).map_err(io_error)?;
    }
    Ok(())
}

fn sibling(base: &Path, suffix: &str) -> PathBuf {
    let mut name = base.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    base.with_file_name(name)
}

/// Downloads the newest llama.cpp Windows build that fits this machine into `base`.
/// `base` has a fixed name, so the executable path stays the same across updates; the
/// installed release is recorded in `base/version.txt`.
pub fn download_latest(
    base: &Path,
    mut report: impl FnMut(DownloadProgress),
) -> Result<DownloadedLlama, RuntimeError> {
    report(DownloadProgress {
        percent: 0,
        stage: "Looking up the latest llama.cpp release".into(),
    });
    let client = Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(1800))
        .build()
        .map_err(http_error)?;
    let releases: Vec<Release> = client
        .get(RELEASES_API)
        .send()
        .and_then(|response| response.error_for_status())
        .and_then(|response| response.json())
        .map_err(http_error)?;
    let gpu = detect_gpu_info().0;
    let (release, variant, assets) = releases
        .into_iter()
        .find_map(|release| {
            let (variant, assets) = select_assets(gpu, &release.assets)?;
            Some((release, variant, assets))
        })
        .ok_or_else(|| {
            RuntimeError::InvalidInput(
                "no llama.cpp Windows x64 build found for this computer".into(),
            )
        })?;

    let stamp = format!("{} {}", release.tag_name, variant);
    if std::fs::read_to_string(base.join(VERSION_FILE)).is_ok_and(|text| text.trim() == stamp) {
        if let Some(exe) = find_in_directory(base) {
            return Ok(DownloadedLlama {
                path: exe.display().to_string(),
                version: release.tag_name,
                variant,
                up_to_date: true,
            });
        }
    }

    let staging = sibling(base, ".download");
    let target = sibling(base, ".new");
    let _ = std::fs::remove_dir_all(&staging);
    let _ = std::fs::remove_dir_all(&target);
    std::fs::create_dir_all(&staging).map_err(io_error)?;
    std::fs::create_dir_all(&target).map_err(io_error)?;

    let total: u64 = assets.iter().map(|asset| asset.size).sum();
    let mut done = 0u64;
    let result = (|| {
        for asset in &assets {
            let archive = staging.join(&asset.name);
            download_asset(&client, asset, &archive, &mut done, total, &mut report)?;
            report(DownloadProgress {
                percent: percent_of(done, total),
                stage: format!("Unpacking {}", asset.name),
            });
            extract_zip(&archive, &target)?;
        }
        std::fs::write(target.join(VERSION_FILE), &stamp).map_err(io_error)?;
        find_in_directory(&target).ok_or_else(|| {
            RuntimeError::MalformedResponse("llama-server.exe is missing from the archive".into())
        })
    })();
    let _ = std::fs::remove_dir_all(&staging);
    if let Err(error) = result {
        let _ = std::fs::remove_dir_all(&target);
        return Err(error);
    }

    // The running server locks its executable, so stop it before replacing the folder.
    report(DownloadProgress {
        percent: 100,
        stage: "Installing".into(),
    });
    shutdown();
    if base.exists() {
        std::fs::remove_dir_all(base).map_err(|error| {
            RuntimeError::Connection(format!(
                "cannot replace the old llama.cpp (is it still running?): {error}"
            ))
        })?;
    }
    std::fs::rename(&target, base).map_err(io_error)?;
    let exe = find_in_directory(base).ok_or_else(|| {
        RuntimeError::MalformedResponse("llama-server.exe is missing after install".into())
    })?;
    Ok(DownloadedLlama {
        path: exe.display().to_string(),
        version: release.tag_name,
        variant,
        up_to_date: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assets(names: &[&str]) -> Vec<Asset> {
        names
            .iter()
            .map(|name| Asset {
                name: (*name).into(),
                browser_download_url: format!("https://example.invalid/{name}"),
                size: 1,
            })
            .collect()
    }

    fn release_assets() -> Vec<Asset> {
        assets(&[
            "llama-b9000-bin-macos-arm64.zip",
            "llama-b9000-bin-win-cpu-x64.zip",
            "llama-b9000-bin-win-cpu-arm64.zip",
            "llama-b9000-bin-win-vulkan-x64.zip",
            "llama-b9000-bin-win-cuda-12.4-x64.zip",
            "llama-b9000-bin-win-cuda-13.1-x64.zip",
            "cudart-llama-bin-win-cuda-12.4-x64.zip",
            "cudart-llama-bin-win-cuda-13.1-x64.zip",
        ])
    }

    fn names(selection: Option<(String, Vec<Asset>)>) -> Vec<String> {
        selection
            .map(|(_, assets)| assets.into_iter().map(|asset| asset.name).collect())
            .unwrap_or_default()
    }

    #[test]
    fn classifies_gpus_by_name_and_driver() {
        let nvidia = vec!["NVIDIA GeForce RTX 3060".to_string()];
        let amd = vec!["AMD Radeon RX 6600".to_string()];
        let basic = vec!["Microsoft Basic Render Driver".to_string()];
        assert_eq!(classify_gpu(&nvidia, true, true), GpuKind::Nvidia);
        assert_eq!(classify_gpu(&nvidia, false, true), GpuKind::Other);
        assert_eq!(classify_gpu(&amd, false, true), GpuKind::Other);
        assert_eq!(classify_gpu(&basic, false, true), GpuKind::None);
        assert_eq!(classify_gpu(&[], false, false), GpuKind::None);
    }

    #[test]
    fn nvidia_gets_cuda_12_with_matching_runtime() {
        let (variant, _) = select_assets(GpuKind::Nvidia, &release_assets()).unwrap();
        assert_eq!(variant, "CUDA");
        assert_eq!(
            names(select_assets(GpuKind::Nvidia, &release_assets())),
            [
                "llama-b9000-bin-win-cuda-12.4-x64.zip",
                "cudart-llama-bin-win-cuda-12.4-x64.zip"
            ]
        );
    }

    #[test]
    fn other_gpus_get_vulkan_and_no_gpu_gets_cpu() {
        assert_eq!(
            names(select_assets(GpuKind::Other, &release_assets())),
            ["llama-b9000-bin-win-vulkan-x64.zip"]
        );
        assert_eq!(
            names(select_assets(GpuKind::None, &release_assets())),
            ["llama-b9000-bin-win-cpu-x64.zip"]
        );
    }

    #[test]
    fn nvidia_falls_back_to_vulkan_without_a_cuda_runtime_archive() {
        let list = assets(&[
            "llama-b9000-bin-win-vulkan-x64.zip",
            "llama-b9000-bin-win-cuda-12.4-x64.zip",
        ]);
        assert_eq!(
            names(select_assets(GpuKind::Nvidia, &list)),
            ["llama-b9000-bin-win-vulkan-x64.zip"]
        );
        assert!(select_assets(GpuKind::Other, &assets(&["notes.txt"])).is_none());
    }

    #[test]
    fn extraction_skips_entries_outside_the_folder() {
        let dir = std::env::temp_dir().join(format!(
            "lingvoloc-zip-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let archive = dir.join("a.zip");
        {
            let mut writer = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
            let options = zip::write::SimpleFileOptions::default();
            writer.start_file("bin/llama-server.exe", options).unwrap();
            writer.write_all(b"x").unwrap();
            writer.start_file("../escape.txt", options).unwrap();
            writer.write_all(b"x").unwrap();
            writer.finish().unwrap();
        }
        let out = dir.join("out");
        std::fs::create_dir_all(&out).unwrap();
        extract_zip(&archive, &out).unwrap();
        assert!(out.join("bin").join("llama-server.exe").is_file());
        assert!(!dir.join("escape.txt").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
