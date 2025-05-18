use std::io;
use std::path::Path;
use little_exif::metadata::Metadata;

pub fn copy_metadata(src: &Path, dst: &Path) -> io::Result<()> {
    let src_metadata = Metadata::new_from_path(src).map_err(|e| {
        io::Error::new(
            io::ErrorKind::Other,
            format!("Failed to read metadata from source file {}: {}", src.display(), e),
        )
    })?;

    src_metadata.write_to_file(dst).map_err(|e| {
        io::Error::new(
            io::ErrorKind::Other,
            format!("Failed to write metadata to destination file {}: {}", dst.display(), e),
        )
    })?;

    Ok(())
}
