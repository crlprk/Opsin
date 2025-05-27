use crate::lut3d::Lut3D;
use crate::metadata_handler::copy_metadata;
use image::{ImageReader, RgbImage};
use std::{
    fs,
    path::Path,
    sync::{Arc, Mutex},
};
use walkdir::WalkDir;

/// Processes images in the input directory by applying LUT transformations and copying to output.
/// 
/// This function walks through all files in the input directory, applies the specified LUT
/// transformation to supported image formats (JPG, JPEG, PNG), and saves both the original
/// and processed versions to the output directory. Non-image files are copied as-is.
/// 
/// # Arguments
/// * `input_dir` - Directory containing source images to process
/// * `output_dir` - Directory where processed images and copies will be saved
/// * `lut_table` - Precomputed LUT lookup table for fast color transformations
/// * `logger` - Thread-safe logger for status updates and progress tracking
/// 
/// # Behavior
/// - For supported image formats: Creates a "_RAW" backup copy and a LUT-processed version
/// - For other files: Creates a direct copy without processing
/// - Preserves directory structure in the output
/// - Copies EXIF metadata from originals to processed images
/// - Logs progress and completion status
pub fn process_images(
    input_dir: &Path,
    output_dir: &Path,
    lut_table: &[u8],
    logger: Arc<Mutex<Vec<String>>>,
) {
    // Validate input directory exists
    if !input_dir.exists() {
        logger.lock().unwrap().push(format!(
            "Image input directory not found: {}",
            input_dir.display()
        ));
        return;
    }

    // Discover all files in the input directory recursively
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

    // Process each discovered file
    for (i, entry) in files.into_iter().enumerate() {
        let path = entry.path();
        // Calculate relative path to preserve directory structure
        let rel = match path.strip_prefix(input_dir) {
            Ok(r) => r,
            Err(_) => continue,
        };
        
        // Log current processing status
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
        // Ensure output directory structure exists
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }

        // Process based on file extension
        if let Some(ext) = path
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_lowercase())
        {
            match ext.as_str() {
                "jpg" | "jpeg" | "png" => {
                    // Create backup copy with "_RAW" suffix
                    let orig_name = format!("{}_RAW", rel.file_name().unwrap().to_string_lossy());
                    let orig_out = out_path.with_file_name(orig_name);
                    fs::copy(path, &orig_out).unwrap();

                    // Load and process the image with LUT transformation
                    let img = ImageReader::open(path).unwrap().decode().unwrap().to_rgb8();
                    let (w, h) = img.dimensions();
                    let mut buf = img.into_raw();
                    
                    // Apply LUT transformation to each pixel
                    buf.chunks_mut(3).for_each(|px| {
                        // Transform RGB values using precomputed LUT table
                        let rgb = Lut3D::apply_precomputed(lut_table, px[0], px[1], px[2]);
                        px.copy_from_slice(&rgb);
                    });
                    
                    // Reconstruct and save the processed image
                    let processed = RgbImage::from_raw(w, h, buf).unwrap();
                    processed.save(&out_path).unwrap();

                    // Copy EXIF metadata from original to processed image
                    if let Err(e) = copy_metadata(path, &out_path) {
                        eprintln!("Warning: failed to copy metadata for {:?}: {}", path, e);
                    }
                }
                _ => {
                    // Copy non-image files without processing
                    if let Err(e) = fs::copy(path, &out_path) {
                        eprintln!("Warning: failed to copy file {:?}: {}", path, e);
                    }
                }
            }
        }

        // Log completion status for this file
        {
            let mut log = logger.lock().unwrap();
            log.push(format!("Completed {}/{}: {}", i + 1, total, rel.display()));
        }
    }
    
    // Log final completion status
    logger
        .lock()
        .unwrap()
        .push(format!("Finished processing {} files.", total));
}

/// Processes video files by copying them from input to output directory.
/// 
/// This function searches for video files with specific extensions (MTS, M2TS) and
/// copies them to the output directory while preserving the directory structure.
/// Currently, no video processing or LUT application is performed.
/// 
/// # Arguments
/// * `input_dir` - Directory containing source video files
/// * `output_dir` - Directory where video files will be copied
/// * `logger` - Thread-safe logger for status updates and progress tracking
/// 
/// # Supported Formats
/// - MTS (AVCHD format)
/// - M2TS (Blu-ray MPEG-2 Transport Stream)
/// 
/// # Behavior
/// - Preserves original directory structure
/// - Logs progress and any copy errors
/// - Only processes files with supported video extensions
pub fn process_videos(input_dir: &Path, output_dir: &Path, logger: Arc<Mutex<Vec<String>>>) {
    // Validate input directory exists
    if !input_dir.exists() {
        logger.lock().unwrap().push(format!(
            "Video input directory not found: {}",
            input_dir.display()
        ));
        return;
    }

    // Define supported video file extensions
    let video_extensions = ["mts", "m2ts"];
    
    // Discover video files matching supported extensions
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

    // Process each discovered video file
    for (i, entry) in files.drain(..).enumerate() {
        let path = entry.path();
        // Calculate relative path to preserve directory structure
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

        // Log current processing status
        logger.lock().unwrap().push(format!(
            "Processing {}/{}: {}",
            i + 1,
            total,
            path.display()
        ));

        // Ensure output directory structure exists
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

        // Copy the video file to the output location
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

    // Log final completion status
    logger
        .lock()
        .unwrap()
        .push(format!("Finished copying {} video files.", total));
}