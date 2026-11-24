//! Interface gráfica (egui/eframe): sidebar, barra superior, cartão-resumo, categorias em
//! seções (Seguro / Revisar), estados de carregamento, vazio e erro, tema claro/escuro e
//! pt-BR/en-US. Veja `outputs/PROPOSTA.md` para o desenho.

use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use eframe::egui::{
    self, vec2, Align, Align2, Color32, CursorIcon, FontId, Key as EKey, Layout, Margin, Modifiers,
    RichText, Rounding, Sense, Stroke, Ui,
};

use crate::categories::{self, Category, Risk, Target};
use crate::cleaner::{self, CleanReport, Failure, Mode};
use crate::diag;
use crate::i18n::{self, Key, Lang};
use crate::lock;
use crate::plan;
use crate::prefs::{LangPref, Prefs, ThemePref};
use crate::scanner::{self, Entry, ScanResult};
use crate::theme::{self, Bold, Palette};
use crate::util::{disk_space, display_path};
use crate::widgets::{self, BannerKind};

/// Quantos itens de cada categoria aparecem na lista expandida (o resto: "Ver todos").
const VISIBLE_ENTRIES: usize = 12;
/// Largura máxima do conteúdo, para telas grandes não esticarem as linhas.
const MAX_CONTENT_WIDTH: f32 = 980.0;
/// Abaixo desta largura a sidebar vira só ícones e a barra de ações encurta.
const NARROW_WIDTH: f32 = 860.0;

enum Msg {
    Scanned(usize, ScanResult),
    Cleaned(usize, CleanReport),
    CleanFinished,
    /// Outra limpeza (outra janela ou o terminal) está rodando: nada foi feito.
    Busy(String),
    Disks(Box<diag::Report>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Cleanup,
    Disks,
    History,
    Settings,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DetailsTab {
    Summary,
    Failures,
}

enum DiskState {
    Idle,
    Loading,
    Ready(Box<diag::Report>),
}

/// O que a barra de ações pediu neste quadro.
enum Action {
    None,
    Confirm,
    Rescan,
}

/// Resultado da limpeza de uma categoria.
struct RunResult {
    cat: usize,
    removed: usize,
    freed: u64,
    failures: Vec<Failure>,
    command: bool,
}

/// Uma limpeza completa (todas as categorias selecionadas).
struct Run {
    mode: Mode,
    results: Vec<RunResult>,
}

struct DoneBanner {
    freed: u64,
    failures: usize,
    mode: Mode,
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
        plan::chosen_entries(self.scan.as_ref(), &self.deselected)
    }

    fn chosen_bytes(&self) -> u64 {
        self.chosen_entries().iter().map(|e| e.size).sum()
    }

    /// Algo selecionado que de fato será limpo.
    fn has_work(&self) -> bool {
        plan::has_work(
            &self.cat,
            self.scan.as_ref(),
            self.selected,
            &self.deselected,
        )
    }

    fn total(&self) -> u64 {
        self.scan.as_ref().map_or(0, |s| s.total)
    }
}

/// Em que grupo da lista a categoria aparece.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum RowKind {
    /// Ainda sem resultado do scan.
    Loading,
    /// Tem o que limpar.
    Content,
    /// O macOS negou a leitura (falta Acesso Total ao Disco).
    Denied,
    /// Nada para limpar, ou a ferramenta não existe nesta máquina.
    Empty,
}

fn row_kind(s: &CatState) -> RowKind {
    let Some(scan) = &s.scan else {
        return RowKind::Loading;
    };
    if scan.unavailable {
        return RowKind::Empty;
    }
    if !scan.entries.is_empty() || scan.items.unwrap_or(0) > 0 {
        RowKind::Content
    } else if scan.denied {
        RowKind::Denied
    } else {
        RowKind::Empty
    }
}

/// Estados que a janela pode abrir direto, para conferir a interface sem mexer na máquina.
/// Só vale em build de debug (`EPMA_PREVIEW=disks,expand:user_caches,...`); no release é inerte.
#[derive(Default)]
struct Preview {
    view: Option<View>,
    expand: Vec<String>,
    filter: Option<String>,
    permanent: bool,
    confirm: bool,
    details: bool,
    fake_denied: bool,
    fake_done: bool,
    fake_fail: bool,
    fake_busy: bool,
    fake_empty: bool,
    /// `EPMA_PREVIEW` presente (só em debug): a janela NUNCA executa limpeza.
    active: bool,
    pending: bool,
}

impl Preview {
    fn from_env() -> Self {
        let mut pv = Self::default();
        if !cfg!(debug_assertions) {
            return pv;
        }
        let Ok(spec) = std::env::var("EPMA_PREVIEW") else {
            return pv;
        };
        for part in spec.split(',').map(str::trim) {
            match part {
                "disks" => pv.view = Some(View::Disks),
                "history" => pv.view = Some(View::History),
                "settings" => pv.view = Some(View::Settings),
                "permanent" => pv.permanent = true,
                "confirm" => pv.confirm = true,
                "details" => pv.details = true,
                "denied" => pv.fake_denied = true,
                "done" => pv.fake_done = true,
                "fail" => pv.fake_fail = true,
                "busy" => pv.fake_busy = true,
                "empty" => pv.fake_empty = true,
                other => {
                    if let Some(id) = other.strip_prefix("expand:") {
                        pv.expand.push(id.to_string());
                    } else if let Some(text) = other.strip_prefix("filter:") {
                        pv.filter = Some(text.to_string());
                    }
                }
            }
        }
        pv.active = true;
        pv.pending = pv.confirm || pv.fake_done || pv.fake_fail || pv.fake_busy;
        pv
    }
}

pub struct App {
    preview: Preview,
    cats: Vec<CatState>,
    mode: Mode,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    cleaning: bool,
    confirm_open: bool,
    confirm_text: String,
    disk: Option<(u64, u64)>,
    scanned_at: Option<Instant>,

    // Resultado das limpezas desta sessão.
    current: Vec<RunResult>,
    runs: Vec<Run>,
    banner: Option<DoneBanner>,
    busy_msg: Option<String>,
    perm_dismissed: bool,
    details_open: bool,
    details_tab: DetailsTab,

    // Navegação e filtros.
    view: View,
    filter: String,
    expanded: HashSet<&'static str>,
    show_all: HashSet<&'static str>,

    // Preferências.
    prefs: Prefs,
    lang: Lang,

