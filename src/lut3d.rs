use std::fs;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Error};
use std::path::Path;

pub struct Lut3D {
    size: usize,
    data: Vec<[f32; 3]>,
    domain_min: [f32; 3],
    domain_max: [f32; 3],
}

impl Lut3D {
    /// Parse a .cube LUT file into a Lut3D
    pub fn from_cube(path: &str) -> Result<Self, Error> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);

        let mut size = 0;
        let mut data = Vec::new();
        let mut domain_min = [0.0; 3];
        let mut domain_max = [1.0; 3];

        for line in reader.lines() {
            let line = line?;
            let line = line.trim();

            if line.starts_with("LUT_3D_SIZE") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                size = parts[1].parse::<usize>().unwrap();
            } else if line.starts_with("DOMAIN_MIN") {
                let parts: Vec<f32> = line.split_whitespace().skip(1)
                    .map(|s| s.parse::<f32>().unwrap()).collect();
                domain_min.copy_from_slice(&parts);
            } else if line.starts_with("DOMAIN_MAX") {
                let parts: Vec<f32> = line.split_whitespace().skip(1)
                    .map(|s| s.parse::<f32>().unwrap()).collect();
                domain_max.copy_from_slice(&parts);
            } else if line.is_empty() || line.starts_with('#') || line.starts_with("TITLE") {
                continue;
            } else {
                let vals: Vec<f32> = line.split_whitespace()
                    .map(|s| s.parse::<f32>().unwrap()).collect();
                if vals.len() == 3 {
                    data.push([vals[0], vals[1], vals[2]]);
                }
            }
        }

        Ok(Lut3D { size, data, domain_min, domain_max })
    }

    /// Apply LUT to a single RGB triplet (u8) via nearest-neighbor
    pub fn apply_lut(&self, r: u8, g: u8, b: u8) -> [u8; 3] {
        let r_f = r as f32 / 255.0;
        let g_f = g as f32 / 255.0;
        let b_f = b as f32 / 255.0;

        // Normalize using domain
        let map = |val: f32, min: f32, max: f32| ((val - min) / (max - min)).clamp(0.0f32, 1.0f32);
        let rn = map(r_f, self.domain_min[0], self.domain_max[0]);
        let gn = map(g_f, self.domain_min[1], self.domain_max[1]);
        let bn = map(b_f, self.domain_min[2], self.domain_max[2]);

        let f = (self.size - 1) as f32;
        let ri = (rn * f).round().clamp(0.0, f) as usize;
        let gi = (gn * f).round().clamp(0.0, f) as usize;
        let bi = (bn * f).round().clamp(0.0, f) as usize;

        let idx = ri + gi * self.size + bi * self.size * self.size;
        let out = self.data[idx];

        [
            (out[0].clamp(0.0, 1.0) * 255.0) as u8,
            (out[1].clamp(0.0, 1.0) * 255.0) as u8,
            (out[2].clamp(0.0, 1.0) * 255.0) as u8,
        ]
    }

    /// Load a precomputed RGB-to-LUT map (binary) or generate and save it.
    /// Returns a flat Vec<u8> of length 256*256*256*3, indexed as (r<<16)|(g<<8)|b.
    pub fn load_or_generate_map(&self, bin_path: &str) -> io::Result<Vec<u8>> {
        let path = Path::new(bin_path);
        if path.exists() {
            fs::read(path)
        } else {
            let mut table = Vec::with_capacity(256 * 256 * 256 * 3);
            for r in 0u8..=255u8 {
                for g in 0u8..=255u8 {
                    for b in 0u8..=255u8 {
                        let color = self.apply_lut(r, g, b);
                        table.push(color[0]);
                        table.push(color[1]);
                        table.push(color[2]);
                    }
                }
            }
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(path, &table)?;
            Ok(table)
        }
    }

    /// Fast apply_lut using a precomputed table loaded via `load_or_generate_map`
    pub fn apply_precomputed(table: &[u8], r: u8, g: u8, b: u8) -> [u8; 3] {
        let idx = ((r as usize) << 16 | (g as usize) << 8 | (b as usize)) * 3;
        [table[idx], table[idx + 1], table[idx + 2]]
    }
}