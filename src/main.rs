use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use std::cmp::Reverse;
use std::fs;

use eframe::egui;
use rayon::prelude::*;

struct FolderScanner {
    target_dir: PathBuf,
    num_folders: usize,
    results: Arc<Mutex<Vec<FolderInfo>>>,
    scanning: bool,
    scan_time: f64,
    error: Option<String>,
    progress: Arc<Mutex<ScanProgress>>,
    scan_time_ptr: Option<Arc<Mutex<f64>>>,
    scanning_ptr: Option<Arc<Mutex<bool>>>,
    target_dir_input: String,
    dark_mode: bool,
    show_pie_chart: bool,
    show_about: bool,
}

#[derive(Debug, Clone)]
struct FolderInfo {
    path: PathBuf,
    size: u64,
}

#[derive(Default)]
struct ScanProgress {
    current: usize,
    total: usize,
    current_path: String,
}

impl Default for FolderScanner {
    fn default() -> Self {
        let current_dir = std::env::current_dir().unwrap();
        Self {
            target_dir: current_dir.clone(),
            num_folders: 10,
            results: Arc::new(Mutex::new(Vec::new())),
            scanning: false,
            scan_time: 0.0,
            error: None,
            progress: Arc::new(Mutex::new(ScanProgress::default())),
            scan_time_ptr: None,
            scanning_ptr: None,
            target_dir_input: current_dir.display().to_string(),
            dark_mode: true,
            show_pie_chart: false,
            show_about: false,
        }
    }
}

impl FolderScanner {
    fn scan(&mut self) -> Result<(), String> {
        self.error = None;
        
        // Validate the target directory
        let path = PathBuf::from(&self.target_dir_input);
        if !path.exists() || !path.is_dir() {
            return Err(format!("Invalid directory: {}", self.target_dir_input));
        }
        self.target_dir = path;
        
        let target_dir = self.target_dir.clone();
        let _num_folders = self.num_folders;
        let results = self.results.clone();
        let progress = self.progress.clone();
        self.scanning = true;
        
        // Reset progress
        *progress.lock().unwrap() = ScanProgress {
            current: 0,
            total: 0,
            current_path: String::new(),
        };
        
        // Clear previous results
        {
            let mut results_lock = results.lock().unwrap();
            results_lock.clear();
        }

        // Create a weak reference to self to update scan_time and scanning state
        let scan_time_ptr = Arc::new(Mutex::new(0.0));
        let scan_time_clone = scan_time_ptr.clone();
        let scanning_ptr = Arc::new(Mutex::new(true));
        let scanning_clone = scanning_ptr.clone();

        rayon::spawn(move || {
            let start_time = Instant::now();
            let dirs = match fs::read_dir(&target_dir) {
                Ok(d) => d,
                Err(e) => {
                    let _error_msg = e.to_string();
                    // In a real app, you'd want to communicate this error back
                    // to the main thread somehow
                    *scanning_clone.lock().unwrap() = false;
                    return;
                }
            };

            let folders: Vec<PathBuf> = dirs
                .filter_map(|entry| entry.ok())
                .filter(|entry| entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false))
                .map(|entry| entry.path())
                .collect();

            // Update total count
            {
                let mut prog = progress.lock().unwrap();
                prog.total = folders.len();
            }

            let sizes: Vec<Result<FolderInfo, String>> = folders
                .par_iter()
                .map(|path| {
                    // Update current path
                    {
                        let mut prog = progress.lock().unwrap();
                        prog.current += 1;
                        prog.current_path = path.display().to_string();
                    }

                    let size = match calculate_dir_size(path, progress.clone()) {
                        Ok(s) => s,
                        Err(e) => return Err(e.to_string()),
                    };
                    Ok(FolderInfo {
                        path: path.clone(),
                        size,
                    })
                })
                .collect();

            let mut successful: Vec<FolderInfo> = sizes
                .into_iter()
                .filter_map(Result::ok)
                .collect();

            // Sort descending by size
            successful.sort_by_key(|info| Reverse(info.size));
            
            let scan_time = start_time.elapsed().as_secs_f64();
            *scan_time_clone.lock().unwrap() = scan_time;
            
            // In a real app, you'd want to communicate these results back
            // to the main thread
            let mut results_lock = results.lock().unwrap();
            *results_lock = successful;
            
            // Mark scanning as complete
            *scanning_clone.lock().unwrap() = false;
        });
        
        // Store the pointers for checking in update
        self.scan_time_ptr = Some(scan_time_ptr);
        self.scanning_ptr = Some(scanning_ptr);
        
        Ok(())
    }
}