    disks: DiskState,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let prefs = Prefs::load();
        theme::install(&cc.egui_ctx, prefs.theme);

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
        let preview = Preview::from_env();
        let mut app = Self {
            preview,
            cats,
            mode: Mode::Trash,
            tx,
            rx,
            cleaning: false,
            confirm_open: false,
            confirm_text: String::new(),
            disk: None,
            scanned_at: None,
            current: Vec::new(),
            runs: Vec::new(),
            banner: None,
            busy_msg: None,
            perm_dismissed: false,
            details_open: false,
            details_tab: DetailsTab::Summary,
            view: View::Cleanup,
            filter: String::new(),
            expanded: HashSet::new(),
            show_all: HashSet::new(),
            lang: resolve_lang(prefs.lang),
            prefs,
            disks: DiskState::Idle,
        };
        app.apply_preview_now();
        app.scan_all(&cc.egui_ctx);
        app
    }

    /// Parte do preview que não depende do resultado do scan.
    fn apply_preview_now(&mut self) {
        if let Some(view) = self.preview.view {
            self.view = view;
        }
        if self.preview.permanent {
            self.mode = Mode::Permanent;
        }
        if self.preview.details {
            self.details_open = true;
        }
        if let Some(f) = self.preview.filter.clone() {
            self.filter = f;
        }
        for id in std::mem::take(&mut self.preview.expand) {
            if let Some(c) = self.cats.iter().find(|c| c.cat.id == id) {
                self.expanded.insert(c.cat.id);
            }
        }
    }

    /// Parte do preview que precisa do scan pronto (modal, resultados falsos).
    fn apply_preview_after_scan(&mut self) {
        if !self.preview.pending || self.scanning_any() {
            return;
        }
        self.preview.pending = false;
        if self.preview.fake_busy {
            self.busy_msg = Some(
                "Outra limpeza está em andamento (PID 4242). Aguarde ela terminar e tente de novo."
                    .into(),
            );
        }
        if self.preview.fake_done || self.preview.fake_fail {
            let with_items: Vec<usize> = (0..self.cats.len())
                .filter(|&i| row_kind(&self.cats[i]) == RowKind::Content)
                .take(2)
                .collect();
            let mut results = Vec::new();
            for (n, &i) in with_items.iter().enumerate() {
                let failures = if self.preview.fake_fail && n == 1 {
                    vec![Failure::new(
                        "/Users/teste/Library/Caches/com.apple.Safari",
                        cleaner::FailureKind::PermissionDenied,
                        "Operation not permitted",
                    )]
                } else {
                    Vec::new()
                };
                results.push(RunResult {
                    cat: i,
                    removed: 47 - n * 40,
                    freed: 767_800_000 / (n as u64 * 100 + 1),
                    failures,
                    command: false,
                });
            }
            self.banner = Some(DoneBanner {
                freed: results.iter().map(|r| r.freed).sum(),
                failures: results.iter().map(|r| r.failures.len()).sum(),
                mode: self.mode,
            });
            self.runs.push(Run {
                mode: self.mode,
                results,
            });
        }
        if self.preview.confirm {
            self.confirm_open = true;
        }
    }

    // ---------------------------------------------------------------- trabalho em segundo plano

    fn scan_all(&mut self, ctx: &egui::Context) {
        self.scanned_at = None;
        self.perm_dismissed = false;
        for i in 0..self.cats.len() {
            self.cats[i].scan = None;
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
        // Pré-visualização é só para olhar a tela: jamais apaga nada, nem em HOME de teste.
        if self.preview.active {
            eprintln!("preview: limpeza ignorada (nada foi executado)");
            return;
        }
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
        self.banner = None;
        self.busy_msg = None;
        self.current.clear();
        let mode = self.mode;
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            // Uma limpeza por vez no usuário, mesmo entre janela, terminal e agendamento.
            let _lock = match lock::acquire() {
                Ok(lock) => lock,
                Err(e) => {
                    let _ = tx.send(Msg::Busy(e.to_string()));
                    ctx.request_repaint();
                    return;
                }
            };
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

    fn load_disks(&mut self, ctx: &egui::Context) {
        self.disks = DiskState::Loading;
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let _ = tx.send(Msg::Disks(Box::new(diag::collect())));
            ctx.request_repaint();
        });
    }

    fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Scanned(i, mut result) => {
                    if self.preview.fake_empty {
                        result = ScanResult::default();
                    }
                    if self.preview.fake_denied && self.cats[i].cat.id == "mail_downloads" {
                        result.entries.clear();
                        result.total = 0;
                        result.denied = true;
                    }
                    let state = &mut self.cats[i];
                    state.scanning = false;
                    state
                        .deselected
                        .retain(|p| result.entries.iter().any(|e| &e.path == p));
                    state.scan = Some(result);
                    if !self.cats.iter().any(|c| c.scanning) {
                        self.scanned_at = Some(Instant::now());
                    }
                }
                Msg::Cleaned(i, report) => {
                    self.current.push(RunResult {
                        cat: i,
                        removed: report.removed,
                        freed: report.freed,
                        command: matches!(self.cats[i].cat.target, Target::Command { .. }),
                        failures: report.failures,
                    });
                    self.cats[i].deselected.clear();
                    self.scan_one(i, ctx);
                }
                Msg::Busy(text) => {
                    self.cleaning = false;
                    self.busy_msg = Some(text);
                }
                Msg::CleanFinished => {
                    self.cleaning = false;
                    self.disk = disk_space("/");
                    let results = std::mem::take(&mut self.current);
                    self.banner = Some(DoneBanner {
                        freed: results.iter().map(|r| r.freed).sum(),
                        failures: results.iter().map(|r| r.failures.len()).sum(),
                        mode: self.mode,
                    });
                    self.runs.push(Run {
                        mode: self.mode,
                        results,
                    });
                }
                Msg::Disks(report) => self.disks = DiskState::Ready(report),
            }
        }
    }

    fn scanning_any(&self) -> bool {
        self.cats.iter().any(|c| c.scanning)
    }

    fn busy(&self) -> bool {
        self.cleaning || self.scanning_any()
    }

    // ---------------------------------------------------------------- preferências

    fn set_theme_pref(&mut self, ctx: &egui::Context, pref: ThemePref) {
        self.prefs.theme = pref;
        theme::apply_pref(ctx, pref);
        self.prefs.save();
    }

    fn set_lang_pref(&mut self, pref: LangPref) {
        self.prefs.lang = pref;
        self.lang = resolve_lang(pref);
        self.prefs.save();
    }

    // ---------------------------------------------------------------- quadro

    fn shortcuts(&mut self, ctx: &egui::Context, filter_id: egui::Id) {
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, EKey::K)) {
            self.view = View::Cleanup;
            ctx.memory_mut(|m| m.request_focus(filter_id));
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, EKey::R)) && !self.busy() {
            self.scan_all(ctx);
        }
        if self.confirm_open && ctx.input(|i| i.key_pressed(EKey::Escape)) {
            self.close_confirm();
        }
    }

    fn close_confirm(&mut self) {
        self.confirm_open = false;
        self.confirm_text.clear();
    }

    fn sidebar(&mut self, ctx: &egui::Context, p: &Palette, narrow: bool) {
        let lang = self.lang;
        let width = if narrow { 64.0 } else { 224.0 };
        egui::SidePanel::left("nav")
            .exact_width(width)
            .resizable(false)
            .frame(
                egui::Frame::none()
                    .fill(p.card)
                    .inner_margin(Margin::symmetric(10.0, 14.0)),
            )
            .show(ctx, |ui| {
                // Marca.
                ui.horizontal(|ui| {
                    ui.add_space(2.0);
                    let (rect, _) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::hover());
                    ui.painter()
                        .rect_filled(rect, Rounding::same(9.0), p.accent);
                    ui.painter().text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        "E",
                        FontId::proportional(18.0),
                        Color32::WHITE,
                    );
                    if !narrow {
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 0.0;
                            ui.label(RichText::new("EPMA").bold().size(15.0).color(p.text));
                            ui.label(RichText::new("System Data").size(12.0).color(p.muted));
                        });
                    }
                });
                ui.add_space(14.0);

                let items = [
                    (View::Cleanup, "🗑", Key::NavCleanup),
                    (View::Disks, "💾", Key::NavDisks),
                    (View::History, "🕘", Key::NavHistory),
                ];
                for (view, icon, key) in items {
                    if nav_item(ui, p, narrow, icon, i18n::t(lang, key), self.view == view)
                        .clicked()
                    {
                        self.view = view;
                    }
                }

                ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                    if nav_item(
                        ui,
                        p,
                        narrow,
                        "⚙",
                        i18n::t(lang, Key::NavSettings),
                        self.view == View::Settings,
                    )
                    .clicked()
                    {
                        self.view = View::Settings;
                    }
                });
            });
    }

    fn topbar(&mut self, ctx: &egui::Context, p: &Palette, narrow: bool, filter_id: egui::Id) {
        let lang = self.lang;
        let view_name = match self.view {
            View::Cleanup => Key::NavCleanup,
            View::Disks => Key::NavDisks,
            View::History => Key::NavHistory,
            View::Settings => Key::NavSettings,
        };
        egui::TopBottomPanel::top("topbar")
            .exact_height(56.0)
            .frame(
                egui::Frame::none()
                    .fill(p.card)
                    .inner_margin(Margin::symmetric(24.0, 0.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(RichText::new(i18n::t(lang, Key::AppName)).color(p.muted));
                    widgets::chevron(ui, p.faint, false);
                    ui.label(RichText::new(i18n::t(lang, view_name)).bold().color(p.text));

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        // Tema: alterna entre claro e escuro (o "automático" fica em Ajustes).
                        let dark = p.dark;
                        let icon = if dark { "☀" } else { "🌙" };
                        if ui
                            .add(
                                egui::Button::new(RichText::new(icon).size(15.0))
                                    .min_size(vec2(34.0, 34.0)),
                            )
                            .on_hover_text(i18n::t(lang, Key::ToggleThemeTip))
                            .clicked()
                        {
                            let next = if dark {
                                ThemePref::Light
                            } else {
                                ThemePref::Dark
                            };
                            self.set_theme_pref(ctx, next);
                        }

                        // Idioma: alterna pt-BR / en-US.
                        let label = format!("🌐 {}", lang.code());
                        if ui
                            .add(egui::Button::new(label).min_size(vec2(34.0, 34.0)))
                            .on_hover_text(i18n::t(lang, Key::ToggleLangTip))
                            .clicked()
                        {
                            let next = match lang {
                                Lang::PtBr => LangPref::EnUs,
                                Lang::EnUs => LangPref::PtBr,
                            };
                            self.set_lang_pref(next);
                        }

                        // Estado do scan.
                        let (dot, text) = self.scan_status();
                        ui.label(RichText::new(text).size(12.5).color(p.muted));
                        widgets::dot(
                            ui,
                            match dot {
                                ScanDot::Busy => p.accent,
                                ScanDot::Done => p.ok_fg,
                            },
                            8.0,
                        );

                        if self.view == View::Cleanup && !narrow {
                            ui.add_space(6.0);
                            let edit = egui::TextEdit::singleline(&mut self.filter)
                                .id(filter_id)
                                .hint_text(
                                    RichText::new(format!(
                                        "🔍  {}",
                                        i18n::t(lang, Key::FilterHint)
                                    ))
                                    .color(p.faint),
                                )
                                .desired_width(230.0)
                                .margin(vec2(10.0, 8.0));
                            ui.add(edit);
                        }
                    });
                });
            });
    }

    fn scan_status(&self) -> (ScanDot, String) {
        let lang = self.lang;
        if self.scanning_any() {
            let total = self.cats.len();
            let done = self.cats.iter().filter(|c| !c.scanning).count();
            let text = i18n::fmt2(
                lang,
                Key::ScanningProgress,
                &done.to_string(),
                &total.to_string(),
            );
            return (ScanDot::Busy, text);
        }
        let secs = self.scanned_at.map_or(0, |t| t.elapsed().as_secs());
        (ScanDot::Done, i18n::scanned_ago(lang, secs))
    }

    /// Barra de ações (rodapé da tela de limpeza). Devolve o que o usuário pediu.
    fn action_bar(&mut self, ctx: &egui::Context, p: &Palette, narrow: bool) -> Action {
        let lang = self.lang;
        let scanning = self.scanning_any();
        let busy = self.busy();
        let bytes: u64 = self
            .cats
            .iter()
            .filter(|c| c.has_work())
            .map(CatState::chosen_bytes)
            .sum();
        let n_cats = self.cats.iter().filter(|c| c.has_work()).count();
        let any_work = n_cats > 0;
        let mut action = Action::None;

        egui::TopBottomPanel::bottom("actions")
            .exact_height(64.0)
            .frame(
                egui::Frame::none()
                    .fill(p.card)
                    .inner_margin(Margin::symmetric(24.0, 0.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    // Resumo da seleção.
                    let size = if scanning {
                        "—".to_string()
                    } else {
                        i18n::bytes(lang, bytes)
                    };
                    ui.label(RichText::new(size).bold().color(p.text));
                    if !narrow || !scanning {
                        ui.label(RichText::new(i18n::t(lang, Key::SelectedWord)).color(p.muted));
                    }
                    if !narrow {
                        let extra = if scanning {
                            format!("· {}", i18n::t(lang, Key::WaitingForScan))
                        } else {
                            format!("· {}", i18n::n_categories(lang, n_cats))
                        };
                        ui.label(RichText::new(extra).color(p.faint));
                    }

                    // Destino da limpeza.
                    let dest_name = match self.mode {
                        Mode::Trash => i18n::t(lang, Key::DestTrash),
                        Mode::Permanent => i18n::t(lang, Key::DestPermanent),
                    };
                    let prefix = if narrow {
                        String::new()
                    } else {
                        format!("{} ", i18n::t(lang, Key::Destination))
                    };
                    let icon = if self.mode == Mode::Trash {
                        "🗑"
                    } else {
                        "⚠"
                    };
                    let dest_label = RichText::new(format!("{icon}  {prefix}{dest_name}")).color(
                        if self.mode == Mode::Permanent {
                            p.err_fg
                        } else {
                            p.text
                        },
                    );
                    ui.add_enabled_ui(!busy, |ui| {
                        ui.scope(|ui| {
                            if self.mode == Mode::Permanent {
                                let w = &mut ui.visuals_mut().widgets;
                                w.inactive.weak_bg_fill = p.err_bg;
                                w.inactive.bg_stroke = Stroke::new(1.0_f32, p.err_border);
                            }
                            let arrow = p.muted;
                            egui::ComboBox::from_id_salt("destination")
                                .icon(move |ui, rect, _visuals, _open, _dir| {
                                    let c = rect.center();
                                    ui.painter().add(egui::Shape::line(
                                        vec![
                                            c + vec2(-4.0, -2.0),
                                            c + vec2(0.0, 2.5),
                                            c + vec2(4.0, -2.0),
                                        ],
                                        Stroke::new(1.6_f32, arrow),
                                    ));
                                })
                                .selected_text(dest_label)
                                .width(if narrow { 120.0 } else { 210.0 })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut self.mode,
                                        Mode::Trash,
                                        i18n::t(lang, Key::DestTrash),
                                    );
                                    ui.selectable_value(
                                        &mut self.mode,
                                        Mode::Permanent,
                                        i18n::t(lang, Key::DeletePermanently),
                                    );
                                });
                        });
                    });

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let danger = self.mode == Mode::Permanent;
                        let verb = if danger {
                            Key::DeleteWord
                        } else {
                            Key::CleanWord
                        };
                        let label = if scanning || !any_work {
                            i18n::t(lang, Key::CleanWord).to_string()
                        } else {
                            format!("{} {}", i18n::t(lang, verb), i18n::bytes(lang, bytes))
                        };
                        if widgets::primary_button(ui, p, &label, !busy && any_work, danger)
                            .clicked()
                        {
                            action = Action::Confirm;
                        }
                        let rescan = if narrow {
                            "⟳".to_string()
                        } else {
                            format!("⟳  {}", i18n::t(lang, Key::Rescan))
                        };
                        if widgets::ghost_button(ui, &rescan, !busy)
                            .on_hover_text(i18n::t(lang, Key::Rescan))
                            .clicked()
                        {
                            action = Action::Rescan;
                        }
                        if busy {
                            ui.spinner();
                        }
                    });
                });
            });
        action
    }

    // ---------------------------------------------------------------- tela: Limpeza

    fn ui_cleanup(&mut self, ui: &mut Ui, p: &Palette, narrow: bool) {
        let lang = self.lang;
        let scanning = self.scanning_any();
        let kinds: Vec<RowKind> = self.cats.iter().map(row_kind).collect();

        // Avisos no topo: nunca pop-up.
        self.ui_banners(ui, p, &kinds);

        // Estado vazio: o scan terminou e não há nada em nenhuma categoria.
        let all_empty =
            !scanning && !self.cats.is_empty() && kinds.iter().all(|k| *k == RowKind::Empty);
        if all_empty {
            self.ui_empty_state(ui, p);
            return;
        }

        self.ui_summary(ui, p, narrow, &kinds);
        ui.add_space(4.0);

        // Barra de título da lista + atalho para navegadores.
        let n_visible = kinds.iter().filter(|k| **k != RowKind::Empty).count();
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!(
                    "{} ({})",
                    i18n::t(lang, Key::Categories),
                    n_visible
                ))
                .bold()
                .color(p.text),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let browsers =
                    widgets::ghost_button(ui, i18n::t(lang, Key::SelectBrowsers), !self.busy())
                        .on_hover_text(i18n::t(lang, Key::SelectBrowsersTip));
                if browsers.clicked() {
                    for state in self
                        .cats
                        .iter_mut()
                        .filter(|c| categories::is_browser(&c.cat))
                    {
                        state.selected = true;
                    }
                }
            });
        });

        // Ordena por tamanho, mas só depois do scan: durante ele a lista não pula de lugar.
        let mut order: Vec<usize> = (0..self.cats.len()).collect();
        if !scanning {
            order.sort_by_key(|&i| std::cmp::Reverse(self.cats[i].total()));
        }
        let max_bytes = self.cats.iter().map(CatState::total).max().unwrap_or(0);
        let filter = self.filter.trim().to_lowercase();
        let matches = |s: &CatState| {
            filter.is_empty()
                || i18n::cat_name(lang, &s.cat)
                    .to_lowercase()
                    .contains(&filter)
                || i18n::cat_desc(lang, &s.cat)
                    .to_lowercase()
                    .contains(&filter)
                || s.cat.id.contains(&filter)
        };

        let rows_of = |risk: Risk| -> Vec<usize> {
            order
                .iter()
                .copied()
                .filter(|&i| {
                    self.cats[i].cat.risk == risk
                        && kinds[i] != RowKind::Empty
                        && matches(&self.cats[i])
                })
                .collect()
        };
        let safe = rows_of(Risk::Safe);
        let review = rows_of(Risk::Review);
        let empty: Vec<usize> = order
            .iter()
            .copied()
            .filter(|&i| kinds[i] == RowKind::Empty && matches(&self.cats[i]))
            .collect();

        let rc = RowCtx {
            p,
            lang,
            locked: self.cleaning,
            max_bytes,
        };
        for (risk, rows) in [(Risk::Safe, safe), (Risk::Review, review)] {
            if rows.is_empty() {
                continue;
            }
            let subtotal: u64 = rows.iter().map(|&i| self.cats[i].total()).sum();
            let (title, hint) = match risk {
                Risk::Safe => (Key::SectionSafe, Key::HintChecked),
                Risk::Review => (Key::SectionReview, Key::HintUnchecked),
            };
            section(
                ui,
                p,
                |ui| match risk {
                    Risk::Safe => widgets::pill_safe(ui, p, i18n::t(lang, title)),
                    Risk::Review => widgets::pill_review(ui, p, i18n::t(lang, title)),
                },
                i18n::t(lang, hint),
                if scanning {
                    None
                } else {
                    Some(i18n::bytes(lang, subtotal))
                },
                |ui| {
                    for (n, &i) in rows.iter().enumerate() {
                        if n > 0 {
                            widgets::hline(ui, p);
                        }
                        category_row(
                            ui,
                            &rc,
                            &mut self.cats[i],
                            kinds[i],
                            &mut self.expanded,
                            &mut self.show_all,
                        );
                    }
                },
            );
            ui.add_space(4.0);
        }

        if !empty.is_empty() {
            self.ui_empty_group(ui, p, &empty, &rc);
        }
    }

    fn ui_banners(&mut self, ui: &mut Ui, p: &Palette, kinds: &[RowKind]) {
        let lang = self.lang;

        if let Some(msg) = self.busy_msg.clone() {
            let closed = widgets::banner(
                ui,
                p,
                BannerKind::Warn,
                i18n::t(lang, Key::BusyTitle),
                &msg,
                |_| {},
            );
            if closed {
                self.busy_msg = None;
            }
            ui.add_space(4.0);
        }

        let denied: Vec<&str> = self
            .cats
            .iter()
            .zip(kinds)
            .filter(|(_, k)| **k == RowKind::Denied)
            .map(|(c, _)| i18n::cat_name(lang, &c.cat))
            .collect();
        if !denied.is_empty() && !self.perm_dismissed {
            let mut names = denied
                .iter()
                .take(3)
                .copied()
                .collect::<Vec<_>>()
                .join(", ");
            if denied.len() > 3 {
                names.push('…');
            }
            let closed = widgets::banner(
                ui,
                p,
                BannerKind::Error,
                &i18n::fmt1(lang, Key::PermissionTitle, &names),
                i18n::t(lang, Key::PermissionBody),
                |ui| {
                    if widgets::ghost_button(ui, i18n::t(lang, Key::OpenSystemSettings), true)
                        .clicked()
                    {
                        open(
                            "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles",
                        );
                    }
                },
            );
            if closed {
                self.perm_dismissed = true;
            }
            ui.add_space(4.0);
        }

        let mut dismiss = false;
        let mut open_details = false;
        if let Some(done) = &self.banner {
            let size = i18n::bytes(lang, done.freed);
            let (kind, title) = if done.failures > 0 {
                (
                    BannerKind::Warn,
                    i18n::fmt1(lang, Key::DoneWithFailures, &done.failures.to_string()),
                )
            } else if done.mode == Mode::Trash {
                (BannerKind::Ok, i18n::fmt1(lang, Key::DoneTrash, &size))
            } else {
                (BannerKind::Ok, i18n::fmt1(lang, Key::DonePermanent, &size))
            };
            let body = if done.mode == Mode::Trash {
                i18n::t(lang, Key::DoneTrashHint)
            } else {
                ""
            };
            let trash = done.mode == Mode::Trash;
            dismiss = widgets::banner(ui, p, kind, &title, body, |ui| {
                if trash && widgets::ghost_button(ui, i18n::t(lang, Key::OpenTrash), true).clicked()
                {
                    if let Some(home) = dirs::home_dir() {
                        open(&home.join(".Trash").to_string_lossy());
                    }
                }
                if widgets::ghost_button(ui, i18n::t(lang, Key::ViewDetails), true).clicked() {
                    open_details = true;
                }
            });
        }
        if open_details {
            self.details_open = true;
        }
        if dismiss {
            self.banner = None;
        }
        if self.banner.is_some() {
            ui.add_space(4.0);
        }
    }

    /// Cartão "Pode ser liberado agora" + cartão do disco.
    fn ui_summary(&mut self, ui: &mut Ui, p: &Palette, narrow: bool, kinds: &[RowKind]) {
        let lang = self.lang;
        let scanning = self.scanning_any();

        let selected: Vec<&CatState> = self.cats.iter().filter(|c| c.has_work()).collect();
        let bytes: u64 = selected.iter().map(|c| c.chosen_bytes()).sum();
        let (mut safe_total, mut review_total, mut found) = (0u64, 0u64, 0usize);
        for (c, k) in self.cats.iter().zip(kinds) {
            if *k != RowKind::Content {
                continue;
            }
            match c.cat.risk {
                Risk::Safe => safe_total += c.total(),
                Risk::Review => review_total += c.total(),
            }
            found += c
                .scan
                .as_ref()
                .map_or(0, |s| s.entries.len().max(s.items.unwrap_or(0)));
        }
        let sub = if selected.is_empty() {
            i18n::t(lang, Key::NothingSelected).to_string()
        } else {
            let tail = match self.mode {
                Mode::Trash => Key::GoesToTrash,
                Mode::Permanent => Key::PermanentDeletion,
            };
            format!(
                "{} · {}",
                i18n::n_selected(lang, selected.len()),
                i18n::t(lang, tail)
            )
        };

        let freeable = |ui: &mut Ui| {
            widgets::card(ui, p, |ui| {
                ui.set_min_height(172.0);
                ui.label(RichText::new(i18n::t(lang, Key::CanBeFreed)).color(p.muted));
                if scanning {
                    widgets::skeleton(ui, p, 190.0, 38.0);
                    widgets::skeleton(ui, p, 300.0, 14.0);
                } else {
                    ui.label(
                        RichText::new(i18n::bytes(lang, bytes))
                            .size(38.0)
                            .bold()
                            .color(p.text),
                    );
                    ui.label(RichText::new(sub.as_str()).color(p.muted));
                }
                ui.add_space(4.0);
                widgets::hline(ui, p);
                ui.add_space(2.0);
                ui.horizontal_top(|ui| {
                    // Três colunas que cabem no cartão, qualquer que seja a largura da janela.
                    let col_w = ((ui.available_width() - 2.0 * ui.spacing().item_spacing.x) / 3.0)
                        .max(70.0);
                    let stats = [
                        (Key::StatSafe, i18n::bytes(lang, safe_total), p.ok_fg),
                        (Key::StatReview, i18n::bytes(lang, review_total), p.warn_fg),
                        (Key::StatItems, found.to_string(), p.faint),
                    ];
                    for (key, value, dot) in stats {
                        ui.allocate_ui_with_layout(
                            vec2(col_w, 40.0),
                            Layout::top_down(Align::Min),
                            |ui| {
                                ui.set_width(col_w);
                                ui.horizontal(|ui| {
                                    widgets::dot(ui, dot, 8.0);
                                    ui.label(
                                        RichText::new(i18n::t(lang, key)).size(12.5).color(p.muted),
                                    );
                                });
                                if scanning {
                                    widgets::skeleton(ui, p, 56.0, 14.0);
                                } else {
                                    ui.label(RichText::new(value).bold().color(p.text));
                                }
                            },
                        );
                    }
                });
            });
        };
        let disk_card = |ui: &mut Ui| self.disk_card(ui, p);

        if narrow || ui.available_width() < 640.0 {
            freeable(ui);
            ui.add_space(4.0);
            disk_card(ui);
        } else {
            let gap = ui.spacing().item_spacing.x;
            let col_w = ((ui.available_width() - gap) / 2.0).floor();
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(vec2(col_w, 0.0), Layout::top_down(Align::Min), |ui| {
                    ui.set_width(col_w);
                    freeable(ui);
                });
                ui.allocate_ui_with_layout(vec2(col_w, 0.0), Layout::top_down(Align::Min), |ui| {
                    ui.set_width(col_w);
                    disk_card(ui);
                });
            });
        }
    }

    fn disk_card(&self, ui: &mut Ui, p: &Palette) {
        let lang = self.lang;
        widgets::card(ui, p, |ui| {
            ui.set_min_height(172.0);
            let Some((total, free)) = self.disk else {
                ui.label(RichText::new("—").color(p.muted));
                return;
            };
            let used = total.saturating_sub(free);
            let pct = used as f64 * 100.0 / total.max(1) as f64;
            let (level_color, pill_text) = if pct >= diag::FULL_CRITICAL {
                (
                    p.bar_crit,
                    i18n::fmt1(lang, Key::DiskCritical, &format!("{pct:.0}")),
                )
            } else if pct >= diag::FULL_WARNING {
                (
                    p.bar_warn,
                    i18n::fmt1(lang, Key::DiskWarning, &format!("{pct:.0}")),
                )
            } else {
                (
                    p.accent,
                    i18n::fmt1(lang, Key::DiskOk, &format!("{pct:.0}")),
                )
            };

            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("💾  {}", i18n::t(lang, Key::SystemDisk))).color(p.muted),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if pct >= diag::FULL_CRITICAL {
                        widgets::pill_critical(ui, p, &pill_text);
                    } else if pct >= diag::FULL_WARNING {
                        widgets::pill_review(ui, p, &pill_text);
                    } else {
                        widgets::pill_safe(ui, p, &pill_text);
                    }
                });
            });
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(i18n::bytes(lang, free))
                        .size(26.0)
                        .bold()
                        .color(p.text),
                );
                ui.label(RichText::new(i18n::t(lang, Key::Free)).color(p.muted));
            });

            // Barra de uso com marcas em 90% e 95%.
            let (rect, _) =
                ui.allocate_exact_size(vec2(ui.available_width(), 10.0), Sense::hover());
            ui.painter().rect_filled(rect, Rounding::same(5.0), p.track);
            let mut filled = rect;
            filled.set_width(rect.width() * (pct as f32 / 100.0).clamp(0.0, 1.0));
            ui.painter()
                .rect_filled(filled, Rounding::same(5.0), level_color);
            for mark in [diag::FULL_WARNING, diag::FULL_CRITICAL] {
                let x = rect.left() + rect.width() * (mark as f32 / 100.0);
                ui.painter().line_segment(
                    [
                        egui::pos2(x, rect.top() - 2.0),
                        egui::pos2(x, rect.bottom() + 2.0),
                    ],
                    Stroke::new(1.0_f32, p.faint),
                );
            }
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(i18n::fmt1(lang, Key::UsedOf, &i18n::bytes(lang, used)))
                        .size(12.5)
                        .color(p.muted),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(i18n::bytes(lang, total))
                            .size(12.5)
                            .color(p.muted),
                    );
                });
            });
            ui.label(
                RichText::new(i18n::t(lang, Key::DiskThresholds))
                    .size(12.0)
                    .color(p.faint),
            );
        });
    }

    fn ui_empty_group(&mut self, ui: &mut Ui, p: &Palette, empty: &[usize], rc: &RowCtx) {
        let lang = self.lang;
        let open = self.expanded.contains("__empty__");
        let preview: Vec<&str> = empty
            .iter()
            .take(3)
            .map(|&i| i18n::cat_name(lang, &self.cats[i].cat))
            .collect();
        let header = egui::Frame::none()
            .stroke(Stroke::new(1.0_f32, p.border))
            .rounding(Rounding::same(12.0))
            .inner_margin(Margin::symmetric(16.0, 10.0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    widgets::chevron(ui, p.muted, open);
                    ui.label(RichText::new(i18n::t(lang, Key::EmptyGroup)).color(p.muted));
                    ui.label(
                        RichText::new(empty.len().to_string())
                            .size(11.5)
                            .color(p.muted)
                            .background_color(p.track),
                    );
                    if !open {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(format!("{}…", preview.join(" · ")))
                                        .size(12.5)
                                        .color(p.faint),
                                )
                                .truncate(),
                            );
                        });
                    }
                });
            });
        let resp = ui
            .interact(
                header.response.rect,
                egui::Id::new("empty_group"),
                Sense::click(),
            )
            .on_hover_cursor(CursorIcon::PointingHand);
        if resp.clicked() {
            if open {
                self.expanded.remove("__empty__");
            } else {
                self.expanded.insert("__empty__");
            }
        }
        if open {
            ui.add_space(4.0);
            for &i in empty {
                let state = &self.cats[i];
                widgets::card(ui, p, |ui| {
                    ui.label(
                        RichText::new(i18n::cat_name(rc.lang, &state.cat))
                            .bold()
                            .color(p.muted),
                    );
                    let note = state
                        .scan
                        .as_ref()
                        .and_then(|s| s.note.as_deref())
                        .map(|n| i18n::scan_note(rc.lang, n))
                        .unwrap_or_else(|| i18n::cat_desc(rc.lang, &state.cat).to_string());
                    ui.label(RichText::new(note).size(12.5).color(p.faint));
                });
            }
        }
    }

    fn ui_empty_state(&mut self, ui: &mut Ui, p: &Palette) {
        let lang = self.lang;
        widgets::card(ui, p, |ui| {
            ui.add_space(36.0);
            ui.vertical_centered(|ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(150.0, 100.0), Sense::hover());
                let stroke = Stroke::new(2.0_f32, p.faint);
                let body =
                    egui::Rect::from_min_size(rect.min + vec2(15.0, 38.0), vec2(120.0, 52.0));
                ui.painter().rect_stroke(body, Rounding::same(10.0), stroke);
                ui.painter().line_segment(
                    [
                        egui::pos2(body.left(), body.top() + 26.0),
                        egui::pos2(body.right(), body.top() + 26.0),
                    ],
                    stroke,
                );
                for dx in [20.0, 32.0] {
                    ui.painter().circle_filled(
                        egui::pos2(body.left() + dx, body.top() + 40.0),
                        2.5,
                        p.faint,
                    );
                }
                let spark = Stroke::new(2.0_f32, p.accent);
                for (cx, cy, r) in [(75.0, 16.0, 11.0), (118.0, 24.0, 6.0)] {
                    let c = rect.min + vec2(cx, cy);
                    ui.painter()
                        .line_segment([c + vec2(0.0, -r), c + vec2(0.0, r)], spark);
                    ui.painter()
                        .line_segment([c + vec2(-r, 0.0), c + vec2(r, 0.0)], spark);
                }
                ui.add_space(10.0);
                ui.label(
                    RichText::new(i18n::t(lang, Key::EmptyTitle))
                        .size(20.0)
                        .bold()
                        .color(p.text),
                );
                ui.label(RichText::new(i18n::t(lang, Key::EmptyBody)).color(p.muted));
                ui.add_space(8.0);
                let label = format!("⟳  {}", i18n::t(lang, Key::Rescan));
                if widgets::primary_button(ui, p, &label, !self.busy(), false).clicked() {
                    self.scan_all(ui.ctx());
                }
            });
            ui.add_space(36.0);
        });
    }

    // ---------------------------------------------------------------- tela: Discos

    fn ui_disks(&mut self, ui: &mut Ui, p: &Palette) {
        let lang = self.lang;
        let mut refresh = false;
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(i18n::t(lang, Key::DisksTitle))
                        .size(20.0)
                        .bold()
                        .color(p.text),
                );
                ui.label(RichText::new(i18n::t(lang, Key::DisksSubtitle)).color(p.muted));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let loading = matches!(self.disks, DiskState::Loading);
                if widgets::ghost_button(
                    ui,
                    &format!("⟳  {}", i18n::t(lang, Key::Refresh)),
                    !loading,
                )
                .clicked()
                {
                    refresh = true;
                }
            });
        });
        ui.add_space(6.0);

        match &self.disks {
            DiskState::Idle | DiskState::Loading => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new(i18n::t(lang, Key::DisksReading)).color(p.muted));
                });
            }
            DiskState::Ready(report) => {
                if !report.disks.is_empty() {
                    ui.label(
                        RichText::new(i18n::t(lang, Key::PhysicalDisks))
                            .bold()
                            .color(p.text),
                    );
                    for disk in &report.disks {
                        disk_card(ui, p, lang, disk);
                    }
                }
                ui.add_space(4.0);
                ui.label(
                    RichText::new(i18n::t(lang, Key::Volumes))
                        .bold()
                        .color(p.text),
                );
                for volume in &report.volumes {
                    volume_card(ui, p, lang, volume);
                }
                if !report.notes.is_empty() {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(i18n::t(lang, Key::CollectionNotes))
                            .bold()
                            .color(p.muted),
                    );
                    for note in &report.notes {
                        ui.label(RichText::new(note).size(12.5).color(p.faint));
                    }
                }
            }
        }
        if refresh {
            self.load_disks(ui.ctx());
        }
    }

    // ---------------------------------------------------------------- detalhes e histórico

    /// Resumo e falhas das limpezas desta sessão (painel lateral e tela Histórico).
    fn details_ui(&mut self, ui: &mut Ui, p: &Palette) {
        let lang = self.lang;
        let n_failures: usize = self
            .runs
            .iter()
            .flat_map(|r| &r.results)
            .map(|r| r.failures.len())
            .sum();

        ui.horizontal(|ui| {
            let tabs = [
                (
                    DetailsTab::Summary,
                    i18n::t(lang, Key::TabSummary).to_string(),
                ),
                (
                    DetailsTab::Failures,
                    i18n::fmt1(lang, Key::TabFailures, &n_failures.to_string()),
                ),
            ];
            for (tab, label) in tabs {
                let active = self.details_tab == tab;
                let text =
                    RichText::new(label)
                        .bold()
                        .color(if active { p.accent_text } else { p.muted });
                if ui
                    .add(egui::Button::new(text).frame(false))
                    .on_hover_cursor(CursorIcon::PointingHand)
                    .clicked()
                {
                    self.details_tab = tab;
                }
            }
        });
        widgets::hline(ui, p);
        ui.add_space(4.0);

        if self.runs.is_empty() {
            ui.label(RichText::new(i18n::t(lang, Key::NoCleanups)).color(p.muted));
            return;
        }

        match self.details_tab {
            DetailsTab::Summary => {
                for (n, run) in self.runs.iter().enumerate().rev() {
                    let mode = match run.mode {
                        Mode::Trash => i18n::t(lang, Key::DestTrash),
                        Mode::Permanent => i18n::t(lang, Key::DestPermanent),
                    };
                    ui.label(
                        RichText::new(format!(
                            "{} · {mode}",
                            i18n::fmt1(lang, Key::RunLabel, &(n + 1).to_string())
                        ))
                        .size(12.5)
                        .color(p.muted),
                    );
                    for r in &run.results {
                        let state = &self.cats[r.cat];
                        widgets::card(ui, p, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(i18n::cat_name(lang, &state.cat))
                                        .bold()
                                        .color(p.text),
                                );
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if r.failures.is_empty() {
                                        ui.label(
                                            RichText::new(i18n::t(lang, Key::ResultDone))
                                                .color(p.ok_fg),
                                        );
                                    } else {
                                        ui.label(
                                            RichText::new(i18n::t(lang, Key::ResultFailed))
                                                .color(p.err_fg),
                                        );
                                    }
                                });
                            });
                            if !r.command {
                                let verb = if run.mode == Mode::Trash {
                                    Key::MovedToTrash
                                } else {
                                    Key::Removed
                                };
                                ui.label(
                                    RichText::new(format!(
                                        "{} {} · {}",
                                        i18n::n_items(lang, r.removed),
                                        i18n::t(lang, verb),
                                        i18n::bytes(lang, r.freed)
                                    ))
                                    .size(12.5)
                                    .color(p.muted),
                                );
                            }
                        });
                    }
                }
                if self.runs.last().is_some_and(|r| r.mode == Mode::Trash) {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(i18n::t(lang, Key::TrashNote))
                            .size(12.5)
                            .color(p.muted),
                    );
                }
            }
            DetailsTab::Failures => {
                if n_failures == 0 {
                    ui.label(RichText::new(i18n::t(lang, Key::NoFailures)).color(p.muted));
                }
                for run in self.runs.iter().rev() {
                    for r in &run.results {
                        for f in &r.failures {
                            widgets::card(ui, p, |ui| {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(display_path(&f.path))
                                            .monospace()
                                            .size(12.5)
                                            .color(p.text),
                                    )
                                    .truncate(),
                                );
                                ui.label(
                                    RichText::new(i18n::failure_text(lang, f))
                                        .size(12.5)
                                        .color(p.err_fg),
                                );
                            });
                        }
                    }
                }
            }
        }
    }

    fn details_panel(&mut self, ctx: &egui::Context, p: &Palette) {
        let lang = self.lang;
        egui::SidePanel::right("details")
            .exact_width(380.0)
            .resizable(false)
            .frame(
                egui::Frame::none()
                    .fill(p.card)
                    .inner_margin(Margin::same(18.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(i18n::t(lang, Key::DetailsTitle))
                            .size(16.0)
                            .bold()
                            .color(p.text),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if widgets::close_button(ui, p.muted).clicked() {
                            self.details_open = false;
                        }
                    });
                });
                ui.add_space(6.0);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.details_ui(ui, p));
            });
    }

    // ---------------------------------------------------------------- tela: Ajustes

    fn ui_settings(&mut self, ui: &mut Ui, p: &Palette) {
        let lang = self.lang;
        let ctx = ui.ctx().clone();
        ui.label(
            RichText::new(i18n::t(lang, Key::SettingsTitle))
                .size(20.0)
                .bold()
                .color(p.text),
        );
        ui.add_space(4.0);

        widgets::card(ui, p, |ui| {
            ui.label(
                RichText::new(i18n::t(lang, Key::LanguageLabel))
                    .bold()
                    .color(p.text),
            );
            ui.add_space(2.0);
            ui.horizontal_wrapped(|ui| {
                let options = [
                    (LangPref::Auto, i18n::t(lang, Key::OptAuto)),
                    (LangPref::PtBr, "Português (Brasil)"),
                    (LangPref::EnUs, "English (US)"),
                ];
                for (value, label) in options {
                    if ui
                        .selectable_label(self.prefs.lang == value, label)
                        .clicked()
                    {
                        self.set_lang_pref(value);
                    }
                }
            });
            ui.label(
                RichText::new(i18n::t(lang, Key::LanguageNote))
                    .size(12.5)
                    .color(p.muted),
            );
        });

        widgets::card(ui, p, |ui| {
            ui.label(
                RichText::new(i18n::t(lang, Key::ThemeLabel))
                    .bold()
                    .color(p.text),
            );
            ui.add_space(2.0);
            ui.horizontal_wrapped(|ui| {
                let options = [
                    (ThemePref::Auto, i18n::t(lang, Key::OptAuto)),
                    (ThemePref::Light, i18n::t(lang, Key::OptLight)),
                    (ThemePref::Dark, i18n::t(lang, Key::OptDark)),
                ];
                for (value, label) in options {
                    if ui
                        .selectable_label(self.prefs.theme == value, label)
                        .clicked()
                    {
                        self.set_theme_pref(&ctx, value);
                    }
                }
            });
        });

        widgets::card(ui, p, |ui| {
            ui.label(
                RichText::new(i18n::t(lang, Key::AboutLabel))
                    .bold()
                    .color(p.text),
            );
            ui.label(
                RichText::new(format!(
                    "{} · {}",
                    i18n::t(lang, Key::AppName),
                    i18n::fmt1(lang, Key::Version, env!("CARGO_PKG_VERSION"))
                ))
                .color(p.muted),
            );
        });
    }

    // ---------------------------------------------------------------- confirmação

    fn confirm_modal(&mut self, ctx: &egui::Context, p: &Palette) {
        let lang = self.lang;
        let chosen: Vec<&CatState> = self.cats.iter().filter(|c| c.has_work()).collect();
        let bytes: u64 = chosen.iter().map(|c| c.chosen_bytes()).sum();
        let n_items: usize = chosen
            .iter()
            .map(|c| match &c.cat.target {
                Target::DirContents { .. } => c.chosen_entries().len(),
                Target::Command { .. } => c.scan.as_ref().and_then(|s| s.items).unwrap_or(0),
            })
            .sum();
        let permanent = self.mode == Mode::Permanent;
        let word = i18n::t(lang, Key::ConfirmWord);
        let typed_ok = self.confirm_text.trim().eq_ignore_ascii_case(word);
        let mut go = false;
        let mut cancel = false;

        // Fundo escurecido: também impede clicar no que está atrás.
        let screen = ctx.screen_rect();
        egui::Area::new(egui::Id::new("scrim"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                let (rect, resp) = ui.allocate_exact_size(screen.size(), Sense::click_and_drag());
                ui.painter()
                    .rect_filled(rect, 0.0, Color32::from_black_alpha(140));
                if resp.clicked() {
                    cancel = true;
                }
            });

        egui::Window::new("confirm_cleanup")
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .order(egui::Order::Tooltip)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .frame(
                egui::Frame::none()
                    .fill(p.card)
                    .stroke(Stroke::new(1.0_f32, p.border))
                    .rounding(Rounding::same(16.0))
                    .inner_margin(Margin::same(24.0))
                    .shadow(egui::Shadow {
                        offset: vec2(0.0, 12.0),
                        blur: 40.0,
                        spread: 0.0,
                        color: Color32::from_black_alpha(90),
                    }),
            )
            .show(ctx, |ui| {
                ui.set_width(440.0);
                let title = if permanent {
                    Key::DeletePermTitle
                } else {
                    Key::ConfirmTitle
                };
                ui.label(
                    RichText::new(i18n::t(lang, title))
                        .size(18.0)
                        .bold()
                        .color(p.text),
                );
                ui.label(
                    RichText::new(format!(
                        "{} · {}",
                        i18n::n_categories(lang, chosen.len()),
                        i18n::n_items(lang, n_items)
                    ))
                    .color(p.muted),
                );
                ui.add_space(6.0);

                egui::Frame::none()
                    .stroke(Stroke::new(1.0_f32, p.border))
                    .rounding(Rounding::same(10.0))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.spacing_mut().item_spacing.y = 0.0;
                        for c in &chosen {
                            ui.horizontal(|ui| {
                                ui.add_space(10.0);
                                ui.label(RichText::new(i18n::cat_name(lang, &c.cat)).color(p.text));
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.add_space(10.0);
                                    let size = match &c.cat.target {
                                        Target::DirContents { .. } => {
                                            i18n::bytes(lang, c.chosen_bytes())
                                        }
                                        Target::Command { .. } => "—".to_string(),
                                    };
                                    ui.label(RichText::new(size).bold().color(p.text));
                                });
                            });
                            widgets::hline(ui, p);
                        }
                        egui::Frame::none().fill(p.hover).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                ui.add_space(10.0);
                                ui.label(
                                    RichText::new(i18n::t(lang, Key::Total))
                                        .bold()
                                        .color(p.text),
                                );
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.add_space(10.0);
                                    ui.label(
                                        RichText::new(i18n::bytes(lang, bytes))
                                            .bold()
                                            .color(p.text),
                                    );
                                });
                            });
                        });
                    });
                ui.add_space(6.0);

                if permanent {
                    egui::Frame::none()
                        .fill(p.err_bg)
                        .stroke(Stroke::new(1.0_f32, p.err_border))
                        .rounding(Rounding::same(10.0))
                        .inner_margin(Margin::symmetric(12.0, 8.0))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(
                                RichText::new(format!("⚠  {}", i18n::t(lang, Key::DeletePermWarn)))
                                    .color(p.err_fg),
                            );
                        });
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(i18n::fmt1(lang, Key::TypeToConfirm, word))
                            .size(12.5)
                            .color(p.muted),
                    );
                    let edit = egui::TextEdit::singleline(&mut self.confirm_text)
                        .desired_width(f32::INFINITY)
                        .font(FontId::monospace(14.0))
                        .margin(vec2(10.0, 8.0));
                    let resp = ui.add(edit);
                    if !resp.has_focus() && self.confirm_text.is_empty() {
                        resp.request_focus();
                    }
                } else {
                    egui::Frame::none()
                        .fill(p.info_bg)
                        .rounding(Rounding::same(10.0))
                        .inner_margin(Margin::symmetric(12.0, 8.0))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(RichText::new(i18n::t(lang, Key::TrashInfo)).color(p.info_fg));
                        });
                    ui.label(
                        RichText::new(i18n::t(lang, Key::CloseApps))
                            .size(12.5)
                            .color(p.muted),
                    );
                }

                ui.add_space(10.0);
                let row_w = ui.available_width();
                ui.allocate_ui_with_layout(
                    vec2(row_w, 36.0),
                    Layout::right_to_left(Align::Center),
                    |ui| {
                        if permanent {
                            let label = i18n::t(lang, Key::DeletePermanently);
                            if widgets::primary_button(ui, p, label, typed_ok, true).clicked() {
                                go = true;
                            }
                        } else if widgets::primary_button(
                            ui,
                            p,
                            i18n::t(lang, Key::MoveToTrash),
                            true,
                            false,
                        )
                        .clicked()
                        {
                            go = true;
                        }
                        if widgets::ghost_button(ui, i18n::t(lang, Key::Cancel), true).clicked() {
                            cancel = true;
                        }
                    },
                );
            });

        if go && (!permanent || typed_ok) {
            self.close_confirm();
            self.start_clean(ctx);
        } else if cancel {
            self.close_confirm();
        }
    }
}

