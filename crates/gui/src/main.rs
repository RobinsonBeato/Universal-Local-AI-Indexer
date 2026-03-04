use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

use eframe::egui::{self, Align, Color32, FontFamily, FontId, Key, RichText, Sense, Stroke};
use lupa_core::{
    DoctorReport, IndexStats, LupaConfig, LupaEngine, SearchHit, SearchOptions, SearchResult,
};

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1240.0, 780.0])
            .with_min_inner_size([980.0, 640.0])
            .with_title("Lupa - Búsqueda Local"),
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

    tx: Sender<UiEvent>,
    rx: Receiver<UiEvent>,
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
            highlight: true,
            busy: false,
            watch: WatchState::default(),
            status: "Listo para buscar".to_string(),
            logs: vec!["App iniciada".to_string()],
            last_build: None,
            last_search: None,
            last_doctor: None,
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
            self.logs.push("Monitor detenéndose...".to_string());
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
                                "Índice actualizado: {} archivos analizados en {} ms",
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
                            self.last_search = Some(result);
                        }
                        Err(err) => {
                            self.status = format!("Error en búsqueda: {err}");
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
                            self.status = format!("Doctor falló: {err}");
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
            ui.label(RichText::new("Buscador local ultrarrápido").size(16.0));
        });

        ui.add_space(10.0);
        ui.horizontal(|ui| {
            let search_box = egui::TextEdit::singleline(&mut self.query)
                .hint_text("Buscar documentos, notas, código, logs...")
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
        ui.heading("Panel rápido");
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
                .add_enabled(!self.busy, egui::Button::new("Actualizar índice"))
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
        ui.label(RichText::new("Opciones de búsqueda").strong());
        ui.add(egui::Slider::new(&mut self.limit, 5..=200).text("Resultados"));
        ui.text_edit_singleline(&mut self.path_prefix)
            .on_hover_text("Filtrar por ruta, por ejemplo: C:/Users/tu_usuario/Documents");
        ui.text_edit_singleline(&mut self.regex)
            .on_hover_text("Filtro regex opcional sobre path + contenido");
        ui.checkbox(&mut self.highlight, "Mostrar fragmento de texto");

        ui.separator();
        if let Some(stats) = &self.last_build {
            ui.label(RichText::new("Última indexación").strong());
            ui.monospace(format!("Analizados: {}", stats.scanned));
            ui.monospace(format!("Nuevos: {}", stats.indexed_new));
            ui.monospace(format!("Actualizados: {}", stats.indexed_updated));
            ui.monospace(format!("Sin cambios: {}", stats.skipped_unchanged));
            ui.monospace(format!("Eliminados: {}", stats.removed));
            ui.monospace(format!("Tiempo: {} ms", stats.duration_ms));
        }
    }

    fn results_panel(&mut self, ui: &mut egui::Ui) {
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

        egui::ScrollArea::vertical().show(ui, |ui| {
            if let Some(result) = &self.last_search {
                if result.hits.is_empty() {
                    ui.add_space(20.0);
                    ui.label("No hay resultados. Probá con otra palabra o actualizá el índice.");
                    return;
                }

                for (idx, hit) in result.hits.iter().enumerate() {
                    result_row(ui, idx + 1, hit);
                    ui.add_space(6.0);
                }
            } else {
                ui.add_space(20.0);
                ui.label("Escribí algo en la barra superior y presioná Buscar.");
            }
        });
    }

    fn right_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Actividad");
        ui.separator();

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
    }
}

impl eframe::App for LupaApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_events();

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
                self.right_panel(ui);
            });

        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(Color32::from_rgb(246, 244, 251))
                    .inner_margin(egui::Margin::same(14.0)),
            )
            .show(ctx, |ui| {
                self.results_panel(ui);
            });

        ctx.request_repaint_after(Duration::from_millis(120));
    }
}

fn result_row(ui: &mut egui::Ui, rank: usize, hit: &SearchHit) {
    let frame = egui::Frame::group(ui.style())
        .fill(Color32::WHITE)
        .rounding(egui::Rounding::same(12.0))
        .inner_margin(egui::Margin::same(12.0))
        .stroke(Stroke::new(1.0, Color32::from_rgb(215, 209, 232)));

    frame.show(ui, |ui| {
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

                if response.double_clicked() {
                    let _ = open_path(Path::new(&hit.path));
                }
            });

            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Abrir").clicked() {
                    let _ = open_path(Path::new(&hit.path));
                }
                if ui.button("Carpeta").clicked() {
                    if let Some(parent) = Path::new(&hit.path).parent() {
                        let _ = open_path(parent);
                    }
                }
                ui.label(RichText::new(format!("Score {:.2}", hit.score)).small());
            });
        });

        ui.add_space(4.0);
        ui.label(
            RichText::new(&hit.path)
                .small()
                .color(Color32::from_rgb(88, 82, 110)),
        );

        if let Some(snippet) = &hit.snippet {
            ui.add_space(6.0);
            ui.label(
                RichText::new(snippet)
                    .small()
                    .color(Color32::from_rgb(66, 62, 84)),
            );
        }
    });
}

fn file_name_from_path(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

fn open_path(path: &Path) -> Result<(), String> {
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
