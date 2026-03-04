use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;
use std::time::UNIX_EPOCH;

use eframe::egui::{
    self, Align, Color32, CursorIcon, FontFamily, FontId, Key, RichText, Sense, Stroke,
    TextureHandle,
};
use lupa_core::{
    DoctorReport, IndexStats, LupaConfig, LupaEngine, SearchHit, SearchOptions, SearchResult,
};
use notify::{recommended_watcher, RecursiveMode, Watcher};

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 840.0])
            .with_min_inner_size([1024.0, 768.0])
            .with_title("Lupa | Local AI Indexer")
            .with_transparent(true),
        ..Default::default()
    };

    eframe::run_native(
        "Lupa GUI",
        options,
        Box::new(|cc| {
            apply_style(&cc.egui_ctx);
            Ok(Box::new(LupaApp::new()))
        }),
    )
}

fn apply_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(12.0, 12.0);
    style.spacing.button_padding = egui::vec2(16.0, 10.0);
    style.spacing.menu_margin = egui::Margin::same(12.0);
    style.spacing.window_margin = egui::Margin::same(16.0);
    style.spacing.interact_size = egui::vec2(28.0, 30.0);

    style.text_styles = [
        (
            egui::TextStyle::Heading,
            FontId::new(30.0, FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Name("Subheading".into()),
            FontId::new(22.0, FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Body,
            FontId::new(15.0, FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Button,
            FontId::new(14.0, FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Monospace,
            FontId::new(14.0, FontFamily::Monospace),
        ),
        (
            egui::TextStyle::Small,
            FontId::new(12.0, FontFamily::Proportional),
        ),
    ]
    .into();

    let mut visuals = egui::Visuals::dark();

    // Modern Midnight/Space Palette
    let bg_deep = Color32::from_rgb(10, 10, 15);
    let bg_pane = Color32::from_rgb(17, 17, 25);
    let bg_hover = Color32::from_rgb(30, 30, 45);
    let accent_primary = Color32::from_rgb(99, 102, 241); // Indigo
    let accent_light = Color32::from_rgb(129, 140, 248);
    let text_dim = Color32::from_rgb(150, 155, 180);
    let text_bright = Color32::from_rgb(230, 235, 255);

    visuals.panel_fill = bg_deep;
    visuals.window_fill = bg_pane;
    visuals.extreme_bg_color = Color32::from_rgb(5, 5, 8); // For text edits

    // Rounding and Strokes
    let rounding = egui::Rounding::same(12.0);
    visuals.window_rounding = rounding;
    visuals.menu_rounding = rounding;

    // Widgets
    // Non-interactive (e.g. frames)
    visuals.widgets.noninteractive.bg_fill = bg_pane;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(40, 40, 60));
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, text_dim);
    visuals.widgets.noninteractive.rounding = rounding;

    // Inactive (default state)
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(25, 25, 38);
    visuals.widgets.inactive.bg_stroke = Stroke::new(0.0, Color32::TRANSPARENT);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, text_dim);
    visuals.widgets.inactive.rounding = rounding;

    // Hovered
    visuals.widgets.hovered.bg_fill = bg_hover;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, accent_primary);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, text_bright);
    visuals.widgets.hovered.rounding = rounding;

    // Active (clicked/selected)
    visuals.widgets.active.bg_fill = accent_primary;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, accent_light);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, text_bright);
    visuals.widgets.active.rounding = rounding;

    visuals.selection.bg_fill = accent_primary.linear_multiply(0.3);
    visuals.selection.stroke = Stroke::new(1.0, accent_primary);

    visuals.override_text_color = Some(text_dim);

    style.visuals = visuals;
    ctx.set_style(style);
}

#[derive(Default)]
struct WatchState {
    running: bool,
    stop: Option<Arc<AtomicBool>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileFilter {
    All,
    Documents,
    Pdf,
    Images,
    Code,
    Media,
}

struct LupaApp {
    root: String,
    query: String,
    limit: usize,
    path_prefix: String,
    regex: String,
    highlight: bool,

    busy: bool,
    watch: WatchState,
    status: String,
    logs: Vec<String>,

    last_build: Option<IndexStats>,
    last_search: Option<SearchResult>,
    last_doctor: Option<DoctorReport>,
    thumbnails: HashMap<String, Thumbnail>,
    selected_path: Option<String>,
    selected_filter: FileFilter,
    preview_cache: HashMap<String, PreviewData>,

    tx: Sender<UiEvent>,
    rx: Receiver<UiEvent>,
}

enum Thumbnail {
    Image(TextureHandle),
    Badge { label: String, color: Color32 },
}

struct PreviewData {
    path: String,
    name: String,
}

enum UiEvent {
    BuildDone(Result<IndexStats, String>),
    SearchDone(Result<SearchResult, String>),
    DoctorDone(Result<DoctorReport, String>),
    WatchTick(Result<IndexStats, String>),
    WatchStopped,
}

impl LupaApp {
    fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let root = std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .display()
            .to_string();