enum ScanDot {
    Busy,
    Done,
}

fn resolve_lang(pref: LangPref) -> Lang {
    match pref {
        LangPref::Auto => Lang::detect(),
        LangPref::PtBr => Lang::PtBr,
        LangPref::EnUs => Lang::EnUs,
    }
}

fn open(arg: &str) {
    let _ = Command::new("open").arg(arg).spawn();
}

// -------------------------------------------------------------------- componentes da lista

struct RowCtx<'a> {
    p: &'a Palette,
    lang: Lang,
    locked: bool,
    max_bytes: u64,
}

/// Item da barra lateral (ícone + texto; só o ícone quando estreito).
fn nav_item(
    ui: &mut Ui,
    p: &Palette,
    narrow: bool,
    icon: &str,
    label: &str,
    active: bool,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 38.0), Sense::click());
    let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
    if active {
        ui.painter()
            .rect_filled(rect, Rounding::same(9.0), p.accent_soft);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, Rounding::same(9.0), p.track);
    }
    let color = if active { p.accent_text } else { p.muted };
    let icon_pos = if narrow {
        rect.center()
    } else {
        egui::pos2(rect.left() + 24.0, rect.center().y)
    };
    ui.painter().text(
        icon_pos,
        Align2::CENTER_CENTER,
        icon,
        FontId::proportional(16.0),
        color,
    );
    if !narrow {
        ui.painter().text(
            egui::pos2(rect.left() + 46.0, rect.center().y),
            Align2::LEFT_CENTER,
            label,
            FontId::proportional(14.0),
            if active { p.accent_text } else { p.text },
        );
    } else {
        return resp.on_hover_text(label);
    }
    resp
}

