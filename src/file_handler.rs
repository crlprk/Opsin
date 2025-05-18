use std::{fs, path::Path};
use image::{ImageReader, RgbImage};
use walkdir::WalkDir;
use crate::lut3d::Lut3D;
use crate::metadata_handler::copy_metadata;

/// Processes files from an input directory, applies a 3D LUT to images,
/// and saves the results to an output directory.
///
/// This function iterates through all files in the `input_dir`.
/// - If a file is an image (JPG, JPEG, PNG), it applies the `lut_table` to its pixels.
///   The original image is copied to the `output_dir` with an "orig_" prefix.
///   The processed image is saved to the `output_dir`, and metadata is copied from the original.
/// - If a file is not one of the specified image types, it is copied directly to the `output_dir`.
/// Directories are created in the `output_dir` as needed to mirror the `input_dir` structure.
///
/// # Arguments
///
/// * `input_dir` - A `Path` to the directory containing files to process.
/// * `output_dir` - A `Path` to the directory where processed files will be saved.
/// * `lut_table` - A slice of `u8` representing the precomputed 3D LUT.
pub fn process_files(input_dir: &Path, output_dir: &Path, lut_table: &[u8]) {
    // Walk through the input directory recursively
    for entry in WalkDir::new(input_dir).into_iter().filter_map(Result::ok) {
        let path = entry.path();
        // Skip directories, only process files
        if !path.is_file() {
            continue;
        }

        // Determine the relative path of the file with respect to the input directory
        let rel_path = match path.strip_prefix(input_dir) {
            Ok(rel) => rel,
            Err(_) => continue, // Should not happen if path is from WalkDir(input_dir)
        };
        // Construct the corresponding output path
        let output_path = output_dir.join(rel_path);
        // Create parent directories in the output path if they don't exist
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }

        // Check the file extension to determine if it's an image to be processed
        match path.extension().and_then(|s| s.to_str()).map(|s| s.to_lowercase()) {
            Some(ext) if ["jpg", "jpeg", "png"].contains(&ext.as_str()) => {
                // For supported image types:
                // 1. Copy the original image to the output directory with an "orig_" prefix
                let orig_name = format!("orig_{}", rel_path.file_name().unwrap().to_string_lossy());
                let orig_out = output_path.with_file_name(orig_name);
                fs::copy(path, &orig_out).unwrap();

                // 2. Open and decode the image
                let img = ImageReader::open(path).unwrap().decode().unwrap().to_rgb8();
                let (w, h) = img.dimensions();
                let mut buf = img.into_raw(); // Get raw pixel data
                
                // 3. Apply the 3D LUT to each pixel
                buf.chunks_mut(3).for_each(|px| {
                    let rgb = Lut3D::apply_precomputed(lut_table, px[0], px[1], px[2]);
                    px.copy_from_slice(&rgb);
                });

                // 4. Save the processed image
                let processed = RgbImage::from_raw(w, h, buf).unwrap();
                processed.save(&output_path).unwrap();

                // 5. Attempt to copy metadata from the original image to the processed image
                if let Err(e) = copy_metadata(path, &output_path) {
                    eprintln!("Warning: failed to copy metadata for {:?}: {}", path, e);
                }
            }
            _ => {
                // For any other file type, just copy it to the output directory
                fs::copy(path, &output_path).unwrap();
            }
        }
    }
}
