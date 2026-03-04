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
            .with_inner_size([1240.0, 780.0])
            .with_min_inner_size([980.0, 640.0])
            .with_title("Lupa - Bsqueda Local"),
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
    style.spacing.item_spacing = egui::vec2(10.0, 10.0);
    style.spacing.button_padding = egui::vec2(12.0, 8.0);
    style.spacing.menu_margin = egui::Margin::same(10.0);
    style.spacing.window_margin = egui::Margin::same(12.0);

    style.text_styles = [
        (
            egui::TextStyle::Heading,
            FontId::new(32.0, FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Body,
            FontId::new(16.0, FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Button,
            FontId::new(15.0, FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Monospace,
            FontId::new(14.0, FontFamily::Monospace),
        ),
        (
            egui::TextStyle::Small,
            FontId::new(13.0, FontFamily::Proportional),
        ),
    ]
    .into();

    // Dark palette with purple accents to match the requested style.
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = Color32::from_rgb(20, 20, 27);
    visuals.window_fill = Color32::from_rgb(20, 20, 27);
    visuals.extreme_bg_color = Color32::from_rgb(16, 16, 22);

    visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(34, 34, 44);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(62, 60, 78));
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, Color32::from_rgb(223, 223, 232));
    visuals.widgets.noninteractive.rounding = egui::Rounding::same(8.0);
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(43, 43, 58);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(81, 76, 112));
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, Color32::from_rgb(225, 225, 225));
    visuals.widgets.inactive.rounding = egui::Rounding::same(8.0);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(58, 55, 82);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(128, 110, 186));
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, Color32::from_rgb(240, 240, 240));
    visuals.widgets.hovered.rounding = egui::Rounding::same(8.0);
    visuals.widgets.active.bg_fill = Color32::from_rgb(124, 74, 227);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, Color32::from_rgb(175, 138, 255));
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, Color32::from_rgb(247, 247, 247));
    visuals.widgets.active.rounding = egui::Rounding::same(8.0);

    visuals.selection.bg_fill = Color32::from_rgb(124, 74, 227);
    visuals.selection.stroke = Stroke::new(1.0, Color32::from_rgb(175, 138, 255));
    visuals.override_text_color = Some(Color32::from_rgb(222, 222, 222));
    visuals.window_rounding = egui::Rounding::same(8.0);
    visuals.menu_rounding = egui::Rounding::same(8.0);

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
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Q")
                    .size(30.0)
                    .color(Color32::from_rgb(155, 96, 255)),
            );
            ui.label(RichText::new("LUPA").size(38.0).strong());
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new("Offline | Privado | Ultra rapido").small());
                ui.add_space(10.0);
                ui.label(
                    RichText::new(if self.watch.running {
                        "Monitor ON"
                    } else {
                        "Monitor OFF"
                    })
                    .small()
                    .color(if self.watch.running {
                        Color32::from_rgb(160, 236, 200)
                    } else {
                        Color32::from_rgb(255, 196, 196)
                    }),
                );
            });
        });

        ui.add_space(10.0);
        ui.horizontal(|ui| {
            let search_w = (ui.available_width() - 140.0).max(220.0);
            let response = ui.add_sized(
                [search_w, 48.0],
                egui::TextEdit::singleline(&mut self.query)
                    .hint_text("Buscar archivos en segundos...")
                    .desired_width(search_w),
            );
            let pressed_enter = response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            if ui
                .add_enabled(
                    !self.busy && !self.query.trim().is_empty(),
                    egui::Button::new(RichText::new("BUSCAR").strong().size(18.0)),
                )
                .clicked()
                || pressed_enter
            {
                self.spawn_search();
            }
        });

        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    !self.busy && !self.watch.running,
                    egui::Button::new("Indexar"),
                )
                .clicked()
            {
                self.spawn_build();
            }
            if !self.watch.running {
                if ui
                    .add_enabled(!self.busy, egui::Button::new("Monitor"))
                    .clicked()
                {
                    self.start_watch();
                }
            } else if ui.button("Detener").clicked() {
                self.stop_watch();
            }
            if ui
                .add_enabled(!self.busy, egui::Button::new("Doctor"))
                .clicked()
            {
                self.spawn_doctor();
            }
        });

        ui.add_space(6.0);
        ui.label(
            RichText::new(&self.status)
                .small()
                .color(Color32::from_rgb(180, 180, 202)),
        );
    }

    fn control_panel(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.label(
                RichText::new("RECIENTES")
                    .small()
                    .strong()
                    .color(Color32::from_rgb(150, 150, 170)),
            );
            if ui
                .selectable_label(self.selected_filter == FileFilter::All, "Recientes")
                .clicked()
            {
                self.selected_filter = FileFilter::All;
            }

            ui.add_space(16.0);
            ui.label(
                RichText::new("CATEGORIAS")
                    .small()
                    .strong()
                    .color(Color32::from_rgb(150, 150, 170)),
            );
            if ui
                .selectable_label(self.selected_filter == FileFilter::Documents, "Documentos")
                .clicked()
            {
                self.selected_filter = FileFilter::Documents;
            }
            if ui
                .selectable_label(self.selected_filter == FileFilter::Images, "Imagenes")
                .clicked()
            {
                self.selected_filter = FileFilter::Images;
            }
            if ui
                .selectable_label(self.selected_filter == FileFilter::Media, "Videos / Audio")
                .clicked()
            {
                self.selected_filter = FileFilter::Media;
            }
            if ui
                .selectable_label(self.selected_filter == FileFilter::Code, "Codigo")
                .clicked()
            {
                self.selected_filter = FileFilter::Code;
            }
            if ui
                .selectable_label(self.selected_filter == FileFilter::Pdf, "PDF")
                .clicked()
            {
                self.selected_filter = FileFilter::Pdf;
            }

            ui.add_space(18.0);
            ui.label(
                RichText::new("CARPETA BASE")
                    .small()
                    .strong()
                    .color(Color32::from_rgb(150, 150, 170)),
            );
            ui.add_sized(
                [ui.available_width(), 30.0],
                egui::TextEdit::singleline(&mut self.root),
            );
            if ui.button("Elegir carpeta").clicked() {
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    self.root = path.display().to_string();
                }
            }
        });
    }

    fn results_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.heading("Resultados");
            if let Some(search) = &self.last_search {
                ui.label(
                    RichText::new(format!("{} encontrados", search.total_hits))
                        .color(Color32::from_rgb(177, 171, 214)),
                );
                ui.label(
                    RichText::new(format!("{} ms", search.took_ms))
                        .small()
                        .color(Color32::from_rgb(177, 171, 214)),
                );
            }
        });

        ui.add_space(8.0);

        egui::ScrollArea::vertical().show(ui, |ui| {
            if let Some(result) = &self.last_search {
                let hits = result
                    .hits
                    .iter()
                    .filter(|h| matches_filter(&h.path, self.selected_filter))
                    .cloned()
                    .collect::<Vec<_>>();
                if hits.is_empty() {
                    ui.add_space(20.0);
                    ui.label("No hay resultados para este filtro.");
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
                    ui.add_space(8.0);
                }
            } else {
                ui.add_space(20.0);
                ui.label("Escribe algo en la barra superior y presiona Buscar.");
            }
        });
    }

    fn right_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("FILTROS");
        ui.add_space(6.0);

        ui.label("Tipo");
        ui.add_sized(
            [ui.available_width(), 30.0],
            egui::TextEdit::singleline(&mut self.regex).hint_text("pdf|docx|png|rs"),
        );
        ui.add_space(6.0);
        ui.label("Path");
        ui.add_sized(
            [ui.available_width(), 30.0],
            egui::TextEdit::singleline(&mut self.path_prefix).hint_text("C:/Users/..."),
        );
        ui.add_space(6.0);
        ui.label("Cantidad");
        ui.add(egui::Slider::new(&mut self.limit, 5..=200).text("Resultados"));
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.highlight, "");
            ui.label("Mostrar fragmentos");
        });

        ui.add_space(12.0);
        ui.separator();
        ui.add_space(8.0);
        ui.label(
            RichText::new("ESTADISTICAS")
                .small()
                .strong()
                .color(Color32::from_rgb(150, 150, 170)),
        );
        if let Some(stats) = &self.last_build {
            ui.label(format!("{} archivos indexados", stats.scanned));
            ui.label(format!("{} ms indexacion", stats.duration_ms));
        } else {
            ui.label("Sin indexacion reciente");
        }
        if let Some(search) = &self.last_search {
            ui.label(format!("{} resultados", search.total_hits));
            ui.label(format!("{} ms busqueda", search.took_ms));
        } else {
            ui.label("Sin busquedas recientes");
        }

        ui.add_space(12.0);
        ui.separator();
        ui.add_space(8.0);
        ui.label(
            RichText::new("SELECCION")
                .small()
                .strong()
                .color(Color32::from_rgb(150, 150, 170)),
        );
        if let Some(path) = self.selected_path.clone() {
            if !self.preview_cache.contains_key(&path) {
                let preview = self.load_preview_data_fast(ctx, &path);
                self.preview_cache.insert(path.clone(), preview);
            }
            if let Some(preview) = self.preview_cache.get(&path) {
                ui.label(RichText::new(&preview.name).strong());
                ui.add(egui::Label::new(RichText::new(&preview.path).small()).wrap());
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Abrir").clicked() {
                        let _ = open_file_path(Path::new(&preview.path));
                    }
                    if ui.button("Carpeta").clicked() {
                        if let Some(parent) = Path::new(&preview.path).parent() {
                            let _ = open_folder_path(parent);
                        }
                    }
                    if ui.button("Copiar").clicked() {
                        ui.ctx().copy_text(preview.path.clone());
                    }
                });
            }
        } else {
            ui.label("Selecciona un resultado.");
        }
    }

    fn result_row(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        rank: usize,
        hit: &SearchHit,
        is_selected: bool,
    ) -> bool {
        let mut row_selected = false;
        let mut action_clicked = false;
        let (ext, size_label, time_label) = file_meta_labels(&hit.path);
        let frame = egui::Frame::group(ui.style())
            .fill(if is_selected {
                Color32::from_rgb(48, 41, 74)
            } else {
                Color32::from_rgb(33, 33, 43)
            })
            .rounding(egui::Rounding::same(14.0))
            .inner_margin(egui::Margin::same(10.0))
            .stroke(Stroke::new(
                1.0,
                if is_selected {
                    Color32::from_rgb(152, 112, 255)
                } else {
                    Color32::from_rgb(69, 63, 97)
                },
            ));

        let row_response = frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                self.paint_thumbnail(ui, ctx, &hit.path);
                ui.add_space(4.0);

                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::left_to_right(Align::Center), |ui| {
                            ui.label(
                                RichText::new(format!("{:02}", rank))
                                    .strong()
                                    .color(Color32::from_rgb(160, 145, 232)),
                            );

                            let name = file_name_from_path(&hit.path);
                            let title = RichText::new(name)
                                .size(20.0)
                                .strong()
                                .color(Color32::from_rgb(236, 230, 255));
                            let response = ui.add(egui::Label::new(title).sense(Sense::click()));
                            if response.clicked() {
                                row_selected = true;
                            }

                            if response.double_clicked() {
                                let _ = open_file_path(Path::new(&hit.path));
                            }
                        });
                    });

                    ui.add(
                        egui::Label::new(
                            RichText::new(&hit.path)
                                .small()
                                .color(Color32::from_rgb(179, 179, 199)),
                        )
                        .wrap(),
                    );

                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(ext)
                                .small()
                                .color(Color32::from_rgb(196, 186, 230)),
                        );
                        ui.add_space(10.0);
                        ui.label(RichText::new(size_label).small());
                        ui.add_space(10.0);
                        ui.label(RichText::new(time_label).small());
                        ui.add_space(10.0);

                        if ui.small_button("Abrir").clicked() {
                            action_clicked = true;
                            let _ = open_file_path(Path::new(&hit.path));
                        }
                        if ui.small_button("Con...").clicked() {
                            action_clicked = true;
                            let _ = open_with_dialog(Path::new(&hit.path));
                        }
                        if ui.small_button("Dir").clicked() {
                            action_clicked = true;
                            if let Some(parent) = Path::new(&hit.path).parent() {
                                let _ = open_folder_path(parent);
                            }
                        }
                        if ui.small_button("Copiar").clicked() {
                            action_clicked = true;
                            ui.ctx().copy_text(hit.path.clone());
                        }
                    });
                });
            });
        });

        let row_response = row_response.response;
        if row_response.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        let row_primary_click = row_response.hovered() && ui.input(|i| i.pointer.primary_clicked());
        if row_primary_click && !action_clicked {
            row_selected = true;
        }

        row_selected
    }

    fn paint_thumbnail(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, path: &str) {
        let entry = self.thumbnail_for_path(ctx, path);
        match entry {
            Thumbnail::Image(texture) => {
                ui.image((texture.id(), egui::vec2(56.0, 56.0)));
            }
            Thumbnail::Badge { label, color } => {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(56.0, 56.0), Sense::hover());
                ui.painter().rect_filled(rect, 14.0, *color);
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
        let compact = win_width < 1220.0;
        let show_right_panel = win_width >= 1040.0;
        let left_width = if compact { 250.0 } else { 300.0 };
        let right_width = if compact { 260.0 } else { 290.0 };

        if !ctx.wants_keyboard_input() && ctx.input(|i| i.key_pressed(Key::Enter)) {
            if let Some(path) = self.selected_path.as_deref() {
                let _ = open_file_path(Path::new(path));
            }
        }

        egui::TopBottomPanel::top("top_search")
            .frame(
                egui::Frame::none()
                    .fill(Color32::from_rgb(37, 37, 38))
                    .stroke(Stroke::new(1.0, Color32::from_rgb(62, 62, 64)))
                    .inner_margin(egui::Margin::same(16.0)),
            )
            .show(ctx, |ui| {
                self.top_search_bar(ui);
            });

        egui::SidePanel::left("controls")
            .resizable(true)
            .default_width(left_width)
            .min_width(220.0)
            .max_width(460.0)
            .frame(
                egui::Frame::none()
                    .fill(Color32::from_rgb(37, 37, 38))
                    .stroke(Stroke::new(1.0, Color32::from_rgb(62, 62, 64)))
                    .inner_margin(egui::Margin::same(14.0)),
            )
            .show(ctx, |ui| {
                self.control_panel(ui);
            });

        if show_right_panel {
            egui::SidePanel::right("activity")
                .resizable(true)
                .default_width(right_width)
                .min_width(240.0)
                .max_width(520.0)
                .frame(
                    egui::Frame::none()
                        .fill(Color32::from_rgb(37, 37, 38))
                        .stroke(Stroke::new(1.0, Color32::from_rgb(62, 62, 64)))
                        .inner_margin(egui::Margin::same(14.0)),
                )
                .show(ctx, |ui| {
                    self.right_panel(ui, ctx);
                });
        } else {
            egui::TopBottomPanel::bottom("preview_compact")
                .resizable(true)
                .default_height(220.0)
                .frame(
                    egui::Frame::none()
                        .fill(Color32::from_rgb(37, 37, 38))
                        .stroke(Stroke::new(1.0, Color32::from_rgb(62, 62, 64)))
                        .inner_margin(egui::Margin::same(12.0)),
                )
                .show(ctx, |ui| {
                    self.right_panel(ui, ctx);
                });
        }

        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(Color32::from_rgb(30, 30, 30))
                    .inner_margin(egui::Margin::same(14.0)),
            )
            .show(ctx, |ui| {
                self.results_panel(ui, ctx);
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