/// Cartão de seção (Seguro / Revisar): cabeçalho com pílula, dica e subtotal.
fn section(
    ui: &mut Ui,
    p: &Palette,
    pill: impl FnOnce(&mut Ui),
    hint: &str,
    subtotal: Option<String>,
    rows: impl FnOnce(&mut Ui),
) {
    egui::Frame::none()
        .fill(p.card)
        .stroke(Stroke::new(1.0_f32, p.border))
        .rounding(Rounding::same(12.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            egui::Frame::none()
                .fill(p.hover)
                .rounding(Rounding {
                    nw: 12.0,
                    ne: 12.0,
                    sw: 0.0,
                    se: 0.0,
                })
                .inner_margin(Margin::symmetric(16.0, 8.0))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        pill(ui);
                        ui.label(RichText::new(hint).size(12.5).color(p.muted));
                        if let Some(sub) = subtotal {
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(RichText::new(sub).size(12.5).color(p.muted));
                            });
                        }
                    });
                });
            widgets::hline(ui, p);
            rows(ui);
        });
}

/// Uma categoria: checkbox, nome e descrição, mini-barra, tamanho e, se houver, a lista de itens.
fn category_row(
    ui: &mut Ui,
    rc: &RowCtx,
    state: &mut CatState,
    kind: RowKind,
    expanded: &mut HashSet<&'static str>,
    show_all: &mut HashSet<&'static str>,
) {
    let p = rc.p;
    let lang = rc.lang;
    let id = state.cat.id;
    let denied = kind == RowKind::Denied;
    let has_list =
        kind == RowKind::Content && state.scan.as_ref().is_some_and(|s| !s.entries.is_empty());
    let open = has_list && expanded.contains(id);

    let frame = egui::Frame::none()
        .fill(if denied {
            p.err_bg
        } else {
            Color32::TRANSPARENT
        })
        .inner_margin(Margin::symmetric(16.0, 10.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let mut left_rect = ui.max_rect();
            ui.horizontal(|ui| {
                let avail = ui.available_width();
                let right_w = 246.0;
                let left_w = (avail - 18.0 - right_w - 2.0 * ui.spacing().item_spacing.x).max(80.0);

                let mut on = state.selected && kind == RowKind::Content;
                let enabled = kind == RowKind::Content && !rc.locked && state.available();
                if widgets::check(ui, p, &mut on, enabled).changed() {
                    state.selected = on;
                }

                let left = ui.allocate_ui_with_layout(
                    vec2(left_w, 40.0),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.set_width(left_w);
                        ui.spacing_mut().item_spacing.y = 1.0;
                        let name = RichText::new(i18n::cat_name(lang, &state.cat))
                            .bold()
                            .color(if kind == RowKind::Empty {
                                p.muted
                            } else {
                                p.text
                            });
                        ui.add(egui::Label::new(name).truncate());
                        let (text, color) = if denied {
                            (
                                i18n::t(lang, Key::PermissionDeniedRow).to_string(),
                                p.err_fg,
                            )
                        } else if let Some(note) = state
                            .scan
                            .as_ref()
                            .filter(|s| s.items.is_some_and(|n| n > 0))
                            .and_then(|s| s.note.as_deref())
                        {
                            (i18n::scan_note(lang, note), p.ok_fg)
                        } else {
                            (i18n::cat_desc(lang, &state.cat).to_string(), p.muted)
                        };
                        ui.add(
                            egui::Label::new(RichText::new(text).size(12.5).color(color))
                                .truncate(),
                        );
                    },
                );
                left_rect = left.response.rect;

                ui.allocate_ui_with_layout(
                    vec2(right_w, 40.0),
                    Layout::right_to_left(Align::Center),
                    |ui| {
                        if has_list {
                            widgets::chevron(ui, p.muted, open);
                        } else {
                            ui.add_space(16.0);
                        }
                        match (kind, &state.scan) {
                            (RowKind::Loading, _) => widgets::skeleton(ui, p, 64.0, 16.0),
                            (_, Some(scan)) if scan.items.is_some() => {
                                ui.label(RichText::new("—").color(p.faint));
                            }
                            (_, Some(scan)) => {
                                ui.add_sized(
                                    [78.0, 18.0],
                                    egui::Label::new(
                                        RichText::new(i18n::bytes(lang, scan.total))
                                            .bold()
                                            .color(p.text),
                                    ),
                                );
                                if rc.max_bytes > 0 {
                                    let frac = scan.total as f32 / rc.max_bytes as f32;
                                    let color = if state.selected {
                                        p.accent.gamma_multiply(0.6)
                                    } else {
                                        p.faint.gamma_multiply(0.55)
                                    };
                                    widgets::mini_bar(ui, frac, color, p.track, 92.0);
                                }
                            }
                            _ => {}
                        }
                    },
                );
            });
            left_rect
        });

    if has_list {
        let click = ui
            .interact(frame.inner, ui.id().with(("row", id)), Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);
        if click.clicked() {
            if open {
                expanded.remove(id);
            } else {
                expanded.insert(id);
            }
        }
    }
    if open {
        entries_panel(ui, rc, state, show_all);
    }
}

