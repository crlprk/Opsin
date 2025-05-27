use std::{
    fs,
    fs::File,
    io::{self, BufRead, BufReader, Error},
    path::Path,
};

/// A 3D Look-Up Table (LUT) for color grading and transformation.
/// 
/// This structure represents a cubic color transformation table that maps
/// input RGB values to output RGB values. LUTs are commonly used in color
/// grading workflows to apply specific color transformations to images and videos.
pub struct Lut3D {
    /// The size of each dimension of the cubic LUT (e.g., 32 means 32x32x32)
    size: usize,
    /// The actual color transformation data stored as RGB triplets in normalized [0,1] range
    data: Vec<[f32; 3]>,
    /// The minimum input domain values for R, G, B channels (typically [0,0,0])
    domain_min: [f32; 3],
    /// The maximum input domain values for R, G, B channels (typically [1,1,1])
    domain_max: [f32; 3],
}

impl Lut3D {
    /// Creates a new 3D LUT from a .cube file.
    /// 
    /// The .cube format is a standard format for 3D LUTs that includes metadata
    /// such as size, domain range, and the actual color transformation data.
    /// 
    /// # Arguments
    /// * `path` - Path to the .cube file to load
    /// 
    /// # Returns
    /// A `Result` containing the loaded `Lut3D` or an `Error` if loading fails
    /// 
    /// # Errors
    /// Returns an error if:
    /// - The file cannot be opened or read
    /// - The file format is invalid (missing LUT_3D_SIZE or malformed data)
    /// - The data size doesn't match the declared LUT size
    pub fn from_cube(path: &str) -> Result<Self, Error> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);

        let mut size = 0;
        let mut data = Vec::new();
        let mut domain_min = [0.0; 3]; // Default domain minimum
        let mut domain_max = [1.0; 3]; // Default domain maximum

        // Parse each line of the .cube file
        for line in reader.lines() {
            let line = line?;
            let line = line.trim();

            if line.starts_with("LUT_3D_SIZE") {
                // Extract the cubic dimension size (e.g., "LUT_3D_SIZE 32")
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    size = parts[1].parse::<usize>().unwrap_or_else(|_| {
                        eprintln!("Warning: Failed to parse LUT_3D_SIZE value: {}", parts[1]);
                        0
                    });
                }
            } else if line.starts_with("DOMAIN_MIN") {
                // Parse minimum domain values for input normalization
                let parts: Vec<f32> = line
                    .split_whitespace()
                    .skip(1) // Skip the "DOMAIN_MIN" keyword
                    .map(|s| {
                        s.parse::<f32>().unwrap_or_else(|_| {
                            eprintln!("Warning: Failed to parse DOMAIN_MIN value: {}", s);
                            0.0
                        })
                    })
                    .collect();
                if parts.len() == 3 {
                    domain_min.copy_from_slice(&parts);
                }
            } else if line.starts_with("DOMAIN_MAX") {
                // Parse maximum domain values for input normalization
                let parts: Vec<f32> = line
                    .split_whitespace()
                    .skip(1) // Skip the "DOMAIN_MAX" keyword
                    .map(|s| {
                        s.parse::<f32>().unwrap_or_else(|_| {
                            eprintln!("Warning: Failed to parse DOMAIN_MAX value: {}", s);
                            1.0
                        })
                    })
                    .collect();
                if parts.len() == 3 {
                    domain_max.copy_from_slice(&parts);
                }
            } else if line.is_empty() || line.starts_with('#') || line.starts_with("TITLE") {
                // Skip empty lines, comments, and title metadata
                continue;
            } else {
                // Parse RGB color data lines (three space-separated float values)
                let vals: Vec<f32> = line
                    .split_whitespace()
                    .map(|s| {
                        s.parse::<f32>().unwrap_or_else(|_| {
                            eprintln!("Warning: Failed to parse color data value: {}", s);
                            0.0
                        })
                    })
                    .collect();
                if vals.len() == 3 {
                    data.push([vals[0], vals[1], vals[2]]);
                }
            }
        }
        
        // Validate the parsed data
        if size == 0 {
            return Err(Error::new(
                io::ErrorKind::InvalidData,
                "LUT_3D_SIZE is missing or invalid.",
            ));
        }
        
        // Ensure data size matches expected cubic dimensions
        if data.len() != size * size * size {
            return Err(Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "LUT data size mismatch. Expected {} entries, found {}",
                    size * size * size,
                    data.len()
                ),
            ));
        }

        Ok(Lut3D {
            size,
            data,
            domain_min,
            domain_max,
        })
    }

    /// Applies the LUT transformation to an RGB color using nearest neighbor interpolation.
    /// 
    /// This is the simplest and fastest method, but may produce visible stepping
    /// artifacts in smooth gradients.
    /// 
    /// # Arguments
    /// * `r`, `g`, `b` - Input RGB values in the range [0, 255]
    /// 
    /// # Returns
    /// An array containing the transformed RGB values in the range [0, 255]
    pub fn apply_lut(&self, r: u8, g: u8, b: u8) -> [u8; 3] {
        // Convert from u8 [0,255] to f32 [0,1] range
        let r_f = r as f32 / 255.0;
        let g_f = g as f32 / 255.0;
        let b_f = b as f32 / 255.0;

        // Map input values to the LUT's domain range and clamp
        let map = |val: f32, min: f32, max: f32| ((val - min) / (max - min)).clamp(0.0f32, 1.0f32);
        let rn = map(r_f, self.domain_min[0], self.domain_max[0]);
        let gn = map(g_f, self.domain_min[1], self.domain_max[1]);
        let bn = map(b_f, self.domain_min[2], self.domain_max[2]);

        // Scale normalized values to LUT indices and round to nearest
        let f = (self.size - 1) as f32;
        let ri = (rn * f).round().clamp(0.0, f) as usize;
        let gi = (gn * f).round().clamp(0.0, f) as usize;
        let bi = (bn * f).round().clamp(0.0, f) as usize;

        // Calculate linear index into the 3D LUT data array
        let idx = ri + gi * self.size + bi * self.size * self.size;

        // Safety check for array bounds
        if idx >= self.data.len() {
            eprintln!(
                "Warning: LUT index out of bounds. idx: {}, data_len: {}",
                idx,
                self.data.len()
            );
            return [0, 0, 0];
        }
        
        let out = self.data[idx];

        // Convert back from f32 [0,1] to u8 [0,255] range
        [
            (out[0].clamp(0.0, 1.0) * 255.0) as u8,
            (out[1].clamp(0.0, 1.0) * 255.0) as u8,
            (out[2].clamp(0.0, 1.0) * 255.0) as u8,
        ]
    }

    /// Applies the LUT transformation using trilinear interpolation.
    /// 
    /// This method provides smooth color transitions by interpolating between
    /// the 8 nearest LUT entries, resulting in higher quality output at the
    /// cost of increased computation.
    /// 
    /// # Arguments
    /// * `r`, `g`, `b` - Input RGB values in the range [0, 255]
    /// 
    /// # Returns
    /// An array containing the transformed RGB values in the range [0, 255]
    pub fn apply_lut_trilinear(&self, r: u8, g: u8, b: u8) -> [u8; 3] {
        // Convert from u8 [0,255] to f32 [0,1] range
        let r_f = r as f32 / 255.0;
        let g_f = g as f32 / 255.0;
        let b_f = b as f32 / 255.0;

        // Map input values to the LUT's domain range
        let map = |val: f32, min: f32, max: f32| ((val - min) / (max - min)).clamp(0.0f32, 1.0f32);
        let rn = map(r_f, self.domain_min[0], self.domain_max[0]);
        let gn = map(g_f, self.domain_min[1], self.domain_max[1]);
        let bn = map(b_f, self.domain_min[2], self.domain_max[2]);

        // Scale to LUT coordinate space (floating point for interpolation)
        let f = (self.size - 1) as f32;
        let rx = rn * f;
        let gx = gn * f;
        let bx = bn * f;

        // Find the 8 surrounding LUT points for interpolation
        let r0 = rx.floor() as usize;
        let g0 = gx.floor() as usize;
        let b0 = bx.floor() as usize;

        let r1 = (r0 + 1).min(self.size - 1);
        let g1 = (g0 + 1).min(self.size - 1);
        let b1 = (b0 + 1).min(self.size - 1);

        // Calculate interpolation weights
        let dr = rx - r0 as f32;
        let dg = gx - g0 as f32;
        let db = bx - b0 as f32;

        // Helper function to calculate linear index
        let idx = |r, g, b| r + g * self.size + b * self.size * self.size;

        // Sample the 8 corner values of the interpolation cube
        let c000 = self.data[idx(r0, g0, b0)];
        let c001 = self.data[idx(r0, g0, b1)];
        let c010 = self.data[idx(r0, g1, b0)];
        let c011 = self.data[idx(r0, g1, b1)];
        let c100 = self.data[idx(r1, g0, b0)];
        let c101 = self.data[idx(r1, g0, b1)];
        let c110 = self.data[idx(r1, g1, b0)];
        let c111 = self.data[idx(r1, g1, b1)];

        // Linear interpolation helpers
        let lerp = |a: f32, b: f32, t: f32| a * (1.0 - t) + b * t;
        let lerp3 = |a: [f32; 3], b: [f32; 3], t: f32| {
            [
                lerp(a[0], b[0], t),
                lerp(a[1], b[1], t),
                lerp(a[2], b[2], t),
            ]
        };

        // Perform trilinear interpolation in three stages
        // First: interpolate along R axis
        let c00 = lerp3(c000, c100, dr);
        let c01 = lerp3(c001, c101, dr);
        let c10 = lerp3(c010, c110, dr);
        let c11 = lerp3(c011, c111, dr);

        // Second: interpolate along G axis
        let c0 = lerp3(c00, c10, dg);
        let c1 = lerp3(c01, c11, dg);

        // Third: interpolate along B axis to get final result
        let c = lerp3(c0, c1, db);

        // Convert back from f32 [0,1] to u8 [0,255] range
        [
            (c[0].clamp(0.0, 1.0) * 255.0) as u8,
            (c[1].clamp(0.0, 1.0) * 255.0) as u8,
            (c[2].clamp(0.0, 1.0) * 255.0) as u8,
        ]
    }

    /// Loads a precomputed LUT table from disk, or generates and saves one if it doesn't exist.
    /// 
    /// Precomputed tables contain the LUT transformation for every possible RGB input value
    /// (256³ = 16.7M entries), allowing for extremely fast lookups during processing.
    /// The table is saved as a binary file for quick loading in future sessions.
    /// 
    /// # Arguments
    /// * `bin_path` - Path where the binary LUT table should be stored
    /// 
    /// # Returns
    /// A `Result` containing the precomputed table as a byte vector, or an I/O error
    /// 
    /// # Format
    /// The binary table contains 48MB of data (256³ × 3 bytes) with RGB values
    /// stored sequentially for each possible input combination.
    pub fn load_or_generate_map(&self, bin_path: &str) -> io::Result<Vec<u8>> {
        let path = Path::new(bin_path);
        if path.exists() {
            // Load existing precomputed table
            fs::read(path)
        } else {
            // Generate new precomputed table
            let mut table = Vec::with_capacity(256 * 256 * 256 * 3); // 48MB allocation
            
            // Generate LUT output for every possible RGB input
            for r_val in 0u8..=255u8 {
                for g_val in 0u8..=255u8 {
                    for b_val in 0u8..=255u8 {
                        // Use trilinear interpolation for highest quality
                        let color = self.apply_lut_trilinear(r_val, g_val, b_val);
                        table.push(color[0]);
                        table.push(color[1]);
                        table.push(color[2]);
                    }
                }
            }
            
            // Ensure the directory exists before writing
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            
            // Save the generated table for future use
            fs::write(path, &table)?;
            Ok(table)
        }
    }

    /// Applies a precomputed LUT transformation to an RGB color.
    /// 
    /// This is the fastest method for applying LUT transformations, using a
    /// pre-generated lookup table that maps every possible input directly to output.
    /// 
    /// # Arguments
    /// * `table` - The precomputed lookup table (from `load_or_generate_map`)
    /// * `r`, `g`, `b` - Input RGB values in the range [0, 255]
    /// 
    /// # Returns
    /// An array containing the transformed RGB values in the range [0, 255]
    /// 
    /// # Performance
    /// This method performs a simple array lookup and is extremely fast,
    /// making it ideal for real-time image and video processing.
    pub fn apply_precomputed(table: &[u8], r: u8, g: u8, b: u8) -> [u8; 3] {
        // Calculate index using bit shifting for optimal performance
        // Formula: (r << 16) | (g << 8) | b gives unique index for each RGB combination
        let idx = ((r as usize) << 16 | (g as usize) << 8 | (b as usize)) * 3;
        [table[idx], table[idx + 1], table[idx + 2]]
    }
}