        Self {
            root,
            query: String::new(),
            limit: 20,
            path_prefix: String::new(),
            regex: String::new(),
            highlight: false,
            busy: false,
            watch: WatchState::default(),
            status: "Listo para buscar".to_string(),
            logs: vec!["App iniciada".to_string()],
            last_build: None,
            last_search: None,
            last_doctor: None,
            thumbnails: HashMap::new(),
            selected_path: None,
            selected_filter: FileFilter::All,
            preview_cache: HashMap::new(),
            tx,
            rx,
        }
    }

    fn spawn_build(&mut self) {
        let root = self.root.clone();
        let tx = self.tx.clone();
        self.busy = true;
        self.status = "Indexando archivos...".to_string();
        self.logs.push(format!("Index build -> {root}"));

        std::thread::spawn(move || {
            let res = run_build(&root);
            let _ = tx.send(UiEvent::BuildDone(res));
        });
    }

    fn spawn_search(&mut self) {
        if self.query.trim().is_empty() {
            return;
        }

        let root = self.root.clone();
        let query = self.query.clone();
        let limit = self.limit;
        let prefix = if self.path_prefix.trim().is_empty() {
            None
        } else {
            Some(self.path_prefix.clone())
        };
        let regex = if self.regex.trim().is_empty() {
            None
        } else {
            Some(self.regex.clone())
        };
        let highlight = self.highlight;
        let tx = self.tx.clone();

        self.busy = true;
        self.status = format!("Buscando \"{}\"...", query);
        self.logs.push(format!("Search -> {query}"));

        std::thread::spawn(move || {
            let res = run_search(
                &root,
                &query,
                SearchOptions {
                    limit,
                    path_prefix: prefix,
                    regex,
                    highlight,
                },
            );
            let _ = tx.send(UiEvent::SearchDone(res));
        });
    }

    fn spawn_doctor(&mut self) {
        let root = self.root.clone();
        let tx = self.tx.clone();

        self.busy = true;
        self.status = "Verificando estado del sistema...".to_string();

        std::thread::spawn(move || {
            let res = run_doctor(&root);
            let _ = tx.send(UiEvent::DoctorDone(res));
        });
    }

    fn start_watch(&mut self) {
        if self.watch.running {
            return;
        }

        let root = self.root.clone();
        let tx = self.tx.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = Arc::clone(&stop);

        self.watch.running = true;
        self.watch.stop = Some(stop);
        self.status = "Monitor activo".to_string();
        self.logs.push("Monitor de cambios iniciado".to_string());

        std::thread::spawn(move || {
            let engine = match run_engine(&root) {
                Ok(engine) => engine,
                Err(err) => {
                    let _ = tx.send(UiEvent::WatchTick(Err(err.to_string())));
                    let _ = tx.send(UiEvent::WatchStopped);
                    return;
                }
            };

            let watch_path = PathBuf::from(&root);
            let (event_tx, event_rx) = mpsc::channel();
            let mut watcher = match recommended_watcher(move |res| {
                let _ = event_tx.send(res);
            }) {
                Ok(w) => w,
                Err(err) => {
                    let _ = tx.send(UiEvent::WatchTick(Err(format!("watch init error: {err}"))));
                    let _ = tx.send(UiEvent::WatchStopped);
                    return;
                }
            };

            if let Err(err) = watcher.watch(&watch_path, RecursiveMode::Recursive) {
                let _ = tx.send(UiEvent::WatchTick(Err(format!("watch path error: {err}"))));
                let _ = tx.send(UiEvent::WatchStopped);
                return;
            }

            let mut dirty = HashSet::<PathBuf>::new();
            while !stop_thread.load(Ordering::SeqCst) {
                match event_rx.recv_timeout(Duration::from_millis(500)) {
                    Ok(Ok(event)) => {
                        for p in event.paths {
                            dirty.insert(p);
                        }
                    }
                    Ok(Err(err)) => {
                        let _ =
                            tx.send(UiEvent::WatchTick(Err(format!("watch event error: {err}"))));
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }

                while let Ok(Ok(event)) = event_rx.try_recv() {
                    for p in event.paths {
                        dirty.insert(p);
                    }
                }

                if dirty.is_empty() {
                    continue;
                }

                let batch = dirty.drain().collect::<Vec<_>>();
                let res = if batch.len() > 5000 {
                    engine.build_incremental().map_err(|e| e.to_string())
                } else {
                    engine.apply_dirty_paths(&batch).map_err(|e| e.to_string())
                };
                let _ = tx.send(UiEvent::WatchTick(res));
            }

            let _ = tx.send(UiEvent::WatchStopped);
        });
    }

    fn stop_watch(&mut self) {
        if let Some(stop) = &self.watch.stop {
            stop.store(true, Ordering::SeqCst);
            self.logs.push("Monitor detenndose...".to_string());
        }
    }

    fn drain_events(&mut self) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                UiEvent::BuildDone(result) => {
                    self.busy = false;
                    match result {
                        Ok(stats) => {
                            self.status = format!(
                                "ndice actualizado: {} archivos analizados en {} ms",
                                stats.scanned, stats.duration_ms
                            );
                            self.logs.push(self.status.clone());
                            self.last_build = Some(stats);
                        }
                        Err(err) => {
                            self.status = format!("Error al indexar: {err}");
                            self.logs.push(self.status.clone());
                        }
                    }
                }
                UiEvent::SearchDone(result) => {
                    self.busy = false;
                    match result {
                        Ok(result) => {
                            self.status = format!(
                                "{} resultados en {} ms",
                                result.total_hits, result.took_ms
                            );
                            self.logs.push(self.status.clone());
                            self.preview_cache.clear();
                            self.selected_filter = FileFilter::All;
                            self.selected_path = result.hits.first().map(|h| h.path.clone());
                            self.last_search = Some(result);
                        }
                        Err(err) => {
                            self.status = format!("Error en bsqueda: {err}");
                            self.logs.push(self.status.clone());
                        }
                    }
                }
                UiEvent::DoctorDone(result) => {
                    self.busy = false;
                    match result {
                        Ok(report) => {
                            self.status = "Sistema listo".to_string();
                            self.logs.push("Doctor OK".to_string());
                            self.last_doctor = Some(report);
                        }
                        Err(err) => {
                            self.status = format!("Doctor fall: {err}");
                            self.logs.push(self.status.clone());
                        }
                    }
                }
                UiEvent::WatchTick(result) => match result {
                    Ok(stats) => {
                        self.last_build = Some(stats.clone());
                        self.status = format!(
                            "Monitor: nuevos {} | actualizados {} | eliminados {}",
                            stats.indexed_new, stats.indexed_updated, stats.removed
                        );
                    }
                    Err(err) => {
                        self.status = format!("Error en monitor: {err}");
                    }
                },
                UiEvent::WatchStopped => {
                    self.watch.running = false;
                    self.watch.stop = None;
                    self.status = "Monitor detenido".to_string();
                    self.logs.push("Monitor detenido".to_string());
                }
            }