/// Lista de itens de uma categoria expandida.
fn entries_panel(
    ui: &mut Ui,
    rc: &RowCtx,
    state: &mut CatState,
    show_all: &mut HashSet<&'static str>,
) {
    let p = rc.p;
    let lang = rc.lang;
    let id = state.cat.id;
    let Some(scan) = &state.scan else { return };
    let total = scan.entries.len();
    let all = show_all.contains(id);
    let limit = if all {
        total
    } else {
        VISIBLE_ENTRIES.min(total)
    };

    egui::Frame::none()
        .fill(p.hover)
        .inner_margin(Margin {
            left: 50.0,
            right: 16.0,
            top: 8.0,
            bottom: 12.0,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(i18n::n_items(lang, total))
                        .size(12.5)
                        .bold()
                        .color(p.text),
                );
                let link = |ui: &mut Ui, text: &str, color: Color32| {
                    ui.add(
                        egui::Button::new(RichText::new(text).size(12.5).color(color)).frame(false),
                    )
                    .on_hover_cursor(CursorIcon::PointingHand)
                };
                if link(ui, i18n::t(lang, Key::SelectAll), p.accent_text).clicked() {
                    state.deselected.clear();
                }
                if link(ui, i18n::t(lang, Key::SelectNone), p.muted).clicked() {
                    if let Some(s) = &state.scan {
                        state.deselected = s.entries.iter().map(|e| e.path.clone()).collect();
                    }
                }
            });

            let Some(scan) = &state.scan else { return };
            egui::Frame::none()
                .fill(p.card)
                .stroke(Stroke::new(1.0_f32, p.border))
                .rounding(Rounding::same(10.0))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for (n, entry) in scan.entries.iter().take(limit).enumerate() {
                        if n > 0 {
                            widgets::hline(ui, p);
                        }
                        entry_row(ui, rc, &mut state.deselected, entry);
                    }
                    if total > limit {
                        widgets::hline(ui, p);
                        ui.horizontal(|ui| {
                            ui.add_space(12.0);
                            let text = i18n::fmt1(lang, Key::ViewAll, &total.to_string());
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new(text).size(12.5).color(p.accent_text),
                                    )
                                    .frame(false),
                                )
                                .on_hover_cursor(CursorIcon::PointingHand)
                                .clicked()
                            {
                                show_all.insert(id);
                            }
                        });
                    }
                });
        });
}

