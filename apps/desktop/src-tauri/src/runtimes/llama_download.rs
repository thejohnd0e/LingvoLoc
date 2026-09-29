use crate::domain::RuntimeError;
use crate::runtimes::llama_server::find_in_directory;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

const RELEASE_API: &str = "https://api.github.com/repos/ggml-org/llama.cpp/releases/latest";
const USER_AGENT: &str = "LingvoLoc";

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
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadProgress {
    pub percent: u8,
    pub stage: String,
}

/// Detects the GPU class from the driver libraries Windows installs with it.
pub fn detect_gpu() -> GpuKind {
    let Some(root) = std::env::var_os("SystemRoot") else {
        return GpuKind::None;
    };
    let system32 = PathBuf::from(root).join("System32");
    if system32.join("nvcuda.dll").is_file() {
        GpuKind::Nvidia
    } else if system32.join("vulkan-1.dll").is_file() {
        GpuKind::Other
    } else {
        GpuKind::None
    }
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

fn remove_other_versions(base: &Path, keep: &str) {
    let Ok(entries) = std::fs::read_dir(base) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_name().to_string_lossy() != keep && entry.path().is_dir() {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// Downloads the newest llama.cpp Windows build that fits this machine into `base`.
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
    let release: Release = client
        .get(RELEASE_API)
        .send()
        .and_then(|response| response.error_for_status())
        .and_then(|response| response.json())
        .map_err(http_error)?;
    let (variant, assets) = select_assets(detect_gpu(), &release.assets).ok_or_else(|| {
        RuntimeError::InvalidInput(format!(
            "release {} has no Windows x64 build for this computer",
            release.tag_name
        ))
    })?;

    let target = base.join(&release.tag_name).join(variant.to_lowercase());
    let staging = base.join(format!("{}.download", release.tag_name));
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
        find_in_directory(&target).ok_or_else(|| {
            RuntimeError::MalformedResponse("llama-server.exe is missing from the archive".into())
        })
    })();
    let _ = std::fs::remove_dir_all(&staging);
    let exe = match result {
        Ok(exe) => exe,
        Err(error) => {
            let _ = std::fs::remove_dir_all(&target);
            return Err(error);
        }
    };
    remove_other_versions(base, &release.tag_name);
    Ok(DownloadedLlama {
        path: exe.display().to_string(),
        version: release.tag_name,
        variant,
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
