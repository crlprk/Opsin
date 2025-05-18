//! Opsin is an application for applying 3D LUTs (Look-Up Tables) to images.
//! It provides a simple GUI to select a LUT, specify input/output directories,
//! and process images in a background thread.

// Module declarations for different parts of the application.
mod lut3d;
mod metadata_handler;
mod file_handler;

use crate::lut3d::Lut3D;
use eframe::{egui, App, NativeOptions};
use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Mutex}, // mpsc for thread communication, Arc/Mutex for shared state.
    thread,
};

/// `Config` struct holds the application's configuration, deserialized from `config.toml`.
#[derive(Deserialize)]
struct Config {
    input: InputPaths,
    lut: LutConfig,
}

/// `InputPaths` struct holds the directory paths for input images and processed output.
#[derive(Deserialize)]
struct InputPaths {
    dir: PathBuf,
    output: PathBuf,
}

/// `LutConfig` struct holds configuration related to LUTs, specifically the default selected LUT.
#[derive(Deserialize)]
struct LutConfig {
    selected: String, // Filename of the default LUT.
}

/// Reads the application configuration from `config.toml`.
/// Panics if the file cannot be read or parsed.
fn read_config() -> Config {
    let toml_str = fs::read_to_string("config.toml").expect("Failed to read config.toml");
    toml::from_str(&toml_str).expect("Failed to parse config.toml")
}

/// Lists all `.cube` LUT files in the specified directory.
///
/// # Arguments
/// * `lut_dir` - A reference to the path of the directory containing LUT files.
///
/// # Returns
/// A vector of strings, where each string is the filename of a `.cube` file.
fn list_luts(lut_dir: &Path) -> Vec<String> {
    let mut entries = Vec::new();
    if let Ok(read_dir) = fs::read_dir(lut_dir) {
        for entry in read_dir.flatten() { // .flatten() ignores errors for individual entries.
            let path = entry.path();
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if ext.eq_ignore_ascii_case("cube") { // Case-insensitive check for ".cube" extension.
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        entries.push(name.to_string());
                    }
                }
            }
        }
    }
    entries.sort(); // Sort LUT names alphabetically.
    entries
}

/// `OpsinApp` is the main application struct, holding its state.
struct OpsinApp {
    input_dir: PathBuf,
    output_dir: PathBuf,
    lut_dir: PathBuf,                             // Directory where LUT files are stored.
    available_luts: Vec<String>,                  // List of LUT filenames available for selection.
    current_lut: String,                          // The currently selected LUT filename.
    status_log: Arc<Mutex<Vec<String>>>,          // Shared log for status messages from the processing thread.
    is_processing: bool,                          // Flag indicating if image processing is currently active.
    processing_completion_receiver: Option<mpsc::Receiver<()>>, // Channel receiver for completion signals from the worker thread.
}

impl OpsinApp {
    /// Creates a new instance of `OpsinApp`, initializing it from `config.toml`
    /// and by scanning the LUT directory.
    fn new() -> Self {
        let cfg = read_config();
        // Define a fixed relative path for LUTs.
        let fixed_lut_dir = PathBuf::from("assets/luts");

        // Create the LUT directory if it doesn't exist.
        if !fixed_lut_dir.exists() {
            if let Err(e) = fs::create_dir_all(&fixed_lut_dir) {
                eprintln!("Warning: Failed to create LUT directory at {}: {}", fixed_lut_dir.display(), e);
            }
        }

        let luts = list_luts(&fixed_lut_dir);
        OpsinApp {
            input_dir: cfg.input.dir,
            output_dir: cfg.input.output,
            lut_dir: fixed_lut_dir,
            available_luts: luts,
            current_lut: cfg.lut.selected, // Default LUT from config.
            status_log: Arc::new(Mutex::new(Vec::new())),
            is_processing: false,
            processing_completion_receiver: None,
        }
    }
}

impl Default for OpsinApp {
    /// Provides a default instance of `OpsinApp`.
    fn default() -> Self {
        OpsinApp::new()
    }
}