fn entry_row(ui: &mut Ui, rc: &RowCtx, deselected: &mut HashSet<PathBuf>, entry: &Entry) {
    let p = rc.p;
    let lang = rc.lang;
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        ui.set_height(34.0);
        let mut on = !deselected.contains(&entry.path);
        if widgets::check(ui, p, &mut on, !rc.locked).changed() {
            if on {
                deselected.remove(&entry.path);
            } else {
                deselected.insert(entry.path.clone());
            }
        }
        ui.allocate_ui_with_layout(
            vec2(78.0, 18.0),
            Layout::right_to_left(Align::Center),
            |ui| {
                ui.label(
                    RichText::new(i18n::bytes(lang, entry.size))
                        .bold()
                        .color(p.text),
                );
            },
        );
        let w = (ui.available_width() - 62.0).max(60.0);
        ui.allocate_ui_with_layout(vec2(w, 18.0), Layout::left_to_right(Align::Center), |ui| {
            ui.set_width(w);
            ui.add(
                egui::Label::new(
                    RichText::new(display_path(&entry.path))
                        .monospace()
                        .size(12.5)
                        .color(p.muted),
                )
                .truncate(),
            );
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.scope(|ui| {
                let widgets_vis = &mut ui.visuals_mut().widgets;
                for s in [
                    &mut widgets_vis.inactive,
                    &mut widgets_vis.hovered,
                    &mut widgets_vis.active,
                    &mut widgets_vis.open,
                ] {
                    s.bg_stroke = Stroke::NONE;
                }
                widgets_vis.inactive.weak_bg_fill = Color32::TRANSPARENT;
                ui.spacing_mut().button_padding = vec2(6.0, 4.0);
                ui.menu_button(RichText::new("•••").size(10.0).color(p.muted), |ui| {
                    if ui.button(i18n::t(lang, Key::MenuShowInFinder)).clicked() {
                        let _ = Command::new("open").arg("-R").arg(&entry.path).spawn();
                        ui.close_menu();
                    }
                    if ui.button(i18n::t(lang, Key::MenuCopyPath)).clicked() {
                        ui.ctx()
                            .copy_text(entry.path.to_string_lossy().into_owned());
                        ui.close_menu();
                    }
                    ui.separator();
                    let included = !deselected.contains(&entry.path);
                    let key = if included {
                        Key::MenuExclude
                    } else {
                        Key::MenuInclude
                    };
                    if ui.button(i18n::t(lang, key)).clicked() {
                        if included {
                            deselected.insert(entry.path.clone());
                        } else {
                            deselected.remove(&entry.path);
                        }
                        ui.close_menu();
                    }
                });
            });
        });
    });
}

