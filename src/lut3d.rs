use std::fs;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Error};
use std::path::Path;

/// Represents a 3D Look-Up Table (LUT).
///
/// A 3D LUT is used for color grading and transformation. It stores a grid of
/// color values that can be interpolated to map input colors to output colors.
pub struct Lut3D {
    /// The size of one dimension of the LUT cube (e.g., 33 for a 33x33x33 LUT).
    size: usize,
    /// The actual color data of the LUT, stored as a flat list of [R, G, B] f32 arrays.
    /// Values are typically normalized between 0.0 and 1.0.
    data: Vec<[f32; 3]>,
    /// The minimum input domain values for R, G, B.
    /// Used for normalizing input colors if they are not in the [0, 1] range.
    domain_min: [f32; 3],
    /// The maximum input domain values for R, G, B.
    /// Used for normalizing input colors if they are not in the [0, 1] range.
    domain_max: [f32; 3],
}

impl Lut3D {
    /// Parses a `.cube` file and creates a `Lut3D` instance.
    ///
    /// # Arguments
    ///
    /// * `path` - A string slice that holds the path to the `.cube` file.
    ///
    /// # Errors
    ///
    /// Returns an `io::Error` if the file cannot be opened or read, or if parsing fails.
    pub fn from_cube(path: &str) -> Result<Self, Error> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);

        let mut size = 0;
        let mut data = Vec::new();
        let mut domain_min = [0.0; 3]; // Default domain min
        let mut domain_max = [1.0; 3]; // Default domain max

        for line in reader.lines() {
            let line = line?;
            let line = line.trim(); // Remove leading/trailing whitespace

            // Parse LUT_3D_SIZE
            if line.starts_with("LUT_3D_SIZE") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    size = parts[1].parse::<usize>().unwrap_or_else(|_| {
                        eprintln!("Warning: Failed to parse LUT_3D_SIZE value: {}", parts[1]);
                        0 // Default or error state for size
                    });
                }
            // Parse DOMAIN_MIN
            } else if line.starts_with("DOMAIN_MIN") {
                let parts: Vec<f32> = line.split_whitespace().skip(1)
                    .map(|s| s.parse::<f32>().unwrap_or_else(|_| {
                        eprintln!("Warning: Failed to parse DOMAIN_MIN value: {}", s);
                        0.0 // Default or error state for domain value
                    })).collect();
                if parts.len() == 3 {
                    domain_min.copy_from_slice(&parts);
                }
            // Parse DOMAIN_MAX
            } else if line.starts_with("DOMAIN_MAX") {
                let parts: Vec<f32> = line.split_whitespace().skip(1)
                    .map(|s| s.parse::<f32>().unwrap_or_else(|_| {
                        eprintln!("Warning: Failed to parse DOMAIN_MAX value: {}", s);
                        1.0 // Default or error state for domain value
                    })).collect();
                if parts.len() == 3 {
                    domain_max.copy_from_slice(&parts);
                }
            // Skip empty lines, comments, and TITLE
            } else if line.is_empty() || line.starts_with('#') || line.starts_with("TITLE") {
                continue;
            // Parse color data lines
            } else {
                let vals: Vec<f32> = line.split_whitespace()
                    .map(|s| s.parse::<f32>().unwrap_or_else(|_| {
                        eprintln!("Warning: Failed to parse color data value: {}", s);
                        0.0 // Default or error state for color value
                    })).collect();
                if vals.len() == 3 {
                    data.push([vals[0], vals[1], vals[2]]);
                }
            }
        }
        // Basic validation
        if size == 0 {
            return Err(Error::new(io::ErrorKind::InvalidData, "LUT_3D_SIZE is missing or invalid."));
        }
        if data.len() != size * size * size {
            return Err(Error::new(io::ErrorKind::InvalidData,
                format!("LUT data size mismatch. Expected {} entries, found {}", size * size * size, data.len())));
        }


        Ok(Lut3D { size, data, domain_min, domain_max })
    }

    /// Applies the LUT to a given RGB color.
    ///
    /// This method performs nearest-neighbor interpolation.
    /// Input RGB values are u8 (0-255) and are normalized to f32 (0.0-1.0)
    /// before applying the LUT. The output is also u8 (0-255).
    ///
    /// # Arguments
    ///
    /// * `r`, `g`, `b` - The red, green, and blue components of the input color (0-255).
    ///
    /// # Returns
    ///
    /// An array `[u8; 3]` representing the transformed RGB color.
    pub fn apply_lut(&self, r: u8, g: u8, b: u8) -> [u8; 3] {
        // Normalize input u8 values to f32 [0.0, 1.0]
        let r_f = r as f32 / 255.0;
        let g_f = g as f32 / 255.0;
        let b_f = b as f32 / 255.0;

        // Map normalized input values to the LUT's domain, then clamp to [0.0, 1.0]
        let map = |val: f32, min: f32, max: f32| ((val - min) / (max - min)).clamp(0.0f32, 1.0f32);
        let rn = map(r_f, self.domain_min[0], self.domain_max[0]);
        let gn = map(g_f, self.domain_min[1], self.domain_max[1]);
        let bn = map(b_f, self.domain_min[2], self.domain_max[2]);

        // Scale normalized coordinates to LUT grid indices
        let f = (self.size - 1) as f32;
        // Use round for nearest neighbor, then clamp to valid index range
        let ri = (rn * f).round().clamp(0.0, f) as usize;
        let gi = (gn * f).round().clamp(0.0, f) as usize;
        let bi = (bn * f).round().clamp(0.0, f) as usize;

        // Calculate the 1D index into the flat data array
        let idx = ri + gi * self.size + bi * self.size * self.size;
        
        // Ensure index is within bounds, though clamping should prevent out-of-bounds.
        // This is a safeguard.
        if idx >= self.data.len() {
            // This case should ideally not be reached if calculations are correct
            // and self.size matches self.data.len().
            // Return a default color (e.g., black or the original color) or panic.
            // For now, let's return black to indicate an error.
            eprintln!("Warning: LUT index out of bounds. idx: {}, data_len: {}", idx, self.data.len());
            return [0, 0, 0];
        }
        let out = self.data[idx];


        // Convert output f32 values [0.0, 1.0] back to u8 [0, 255]
        [
            (out[0].clamp(0.0, 1.0) * 255.0) as u8,
            (out[1].clamp(0.0, 1.0) * 255.0) as u8,
            (out[2].clamp(0.0, 1.0) * 255.0) as u8,
        ]
    }

    /// Loads a precomputed LUT table from a binary file or generates it if it doesn't exist.
    ///
    /// The precomputed table maps all possible 24-bit RGB colors (256x256x256)
    /// to their transformed values, speeding up repeated applications.
    ///
    /// # Arguments
    ///
    /// * `bin_path` - Path to the binary file for storing/loading the precomputed table.
    ///
    /// # Returns
    ///
    /// An `io::Result<Vec<u8>>` containing the precomputed table.
    pub fn load_or_generate_map(&self, bin_path: &str) -> io::Result<Vec<u8>> {
        let path = Path::new(bin_path);
        if path.exists() {
            // If the precomputed table exists, read it from disk
            fs::read(path)
        } else {
            // If not, generate the table
            // Capacity: 256^3 entries, each with 3 bytes (R, G, B)
            let mut table = Vec::with_capacity(256 * 256 * 256 * 3);
            // Iterate over all possible u8 RGB values
            for r_val in 0u8..=255u8 { // Renamed r to r_val to avoid conflict
                for g_val in 0u8..=255u8 { // Renamed g to g_val
                    for b_val in 0u8..=255u8 { // Renamed b to b_val
                        // Apply the LUT transformation for each color
                        let color = self.apply_lut(r_val, g_val, b_val);
                        table.push(color[0]);
                        table.push(color[1]);
                        table.push(color[2]);
                    }
                }
            }
            // Create parent directories if they don't exist before writing the file
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            // Write the newly generated table to disk for future use
            fs::write(path, &table)?;
            Ok(table)
        }
    }

    /// Applies a precomputed LUT table to a given RGB color.
    ///
    /// This is a fast lookup method using a table generated by `load_or_generate_map`.
    ///
    /// # Arguments
    ///
    /// * `table` - A slice `&[u8]` representing the precomputed LUT.
    /// * `r`, `g`, `b` - The red, green, and blue components of the input color (0-255).
    ///
    /// # Returns
    ///
    /// An array `[u8; 3]` representing the transformed RGB color.
    pub fn apply_precomputed(table: &[u8], r: u8, g: u8, b: u8) -> [u8; 3] {
        // Calculate the index in the flat precomputed table.
        // Each color (r, g, b) maps to a unique index.
        // The table stores RGB values sequentially: R1G1B1, R1G1B2, ...
        // Index = (r * 256*256 + g * 256 + b) * 3
        // This can be optimized using bit shifts: (r << 16 | g << 8 | b) * 3
        let idx = ((r as usize) << 16 | (g as usize) << 8 | (b as usize)) * 3;
        // Return the R, G, B values from the table at the calculated index
        [table[idx], table[idx + 1], table[idx + 2]]
    }
}