//! Per-user "Start with Windows" through the HKCU Run registry key (no admin rights).

use std::process::Command;

const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "LingvoLoc";
/// Passed on autostart so the main window stays in the tray.
pub const TRAY_ARG: &str = "--tray";

fn reg(args: &[&str]) -> std::io::Result<std::process::Output> {
    let mut command = Command::new("reg.exe");
    command.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command.output()
}

pub fn is_enabled() -> bool {
    reg(&["query", RUN_KEY, "/v", VALUE_NAME]).is_ok_and(|output| output.status.success())
}

pub fn set_enabled(enabled: bool) -> Result<(), String> {
    let output = if enabled {
        let exe = std::env::current_exe().map_err(|error| error.to_string())?;
        let command = format!("\"{}\" {TRAY_ARG}", exe.display());
        reg(&[
            "add", RUN_KEY, "/v", VALUE_NAME, "/t", "REG_SZ", "/d", &command, "/f",
        ])
    } else if is_enabled() {
        reg(&["delete", RUN_KEY, "/v", VALUE_NAME, "/f"])
    } else {
        return Ok(());
    }
    .map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}