fn calculate_dir_size(path: &Path, progress: Arc<Mutex<ScanProgress>>) -> Result<u64, std::io::Error> {
    let mut total = 0;
    let entries = fs::read_dir(path)?;
    
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            total += calculate_dir_size(&path, progress.clone())?;
        } else {
            total += entry.metadata()?.len();
        }
    }
    Ok(total)
}

impl eframe::App for FolderScanner {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Set theme
        if self.dark_mode {
            ctx.set_visuals(egui::Visuals::dark());
        } else {
            ctx.set_visuals(egui::Visuals::light());
        }

        // Set custom style
        let mut style = (*ctx.style()).clone();
        style.spacing.item_spacing = egui::vec2(10.0, 10.0);
        style.spacing.window_margin = egui::style::Margin::same(12.0);
        style.spacing.button_padding = egui::vec2(8.0, 4.0);
        style.visuals.widgets.noninteractive.rounding = egui::Rounding::same(4.0);
        style.visuals.widgets.inactive.rounding = egui::Rounding::same(4.0);
        style.visuals.widgets.active.rounding = egui::Rounding::same(4.0);
        style.visuals.widgets.hovered.rounding = egui::Rounding::same(4.0);
        style.visuals.window_rounding = egui::Rounding::same(6.0);
        ctx.set_style(style);

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.heading("Folder Size Analyzer");
                
                // Add flexible space to push the buttons to the right
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Theme switch
                    let theme_text = if self.dark_mode { "☀ Light" } else { "🌙 Dark" };
                    if ui.button(theme_text).clicked() {
                        self.dark_mode = !self.dark_mode;
                    }
                    ui.add_space(5.0);
                    
