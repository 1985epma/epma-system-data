//! Interface gráfica (egui/eframe).

use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;

use eframe::egui::{self, Align, Align2, Color32, Layout, RichText};

use crate::categories::{self, Category, Risk, Target};
use crate::cleaner::{self, CleanReport, Mode};
use crate::scanner::{self, Entry, ScanResult};
use crate::util::{disk_space, display_path, format_bytes};

/// Quantos itens de cada categoria aparecem na lista expandida.
const VISIBLE_ENTRIES: usize = 50;

const GREEN: Color32 = Color32::from_rgb(46, 160, 67);
const AMBER: Color32 = Color32::from_rgb(210, 153, 34);
const RED: Color32 = Color32::from_rgb(218, 54, 51);

enum Msg {
    Scanned(usize, ScanResult),
    Cleaned(usize, CleanReport),
    CleanFinished,
}

struct CatState {
    cat: Category,
    selected: bool,
    scanning: bool,
    scan: Option<ScanResult>,
    /// Itens que o usuário desmarcou individualmente.
    deselected: HashSet<PathBuf>,
}

impl CatState {
    fn available(&self) -> bool {
        self.scan.as_ref().is_none_or(|s| !s.unavailable)
    }

    fn chosen_entries(&self) -> Vec<Entry> {
        self.scan
            .iter()
            .flat_map(|s| &s.entries)
            .filter(|e| !self.deselected.contains(&e.path))
            .cloned()
            .collect()
    }

    fn chosen_bytes(&self) -> u64 {
        self.chosen_entries().iter().map(|e| e.size).sum()
    }

    /// Algo selecionado que de fato será limpo.
    fn has_work(&self) -> bool {
        if !self.selected || !self.available() {
            return false;
        }
        match &self.cat.target {
            Target::DirContents { .. } => !self.chosen_entries().is_empty(),
            Target::Command { .. } => self.scan.as_ref().and_then(|s| s.items).unwrap_or(0) > 0,
        }
    }
}

