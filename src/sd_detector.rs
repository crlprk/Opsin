// src/sd_detector.rs

use std::io;
use std::path::Path;

/// Checks if we're running in Windows Subsystem for Linux (WSL)
fn is_wsl() -> bool {
    std::fs::read_to_string("/proc/version")
        .map(|s| s.contains("Microsoft") || s.contains("WSL"))
        .unwrap_or(false)
}

/// Detects mounted SD card path depending on platform (Linux, WSL, Windows)
pub fn detect_sd_mount() -> io::Result<String> {
    #[cfg(target_os = "windows")]
    {
        let fallback = Path::new("D:\\");
        if fallback.exists() {
            return Ok(fallback.display().to_string());
        } else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "No SD card found at fallback path D:\\",
            ));
        }
    }

    #[cfg(target_os = "linux")]
    {
        if is_wsl() {
            let fallback = Path::new("/mnt/d");
            if fallback.exists() {
                return Ok(fallback.display().to_string());
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "No fallback SD path found in WSL",
                ));
            }
        }

        use udev::Enumerator;

        let mut en = Enumerator::new()?;
        en.match_subsystem("block")?;
        for dev in en.scan_devices()? {
            if let Some(label) = dev.property_value("ID_FS_LABEL") {
                if label == "SONY_DSCWX5" {
                    if let Some(node) = dev.devnode() {
                        return Ok(format!("/media/{}", node.to_string_lossy()));
                    }
                }
            }
        }
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "SD card not found",
        ));
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "SD detection not supported on this OS",
        ))
    }
}