                    // About button
                    if ui.button("ℹ About").clicked() {
                        self.show_about = !self.show_about;
                    }
                });
            });
            ui.add_space(4.0);
        });

        // Check if we need to show the about dialog
        if self.show_about {
            egui::Window::new("About")
                .collapsible(false)
                .resizable(true)
                .min_width(600.0)
                .min_height(300.0)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("About Me");
                        ui.add_space(ui.available_width() - 100.0);
                        if ui.button("Close").clicked() {
                            self.show_about = false;
                        }
                    });
                    ui.add_space(5.0);
                    
                    ui.label("Hi! I'm Ivar Avello (aka argos3dworld). I'm building open-source tools to improve system management and other random stuff. Currently, I'm working on a folder size scanning tool for Windows that visually tracks storage usage, helping users identify what's taking up space. The project is released under the MIT License, allowing developers worldwide to contribute and expand its capabilities.");
                    ui.add_space(10.0); 
                    
                    ui.label("I'm looking to connect with fellow developers and raise funds to bring this tool to more platforms and enhance its features. If you'd like to support the project, consider donating:");
                    ui.add_space(10.0);
                    
                    // Cryptocurrency donation addresses
                    ui.group(|ui| {
                        ui.label("BTC: bc1p5dap9ffeumg82mm4vsv6zhks4zdsv5mvjs85kvjmn72lfh58plqsg3g9c5");
                        if ui.button("📋 Copy BTC Address").clicked() {
                            ui.output_mut(|o| o.copied_text = "bc1p5dap9ffeumg82mm4vsv6zhks4zdsv5mvjs85kvjmn72lfh58plqsg3g9c5".to_string());
                        }
                    });
                    
                    ui.group(|ui| {
                        ui.label("ETH: 0xD03ff9f2d25Cc43b60076baB3F4D1a2b07501Dfb");
                        if ui.button("📋 Copy ETH Address").clicked() {
                            ui.output_mut(|o| o.copied_text = "0xD03ff9f2d25Cc43b60076baB3F4D1a2b07501Dfb".to_string());
                        }
                    });
                    
                    ui.group(|ui| {
                        ui.label("SOL: FyRJhuRNRQuMQRawgkJmYrsjUS7K93buWqqoWftYwJMP");
                        if ui.button("📋 Copy SOL Address").clicked() {
                            ui.output_mut(|o| o.copied_text = "FyRJhuRNRQuMQRawgkJmYrsjUS7K93buWqqoWftYwJMP".to_string());
                        }
                    });
                    
                    ui.group(|ui| {
                        ui.label("TON: UQCJmfmBxgVWNIUC_zAEJzAog9sQ2pZWWiNQ7IC72sBAG1Ij");
                        if ui.button("📋 Copy TON Address").clicked() {
                            ui.output_mut(|o| o.copied_text = "UQCJmfmBxgVWNIUC_zAEJzAog9sQ2pZWWiNQ7IC72sBAG1Ij".to_string());
                        }
                    });
                    
                    ui.add_space(10.0);
                    
                    // GitHub repository link
                    ui.horizontal(|ui| {
                        ui.label("GitHub Repository:");
                        ui.hyperlink_to("https://github.com/ivar-avello/folder-size-analyzer", "https://github.com/ivar-avello/folder-size-analyzer");
                    });
                    
                    ui.add_space(10.0);
                    
                    // Close button
                });
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(8.0);
            egui::Frame::none()
                .fill(ui.visuals().extreme_bg_color)
                .inner_margin(egui::style::Margin::same(12.0))
                .rounding(egui::Rounding::same(6.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.strong("Directory:");
                        ui.add_space(4.0);
                        
                        // Directory text input with improved styling
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut self.target_dir_input)
                                .hint_text("Enter directory path...")
                                .desired_width(ui.available_width() - 120.0)
                        );
                        
                        // Handle Enter key press
                        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            let path = PathBuf::from(&self.target_dir_input);
                            if path.is_dir() {
                                self.target_dir = path;
                                self.error = None;
                            } else {
                                self.error = Some(format!("Invalid directory: {}", self.target_dir_input));
                            }
                        }
                        
                        // Handle Ctrl+V for paste
                        if response.has_focus() && ui.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::V)) {
                            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                                if let Ok(text) = clipboard.get_text() {
                                    self.target_dir_input = text;
                                }
                            }
                        }
                    });
                    
                    ui.add_space(4.0);
                    
                    // Place buttons below the directory input
                    ui.horizontal(|ui| {
                        // Browse button
                        if ui.button("📂 Browse").clicked() {
                            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                self.target_dir_input = path.display().to_string();
                                self.target_dir = path;
                                self.error = None;
                            }
                        }
                        
                        // Scan button
                        let scan_button = egui::Button::new(
                            if self.scanning { "⏳ Scanning..." } else { "🔍 Scan" }
                        ).min_size(egui::vec2(100.0, 0.0));
                        
                        if ui.add_enabled(!self.scanning, scan_button).clicked() {
                            match self.scan() {
                                Ok(_) => {},
                                Err(e) => self.error = Some(e),
                            }
                        }
                    });
                    
                    // Show error message if any
                    if let Some(error) = &self.error {
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("⚠").color(egui::Color32::RED));
                            ui.label(egui::RichText::new(error).color(egui::Color32::RED));
                        });
                    }
                });
            
            ui.add_space(8.0);
            
            ui.horizontal(|ui| {
                ui.label("Number of folders to show:");
                ui.add(egui::DragValue::new(&mut self.num_folders)
                    .clamp_range(1..=50)
                    .speed(1.0));
            });
            
            ui.separator();
            
            if self.scanning {
                let progress = self.progress.lock().unwrap();
                
                // Show progress bar
                if progress.total > 0 {
                    let fraction = progress.current as f32 / progress.total as f32;
                    ui.add(egui::ProgressBar::new(fraction)
                        .show_percentage()
                        .animate(true));
                    
                    ui.label(format!(
                        "Scanning {}/{}: {}",
                        progress.current,
                        progress.total,
                        progress.current_path
                    ));
                } else {
                    ui.spinner();
                    ui.label("Preparing scan...");
                }
            }
            
            self.render_results_ui(ui);
        });

        // Check for scan completion
        if self.scanning {
            // Check if the background task has completed
            if let Some(scanning_ptr) = &self.scanning_ptr {
                let is_scanning = *scanning_ptr.lock().unwrap();
                if !is_scanning {
                    self.scanning = false;
                    
                    // Update scan time
                    if let Some(scan_time_ptr) = &self.scan_time_ptr {
                        self.scan_time = *scan_time_ptr.lock().unwrap();
                    }
                }
            }
            
            // In a real implementation, you would check the result of the background task here
            // For now, we'll just keep the UI responsive
            ctx.request_repaint();
        }
    }
}

impl FolderScanner {
    fn show_size_chart(&self, ui: &mut egui::Ui, results: &[FolderInfo], available_width: f32, available_height: f32) {
        let _max_size = results.first().map(|i| i.size as f32).unwrap_or(0.0);
        let total_size: u64 = results.iter().map(|i| i.size).sum();
        
        egui::plot::Plot::new("sizes")
            .height(available_height)
            .width(available_width)
            .show(ui, |plot_ui| {
                let bars: Vec<_> = results
                    .iter()
                    .take(self.num_folders)
                    .enumerate()
                    .map(|(i, info)| {
                        let x = i as f64;
                        
                        // Always use absolute size in GB
                        let size_gb = info.size as f64 / 1e9;
                        
                        // Create label with folder name and size
                        let label = format!(
                            "{}\n{:.2} GB ({:.1}%)",
                            info.path.file_name().unwrap().to_str().unwrap(),
                            size_gb,
                            (info.size as f64 / total_size as f64) * 100.0
                        );
                        
                        egui::plot::Bar::new(x, size_gb)
                            .width(0.6)
                            .name(label)
                    })
                    .collect();

                // Configure the bar chart
                let bar_chart = egui::plot::BarChart::new(bars);
                plot_ui.bar_chart(bar_chart);
            });
    }