            if self.logs.len() > 220 {
                let keep_from = self.logs.len().saturating_sub(220);
                self.logs = self.logs.split_off(keep_from);
            }
        }
    }
    fn top_search_bar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            ui.label(RichText::new("LUPA").strong().size(34.0));

            let search_bar_width = (ui.available_width() - 120.0).max(220.0);
            let response = ui.add_sized(
                [search_bar_width, 42.0],
                egui::TextEdit::singleline(&mut self.query)
                    .hint_text("Search documents, code, or images...")
                    .margin(egui::vec2(12.0, 9.0)),
            );
            let pressed_enter = response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));

            if ui
                .add_sized(
                    [96.0, 42.0],
                    egui::Button::new(RichText::new("Search").strong()),
                )
                .on_hover_cursor(CursorIcon::PointingHand)
                .clicked()
                || pressed_enter
            {
                self.spawn_search();
            }

            let status_text = if self.busy {
                "Indexing"
            } else if self.watch.running {
                "Monitoring"
            } else {
                "Idle"
            };
            ui.label(
                RichText::new(status_text)
                    .small()
                    .color(Color32::from_rgb(160, 170, 185)),
            );
        });
        ui.add_space(6.0);
    }

    fn control_panel(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        egui::ScrollArea::vertical()
            .auto_shrink([false, true])
            .show(ui, |ui: &mut egui::Ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                let count_for = |f: FileFilter, s: &Option<SearchResult>| -> usize {
                    s.as_ref()
                        .map(|res| {
                            res.hits
                                .iter()
                                .filter(|h| matches_filter(&h.path, f))
                                .count()
                        })
                        .unwrap_or(0)
                };

                ui.label(
                    RichText::new("SYSTEM TOOLS")
                        .small()
                        .strong()
                        .color(Color32::from_rgb(140, 150, 170)),
                );
                ui.add_space(6.0);

                let full_w = ui.available_width();
                if ui
                    .add_enabled(
                        !self.busy && !self.watch.running,
                        egui::Button::new("\u{26A1}  Build Index")
                            .min_size(egui::vec2(full_w, 34.0)),
                    )
                    .clicked()
                {
                    self.spawn_build();
                }
                if !self.watch.running {
                    if ui
                        .add_enabled(
                            !self.busy,
                            egui::Button::new("\u{1F441}  Start Monitor")
                                .min_size(egui::vec2(full_w, 34.0)),
                        )
                        .clicked()
                    {
                        self.start_watch();
                    }
                } else if ui
                    .add(
                        egui::Button::new("\u{1F6D1}  Stop Monitor")
                            .min_size(egui::vec2(full_w, 34.0)),
                    )
                    .clicked()
                {
                    self.stop_watch();
                }
                if ui
                    .add_enabled(
                        !self.busy,
                        egui::Button::new("\u{1FA7A}  System Doctor")
                            .min_size(egui::vec2(full_w, 34.0)),
                    )
                    .clicked()
                {
                    self.spawn_doctor();
                }

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(12.0);

                ui.label(
                    RichText::new("COLLECTIONS")
                        .small()
                        .strong()
                        .color(Color32::from_rgb(140, 150, 170)),
                );
                let categories = [
                    (FileFilter::All, "\u{1F55A}  Recents"),
                    (FileFilter::Documents, "\u{1F4C4}  Documents"),
                    (FileFilter::Images, "\u{1F5BC}  Images"),
                    (FileFilter::Media, "\u{1F3AC}  Media"),
                    (FileFilter::Code, "\u{1F4BB}  Source Code"),
                    (FileFilter::Pdf, "\u{1F4D5}  PDF Files"),
                ];
                for (filter, label) in categories {
                    let is_selected = self.selected_filter == filter;
                    let count = count_for(filter, &self.last_search);
                    let mut clicked = false;
                    egui::Frame::none()
                        .fill(if is_selected {
                            Color32::from_rgb(54, 51, 86)
                        } else {
                            Color32::TRANSPARENT
                        })
                        .rounding(egui::Rounding::same(8.0))
                        .inner_margin(egui::Margin::symmetric(8.0, 4.0))
                        .show(ui, |ui| {
                            let r = ui
                                .horizontal(|ui| {
                                    ui.label(RichText::new(label));
                                    ui.with_layout(
                                        egui::Layout::right_to_left(Align::Center),
                                        |ui| {
                                            ui.label(
                                                RichText::new(format!("{count}"))
                                                    .small()
                                                    .color(Color32::from_rgb(146, 155, 188)),
                                            );
                                        },
                                    );
                                })
                                .response
                                .interact(Sense::click());
                            if r.clicked() {
                                clicked = true;
                            }
                            if r.hovered() {
                                ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                            }
                        });
                    if clicked {
                        self.selected_filter = filter;
                    }
                }

                ui.add_space(14.0);
                ui.separator();
                ui.add_space(12.0);

                ui.label(
                    RichText::new("INDEX PATH")
                        .small()
                        .strong()
                        .color(Color32::from_rgb(140, 150, 170)),
                );
                ui.horizontal(|ui: &mut egui::Ui| {
                    let input_w = (ui.available_width() - 34.0).max(120.0);
                    ui.add_sized(
                        [input_w, 30.0],
                        egui::TextEdit::singleline(&mut self.root).hint_text("Path to index..."),
                    );
                    if ui.button("...").on_hover_text("Choose folder").clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_folder() {
                            self.root = path.display().to_string();
                        }
                    }
                });

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(12.0);
                ui.label(
                    RichText::new("ADVANCED SEARCH")
                        .small()
                        .strong()
                        .color(Color32::from_rgb(140, 150, 170)),
                );
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut self.regex)
                            .hint_text("Regex (e.g. pdf|rs)"),
                    )
                    .changed()
                    && !self.busy
                    && !self.query.is_empty()
                {
                    self.spawn_search();
                }
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut self.path_prefix)
                            .hint_text("Path prefix (e.g. projects/)"),
                    )
                    .changed()
                    && !self.busy
                    && !self.query.is_empty()
                {
                    self.spawn_search();
                }
                ui.label(format!("Max Results: {}", self.limit));
                if ui
                    .add(
                        egui::Slider::new(&mut self.limit, 5..=500)
                            .show_value(false)
                            .step_by(1.0),
                    )
                    .changed()
                    && !self.busy
                    && !self.query.is_empty()
                {
                    self.spawn_search();
                }
                if ui
                    .checkbox(&mut self.highlight, "Show text snippets")
                    .changed()
                    && !self.busy
                    && !self.query.is_empty()
                {
                    self.spawn_search();
                }
            });
    }

    fn results_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Search Results")
                    .strong()
                    .size(22.0)
                    .color(Color32::WHITE),
            );
            if let Some(search) = &self.last_search {
                ui.add_space(8.0);
                ui.label(
                    RichText::new(format!("\u{2022} {} hits", search.total_hits))
                        .color(Color32::from_rgb(120, 125, 150)),
                );
                ui.label(
                    RichText::new(format!("\u{2022} {}ms", search.took_ms))
                        .small()
                        .color(Color32::from_rgb(99, 102, 241)),
                );
            }
        });

        ui.add_space(16.0);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if let Some(result) = &self.last_search {
                    let hits = result
                        .hits
                        .iter()
                        .filter(|h| matches_filter(&h.path, self.selected_filter))
                        .cloned()
                        .collect::<Vec<_>>();

                    if hits.is_empty() {
                        ui.centered_and_justified(|ui| {
                            ui.label(
                                RichText::new("No files found in this collection.")
                                    .color(Color32::from_rgb(100, 116, 139)),
                            );
                        });
                        return;
                    }

                    let selected_missing = match self.selected_path.as_ref() {
                        Some(p) => !hits.iter().any(|h| &h.path == p),
                        None => true,
                    };
                    if selected_missing {
                        self.selected_path = hits.first().map(|h| h.path.clone());
                    }

                    for (idx, hit) in hits.iter().enumerate() {
                        let is_selected = self.selected_path.as_deref() == Some(hit.path.as_str());
                        if self.result_row(ui, ctx, idx + 1, hit, is_selected) {
                            self.selected_path = Some(hit.path.clone());
                        }
                        ui.add_space(10.0);
                    }
                } else {
                    ui.centered_and_justified(|ui| {
                        ui.vertical_centered(|ui| {
                            ui.add_space(100.0);
                            ui.label(RichText::new("\u{1F50D}").size(48.0));
                            ui.add_space(12.0);
                            ui.label(
                                RichText::new("Ready to explore your local files")
                                    .strong()
                                    .size(20.0)
                                    .color(Color32::from_rgb(148, 163, 184)),
                            );
                            ui.label(
                                RichText::new("Type something in the search bar above")
                                    .color(Color32::from_rgb(100, 116, 139)),
                            );
                        });
                    });
                }
            });
    }

    fn right_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.spacing_mut().item_spacing.y = 12.0;
        egui::Frame::none()
            .fill(Color32::from_rgb(20, 20, 30))
            .rounding(egui::Rounding::same(16.0))
            .inner_margin(egui::Margin::same(16.0))
            .show(ui, |ui| {
                ui.label(
                    RichText::new("VISTA PREVIA")
                        .small()
                        .strong()
                        .color(Color32::from_rgb(99, 102, 241)),
                );
                ui.add_space(8.0);

                if let Some(path) = self.selected_path.clone() {
                    if !self.preview_cache.contains_key(&path) {
                        let preview = self.load_preview_data_fast(ctx, &path);
                        self.preview_cache.insert(path.clone(), preview);
                    }
                    if let Some(preview) = self.preview_cache.get(&path) {
                        ui.label(
                            RichText::new(&preview.name)
                                .strong()
                                .size(18.0)
                                .color(Color32::WHITE),
                        );
                        ui.add(egui::Label::new(RichText::new(&preview.path).small()).wrap());
                        let (kind, size_label, time_label) = file_meta_labels(&preview.path);
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                RichText::new(kind)
                                    .small()
                                    .color(Color32::from_rgb(130, 140, 180)),
                            );
                            ui.label(RichText::new(size_label).small());
                            ui.label(RichText::new(time_label).small());
                        });

                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if ui.button("\u{1F680} Open").clicked() {
                                let _ = open_file_path(Path::new(&preview.path));
                            }
                            if ui.button("\u{1F4CE} Open with...").clicked() {
                                let _ = open_with_dialog(Path::new(&preview.path));
                            }
                            if ui.button("\u{1F4C1} Folder").clicked() {
                                if let Some(parent) = Path::new(&preview.path).parent() {
                                    let _ = open_folder_path(parent);
                                }
                            }
                            if ui.button("\u{1F4CB} Copy").clicked() {
                                ui.ctx().copy_text(preview.path.clone());
                            }
                        });

                        ui.add_space(10.0);
                        ui.label(
                            RichText::new("Coincidencia en documento")
                                .small()
                                .strong()
                                .color(Color32::from_rgb(99, 102, 241)),
                        );
                        if let Some(hit) = self.selected_hit() {
                            if let Some(snippet) = hit.snippet {
                                ui.add(egui::Label::new(RichText::new(snippet).small()).wrap());
                            } else {
                                ui.label(
                                    RichText::new("Sin fragmento disponible para este archivo.")
                                        .small()
                                        .color(Color32::from_rgb(120, 130, 155)),
                                );
                            }
                        }
                    }
                } else {
                    ui.label(
                        RichText::new("Selecciona un resultado para ver la vista previa.")
                            .small()
                            .color(Color32::from_rgb(120, 130, 155)),
                    );
                }
            });

        ui.add_space(8.0);
        egui::Frame::none()
            .fill(Color32::from_rgb(20, 20, 30))
            .rounding(egui::Rounding::same(16.0))
            .inner_margin(egui::Margin::same(16.0))
            .show(ui, |ui| {
                ui.label(
                    RichText::new("METRICAS")
                        .small()
                        .strong()
                        .color(Color32::from_rgb(99, 102, 241)),
                );
                ui.add_space(8.0);
                if let Some(search) = &self.last_search {
                    ui.label(format!("Resultados: {}", search.total_hits));
                    ui.label(format!("Tiempo busqueda: {}ms", search.took_ms));
                } else {
                    ui.label("Resultados: -");
                    ui.label("Tiempo busqueda: -");
                }
                ui.add_space(6.0);
                if let Some(stats) = &self.last_build {
                    ui.label(format!("Indexados: {}", stats.scanned));
                    ui.label(format!(
                        "Nuevos/Act/Elim: {}/{}/{}",
                        stats.indexed_new, stats.indexed_updated, stats.removed
                    ));
                    ui.label(format!("Tiempo indexacion: {}ms", stats.duration_ms));
                } else {
                    ui.label("Indexados: -");
                    ui.label("Tiempo indexacion: -");
                }
            });
    }

    fn selected_hit(&self) -> Option<SearchHit> {
        let selected = self.selected_path.as_ref()?;
        let search = self.last_search.as_ref()?;
        search.hits.iter().find(|h| &h.path == selected).cloned()
    }

    fn result_row(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        rank: usize,
        hit: &SearchHit,
        is_selected: bool,
    ) -> bool {
        let mut row_clicked = false;
        let (ext, size_label, time_label) = file_meta_labels(&hit.path);

        let bg_color = if is_selected {
            Color32::from_rgb(30, 30, 45)
        } else {
            Color32::from_rgb(20, 20, 28)
        };

        let border_color = if is_selected {
            Color32::from_rgb(99, 102, 241)
        } else {
            Color32::from_rgb(35, 35, 48)
        };

        let frame = egui::Frame::none()
            .fill(bg_color)
            .rounding(egui::Rounding::same(12.0))
            .inner_margin(egui::Margin::symmetric(16.0, 12.0))
            .stroke(Stroke::new(1.0, border_color));

        let inner_response = frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                // ICON / THUMBNAIL AREA
                ui.allocate_ui(egui::vec2(60.0, 60.0), |ui| {
                    self.paint_thumbnail(ui, ctx, &hit.path);
                });

                ui.add_space(8.0);

                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        let name = file_name_from_path(&hit.path);
                        let title = RichText::new(name)
                            .size(17.0)
                            .strong()
                            .color(if is_selected {
                                Color32::WHITE
                            } else {
                                Color32::from_rgb(209, 213, 219)
                            });

                        ui.label(title);

                        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                            ui.label(
                                RichText::new(format!("#{}", rank))
                                    .small()
                                    .color(Color32::from_rgb(99, 102, 241)),
                            );
                        });
                    });

                    ui.add_space(2.0);
                    ui.add(
                        egui::Label::new(
                            RichText::new(&hit.path)
                                .small()
                                .color(Color32::from_rgb(100, 116, 139)),
                        )
                        .truncate(),
                    );

                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        // Extension Badge
                        let ext_bg = Color32::from_rgb(31, 41, 55);
                        egui::Frame::none()
                            .fill(ext_bg)
                            .rounding(egui::Rounding::same(4.0))
                            .inner_margin(egui::Margin::symmetric(6.0, 2.0))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(ext.to_uppercase())
                                        .small()
                                        .strong()
                                        .color(Color32::from_rgb(156, 163, 175)),
                                );
                            });

                        ui.add_space(8.0);
                        ui.label(
                            RichText::new(size_label)
                                .small()
                                .color(Color32::from_rgb(100, 116, 139)),
                        );
                        ui.add_space(12.0);
                        ui.label(
                            RichText::new(time_label)
                                .small()
                                .color(Color32::from_rgb(100, 116, 139)),
                        );
                    });
                });
            });
        });

        let response = ui.interact(
            inner_response.response.rect,
            inner_response.response.id,
            Sense::click(),
        );

        if response.hovered() {
            ctx.set_cursor_icon(CursorIcon::PointingHand);
        }

        if response.clicked() {
            row_clicked = true;
        }

        if response.double_clicked() {
            let _ = open_file_path(Path::new(&hit.path));
        }

        row_clicked
    }

    fn paint_thumbnail(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, path: &str) {
        let entry = self.thumbnail_for_path(ctx, path);
        match entry {
            Thumbnail::Image(texture) => {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(56.0, 56.0), Sense::hover());
                ui.painter().rect_stroke(
                    rect,
                    10.0,
                    Stroke::new(1.0, Color32::from_rgb(70, 73, 98)),
                );
                ui.painter().image(
                    texture.id(),
                    rect.shrink(2.0),
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            Thumbnail::Badge { label, color } => {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(56.0, 56.0), Sense::hover());
                ui.painter().rect_filled(rect, 14.0, *color);
                ui.painter().rect_stroke(
                    rect,
                    14.0,
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 40)),
                );
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    label,
                    FontId::new(12.0, FontFamily::Proportional),
                    Color32::from_rgb(250, 248, 255),
                );
            }
        }
    }

    fn thumbnail_for_path(&mut self, ctx: &egui::Context, path: &str) -> &Thumbnail {
        if !self.thumbnails.contains_key(path) {
            let thumb = load_thumbnail(ctx, path);
            self.thumbnails.insert(path.to_string(), thumb);
        }
        self.thumbnails
            .get(path)
            .expect("thumbnail should exist after insertion")
    }

    fn load_preview_data_fast(&mut self, _ctx: &egui::Context, path: &str) -> PreviewData {
        let name = file_name_from_path(path);

        PreviewData {
            path: path.to_string(),
            name,
        }
    }
}

