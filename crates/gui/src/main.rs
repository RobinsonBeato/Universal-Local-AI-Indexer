use std::collections::HashMap;
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

    style.text_styles = [
        (
            egui::TextStyle::Heading,
            FontId::new(30.0, FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Body,
            FontId::new(17.0, FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Button,
            FontId::new(16.0, FontFamily::Proportional),
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

    let mut visuals = egui::Visuals::light();
    visuals.panel_fill = Color32::from_rgb(243, 241, 248);
    visuals.window_fill = Color32::from_rgb(243, 241, 248);
    visuals.extreme_bg_color = Color32::from_rgb(255, 255, 255);

    visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(236, 232, 246);
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(227, 222, 240);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(210, 204, 227);
    visuals.widgets.active.bg_fill = Color32::from_rgb(188, 182, 208);

    visuals.selection.bg_fill = Color32::from_rgb(116, 108, 144);
    visuals.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    visuals.override_text_color = Some(Color32::from_rgb(34, 30, 50));

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
    Other,
}

impl FileFilter {
    fn label(self) -> &'static str {
        match self {
            FileFilter::All => "Todos",
            FileFilter::Documents => "Documentos",
            FileFilter::Pdf => "PDF",
            FileFilter::Images => "Imagenes",
            FileFilter::Code => "Codigo",
            FileFilter::Media => "Audio/Video",
            FileFilter::Other => "Otros",
        }
    }
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
    ext: String,
    size_bytes: u64,
    modified_unix: u64,
    image: Option<TextureHandle>, // only reused image thumbnail texture
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
            while !stop_thread.load(Ordering::SeqCst) {
                let res = run_build(&root);
                let _ = tx.send(UiEvent::WatchTick(res));
                for _ in 0..20 {
                    if stop_thread.load(Ordering::SeqCst) {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
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
                RichText::new("LUPA")
                    .size(34.0)
                    .strong()
                    .color(Color32::from_rgb(100, 90, 128)),
            );
            ui.add_space(8.0);
            ui.label(RichText::new("Buscador local ultrarrpido").size(16.0));
        });

        ui.add_space(10.0);
        ui.horizontal(|ui| {
            let search_box = egui::TextEdit::singleline(&mut self.query)
                .hint_text("Buscar documentos, notas, cdigo, logs...")
                .desired_width(f32::INFINITY);

            let response = ui.add_sized([ui.available_width() - 130.0, 42.0], search_box);
            let pressed_enter = response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));

            let search_button = ui.add_enabled(
                !self.busy && !self.query.trim().is_empty(),
                egui::Button::new(RichText::new("Buscar").size(18.0).strong()),
            );

            if search_button.clicked() || pressed_enter {
                self.spawn_search();
            }
        });

        ui.add_space(6.0);
        ui.label(
            RichText::new(&self.status)
                .small()
                .color(Color32::from_rgb(70, 65, 90)),
        );
    }

    fn control_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Panel rpido");
        ui.add_space(4.0);

        ui.label("Carpeta principal");
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut self.root);
            if ui.button("Elegir").clicked() {
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    self.root = path.display().to_string();
                }
            }
        });

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !self.busy && !self.watch.running,
                    egui::Button::new("Actualizar ndice"),
                )
                .clicked()
            {
                self.spawn_build();
            }
            if !self.watch.running {
                if ui
                    .add_enabled(!self.busy, egui::Button::new("Iniciar monitor"))
                    .clicked()
                {
                    self.start_watch();
                }
            } else if ui.button("Detener monitor").clicked() {
                self.stop_watch();
            }
        });

        if ui
            .add_enabled(!self.busy, egui::Button::new("Revisar sistema"))
            .clicked()
        {
            self.spawn_doctor();
        }

        ui.separator();
        ui.label(RichText::new("Opciones de bsqueda").strong());
        ui.add(egui::Slider::new(&mut self.limit, 5..=200).text("Resultados"));
        ui.text_edit_singleline(&mut self.path_prefix)
            .on_hover_text("Filtrar por ruta, por ejemplo: C:/Users/tu_usuario/Documents");
        ui.text_edit_singleline(&mut self.regex)
            .on_hover_text("Filtro regex opcional sobre path + contenido");
        ui.checkbox(&mut self.highlight, "Mostrar fragmento de texto");

        ui.separator();
        if let Some(stats) = &self.last_build {
            ui.label(RichText::new("ltima indexacin").strong());
            ui.monospace(format!("Analizados: {}", stats.scanned));
            ui.monospace(format!("Nuevos: {}", stats.indexed_new));
            ui.monospace(format!("Actualizados: {}", stats.indexed_updated));
            ui.monospace(format!("Sin cambios: {}", stats.skipped_unchanged));
            ui.monospace(format!("Eliminados: {}", stats.removed));
            ui.monospace(format!("Tiempo: {} ms", stats.duration_ms));
        }
    }

    fn results_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let all_filters = [
            FileFilter::All,
            FileFilter::Documents,
            FileFilter::Pdf,
            FileFilter::Images,
            FileFilter::Code,
            FileFilter::Media,
            FileFilter::Other,
        ];

        ui.horizontal(|ui| {
            ui.heading("Resultados");
            if let Some(search) = &self.last_search {
                ui.label(
                    RichText::new(format!("{} encontrados", search.total_hits))
                        .color(Color32::from_rgb(84, 76, 108)),
                );
                ui.label(RichText::new(format!("{} ms", search.took_ms)).small());
            }
        });

        ui.add_space(4.0);

        if let Some(result) = &self.last_search {
            ui.horizontal_wrapped(|ui| {
                for filter in all_filters {
                    let count = result
                        .hits
                        .iter()
                        .filter(|h| matches_filter(&h.path, filter))
                        .count();
                    let text = format!("{} ({})", filter.label(), count);
                    if ui
                        .selectable_label(self.selected_filter == filter, text)
                        .clicked()
                    {
                        self.selected_filter = filter;
                    }
                }
            });
            ui.add_space(4.0);
        }

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
                    ui.add_space(6.0);
                }
            } else {
                ui.add_space(20.0);
                ui.label("Escrib algo en la barra superior y presion Buscar.");
            }
        });
    }

    fn right_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("Vista previa");
        ui.separator();

        if let Some(path) = self.selected_path.clone() {
            let selected_hit = self.selected_hit();
            if !self.preview_cache.contains_key(&path) {
                let preview = self.load_preview_data_fast(ctx, &path);
                self.preview_cache.insert(path.clone(), preview);
            }

            if let Some(preview) = self.preview_cache.get(&path) {
                ui.label(RichText::new(&preview.name).strong());
                ui.label(RichText::new(&preview.path).small());
                ui.add_space(6.0);
                ui.label(format!("tipo: {}", preview.ext));
                ui.label(format!("tamano: {} bytes", preview.size_bytes));
                ui.label(format!("modificado: {}", preview.modified_unix));
                ui.add_space(8.0);

                if let Some(image) = &preview.image {
                    let size = image.size_vec2();
                    let max_w = ui.available_width().max(120.0);
                    let scale = (max_w / size.x).min(280.0 / size.y).min(1.0);
                    ui.image((image.id(), size * scale));
                    ui.add_space(8.0);
                }

                if let Some(hit) = selected_hit {
                    if let Some(text) = &hit.snippet {
                        ui.label(RichText::new("Fragmento").strong());
                        egui::ScrollArea::vertical()
                            .max_height(260.0)
                            .show(ui, |ui| {
                                ui.label(RichText::new(text).small());
                            });
                    } else {
                        ui.label("No hay fragmento disponible para este resultado.");
                    }
                } else {
                    ui.label(RichText::new("Fragmento").strong());
                    ui.label("Selecciona un resultado para ver contexto.");
                }

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Abrir").clicked() {
                        let _ = open_file_path(Path::new(&preview.path));
                    }
                    if ui.button("Abrir con...").clicked() {
                        let _ = open_with_dialog(Path::new(&preview.path));
                    }
                    if ui.button("Carpeta").clicked() {
                        if let Some(parent) = Path::new(&preview.path).parent() {
                            let _ = open_folder_path(parent);
                        }
                    }
                    if ui.button("Copiar ruta").clicked() {
                        ui.ctx().copy_text(preview.path.clone());
                    }
                });
            }
        } else {
            ui.label("Selecciona un resultado para ver preview.");
        }

        ui.separator();
        ui.collapsing("Actividad", |ui| {
            if let Some(doctor) = &self.last_doctor {
                ui.label(RichText::new("Estado del sistema").strong());
                for check in &doctor.checks {
                    ui.label(check);
                }
                ui.add_space(8.0);
            }

            egui::ScrollArea::vertical().show(ui, |ui| {
                for line in self.logs.iter().rev() {
                    ui.label(RichText::new(line).small());
                }
            });
        });
    }

    fn selected_hit(&self) -> Option<SearchHit> {
        let path = self.selected_path.as_ref()?;
        let result = self.last_search.as_ref()?;
        result.hits.iter().find(|h| &h.path == path).cloned()
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
        let frame = egui::Frame::group(ui.style())
            .fill(if is_selected {
                Color32::from_rgb(239, 233, 250)
            } else {
                Color32::WHITE
            })
            .rounding(egui::Rounding::same(12.0))
            .inner_margin(egui::Margin::same(12.0))
            .stroke(Stroke::new(
                1.0,
                if is_selected {
                    Color32::from_rgb(160, 140, 200)
                } else {
                    Color32::from_rgb(215, 209, 232)
                },
            ));

        let row_response = frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                self.paint_thumbnail(ui, ctx, &hit.path);

                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::left_to_right(Align::Center), |ui| {
                            ui.label(
                                RichText::new(format!("{:02}", rank))
                                    .strong()
                                    .color(Color32::from_rgb(110, 101, 140)),
                            );

                            let name = file_name_from_path(&hit.path);
                            let title = RichText::new(name)
                                .size(17.0)
                                .strong()
                                .color(Color32::from_rgb(36, 30, 52));
                            let response = ui.add(egui::Label::new(title).sense(Sense::click()));
                            if response.clicked() {
                                row_selected = true;
                            }

                            if response.double_clicked() {
                                let _ = open_file_path(Path::new(&hit.path));
                            }
                        });
                    });

                    ui.label(
                        RichText::new(&hit.path)
                            .small()
                            .color(Color32::from_rgb(88, 82, 110)),
                    );

                    if let Some(snippet) = &hit.snippet {
                        ui.add_space(2.0);
                        ui.label(
                            RichText::new(snippet)
                                .small()
                                .color(Color32::from_rgb(66, 62, 84)),
                        );
                    }

                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if ui.button("Abrir").clicked() {
                            action_clicked = true;
                            let _ = open_file_path(Path::new(&hit.path));
                        }
                        if ui.button("Abrir con...").clicked() {
                            action_clicked = true;
                            let _ = open_with_dialog(Path::new(&hit.path));
                        }
                        if ui.button("Carpeta").clicked() {
                            action_clicked = true;
                            if let Some(parent) = Path::new(&hit.path).parent() {
                                let _ = open_folder_path(parent);
                            }
                        }
                        if ui.button("Copiar ruta").clicked() {
                            action_clicked = true;
                            ui.ctx().copy_text(hit.path.clone());
                        }
                        ui.label(RichText::new(format!("Score {:.2}", hit.score)).small());
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
                ui.painter().rect_filled(rect, 10.0, *color);
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

    fn load_preview_data_fast(&mut self, ctx: &egui::Context, path: &str) -> PreviewData {
        let p = Path::new(path);
        let ext = p
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_else(|| "file".to_string());
        let name = file_name_from_path(path);

        let mut size_bytes = 0u64;
        let mut modified_unix = 0u64;
        if let Ok(meta) = fs::metadata(p) {
            size_bytes = meta.len();
            if let Ok(modified) = meta.modified() {
                modified_unix = modified
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
            }
        }

        let image = if is_image_extension(&ext) {
            match self.thumbnail_for_path(ctx, path) {
                Thumbnail::Image(tex) => Some(tex.clone()),
                Thumbnail::Badge { .. } => None,
            }
        } else {
            None
        };

        PreviewData {
            path: path.to_string(),
            name,
            ext,
            size_bytes,
            modified_unix,
            image,
        }
    }
}

