use opsin::sd_detector::detect_sd_mount;

fn main() {
    match detect_sd_mount() {
        Ok(path) => println!("Detected SD card mount at: {}", path),
        Err(e) => eprintln!("Failed to detect SD card: {}", e),
    }
}