impl eframe::App for LupaApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_events();

        let win_width = ctx.screen_rect().width();
        let show_right_panel = win_width >= 1100.0;
        let left_width = 280.0;
        let right_width = 320.0;

        // Background Painting (simulating a slight gradient or depth)
        let bg_color = ctx.style().visuals.panel_fill;
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(bg_color))
            .show(ctx, |_| {});

        if !ctx.wants_keyboard_input() && ctx.input(|i| i.key_pressed(Key::Enter)) {
            if let Some(path) = self.selected_path.as_deref() {
                let _ = open_file_path(Path::new(path));
            }
        }

        // Header Panel
        egui::TopBottomPanel::top("top_search")
            .frame(
                egui::Frame::none()
                    .fill(Color32::from_rgb(12, 12, 18))
                    .stroke(Stroke::new(1.0, Color32::from_rgb(30, 30, 45)))
                    .inner_margin(egui::Margin::symmetric(24.0, 12.0)),
            )
            .show(ctx, |ui| {
                self.top_search_bar(ui);
            });

        // Sidebar Panel
        egui::SidePanel::left("controls")
            .resizable(true)
            .default_width(left_width)
            .min_width(220.0)
            .max_width(420.0)
            .frame(
                egui::Frame::none()
                    .fill(Color32::from_rgb(15, 15, 22))
                    .stroke(Stroke::new(1.0, Color32::from_rgb(25, 25, 38)))
                    .inner_margin(egui::Margin::symmetric(20.0, 16.0)),
            )
            .show(ctx, |ui| {
                self.control_panel(ui);
            });

        // Right Detail Panel
        if show_right_panel {
            egui::SidePanel::right("activity")
                .resizable(true)
                .default_width(right_width)
                .min_width(280.0)
                .max_width(500.0)
                .frame(
                    egui::Frame::none()
                        .fill(Color32::from_rgb(15, 15, 22))
                        .stroke(Stroke::new(1.0, Color32::from_rgb(25, 25, 38)))
                        .inner_margin(egui::Margin::same(20.0)),
                )
                .show(ctx, |ui| {
                    self.right_panel(ui, ctx);
                });
        } else {
            egui::TopBottomPanel::bottom("mobile_right_panel")
                .resizable(true)
                .default_height(260.0)
                .min_height(180.0)
                .frame(
                    egui::Frame::none()
                        .fill(Color32::from_rgb(15, 15, 22))
                        .stroke(Stroke::new(1.0, Color32::from_rgb(25, 25, 38)))
                        .inner_margin(egui::Margin::same(12.0)),
                )
                .show(ctx, |ui| {
                    self.right_panel(ui, ctx);
                });
        }

        // Main Content Area
        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(Color32::from_rgb(10, 10, 15))
                    .inner_margin(egui::Margin::same(24.0)),
            )
            .show(ctx, |ui| {
                self.results_panel(ui, ctx);

                // Floating status toast if needed could go here
                if !self.status.is_empty() && !self.busy {
                    ui.with_layout(egui::Layout::bottom_up(egui::Align::RIGHT), |ui| {
                        ui.add_space(10.0);
                        egui::Frame::popup(ui.style())
                            .fill(Color32::from_rgb(30, 30, 45))
                            .show(ui, |ui| {
                                ui.label(RichText::new(&self.status).small());
                            });
                    });
                }
            });

        ctx.request_repaint_after(Duration::from_millis(120));
    }
}