impl eframe::App for LupaApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_events();

        if !ctx.wants_keyboard_input() && ctx.input(|i| i.key_pressed(Key::Enter)) {
            if let Some(path) = self.selected_path.as_deref() {
                let _ = open_file_path(Path::new(path));
            }
        }

        egui::TopBottomPanel::top("top_search")
            .frame(
                egui::Frame::none()
                    .fill(Color32::from_rgb(233, 228, 244))
                    .inner_margin(egui::Margin::same(16.0)),
            )
            .show(ctx, |ui| {
                self.top_search_bar(ui);
            });

        egui::SidePanel::left("controls")
            .resizable(true)
            .default_width(310.0)
            .frame(
                egui::Frame::none()
                    .fill(Color32::from_rgb(238, 234, 247))
                    .inner_margin(egui::Margin::same(14.0)),
            )
            .show(ctx, |ui| {
                self.control_panel(ui);
            });

        egui::SidePanel::right("activity")
            .resizable(true)
            .default_width(280.0)
            .frame(
                egui::Frame::none()
                    .fill(Color32::from_rgb(238, 234, 247))
                    .inner_margin(egui::Margin::same(14.0)),
            )
            .show(ctx, |ui| {
                self.right_panel(ui, ctx);
            });

        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(Color32::from_rgb(246, 244, 251))
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
        "pdf" => Color32::from_rgb(173, 88, 99),
        "doc" | "docx" => Color32::from_rgb(96, 127, 176),
        "xls" | "xlsx" | "csv" => Color32::from_rgb(89, 150, 117),
        "ppt" | "pptx" => Color32::from_rgb(190, 128, 92),
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" => Color32::from_rgb(125, 106, 168),
        "zip" | "rar" | "7z" => Color32::from_rgb(112, 98, 128),
        "mp3" | "wav" | "flac" => Color32::from_rgb(115, 120, 176),
        "mp4" | "mkv" | "mov" => Color32::from_rgb(120, 90, 145),
        _ => Color32::from_rgb(118, 110, 142),
    }
}

fn extension_of(path: &str) -> String {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default()
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
        FileFilter::Other => {
            !matches_filter(path, FileFilter::Documents)
                && !matches_filter(path, FileFilter::Images)
                && !matches_filter(path, FileFilter::Code)
                && !matches_filter(path, FileFilter::Media)
        }
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