    fn show_pie_chart(&self, ui: &mut egui::Ui, results: &[FolderInfo], available_width: f32, available_height: f32) {
        if results.is_empty() {
            return;
        }
        
        let total_size: u64 = results.iter().map(|i| i.size).sum();
        
        // Create a custom pie chart visualization since egui doesn't have a built-in pie chart
        let rect = egui::Rect::from_min_size(
            ui.cursor().min, 
            egui::vec2(available_width, available_height)
        );
        
        let painter = ui.painter();
        let center = rect.center();
        let radius = (rect.height().min(rect.width()) * 0.4).min(200.0);
        
        // Draw pie segments
        let mut start_angle = 0.0;
        let colors = [
            egui::Color32::from_rgb(25, 130, 196),
            egui::Color32::from_rgb(106, 176, 76),
            egui::Color32::from_rgb(234, 67, 53),
            egui::Color32::from_rgb(250, 187, 5),
            egui::Color32::from_rgb(145, 65, 172),
            egui::Color32::from_rgb(66, 133, 244),
            egui::Color32::from_rgb(219, 68, 55),
            egui::Color32::from_rgb(244, 180, 0),
            egui::Color32::from_rgb(15, 157, 88),
            egui::Color32::from_rgb(66, 133, 244),
        ];
        
        let mut legend_items = Vec::new();
        
        for (i, info) in results.iter().take(self.num_folders).enumerate() {
            let percentage = info.size as f64 / total_size as f64;
            let sweep_angle = percentage * std::f64::consts::TAU;
            let end_angle = start_angle + sweep_angle;
            
            // Draw pie segment - using an arc with multiple points for a smooth curve
            let color = colors[i % colors.len()];
            
            // Create a proper pie slice with multiple points along the arc
            let mut points = vec![center];
            
            // Add points along the arc
            let num_points = 40; // More points = smoother curve
            for j in 0..=num_points {
                let angle = start_angle + (sweep_angle * j as f64 / num_points as f64);
                points.push(center + radius * egui::vec2(angle.cos() as f32, angle.sin() as f32));
            }
            
            // Close the shape back to center
            points.push(center);
            
            // Draw the filled shape
            painter.add(egui::Shape::Path(egui::epaint::PathShape {
                points,
                closed: true,
                fill: color,
                stroke: egui::Stroke::new(1.0, egui::Color32::WHITE),
            }));
            
            // Add to legend
            let folder_name = info.path.file_name().unwrap().to_str().unwrap();
            legend_items.push((
                folder_name.to_string(),
                format!("{:.2} GB ({:.1}%)", info.size as f64 / 1e9, percentage * 100.0),
                color
            ));
            
            start_angle = end_angle;
        }
        
        // Draw legend
        let legend_rect = egui::Rect::from_min_size(
            egui::pos2(rect.right() - 200.0, rect.top()), 
            egui::vec2(200.0, rect.height())
        );
        
        ui.allocate_ui_at_rect(legend_rect, |ui| {
            ui.vertical(|ui| {
                ui.heading("Legend");
                ui.add_space(5.0);
                
                for (name, size, color) in legend_items {
                    ui.horizontal(|ui| {
                        let rect = ui.spacing().interact_size;
                        ui.painter().rect_filled(
                            egui::Rect::from_min_size(ui.cursor().min, rect),
                            0.0,
                            color
                        );
                        ui.add_space(rect.x);
                        ui.label(format!("{}: {}", name, size));
                    });
                }
            });
        });
        
        // Advance cursor
        ui.allocate_rect(rect, egui::Sense::hover());
    }
    