fn file_name_from_path(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

fn load_thumbnail(ctx: &egui::Context, path: &str) -> Thumbnail {
    let p = Path::new(path);
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();

    if is_image_extension(&ext) {
        if let Ok(img) = image::open(p) {
            let thumb = img.thumbnail(64, 64).to_rgba8();
            let size = [thumb.width() as usize, thumb.height() as usize];
            let color_img = egui::ColorImage::from_rgba_unmultiplied(size, thumb.as_raw());
            let texture = ctx.load_texture(
                format!("thumb:{path}"),
                color_img,
                egui::TextureOptions::LINEAR,
            );
            return Thumbnail::Image(texture);
        }
    }

    let label = if ext.is_empty() {
        "FILE".to_string()
    } else {
        ext.chars().take(4).collect::<String>().to_uppercase()
    };
    let color = ext_color(&ext);
    Thumbnail::Badge { label, color }
}

fn is_image_extension(ext: &str) -> bool {
    matches!(
        ext,
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "tif" | "tiff" | "ico"
    )
}

fn ext_color(ext: &str) -> Color32 {
    match ext {
        "pdf" => Color32::from_rgb(186, 92, 104),
        "doc" | "docx" => Color32::from_rgb(97, 139, 216),
        "xls" | "xlsx" | "csv" => Color32::from_rgb(86, 172, 126),
        "ppt" | "pptx" => Color32::from_rgb(215, 150, 88),
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" => Color32::from_rgb(139, 118, 214),
        "zip" | "rar" | "7z" => Color32::from_rgb(136, 122, 170),
        "mp3" | "wav" | "flac" => Color32::from_rgb(103, 132, 209),
        "mp4" | "mkv" | "mov" => Color32::from_rgb(171, 101, 195),
        _ => Color32::from_rgb(112, 121, 167),
    }
}

fn extension_of(path: &str) -> String {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default()
}

fn file_meta_labels(path: &str) -> (String, String, String) {
    let ext = extension_of(path);
    let kind = if ext.is_empty() {
        "FILE".to_string()
    } else {
        ext.to_uppercase()
    };

    match fs::metadata(path) {
        Ok(meta) => {
            let size = human_size(meta.len());
            let time = meta
                .modified()
                .ok()
                .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .map(short_time_label)
                .unwrap_or_else(|| "--:--".to_string());
            (kind, size, time)
        }
        Err(_) => (kind, "-".to_string(), "--:--".to_string()),
    }
}

fn human_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.1} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.0} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

