use std::path::Path;
use image::RgbImage;
use rayon::prelude::*;
use opsin::lut3d::Lut3D;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load LUT and precompute table (or load existing)
    let lut = Lut3D::from_cube("assets/luts/SONY_CYBERSHOT_DSC-WX5.CUBE")?;
    let table = lut.load_or_generate_map("assets/luts/lut_precomputed.bin")?;

    // Load input image and get raw pixel buffer
    let input_path = Path::new("testing images/DSC01067.JPG");
    let img = image::open(&input_path)?.to_rgb8();
    let (w, h) = img.dimensions();
    let mut raw = img.into_raw();

    // Parallel apply precomputed LUT to each pixel chunk
    raw.par_chunks_mut(3).for_each(|pixel| {
        let rgb = Lut3D::apply_precomputed(&table, pixel[0], pixel[1], pixel[2]);
        pixel[0] = rgb[0];
        pixel[1] = rgb[1];
        pixel[2] = rgb[2];
    });

    // Reconstruct image from modified buffer
    let out = RgbImage::from_raw(w, h, raw).expect("Buffer size mismatch");
    let output_path = Path::new("example_lut.jpg");
    out.save(output_path)?;

    println!("Applied precomputed LUT to {:?} → {:?}", input_path, output_path);
    Ok(())
}