fn disk_card(ui: &mut Ui, p: &Palette, lang: Lang, disk: &diag::Disk) {
    widgets::card(ui, p, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(&disk.name).bold().color(p.text));
            if let Some(model) = &disk.model {
                ui.label(RichText::new(model).color(p.muted));
            }
            if let Some(size) = disk.size_bytes {
                ui.label(RichText::new(i18n::bytes(lang, size)).color(p.muted));
            }
        });
        ui.horizontal_wrapped(|ui| {
            match disk.kind {
                diag::DiskKind::Ssd => widgets::pill(ui, "SSD", p.info_bg, p.info_fg),
                diag::DiskKind::Hdd => widgets::pill(ui, "HDD", p.info_bg, p.info_fg),
                diag::DiskKind::Unknown => {}
            }
            match disk.internal {
                Some(true) => widgets::pill(ui, i18n::t(lang, Key::Internal), p.track, p.muted),
                Some(false) => widgets::pill(ui, i18n::t(lang, Key::External), p.track, p.muted),
                None => {}
            }
            if disk.removable {
                widgets::pill(ui, i18n::t(lang, Key::Removable), p.track, p.muted);
            }
            let (key, bg, fg) = match disk.smart {
                diag::SmartStatus::Passed => (Key::SmartPassed, p.ok_bg, p.ok_fg),
                diag::SmartStatus::Failing => (Key::SmartFailing, p.err_bg, p.err_fg),
                diag::SmartStatus::NotSupported => (Key::SmartUnsupported, p.track, p.muted),
                diag::SmartStatus::Unknown => (Key::SmartUnknown, p.track, p.muted),
            };
            widgets::pill(ui, i18n::t(lang, key), bg, fg);
        });
    });
}