fn short_time_label(unix_secs: u64) -> String {
    let mins = (unix_secs / 60) % 60;
    let hours = (unix_secs / 3600) % 24;
    format!("{hours:02}:{mins:02}")
}

fn matches_filter(path: &str, filter: FileFilter) -> bool {
    let ext = extension_of(path);
    match filter {
        FileFilter::All => true,
        FileFilter::Pdf => ext == "pdf",
        FileFilter::Images => is_image_extension(&ext),
        FileFilter::Documents => matches!(
            ext.as_str(),
            "doc" | "docx" | "odt" | "rtf" | "txt" | "md" | "pdf"
        ),
        FileFilter::Code => matches!(
            ext.as_str(),
            "rs" | "js"
                | "ts"
                | "tsx"
                | "jsx"
                | "py"
                | "java"
                | "go"
                | "cs"
                | "cpp"
                | "h"
                | "hpp"
                | "html"
                | "css"
                | "json"
                | "toml"
                | "yaml"
                | "yml"
                | "sql"
                | "xml"
                | "sh"
                | "ps1"
        ),
        FileFilter::Media => matches!(
            ext.as_str(),
            "mp3" | "wav" | "flac" | "aac" | "ogg" | "mp4" | "mkv" | "mov" | "avi" | "webm"
        ),
    }
}

