use little_exif::metadata::Metadata;
use std::{io, path::Path};

/// Copies EXIF metadata from a source image file to a destination image file.
/// 
/// This function preserves important image metadata such as camera settings, GPS data,
/// timestamps, and other EXIF information when processing images. This is crucial for
/// maintaining the original context and technical details of photographs after applying
/// LUT transformations or other processing operations.
/// 
/// # Arguments
/// * `src` - Path to the source image file containing the original metadata
/// * `dst` - Path to the destination image file where metadata will be written
/// 
/// # Returns
/// * `Ok(())` - If metadata was successfully copied
/// * `Err(io::Error)` - If reading from source or writing to destination fails
/// 
/// # Examples
/// ```rust
/// use std::path::Path;
/// 
/// let source = Path::new("original.jpg");
/// let destination = Path::new("processed.jpg");
/// 
/// match copy_metadata(source, destination) {
///     Ok(()) => println!("Metadata copied successfully"),
///     Err(e) => eprintln!("Failed to copy metadata: {}", e),
/// }
/// ```
/// 
/// # Notes
/// - Supports common image formats that can contain EXIF data (JPEG, TIFF, etc.)
/// - Preserves camera settings like ISO, aperture, shutter speed, focal length
/// - Maintains timestamps, GPS coordinates, and camera manufacturer information
/// - Essential for professional photography workflows where metadata integrity is important
pub fn copy_metadata(src: &Path, dst: &Path) -> io::Result<()> {
    // Read metadata from the source file
    // This extracts all available EXIF data including camera settings, timestamps, GPS data, etc.
    let src_metadata = Metadata::new_from_path(src).map_err(|e| {
        io::Error::new(
            io::ErrorKind::Other,
            format!(
                "Failed to read metadata from source file {}: {}",
                src.display(),
                e
            ),
        )
    })?;

    // Write the extracted metadata to the destination file
    // This embeds the EXIF data into the processed image, preserving original context
    src_metadata.write_to_file(dst).map_err(|e| {
        io::Error::new(
            io::ErrorKind::Other,
            format!(
                "Failed to write metadata to destination file {}: {}",
                dst.display(),
                e
            ),
        )
    })?;

    Ok(())
}