fn volume_card(ui: &mut Ui, p: &Palette, lang: Lang, v: &diag::Volume) {
    let pct = v.used_percent();
    widgets::card(ui, p, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(&v.mount).bold().color(p.text));
            ui.label(
                RichText::new(format!("{} · {}", v.device, v.fstype))
                    .size(12.5)
                    .color(p.muted),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if v.read_only {
                    widgets::pill(ui, i18n::t(lang, Key::ReadOnly), p.track, p.muted);
                } else if pct >= diag::FULL_CRITICAL {
                    widgets::pill_critical(
                        ui,
                        p,
                        &i18n::fmt1(lang, Key::DiskCritical, &format!("{pct:.0}")),
                    );
                } else if pct >= diag::FULL_WARNING {
                    widgets::pill_review(
                        ui,
                        p,
                        &i18n::fmt1(lang, Key::DiskWarning, &format!("{pct:.0}")),
                    );
                } else {
                    widgets::pill_safe(ui, p, &i18n::fmt1(lang, Key::DiskOk, &format!("{pct:.0}")));
                }
            });
        });
        let color = if pct >= diag::FULL_CRITICAL {
            p.bar_crit
        } else if pct >= diag::FULL_WARNING {
            p.bar_warn
        } else {
            p.accent
        };
        widgets::mini_bar(
            ui,
            (pct / 100.0) as f32,
            color,
            p.track,
            ui.available_width(),
        );
        ui.label(
            RichText::new(i18n::fmt2(
                lang,
                Key::VolumeFreeOfTotal,
                &i18n::bytes(lang, v.free_bytes),
                &i18n::bytes(lang, v.total_bytes),
            ))
            .size(12.5)
            .color(p.muted),
        );
    });
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll(ctx);
        self.apply_preview_after_scan();
        let p = Palette::current(ctx);
        let narrow = ctx.screen_rect().width() < NARROW_WIDTH;
        let filter_id = egui::Id::new("filter");
        self.shortcuts(ctx, filter_id);

        if self.view == View::Disks && matches!(self.disks, DiskState::Idle) {
            self.load_disks(ctx);
        }
        // Atualiza o "Escaneado há N min" sem gastar CPU.
        ctx.request_repaint_after(Duration::from_secs(30));

        self.sidebar(ctx, &p, narrow);
        self.topbar(ctx, &p, narrow, filter_id);

        let mut action = Action::None;
        if self.view == View::Cleanup {
            action = self.action_bar(ctx, &p, narrow);
            if self.details_open {
                self.details_panel(ctx, &p);
            }
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(p.bg))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let avail = ui.available_width();
                        let w = (avail - 48.0).clamp(280.0, MAX_CONTENT_WIDTH);
                        let side = ((avail - w) / 2.0).max(0.0);
                        ui.add_space(20.0);
                        ui.horizontal_top(|ui| {
                            ui.add_space(side);
                            ui.vertical(|ui| {
                                ui.set_width(w);
                                match self.view {
                                    View::Cleanup => self.ui_cleanup(ui, &p, narrow),
                                    View::Disks => self.ui_disks(ui, &p),
                                    View::History => {
                                        let lang = self.lang;
                                        ui.label(
                                            RichText::new(i18n::t(lang, Key::DetailsTitle))
                                                .size(20.0)
                                                .bold()
                                                .color(p.text),
                                        );
                                        self.details_ui(ui, &p);
                                    }
                                    View::Settings => self.ui_settings(ui, &p),
                                }
                            });
                        });
                        ui.add_space(20.0);
                    });
            });

        match action {
            Action::Confirm => {
                self.confirm_text.clear();
                self.confirm_open = true;
            }
            Action::Rescan => self.scan_all(ctx),
            Action::None => {}
        }
        if self.confirm_open {
            self.confirm_modal(ctx, &p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::Entry;

    fn state(scan: Option<ScanResult>) -> CatState {
        let cat = categories::build(std::path::Path::new("/Users/teste"))
            .into_iter()
            .find(|c| c.id == "user_caches")
            .unwrap();
        CatState {
            selected: true,
            cat,
            scanning: false,
            scan,
            deselected: HashSet::new(),
        }
    }

    fn entry(size: u64) -> Entry {
        Entry {
            path: PathBuf::from("/Users/teste/Library/Caches/x"),
            size,
        }
    }

    #[test]
    fn row_without_scan_is_loading() {
        assert_eq!(row_kind(&state(None)), RowKind::Loading);
    }

    #[test]
    fn row_with_entries_is_content() {
        let scan = ScanResult {
            entries: vec![entry(10)],
            total: 10,
            ..Default::default()
        };
        assert_eq!(row_kind(&state(Some(scan))), RowKind::Content);
    }

    #[test]
    fn command_with_items_is_content_and_without_is_empty() {
        let some = ScanResult {
            items: Some(3),
            ..Default::default()
        };
        let none = ScanResult {
            items: Some(0),
            ..Default::default()
        };
        assert_eq!(row_kind(&state(Some(some))), RowKind::Content);
        assert_eq!(row_kind(&state(Some(none))), RowKind::Empty);
    }

    #[test]
    fn permission_denied_without_content_is_an_error_row() {
        let scan = ScanResult {
            denied: true,
            ..Default::default()
        };
        assert_eq!(row_kind(&state(Some(scan))), RowKind::Denied);
    }

    #[test]
    fn permission_denied_with_partial_content_still_lists_the_content() {
        let scan = ScanResult {
            denied: true,
            entries: vec![entry(5)],
            total: 5,
            ..Default::default()
        };
        assert_eq!(row_kind(&state(Some(scan))), RowKind::Content);
    }

    #[test]
    fn unavailable_tool_is_grouped_with_the_empty_ones() {
        let scan = ScanResult {
            unavailable: true,
            items: Some(9),
            ..Default::default()
        };
        assert_eq!(row_kind(&state(Some(scan))), RowKind::Empty);
    }

    #[test]
    fn preview_is_inert_in_release_and_never_cleans_in_debug() {
        // Sem EPMA_PREVIEW nada fica ativo.
        std::env::remove_var("EPMA_PREVIEW");
        let pv = Preview::from_env();
        assert!(!pv.active && !pv.pending);
    }
}