fn open_file_path(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/C", "start", "", &path.to_string_lossy()])
            .spawn()
            .map_err(|e| format!("No se pudo abrir {}: {e}", path.display()))?;
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|e| format!("No se pudo abrir {}: {e}", path.display()))?;
        return Ok(());
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|e| format!("No se pudo abrir {}: {e}", path.display()))?;
        return Ok(());
    }

    #[allow(unreachable_code)]
    Err("Plataforma no soportada para abrir archivos".to_string())
}

fn open_folder_path(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        Command::new("explorer")
            .arg(path)
            .spawn()
            .map_err(|e| format!("No se pudo abrir carpeta {}: {e}", path.display()))?;
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|e| format!("No se pudo abrir carpeta {}: {e}", path.display()))?;
        return Ok(());
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|e| format!("No se pudo abrir carpeta {}: {e}", path.display()))?;
        return Ok(());
    }

    #[allow(unreachable_code)]
    Err("Plataforma no soportada para abrir carpetas".to_string())
}

fn open_with_dialog(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        Command::new("rundll32.exe")
            .args(["shell32.dll,OpenAs_RunDLL", &path.to_string_lossy()])
            .spawn()
            .map_err(|e| {
                format!(
                    "No se pudo abrir dialogo 'Abrir con' para {}: {e}",
                    path.display()
                )
            })?;
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    {
        open_file_path(path)
    }
}

fn run_engine(root: &str) -> anyhow::Result<LupaEngine> {
    let root = PathBuf::from(root);
    let cfg = LupaConfig::load(&root)?;
    LupaEngine::new(root, cfg)
}

fn run_build(root: &str) -> Result<IndexStats, String> {
    run_engine(root)
        .and_then(|engine| engine.build_incremental())
        .map_err(|e| e.to_string())
}

fn run_search(root: &str, query: &str, options: SearchOptions) -> Result<SearchResult, String> {
    run_engine(root)
        .and_then(|engine| engine.search(query, &options))
        .map_err(|e| e.to_string())
}

fn run_doctor(root: &str) -> Result<DoctorReport, String> {
    run_engine(root)
        .and_then(|engine| engine.doctor())
        .map_err(|e| e.to_string())
}
