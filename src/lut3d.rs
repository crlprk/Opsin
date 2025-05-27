use std::{
    fs,
    fs::File,
    io::{self, BufRead, BufReader, Error},
    path::Path,
};

pub struct Lut3D {
    size: usize,
    data: Vec<[f32; 3]>,
    domain_min: [f32; 3],
    domain_max: [f32; 3],
}

impl Lut3D {
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
                if parts.len() >= 2 {
                    size = parts[1].parse::<usize>().unwrap_or_else(|_| {
                        eprintln!("Warning: Failed to parse LUT_3D_SIZE value: {}", parts[1]);
                        0
                    });
                }
            } else if line.starts_with("DOMAIN_MIN") {
                let parts: Vec<f32> = line
                    .split_whitespace()
                    .skip(1)
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
                let parts: Vec<f32> = line
                    .split_whitespace()
                    .skip(1)
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
                continue;
            } else {
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
        if size == 0 {
            return Err(Error::new(
                io::ErrorKind::InvalidData,
                "LUT_3D_SIZE is missing or invalid.",
            ));
        }
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

    pub fn apply_lut(&self, r: u8, g: u8, b: u8) -> [u8; 3] {
        let r_f = r as f32 / 255.0;
        let g_f = g as f32 / 255.0;
        let b_f = b as f32 / 255.0;

        let map = |val: f32, min: f32, max: f32| ((val - min) / (max - min)).clamp(0.0f32, 1.0f32);
        let rn = map(r_f, self.domain_min[0], self.domain_max[0]);
        let gn = map(g_f, self.domain_min[1], self.domain_max[1]);
        let bn = map(b_f, self.domain_min[2], self.domain_max[2]);

        let f = (self.size - 1) as f32;
        let ri = (rn * f).round().clamp(0.0, f) as usize;
        let gi = (gn * f).round().clamp(0.0, f) as usize;
        let bi = (bn * f).round().clamp(0.0, f) as usize;

        let idx = ri + gi * self.size + bi * self.size * self.size;

        if idx >= self.data.len() {
            eprintln!(
                "Warning: LUT index out of bounds. idx: {}, data_len: {}",
                idx,
                self.data.len()
            );
            return [0, 0, 0];
        }
        let out = self.data[idx];

        [
            (out[0].clamp(0.0, 1.0) * 255.0) as u8,
            (out[1].clamp(0.0, 1.0) * 255.0) as u8,
            (out[2].clamp(0.0, 1.0) * 255.0) as u8,
        ]
    }

    pub fn apply_lut_trilinear(&self, r: u8, g: u8, b: u8) -> [u8; 3] {
        let r_f = r as f32 / 255.0;
        let g_f = g as f32 / 255.0;
        let b_f = b as f32 / 255.0;

        let map = |val: f32, min: f32, max: f32| ((val - min) / (max - min)).clamp(0.0f32, 1.0f32);
        let rn = map(r_f, self.domain_min[0], self.domain_max[0]);
        let gn = map(g_f, self.domain_min[1], self.domain_max[1]);
        let bn = map(b_f, self.domain_min[2], self.domain_max[2]);

        let f = (self.size - 1) as f32;
        let rx = rn * f;
        let gx = gn * f;
        let bx = bn * f;

        let r0 = rx.floor() as usize;
        let g0 = gx.floor() as usize;
        let b0 = bx.floor() as usize;

        let r1 = (r0 + 1).min(self.size - 1);
        let g1 = (g0 + 1).min(self.size - 1);
        let b1 = (b0 + 1).min(self.size - 1);

        let dr = rx - r0 as f32;
        let dg = gx - g0 as f32;
        let db = bx - b0 as f32;

        let idx = |r, g, b| r + g * self.size + b * self.size * self.size;

        let c000 = self.data[idx(r0, g0, b0)];
        let c001 = self.data[idx(r0, g0, b1)];
        let c010 = self.data[idx(r0, g1, b0)];
        let c011 = self.data[idx(r0, g1, b1)];
        let c100 = self.data[idx(r1, g0, b0)];
        let c101 = self.data[idx(r1, g0, b1)];
        let c110 = self.data[idx(r1, g1, b0)];
        let c111 = self.data[idx(r1, g1, b1)];

        let lerp = |a: f32, b: f32, t: f32| a * (1.0 - t) + b * t;
        let lerp3 = |a: [f32; 3], b: [f32; 3], t: f32| {
            [
                lerp(a[0], b[0], t),
                lerp(a[1], b[1], t),
                lerp(a[2], b[2], t),
            ]
        };

        let c00 = lerp3(c000, c100, dr);
        let c01 = lerp3(c001, c101, dr);
        let c10 = lerp3(c010, c110, dr);
        let c11 = lerp3(c011, c111, dr);

        let c0 = lerp3(c00, c10, dg);
        let c1 = lerp3(c01, c11, dg);

        let c = lerp3(c0, c1, db);

        [
            (c[0].clamp(0.0, 1.0) * 255.0) as u8,
            (c[1].clamp(0.0, 1.0) * 255.0) as u8,
            (c[2].clamp(0.0, 1.0) * 255.0) as u8,
        ]
    }

    pub fn load_or_generate_map(&self, bin_path: &str) -> io::Result<Vec<u8>> {
        let path = Path::new(bin_path);
        if path.exists() {
            fs::read(path)
        } else {
            let mut table = Vec::with_capacity(256 * 256 * 256 * 3);
            for r_val in 0u8..=255u8 {
                for g_val in 0u8..=255u8 {
                    for b_val in 0u8..=255u8 {
                        let color = self.apply_lut_trilinear(r_val, g_val, b_val);
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

    pub fn apply_precomputed(table: &[u8], r: u8, g: u8, b: u8) -> [u8; 3] {
        let idx = ((r as usize) << 16 | (g as usize) << 8 | (b as usize)) * 3;
        [table[idx], table[idx + 1], table[idx + 2]]
    }
}