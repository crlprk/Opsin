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

/// Configuration structure for the application, loaded from `config.toml`.
/// Contains input/output paths and LUT selection settings.
#[derive(Deserialize)]
struct Config {
    input: InputPaths,
    lut: LutConfig,
}

/// Defines the input and output directory paths used by the application.
/// These paths specify where to find source files and where to save processed results.
#[derive(Deserialize)]
struct InputPaths {
    /// Directory containing input images to be processed
    image_dir: PathBuf,
    /// Directory containing input videos to be processed
    video_dir: PathBuf,
    /// Directory where processed files will be saved
    output: PathBuf,
}

/// Configuration related to Look-Up Tables (LUTs).
/// Stores the currently selected LUT for color grading operations.
#[derive(Deserialize)]
struct LutConfig {
    /// The filename of the currently selected LUT file
    selected: String,
}

/// Reads the application configuration from the `config.toml` file.
/// 
/// # Returns
/// A `Config` struct containing all application settings
/// 
/// # Panics
/// Panics if `config.toml` cannot be read or if the TOML format is invalid
fn read_config() -> Config {
    let toml_str = fs::read_to_string("config.toml").expect("Failed to read config.toml");
    toml::from_str(&toml_str).expect("Failed to parse config.toml")
}

/// Loads an application icon from the specified file path.
/// 
/// # Arguments
/// * `path` - File path to the icon image
/// 
/// # Returns
/// An `IconData` structure containing the icon's RGBA data and dimensions
/// 
/// # Panics
/// Panics if the icon file cannot be loaded or decoded as an image
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

/// Scans a directory for available LUT files and returns their names.
/// Only files with the `.cube` extension are considered valid LUTs.
/// 
/// # Arguments
/// * `lut_dir` - Path to the directory containing LUT files
/// 
/// # Returns
/// A sorted vector of LUT filenames (without path, including extension)
fn list_luts(lut_dir: &Path) -> Vec<String> {
    let mut entries = Vec::new();
    if let Ok(read_dir) = fs::read_dir(lut_dir) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            // Check for .cube file extension (case-insensitive)
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if ext.eq_ignore_ascii_case("cube") {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        entries.push(name.to_string());
                    }
                }
            }
        }
    }
    entries.sort(); // Alphabetical ordering for consistent UI display
    entries
}

/// Main application structure for the Opsin color grading tool.
/// Manages the GUI state, file paths, processing status, and threading.
struct OpsinApp {
    /// Directory containing source images for processing
    image_input_dir: PathBuf,
    /// Directory containing source videos for processing
    video_input_dir: PathBuf,
    /// Directory where processed files will be saved
    output_dir: PathBuf,
    /// Directory containing available LUT files
    lut_dir: PathBuf,
    /// List of discovered LUT filenames
    available_luts: Vec<String>,
    /// Currently selected LUT filename
    current_lut: String,
    /// Thread-safe log for status messages displayed in the GUI
    status_log: Arc<Mutex<Vec<String>>>,
    /// Flag indicating whether file processing is currently active
    is_processing: bool,
    /// Channel receiver for completion signals from the processing thread
    processing_completion_receiver: Option<mpsc::Receiver<()>>,
}

impl OpsinApp {
    /// Creates a new `OpsinApp` instance with configuration loaded from file.
    /// Initializes the LUT directory, discovers available LUTs, and sets up initial state.
    /// 
    /// # Returns
    /// A fully initialized `OpsinApp` ready for use
    fn new() -> Self {
        let cfg = read_config();
        let fixed_lut_dir = PathBuf::from("assets/luts");
        
        // Ensure the LUT directory exists
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
    /// Provides a default `OpsinApp` instance by calling `new()`.
    fn default() -> Self {
        OpsinApp::new()
    }
}

impl App for OpsinApp {
    /// Main update loop called each frame by the eframe framework.
    /// Handles GUI rendering, user interactions, and processing thread management.
    /// 
    /// # Arguments
    /// * `ctx` - The egui context for rendering GUI elements
    /// * `_frame` - Frame information (unused in this implementation)
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Check if background processing has completed
        if self.is_processing {
            if let Some(receiver) = &self.processing_completion_receiver {
                if matches!(
                    receiver.try_recv(),
                    Ok(()) | Err(mpsc::TryRecvError::Disconnected)
                ) {
                    // Processing thread has finished
                    self.is_processing = false;
                    self.processing_completion_receiver = None;
                }
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Opsin");

            // LUT selection dropdown
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

            // Processing control button
            if self.is_processing {
                ui.label("Processing... please wait.");
            } else if ui.button("Start Processing").clicked() {
                self.is_processing = true;
                
                // Clone data needed for the background thread
                let image_dir = self.image_input_dir.clone();
                let video_dir = self.video_input_dir.clone();
                let output_dir = self.output_dir.clone();
                let lut_file = self.lut_dir.join(&self.current_lut);
                let bin_name = format!("precomputed_{}.bin", &self.current_lut);
                let bin_path = self.lut_dir.join(bin_name);
                let log_arc = self.status_log.clone();

                // Set up completion signaling
                let (sender, receiver) = mpsc::channel::<()>();
                self.processing_completion_receiver = Some(receiver);

                // Spawn background processing thread
                thread::spawn(move || {
                    // Helper closure for thread-safe logging
                    let local_log = |msg: &str| {
                        if let Ok(mut log_vec) = log_arc.lock() {
                            log_vec.push(msg.to_string());
                        }
                    };

                    // Load and process the selected LUT
                    local_log(&format!("Loading LUT from {}", lut_file.display()));
                    if let Ok(lut3d) = Lut3D::from_cube(lut_file.to_str().unwrap_or_default()) {
                        local_log(&format!("Loaded LUT: {}", lut_file.display()));
                        
                        // Generate or load precomputed LUT mapping table
                        match lut3d.load_or_generate_map(bin_path.to_str().unwrap_or_default()) {
                            Ok(table) => {
                                // Process images using the LUT
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

                    // Process videos (note: video processing doesn't use LUT in current implementation)
                    local_log("Starting video processing...");
                    file_handler::process_videos(&video_dir, &output_dir, log_arc.clone());
                    local_log("Video processing complete.");

                    // Signal completion to the main thread
                    let _ = sender.send(());
                });
            }

            // Status log display
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

        // Request frequent repaints to keep the UI responsive during processing
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}

/// Application entry point.
/// Sets up the native window with an icon and starts the eframe event loop.
/// 
/// # Returns
/// Result indicating success or failure of the application startup
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