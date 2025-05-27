use crate::lut3d::Lut3D;
use crate::metadata_handler::copy_metadata;
use image::{ImageReader, RgbImage};
use std::{
    fs,
    path::Path,
    sync::{Arc, Mutex},
};
use walkdir::WalkDir;

pub fn process_images(
    input_dir: &Path,
    output_dir: &Path,
    lut_table: &[u8],
    logger: Arc<Mutex<Vec<String>>>,
) {
    if !input_dir.exists() {
        logger.lock().unwrap().push(format!(
            "Image input directory not found: {}",
            input_dir.display()
        ));
        return;
    }

    let files: Vec<_> = WalkDir::new(input_dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.path().is_file())
        .collect();
    let total = files.len();
    logger
        .lock()
        .unwrap()
        .push(format!("Found {} image files to copy.", total));

    for (i, entry) in files.into_iter().enumerate() {
        let path = entry.path();
        let rel = match path.strip_prefix(input_dir) {
            Ok(r) => r,
            Err(_) => continue,
        };
        {
            let mut log = logger.lock().unwrap();
            log.push(format!(
                "Processing {}/{}: {}",
                i + 1,
                total,
                path.display()
            ));
        }
        let out_path = output_dir.join(rel);
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }

        if let Some(ext) = path
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_lowercase())
        {
            match ext.as_str() {
                "jpg" | "jpeg" | "png" => {
                    let orig_name = format!("{}_RAW", rel.file_name().unwrap().to_string_lossy());
                    let orig_out = out_path.with_file_name(orig_name);
                    fs::copy(path, &orig_out).unwrap();

                    let img = ImageReader::open(path).unwrap().decode().unwrap().to_rgb8();
                    let (w, h) = img.dimensions();
                    let mut buf = img.into_raw();
                    buf.chunks_mut(3).for_each(|px| {
                        let rgb = Lut3D::apply_precomputed(lut_table, px[0], px[1], px[2]);
                        px.copy_from_slice(&rgb);
                    });
                    let processed = RgbImage::from_raw(w, h, buf).unwrap();
                    processed.save(&out_path).unwrap();

                    if let Err(e) = copy_metadata(path, &out_path) {
                        eprintln!("Warning: failed to copy metadata for {:?}: {}", path, e);
                    }
                }
                _ => {
                    if let Err(e) = fs::copy(path, &out_path) {
                        eprintln!("Warning: failed to copy file {:?}: {}", path, e);
                    }
                }
            }
        }

        {
            let mut log = logger.lock().unwrap();
            log.push(format!("Completed {}/{}: {}", i + 1, total, rel.display()));
        }
    }
    logger
        .lock()
        .unwrap()
        .push(format!("Finished processing {} files.", total));
}

pub fn process_videos(input_dir: &Path, output_dir: &Path, logger: Arc<Mutex<Vec<String>>>) {
    if !input_dir.exists() {
        logger.lock().unwrap().push(format!(
            "Video input directory not found: {}",
            input_dir.display()
        ));
        return;
    }

    let video_extensions = ["mts", "m2ts"];
    let mut files: Vec<_> = WalkDir::new(input_dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| {
            e.path().is_file()
                && e.path()
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| video_extensions.contains(&ext.to_lowercase().as_str()))
                    .unwrap_or(false)
        })
        .collect();

    let total = files.len();
    logger
        .lock()
        .unwrap()
        .push(format!("Found {} video files to copy.", total));

    for (i, entry) in files.drain(..).enumerate() {
        let path = entry.path();
        let rel = match path.strip_prefix(input_dir) {
            Ok(r) => r,
            Err(_) => {
                logger.lock().unwrap().push(format!(
                    "Skipping {}: could not strip prefix {}",
                    path.display(),
                    input_dir.display()
                ));
                continue;
            }
        };
        let out_path = output_dir.join(rel);

        logger.lock().unwrap().push(format!(
            "Processing {}/{}: {}",
            i + 1,
            total,
            path.display()
        ));

        if let Some(parent) = out_path.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                logger.lock().unwrap().push(format!(
                    "Error creating directory {}: {}",
                    parent.display(),
                    e
                ));
                continue;
            }
        }

        match fs::copy(path, &out_path) {
            Ok(_) => {
                logger.lock().unwrap().push(format!(
                    "Completed {}/{}: {}",
                    i + 1,
                    total,
                    rel.display()
                ));
            }
            Err(e) => {
                logger.lock().unwrap().push(format!(
                    "Error copying {} to {}: {}",
                    path.display(),
                    out_path.display(),
                    e
                ));
            }
        }
    }

    logger
        .lock()
        .unwrap()
        .push(format!("Finished copying {} video files.", total));
}