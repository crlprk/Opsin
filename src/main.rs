mod file_handler;
mod lut3d;
mod metadata_handler;

use crate::lut3d::Lut3D;
use eframe::{egui, App, NativeOptions};
use egui::IconData;
use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Mutex},
    thread,
};

#[derive(Deserialize)]
struct Config {
    input: InputPaths,
    lut: LutConfig,
}

#[derive(Deserialize)]
struct InputPaths {
    image_dir: PathBuf,
    video_dir: PathBuf,
    output: PathBuf,
}

#[derive(Deserialize)]
struct LutConfig {
    selected: String,
}

fn read_config() -> Config {
    let toml_str = fs::read_to_string("config.toml").expect("Failed to read config.toml");
    toml::from_str(&toml_str).expect("Failed to parse config.toml")
}

fn load_icon(path: &str) -> IconData {
    let img = image::open(path)
        .unwrap_or_else(|e| panic!("Failed to load icon `{}`: {}", path, e))
        .into_rgba8();
    let (width, height) = img.dimensions();
    IconData {
        rgba: img.into_raw(),
        width,
        height,
    }
}

fn list_luts(lut_dir: &Path) -> Vec<String> {
    let mut entries = Vec::new();
    if let Ok(read_dir) = fs::read_dir(lut_dir) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if ext.eq_ignore_ascii_case("cube") {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        entries.push(name.to_string());
                    }
                }
            }
        }
    }
    entries.sort();
    entries
}

struct OpsinApp {
    image_input_dir: PathBuf,
    video_input_dir: PathBuf,
    output_dir: PathBuf,
    lut_dir: PathBuf,
    available_luts: Vec<String>,
    current_lut: String,
    status_log: Arc<Mutex<Vec<String>>>,
    is_processing: bool,
    processing_completion_receiver: Option<mpsc::Receiver<()>>,
}

impl OpsinApp {
    fn new() -> Self {
        let cfg = read_config();
        let fixed_lut_dir = PathBuf::from("assets/luts");
        if !fixed_lut_dir.exists() {
            if let Err(e) = fs::create_dir_all(&fixed_lut_dir) {
                eprintln!(
                    "Warning: Failed to create LUT directory at {}: {}",
                    fixed_lut_dir.display(),
                    e
                );
            }
        }
        let luts = list_luts(&fixed_lut_dir);
        OpsinApp {
            image_input_dir: cfg.input.image_dir,
            video_input_dir: cfg.input.video_dir,
            output_dir: cfg.input.output,
            lut_dir: fixed_lut_dir,
            available_luts: luts,
            current_lut: cfg.lut.selected,
            status_log: Arc::new(Mutex::new(Vec::new())),
            is_processing: false,
            processing_completion_receiver: None,
        }
    }
}

impl Default for OpsinApp {
    fn default() -> Self {
        OpsinApp::new()
    }
}

impl App for OpsinApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.is_processing {
            if let Some(receiver) = &self.processing_completion_receiver {
                if matches!(
                    receiver.try_recv(),
                    Ok(()) | Err(mpsc::TryRecvError::Disconnected)
                ) {
                    self.is_processing = false;
                    self.processing_completion_receiver = None;
                }
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Opsin");

            ui.horizontal(|ui| {
                ui.label("Select LUT:");
                egui::ComboBox::from_label("LUT")
                    .selected_text(&self.current_lut)
                    .show_ui(ui, |ui| {
                        for lut in &self.available_luts {
                            ui.selectable_value(&mut self.current_lut, lut.clone(), lut);
                        }
                    });
            });

            if self.is_processing {
                ui.label("Processing... please wait.");
            } else if ui.button("Start Processing").clicked() {
                self.is_processing = true;
                let image_dir = self.image_input_dir.clone();
                let video_dir = self.video_input_dir.clone();
                let output_dir = self.output_dir.clone();
                let lut_file = self.lut_dir.join(&self.current_lut);
                let bin_name = format!("precomputed_{}.bin", &self.current_lut);
                let bin_path = self.lut_dir.join(bin_name);
                let log_arc = self.status_log.clone();

                let (sender, receiver) = mpsc::channel::<()>();
                self.processing_completion_receiver = Some(receiver);

                thread::spawn(move || {
                    let local_log = |msg: &str| {
                        if let Ok(mut log_vec) = log_arc.lock() {
                            log_vec.push(msg.to_string());
                        }
                    };

                    local_log(&format!("Loading LUT from {}", lut_file.display()));
                    if let Ok(lut3d) = Lut3D::from_cube(lut_file.to_str().unwrap_or_default()) {
                        local_log(&format!("Loaded LUT: {}", lut_file.display()));
                        match lut3d.load_or_generate_map(bin_path.to_str().unwrap_or_default()) {
                            Ok(table) => {
                                local_log("Starting image processing...");
                                file_handler::process_images(
                                    &image_dir,
                                    &output_dir,
                                    &table,
                                    log_arc.clone(),
                                );
                                local_log("Image processing complete.");
                            }
                            Err(e) => {
                                local_log(&format!(
                                    "Error loading LUT map {}: {}",
                                    bin_path.display(),
                                    e
                                ));
                            }
                        }
                    } else {
                        local_log(&format!("Error reading LUT file {}", lut_file.display()));
                    }

                    local_log("Starting video processing...");
                    file_handler::process_videos(&video_dir, &output_dir, log_arc.clone());
                    local_log("Video processing complete.");

                    let _ = sender.send(());
                });
            }

            ui.separator();
            ui.label("Log:");
            egui::ScrollArea::vertical().show(ui, |ui| {
                if let Ok(log_entries) = self.status_log.lock() {
                    for entry in log_entries.iter() {
                        ui.label(entry);
                    }
                }
            });
        });

        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}

fn main() -> eframe::Result<()> {
    let icon = load_icon("assets/icon.png");
    let mut native_options = NativeOptions::default();
    native_options.viewport = native_options.viewport.with_icon(Arc::new(icon));
    eframe::run_native(
        "Opsin",
        native_options,
        Box::new(|_cc| Ok(Box::new(OpsinApp::default()))),
    )
}