impl App for OpsinApp {
    /// `update` is called by `eframe` on each frame to draw the UI and handle logic.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Check if the background processing thread has signaled completion.
        if self.is_processing {
            if let Some(receiver) = &self.processing_completion_receiver {
                match receiver.try_recv() { // Non-blocking check for a message.
                    Ok(()) | Err(mpsc::TryRecvError::Disconnected) => {
                        // Message received or channel disconnected (thread finished).
                        self.is_processing = false;
                        self.processing_completion_receiver = None; // Clear the receiver.
                    }
                    Err(mpsc::TryRecvError::Empty) => {
                        // No message yet, processing is still ongoing.
                    }
                }
            }
        }

        // Define the central panel for the UI.
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Opsin"); // Application title.

            // Horizontal layout for LUT selection.
            ui.horizontal(|ui| {
                ui.label("Select LUT:");
                egui::ComboBox::from_label("LUT")
                    .selected_text(&self.current_lut)
                    .show_ui(ui, |ui| {
                        for lut in &self.available_luts {
                            // `selectable_value` updates `self.current_lut` when a new item is chosen.
                            ui.selectable_value(&mut self.current_lut, lut.clone(), lut);
                        }
                    });
            });

            // Display either the "Processing..." message or the "Start Processing" button.
            if self.is_processing {
                ui.label("Processing... please wait.");
            } else if ui.button("Start Processing").clicked() {
                self.is_processing = true; // Set processing flag.
                // Clone necessary data for the worker thread.
                let input_dir = self.input_dir.clone();
                let output_dir = self.output_dir.clone();
                let lut_file = self.lut_dir.join(&self.current_lut);
                let bin_name = format!("precomputed_{}.bin", &self.current_lut); // For cached precomputed LUT data.
                let bin_path = self.lut_dir.join(bin_name);
                let log_arc = self.status_log.clone(); // Clone Arc for the log.

                // Create a channel to signal completion from the thread.
                let (sender, receiver) = mpsc::channel::<()>();
                self.processing_completion_receiver = Some(receiver);

                // Spawn a new thread for image processing to keep the UI responsive.
                thread::spawn(move || {
                    // Helper closure for logging messages from the thread.
                    let local_log = |msg: &str| {
                        if let Ok(mut log_vec) = log_arc.lock() {
                             log_vec.push(msg.to_string());
                        }
                    };

                    local_log(&format!("Loading LUT from {}", lut_file.display()));
                    match Lut3D::from_cube(lut_file.to_str().unwrap_or_default()) { // `unwrap_or_default` handles non-UTF8 paths gracefully.
                        Ok(lut3d) => {
                            local_log(&format!("Successfully loaded LUT: {}", lut_file.display()));
                            // Load or generate a precomputed version of the LUT for faster application.
                            match lut3d.load_or_generate_map(bin_path.to_str().unwrap_or_default()) {
                                Ok(table) => {
                                    local_log("Precomputed LUT map loaded/generated. Starting image processing...");
                                    // Call the file processing function.
                                    file_handler::process_files(&input_dir, &output_dir, &table);
                                    local_log("Done processing images.");
                                }
                                Err(e) => local_log(&format!("Error generating/loading precomputed LUT map from {}: {}", bin_path.display(), e)),
                            }
                        }
                        Err(e) => local_log(&format!("Error reading LUT file {}: {}", lut_file.display(), e)),
                    }
                    // Send a signal indicating the thread has completed its work.
                    // The result is ignored in case the receiver has been dropped (e.g., UI closed).
                    let _ = sender.send(());
                });
            }

            ui.separator(); // Visual separator.
            ui.label("Log:"); // Log display area.
            egui::ScrollArea::vertical().show(ui, |ui| {
                if let Ok(log_entries) = self.status_log.lock() {
                    for entry in log_entries.iter() {
                        ui.label(entry);
                    }
                }
            });
        });

        // Request a repaint to ensure the UI updates, especially for the log and processing status.
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}

/// The main entry point of the application.
fn main() -> eframe::Result<()> {
    let native_options = NativeOptions::default(); // Default options for the native window.
    // Run the eframe application.
    eframe::run_native(
        "Opsin", // Window title.
        native_options,
        Box::new(|_cc| Ok(Box::new(OpsinApp::default()))), // Creation callback.
    )
}