pub struct App {
    cats: Vec<CatState>,
    mode: Mode,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    cleaning: bool,
    confirm_open: bool,
    disk: Option<(u64, u64)>,
    log: Vec<String>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let (tx, rx) = channel();
        let cats = categories::all()
            .into_iter()
            .map(|cat| CatState {
                selected: cat.risk == Risk::Safe,
                cat,
                scanning: false,
                scan: None,
                deselected: HashSet::new(),
            })
            .collect();
        let mut app = Self {
            cats,
            mode: Mode::Trash,
            tx,
            rx,
            cleaning: false,
            confirm_open: false,
            disk: None,
            log: Vec::new(),
        };
        app.scan_all(&cc.egui_ctx);
        app
    }

    fn scan_all(&mut self, ctx: &egui::Context) {
        for i in 0..self.cats.len() {
            self.scan_one(i, ctx);
        }
        self.disk = disk_space("/");
    }

    fn scan_one(&mut self, i: usize, ctx: &egui::Context) {
        let state = &mut self.cats[i];
        state.scanning = true;
        let cat = state.cat.clone();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let _ = tx.send(Msg::Scanned(i, scanner::scan(&cat)));
            ctx.request_repaint();
        });
    }

    fn start_clean(&mut self, ctx: &egui::Context) {
        let jobs: Vec<(usize, Target, Vec<Entry>)> = self
            .cats
            .iter()
            .enumerate()
            .filter(|(_, c)| c.has_work())
            .map(|(i, c)| (i, c.cat.target.clone(), c.chosen_entries()))
            .collect();
        if jobs.is_empty() {
            return;
        }
        self.cleaning = true;
        let mode = self.mode;
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            for (i, target, entries) in jobs {
                let report = match target {
                    Target::DirContents { .. } => cleaner::clean_entries(&entries, mode),
                    Target::Command { .. } => cleaner::run_command(&target),
                };
                let _ = tx.send(Msg::Cleaned(i, report));
                ctx.request_repaint();
            }
            let _ = tx.send(Msg::CleanFinished);
            ctx.request_repaint();
        });
    }

    fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Scanned(i, result) => {
                    let state = &mut self.cats[i];
                    state.scanning = false;
                    state
                        .deselected
                        .retain(|p| result.entries.iter().any(|e| &e.path == p));
                    state.scan = Some(result);
                }
                Msg::Cleaned(i, report) => {
                    self.log_report(i, &report);
                    self.cats[i].deselected.clear();
                    self.scan_one(i, ctx);
                }
                Msg::CleanFinished => {
                    self.cleaning = false;
                    self.disk = disk_space("/");
                    if self.mode == Mode::Trash {
                        self.log.push(
                            "Itens movidos para a Lixeira. Esvazie a Lixeira para liberar o espaço."
                                .into(),
                        );
                    }
                }
            }
        }
    }

    fn log_report(&mut self, i: usize, report: &CleanReport) {
        let name = self.cats[i].cat.name;
        let mut line = match self.cats[i].cat.target {
            Target::Command { .. } if report.failures.is_empty() => format!("{name}: concluído"),
            Target::Command { .. } => format!("{name}: falhou"),
            Target::DirContents { .. } => format!(
                "{name}: {} itens, {} {}",
                report.removed,
                format_bytes(report.freed),
                if self.mode == Mode::Trash { "na Lixeira" } else { "liberados" },
            ),
        };
        if !report.failures.is_empty() {
            line.push_str(&format!(" — {} falha(s)", report.failures.len()));
        }
        self.log.push(line);
        for (path, err) in report.failures.iter().take(5) {
            let err: String = if err.contains("permission") {
                "sem permissão (protegido pelo macOS)".into()
            } else {
                err.chars().take(120).collect()
            };
            self.log.push(format!("    {}: {err}", display_path(path)));
        }
        if let Some(out) = report.output.as_deref().filter(|o| !o.is_empty()) {
            for l in out.lines().take(5) {
                self.log.push(format!("    {l}"));
            }
        }
    }

    fn busy(&self) -> bool {
        self.cleaning || self.cats.iter().any(|c| c.scanning)
    }

    fn header(&self, ui: &mut egui::Ui) {
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.heading(RichText::new("EPMA System Data").strong());
            ui.label(RichText::new("limpeza de Dados do Sistema do macOS").weak());
        });
        if let Some((total, free)) = self.disk {
            let used = total.saturating_sub(free);
            ui.add_space(6.0);
            ui.add(
                egui::ProgressBar::new(used as f32 / total.max(1) as f32).text(format!(
                    "Disco: {} usados de {} — {} livres",
                    format_bytes(used),
                    format_bytes(total),
                    format_bytes(free)
                )),
            );
        }
        ui.add_space(10.0);
    }

    /// Retorna (abrir confirmação, reescanear).
    fn footer(&mut self, ui: &mut egui::Ui) -> (bool, bool) {
        let mut confirm = false;
        let mut rescan = false;
        let total: u64 = self
            .cats
            .iter()
            .filter(|c| c.has_work())
            .map(CatState::chosen_bytes)
            .sum();
        let any_work = self.cats.iter().any(CatState::has_work);
        let busy = self.busy();

        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.add_enabled_ui(!busy, |ui| {
                ui.label("Modo:");
                ui.radio_value(&mut self.mode, Mode::Trash, "Mover para a Lixeira");
                ui.radio_value(&mut self.mode, Mode::Permanent, "Excluir permanentemente");
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let label = RichText::new(format!("Limpar {}", format_bytes(total))).strong();
                if ui.add_enabled(!busy && any_work, egui::Button::new(label)).clicked() {
                    confirm = true;
                }
                if ui.add_enabled(!busy, egui::Button::new("Escanear de novo")).clicked() {
                    rescan = true;
                }
                if busy {
                    ui.spinner();
                }
            });
        });
        ui.add_space(10.0);
        (confirm, rescan)
    }

    fn category_card(&mut self, ui: &mut egui::Ui, i: usize) {
        let locked = self.cleaning;
        let state = &mut self.cats[i];
        let available = state.available();

        egui::Frame::group(ui.style()).inner_margin(10.0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.add_enabled(
                    available && !locked,
                    egui::Checkbox::new(&mut state.selected, RichText::new(state.cat.name).strong()),
                );
                let (badge, color) = match state.cat.risk {
                    Risk::Safe => ("seguro", GREEN),
                    Risk::Review => ("revisar", AMBER),
                };
                ui.label(RichText::new(badge).small().color(color));

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if state.scanning {
                        ui.spinner();
                    } else if let Some(scan) = &state.scan {
                        if scan.items.is_some() || scan.unavailable {
                            ui.label(RichText::new("—").weak());
                        } else {
                            ui.label(RichText::new(format_bytes(scan.total)).strong());
                        }
                    }
                });
            });
            ui.label(RichText::new(state.cat.description).small().weak());

            let Some(scan) = &state.scan else { return };
            if let Some(note) = &scan.note {
                let color = if scan.items.is_some() { GREEN } else { AMBER };
                ui.label(RichText::new(note).small().color(color));
            }
            if scan.entries.is_empty() {
                return;
            }
            egui::CollapsingHeader::new(format!("{} itens", scan.entries.len()))
                .id_salt(state.cat.id)
                .show(ui, |ui| {
                    for entry in scan.entries.iter().take(VISIBLE_ENTRIES) {
                        ui.horizontal(|ui| {
                            let mut on = !state.deselected.contains(&entry.path);
                            if ui.add_enabled(!locked, egui::Checkbox::without_text(&mut on)).changed() {
                                if on {
                                    state.deselected.remove(&entry.path);
                                } else {
                                    state.deselected.insert(entry.path.clone());
                                }
                            }
                            ui.add_sized([72.0, 18.0], egui::Label::new(format_bytes(entry.size)));
                            if ui.small_button("Finder").on_hover_text("Mostrar no Finder").clicked() {
                                let _ = Command::new("open").arg("-R").arg(&entry.path).spawn();
                            }
                            ui.add(egui::Label::new(display_path(&entry.path)).truncate());
                        });
                    }
                    let hidden = scan.entries.len().saturating_sub(VISIBLE_ENTRIES);
                    if hidden > 0 {
                        ui.label(RichText::new(format!("… e mais {hidden} itens menores")).small().weak());
                    }
                });
        });
    }

    fn confirm_window(&mut self, ctx: &egui::Context) {
        let chosen: Vec<&CatState> = self.cats.iter().filter(|c| c.has_work()).collect();
        let bytes: u64 = chosen.iter().map(|c| c.chosen_bytes()).sum();
        let mut go = false;
        let mut cancel = false;

        egui::Window::new("Confirmar limpeza")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(format!(
                    "{} categoria(s) selecionada(s), {} em arquivos.",
                    chosen.len(),
                    format_bytes(bytes)
                ));
                for c in &chosen {
                    ui.label(RichText::new(format!("• {}", c.cat.name)).small());
                }
                ui.add_space(6.0);
                match self.mode {
                    Mode::Trash => ui.label(
                        "Os itens vão para a Lixeira. O espaço só é liberado quando você esvaziá-la.",
                    ),
                    Mode::Permanent => ui.label(
                        RichText::new("Os itens serão excluídos permanentemente. Não dá para desfazer.")
                            .color(RED),
                    ),
                };
                ui.label(RichText::new("Feche os apps abertos antes de limpar os caches.").small().weak());
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Cancelar").clicked() {
                        cancel = true;
                    }
                    if ui.button(RichText::new("Limpar agora").strong()).clicked() {
                        go = true;
                    }
                });
            });

        if go {
            self.confirm_open = false;
            self.start_clean(ctx);
        } else if cancel {
            self.confirm_open = false;
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll(ctx);

        egui::TopBottomPanel::top("header").show(ctx, |ui| self.header(ui));
        let (confirm, rescan) = egui::TopBottomPanel::bottom("footer")
            .show(ctx, |ui| self.footer(ui))
            .inner;

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                for i in 0..self.cats.len() {
                    self.category_card(ui, i);
                    ui.add_space(6.0);
                }
                if !self.log.is_empty() {
                    egui::CollapsingHeader::new("Histórico")
                        .default_open(true)
                        .show(ui, |ui| {
                            for line in &self.log {
                                ui.label(RichText::new(line).small().monospace());
                            }
                        });
                }
            });
        });

        if confirm {
            self.confirm_open = true;
        }
        if rescan {
            self.scan_all(ctx);
        }
        if self.confirm_open {
            self.confirm_window(ctx);
        }
    }
}