    fn render_results_ui(&mut self, ui: &mut egui::Ui) {
        // Get a clone of the results to avoid borrow checker issues
        let results = self.results.lock().unwrap().clone();
        
        if !results.is_empty() {
            if let Some(scan_time_ptr) = &self.scan_time_ptr {
                let scan_time = *scan_time_ptr.lock().unwrap();
                ui.horizontal(|ui| {
                    ui.label(format!("Scan completed in {:.2} seconds", scan_time));
                    
                    // Add copy path button
                    if ui.button("📋 Copy Path").clicked() {
                        if let Ok(mut clipboard) = arboard::Clipboard::new() {
                            let _ = clipboard.set_text(self.target_dir.display().to_string());
                        }
                    }
                });
            }
            
            ui.add_space(8.0);
            
            // Results section with improved styling
            egui::Frame::none()
                .fill(ui.visuals().extreme_bg_color)
                .inner_margin(egui::style::Margin::same(12.0))
                .rounding(egui::Rounding::same(6.0))
                .show(ui, |ui| {
                    // Size distribution header with chart toggle
                    ui.horizontal(|ui| {
                        ui.columns(2, |columns| {
                            columns[0].strong("Size Distribution");
                            columns[1].with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let pie_text = if self.show_pie_chart { "📊 Bar Chart" } else { "🥧 Pie Chart" };
                                if ui.button(pie_text).clicked() {
                                    self.show_pie_chart = !self.show_pie_chart;
                                }
                            });
                        });
                    });
                    
                    ui.add_space(8.0);
                    
                    // Chart area with dynamic sizing
                    let available_width = ui.available_width();
                    let chart_height = 200.0;
                    
                    if self.show_pie_chart {
                        self.show_pie_chart(ui, &results, available_width, chart_height);
                    } else {
                        self.show_size_chart(ui, &results, available_width, chart_height);
                    }
                    
                    ui.add_space(8.0);
                    
                    // Folder list with improved styling
                    ui.strong("Folder Details");
                    ui.add_space(4.0);
                    
                    egui::ScrollArea::vertical()
                        .max_height(300.0)
                        .show(ui, |ui| {
                            // Table header
                            ui.horizontal(|ui| {
                                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                    ui.add(egui::Label::new(egui::RichText::new("Folder").strong()).wrap(false))
                                        .on_hover_text("Folder name");
                                    ui.add_space(ui.available_width() * 0.6);
                                });
                                
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.add(egui::Label::new(egui::RichText::new("Size").strong()).wrap(false));
                                    ui.add_space(100.0);
                                    ui.add(egui::Label::new(egui::RichText::new("%").strong()).wrap(false));
                                    ui.add_space(50.0);
                                });
                            });
                            
                            ui.separator();
                            
                            // Table rows
                            for info in results.iter() {
                                ui.horizontal(|ui| {
                                    // Folder path with tooltip
                                    let path_text = if let Some(file_name) = info.path.file_name() {
                                        file_name.to_string_lossy().to_string()
                                    } else {
                                        info.path.display().to_string()
                                    };
                                    
                                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                        let path_text_clone = path_text.clone();
                                        let path_label = ui.add(egui::Label::new(path_text).wrap(false));
                                        if path_label.hovered() {
                                            egui::show_tooltip(ui.ctx(), egui::Id::new("path_tooltip"), |ui| {
                                                ui.label(info.path.display().to_string());
                                            });
                                        }
                                        
                                        // Copy button
                                        if ui.small_button("📋").clicked() {
                                            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                                                let _ = clipboard.set_text(info.path.display().to_string());
                                            }
                                        }
                                        
                                        ui.add_space(ui.available_width() * 0.6 - path_text_clone.len() as f32 * 7.0 - 30.0);
                                    });
                                    
                                    // Size and percentage
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        ui.add(egui::Label::new(format!("{:.1} MB", info.size as f64 / 1_000_000.0)).wrap(false));
                                        ui.add_space(100.0 - 50.0);
                                        
                                        // Calculate percentage
                                        let total_size: u64 = results.iter().map(|i| i.size).sum();
                                        let percentage = if total_size > 0 {
                                            (info.size as f64 / total_size as f64) * 100.0
                                        } else {
                                            0.0
                                        };
                                        
                                        ui.add(egui::Label::new(format!("{:.1}%", percentage)).wrap(false));
                                        ui.add_space(50.0 - 20.0);
                                    });
                                });
                            }
                        });
                });
        }
    }
}
fn main() {
    let options = eframe::NativeOptions {
        initial_window_size: Some(egui::vec2(800.0, 600.0)),
        min_window_size: Some(egui::vec2(600.0, 400.0)),
        follow_system_theme: false,
        default_theme: eframe::Theme::Dark,
        centered: true,
        decorated: true,
        transparent: false,
        ..Default::default()
    };
    
    eframe::run_native(
        "Folder Size Analyzer",
        options,
        Box::new(|_cc| Box::new(FolderScanner::default())),
    ).unwrap();
}