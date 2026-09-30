//! Locates and binds the PDFium library. LingvoLoc does not link PDFium
//! statically: `pdfium.dll` (official `bblanchon/pdfium-binaries`, Windows x64)
//! is loaded at runtime from one of the locations below.

use std::path::PathBuf;
use std::sync::Mutex;

use pdfium_render::prelude::*;

use crate::domain::RuntimeError;

const DLL_NAME: &str = "pdfium.dll";

fn candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(custom) = std::env::var_os("LINGVOLOC_PDFIUM") {
        paths.push(PathBuf::from(custom));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            paths.push(dir.join(DLL_NAME));
            paths.push(dir.join("resources").join(DLL_NAME));
            paths.push(dir.join("pdfium").join(DLL_NAME));
        }
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        paths.push(
            PathBuf::from(local)
                .join("com.lingoloc.desktop")
                .join("pdfium")
                .join(DLL_NAME),
        );
    }
    #[cfg(debug_assertions)]
    paths.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../tools/pdf-feasibility/bin")
            .join(DLL_NAME),
    );
    paths
}

/// Runs `operation` with the process-wide PDFium instance. The bindings can be
/// created only once per process, so the instance is kept and shared; the lock
/// serialises use because PDFium is not re-entrant.
pub fn with_pdfium<T>(
    operation: impl FnOnce(&Pdfium) -> Result<T, RuntimeError>,
) -> Result<T, RuntimeError> {
    static INSTANCE: Mutex<Option<Pdfium>> = Mutex::new(None);
    let mut guard = INSTANCE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if guard.is_none() {
        *guard = Some(bind()?);
    }
    operation(guard.as_ref().expect("initialised above"))
}

fn bind() -> Result<Pdfium, RuntimeError> {
    for path in candidates() {
        if path.is_file() {
            if let Ok(bindings) = Pdfium::bind_to_library(&path) {
                return Ok(Pdfium::new(bindings));
            }
        }
    }
    Err(RuntimeError::InvalidInput(
        "PDF support needs pdfium.dll next to the application (official PDFium Windows x64 build)"
            .into(),
    ))
}
