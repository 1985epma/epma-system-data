//! Interface de linha de comando: tudo o que a janela faz, também no terminal.
//!
//! | Janela                                  | CLI                                      |
//! |-----------------------------------------|------------------------------------------|
//! | Barra de uso do disco                   | `disk` e cabeçalho do `scan`             |
//! | Cartões de categoria (risco, descrição) | `list`                                   |
//! | Tamanho por categoria e itens           | `scan [--details] [--only ID]`           |
//! | Marcar categorias (padrão: "seguro")    | `clean [--only ID] [--include-review]`   |
//! | Desmarcar itens individuais             | `clean --skip CAMINHO`                   |
//! | Lixeira ou exclusão permanente          | `clean [--permanent]`                    |
//! | Janela "Confirmar limpeza"              | plano + confirmação (`--yes` pula)       |
//! | Histórico                               | relatório por categoria no `clean`       |
//! | Botão "Finder"                          | `reveal CAMINHO`                         |
//!
//! Dados vão para o `stdout`; avisos, progresso e erros para o `stderr`.
//! Códigos de saída: 0 ok, 1 falhas na limpeza, 2 uso incorreto, 3 recusado
//! (limpeza pediria confirmação, mas não há terminal e faltou `--yes`).

use std::collections::HashSet;
use std::ffi::OsString;
use std::fmt::Write as _;
use std::io::{self, BufRead, IsTerminal, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command as Proc, ExitCode};
use std::thread;

use clap::{Args, CommandFactory, Parser, Subcommand};
use clap_complete::Shell;
use serde::Serialize;

use crate::categories::{self, Category, Risk, Target};
use crate::cleaner::{self, CleanReport, Mode};
use crate::plan;
use crate::scanner::{self, Entry, ScanResult};
use crate::util::{disk_space, display_path, format_bytes};

const BIN: &str = "epma-system-data";
/// Mesmo limite da lista expandida na janela.
const DEFAULT_TOP: usize = 50;
/// Palavra digitada para confirmar uma exclusão permanente interativa.
const PERMANENT_WORD: &str = "apagar";

pub const EXIT_FAILED: u8 = 1;
pub const EXIT_USAGE: u8 = 2;
pub const EXIT_REFUSED: u8 = 3;

const EXAMPLES: &str = "\
Exemplos:
  epma-system-data                        abre a janela
  epma-system-data scan --details         mede e lista os maiores itens de cada categoria
  epma-system-data clean --dry-run        mostra o que seria limpo, sem alterar nada
  epma-system-data clean                  limpa as categorias \"seguro\" (Lixeira, pede confirmação)
  epma-system-data clean -o ios_backups   limpa só os backups de iPhone/iPad
  epma-system-data clean --skip ~/Library/Caches/com.exemplo.app

Códigos de saída: 0 ok, 1 falhas na limpeza, 2 uso incorreto, 3 recusado (sem terminal e sem --yes).";

pub enum Outcome {
    /// Nenhum comando de terminal: abrir a janela.
    Gui,
    Exit(ExitCode),
}

#[derive(Parser, Debug)]
#[command(name = "epma-system-data", version, about, after_help = EXAMPLES)]
struct Cli {
    /// Alias antigo de `scan`.
    #[arg(long, hide = true)]
    scan: bool,

    /// Sem cores (também respeita a variável NO_COLOR).
    #[arg(long, global = true)]
    no_color: bool,

    /// Não imprime progresso nem avisos no stderr.
    #[arg(short, long, global = true)]
    quiet: bool,

    #[command(subcommand)]
    command: Option<Cmd>,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Abre a janela (o mesmo que rodar sem argumentos).
    Gui,
    /// Lista as categorias, o risco de cada uma e o que vem marcado por padrão.
    List(JsonArgs),
    /// Mede o espaço de cada categoria, sem alterar nada.
    Scan(ScanArgs),
    /// Mostra o uso do disco.
    Disk(JsonArgs),
    /// Limpa categorias (Lixeira por padrão; pede confirmação).
    Clean(CleanArgs),
    /// Mostra um item no Finder.
    Reveal {
        /// Caminho do arquivo ou pasta (aceita `~`).
        path: String,
    },
    /// Gera o script de autocompletar para o seu shell.
    Completions {
        #[arg(value_enum)]
        shell: Shell,
    },
}

#[derive(Args, Debug, Default)]
struct JsonArgs {
    /// Saída em JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Args, Debug)]
struct ScanArgs {
    /// Só estas categorias (ids de `list`; separe por vírgula ou repita a flag).
    #[arg(short, long, value_name = "ID", value_delimiter = ',')]
    only: Vec<String>,

    /// Lista também os maiores itens de cada categoria.
    #[arg(short, long)]
    details: bool,

    /// Quantos itens listar por categoria com --details.
    #[arg(long, value_name = "N", default_value_t = DEFAULT_TOP)]
    top: usize,

    /// Saída em JSON.
    #[arg(long)]
    json: bool,
}

impl Default for ScanArgs {
    fn default() -> Self {
        Self { only: vec![], details: false, top: DEFAULT_TOP, json: false }
    }
}

#[derive(Args, Debug)]
struct CleanArgs {
    /// Limpa exatamente estas categorias, mesmo as "revisar" (ids de `list`).
    #[arg(short, long, value_name = "ID", value_delimiter = ',')]
    only: Vec<String>,

    /// Soma as categorias "revisar" às marcadas por padrão.
    #[arg(long, conflicts_with = "only")]
    include_review: bool,

    /// Tira estas categorias da seleção.
    #[arg(short = 'x', long, value_name = "ID", value_delimiter = ',')]
    exclude: Vec<String>,

    /// Preserva este item (o mesmo que desmarcá-lo na janela). Pode repetir.
    #[arg(long, value_name = "CAMINHO")]
    skip: Vec<String>,

    /// Exclui de vez em vez de mover para a Lixeira. Não dá para desfazer.
    #[arg(long)]
    permanent: bool,

    /// Mostra o plano e sai, sem alterar nada.
    #[arg(short = 'n', long)]
    dry_run: bool,

    /// Não pergunta antes de limpar (para scripts e agendamentos).
    #[arg(short, long)]
    yes: bool,

    /// Saída em JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Debug)]
enum CliError {
    Usage(String),
    Refused(String),
    Failed(String),
}

type Res = Result<u8, CliError>;

pub fn run<I, T>(args: I) -> Outcome
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(e) => {
            let code = e.exit_code();
            let _ = e.print();
            return Outcome::Exit(ExitCode::from(u8::try_from(code).unwrap_or(EXIT_USAGE)));
        }
    };
    let ui = Ui::new(cli.no_color, cli.quiet);
    let command = match (cli.command, cli.scan) {
        (Some(cmd), _) => cmd,
        (None, true) => Cmd::Scan(ScanArgs::default()),
        (None, false) => return Outcome::Gui,
    };

    let result = match command {
        Cmd::Gui => return Outcome::Gui,
        Cmd::List(a) => cmd_list(&ui, &a),
        Cmd::Scan(a) => cmd_scan(&ui, &a),
        Cmd::Disk(a) => cmd_disk(&ui, &a),
        Cmd::Clean(a) => cmd_clean(&ui, &a),
        Cmd::Reveal { path } => cmd_reveal(&path),
        Cmd::Completions { shell } => {
            clap_complete::generate(shell, &mut Cli::command(), BIN, &mut io::stdout());
            Ok(0)
        }
    };

    let code = match result {
        Ok(code) => code,
        Err(CliError::Usage(msg)) => {
            eprintln!("erro: {msg}\nVeja `{BIN} --help`.");
            EXIT_USAGE
        }
        Err(CliError::Refused(msg)) => {
            eprintln!("erro: {msg}");
            EXIT_REFUSED
        }
        Err(CliError::Failed(msg)) => {
            eprintln!("erro: {msg}");
            EXIT_FAILED
        }
    };
    Outcome::Exit(ExitCode::from(code))
}

// ---------------------------------------------------------------- saída

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tone {
    Green,
    Amber,
    Red,
    Dim,
    Bold,
}

#[derive(Clone, Copy)]
struct Ui {
    color: bool,
    quiet: bool,
}

impl Ui {
    fn new(no_color: bool, quiet: bool) -> Self {
        let no_color_env = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
        let dumb = std::env::var("TERM").is_ok_and(|t| t == "dumb");
        Self {
            color: !no_color && !no_color_env && !dumb && io::stdout().is_terminal(),
            quiet,
        }
    }

    fn paint(&self, tone: Tone, text: &str) -> String {
        if !self.color {
            return text.to_string();
        }
        let code = match tone {
            Tone::Green => "32",
            Tone::Amber => "33",
            Tone::Red => "31",
            Tone::Dim => "2",
            Tone::Bold => "1",
        };
        format!("\x1b[{code}m{text}\x1b[0m")
    }

    /// Progresso e avisos: stderr, silenciados por --quiet.
    fn info(&self, msg: &str) {
        if !self.quiet {
            eprintln!("{msg}");
        }
    }
}

/// Escreve no stdout de uma vez; `| head` fechando o pipe não gera pânico.
fn emit(text: &str) {
    let mut out = io::stdout().lock();
    let _ = out.write_all(text.as_bytes());
    let _ = out.flush();
}

fn emit_json<T: Serialize>(value: &T) {
    if let Ok(mut text) = serde_json::to_string_pretty(value) {
        text.push('\n');
        emit(&text);
    }
}

type Cell = (String, Option<Tone>);

fn cell(text: impl Into<String>) -> Cell {
    (text.into(), None)
}

fn tone(text: impl Into<String>, tone: Tone) -> Cell {
    (text.into(), Some(tone))
}

/// Tabela com colunas alinhadas; a última coluna não recebe preenchimento.
fn table(ui: &Ui, heads: &[&str], rows: &[Vec<Cell>]) -> String {
    let mut widths: Vec<usize> = heads.iter().map(|h| h.chars().count()).collect();
    for row in rows {
        for (w, (text, _)) in widths.iter_mut().zip(row) {
            *w = (*w).max(text.chars().count());
        }
    }
    let last = heads.len() - 1;
    let line = |cells: Vec<(String, Option<Tone>)>| {
        let mut out = String::new();
        for (i, (text, t)) in cells.into_iter().enumerate() {
            let pad = if i == last { 0 } else { widths[i].saturating_sub(text.chars().count()) };
            let shown = match t {
                Some(t) => ui.paint(t, &text),
                None => text,
            };
            out.push_str(&shown);
            if i != last {
                out.push_str(&" ".repeat(pad + 2));
            }
        }
        out.push('\n');
        out
    };

    let mut out = line(heads.iter().map(|h| (h.to_string(), Some(Tone::Bold))).collect());
    for row in rows {
        out.push_str(&line(row.clone()));
    }
    out
}

fn risk_id(risk: Risk) -> &'static str {
    match risk {
        Risk::Safe => "safe",
        Risk::Review => "review",
    }
}

fn risk_cell(risk: Risk) -> Cell {
    match risk {
        Risk::Safe => tone("seguro", Tone::Green),
        Risk::Review => tone("revisar", Tone::Amber),
    }
}

fn mode_id(mode: Mode) -> &'static str {
    match mode {
        Mode::Trash => "trash",
        Mode::Permanent => "permanent",
    }
}

fn kind_id(cat: &Category) -> &'static str {
    match cat.target {
        Target::DirContents { .. } => "directory",
        Target::Command { .. } => "command",
    }
}

fn command_line(target: &Target) -> Option<String> {
    match target {
        Target::Command { program, args, .. } => Some(format!("{program} {}", args.join(" "))),
        Target::DirContents { .. } => None,
    }
}

#[derive(Serialize, Clone)]
struct DiskJson {
    total_bytes: u64,
    used_bytes: u64,
    free_bytes: u64,
}

fn disk_json() -> Option<DiskJson> {
    disk_space("/").map(|(total, free)| DiskJson {
        total_bytes: total,
        used_bytes: total.saturating_sub(free),
        free_bytes: free,
    })
}

fn disk_line(d: &DiskJson) -> String {
    format!(
        "Disco: {} usados de {} — {} livres",
        format_bytes(d.used_bytes),
        format_bytes(d.total_bytes),
        format_bytes(d.free_bytes)
    )
}

// ------------------------------------------------------------ seleção

/// Índices (na ordem do catálogo) das categorias citadas em `ids`.
fn lookup_ids(cats: &[Category], ids: &[String]) -> Result<Vec<usize>, CliError> {
    let mut found = HashSet::new();
    for id in ids {
        match cats.iter().position(|c| c.id == id) {
            Some(i) => {
                found.insert(i);
            }
            None => {
                let valid: Vec<&str> = cats.iter().map(|c| c.id).collect();
                return Err(CliError::Usage(format!(
                    "categoria desconhecida: `{id}`. Disponíveis: {}",
                    valid.join(", ")
                )));
            }
        }
    }
    let mut out: Vec<usize> = found.into_iter().collect();
    out.sort_unstable();
    Ok(out)
}

/// Mesma regra da janela: "seguro" vem marcado, "revisar" começa desmarcado.
fn select_categories(
    cats: &[Category],
    only: &[String],
    include_review: bool,
    exclude: &[String],
) -> Result<Vec<usize>, CliError> {
    let excluded = lookup_ids(cats, exclude)?;
    let base: Vec<usize> = if only.is_empty() {
        (0..cats.len())
            .filter(|&i| include_review || cats[i].risk == Risk::Safe)
            .collect()
    } else {
        lookup_ids(cats, only)?
    };
    Ok(base.into_iter().filter(|i| !excluded.contains(i)).collect())
}

fn expand_path(raw: &str) -> PathBuf {
    let home = dirs::home_dir();
    let path = match (raw, &home) {
        ("~", Some(h)) => h.clone(),
        (r, Some(h)) if r.starts_with("~/") => h.join(&r[2..]),
        _ => PathBuf::from(raw),
    };
    if path.is_absolute() {
        path
    } else {
        std::env::current_dir().map(|d| d.join(&path)).unwrap_or(path)
    }
}

/// Varre as categorias em paralelo, como a janela faz.
fn scan_parallel(cats: &[&Category]) -> Vec<ScanResult> {
    thread::scope(|s| {
        let handles: Vec<_> = cats.iter().map(|c| s.spawn(move || scanner::scan(c))).collect();
        handles.into_iter().map(|h| h.join().unwrap_or_default()).collect()
    })
}

/// Texto da coluna de tamanho: nota para categorias de comando, bytes para as demais.
fn size_text(scan: &ScanResult) -> String {
    match &scan.note {
        Some(note) if scan.items.is_some() || scan.unavailable => note.clone(),
        _ => format_bytes(scan.total),
    }
}

// ------------------------------------------------------------- list

#[derive(Serialize)]
struct CategoryJson {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    risk: &'static str,
    selected_by_default: bool,
    kind: &'static str,
}

fn cmd_list(ui: &Ui, a: &JsonArgs) -> Res {
    let cats = categories::all();
    if a.json {
        let rows: Vec<CategoryJson> = cats
            .iter()
            .map(|c| CategoryJson {
                id: c.id,
                name: c.name,
                description: c.description,
                risk: risk_id(c.risk),
                selected_by_default: c.risk == Risk::Safe,
                kind: kind_id(c),
            })
            .collect();
        emit_json(&rows);
        return Ok(0);
    }

    let rows: Vec<Vec<Cell>> = cats
        .iter()
        .map(|c| {
            vec![
                cell(c.id),
                risk_cell(c.risk),
                cell(if c.risk == Risk::Safe { "sim" } else { "não" }),
                cell(match c.target {
                    Target::DirContents { .. } => "pasta",
                    Target::Command { .. } => "comando",
                }),
                cell(c.name),
            ]
        })
        .collect();
    let mut out = table(ui, &["ID", "RISCO", "PADRÃO", "TIPO", "CATEGORIA"], &rows);
    out.push('\n');
    out.push_str(&ui.paint(
        Tone::Dim,
        "PADRÃO = vem marcada no `clean`. As \"revisar\" exigem --only ou --include-review.\n",
    ));
    emit(&out);
    Ok(0)
}

// ------------------------------------------------------------- disk

fn cmd_disk(_ui: &Ui, a: &JsonArgs) -> Res {
    let Some(d) = disk_json() else {
        return Err(CliError::Failed("não foi possível ler o uso do disco".into()));
    };
    if a.json {
        emit_json(&d);
    } else {
        emit(&format!("{}\n", disk_line(&d)));
    }
    Ok(0)
}

// ------------------------------------------------------------- scan

#[derive(Serialize)]
struct EntryJson {
    path: String,
    bytes: u64,
}

#[derive(Serialize)]
struct ScanCategoryJson {
    id: &'static str,
    name: &'static str,
    risk: &'static str,
    kind: &'static str,
    /// `null` em categorias de comando (não têm tamanho em arquivos).
    bytes: Option<u64>,
    /// Itens que o comando afetaria (só em categorias de comando).
    items: Option<usize>,
    note: Option<String>,
    unavailable: bool,
    entry_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    entries: Option<Vec<EntryJson>>,
}

#[derive(Serialize)]
struct ScanJson {
    disk: Option<DiskJson>,
    categories: Vec<ScanCategoryJson>,
    total_bytes: u64,
}

fn entry_json(e: &Entry) -> EntryJson {
    EntryJson { path: e.path.display().to_string(), bytes: e.size }
}

fn cmd_scan(ui: &Ui, a: &ScanArgs) -> Res {
    let cats = categories::all();
    let idx = if a.only.is_empty() {
        (0..cats.len()).collect()
    } else {
        lookup_ids(&cats, &a.only)?
    };
    let picked: Vec<&Category> = idx.iter().map(|&i| &cats[i]).collect();

    ui.info(&format!("Escaneando {} categoria(s)…", picked.len()));
    let scans = scan_parallel(&picked);
    let total: u64 = scans.iter().map(|s| s.total).sum();
    let disk = disk_json();

    if a.json {
        let categories = picked
            .iter()
            .zip(&scans)
            .map(|(c, s)| ScanCategoryJson {
                id: c.id,
                name: c.name,
                risk: risk_id(c.risk),
                kind: kind_id(c),
                bytes: matches!(c.target, Target::DirContents { .. }).then_some(s.total),
                items: s.items,
                note: s.note.clone(),
                unavailable: s.unavailable,
                entry_count: s.entries.len(),
                entries: a
                    .details
                    .then(|| s.entries.iter().take(a.top).map(entry_json).collect()),
            })
            .collect();
        emit_json(&ScanJson { disk, categories, total_bytes: total });
        return Ok(0);
    }

    let mut out = String::new();
    if let Some(d) = &disk {
        let _ = writeln!(out, "{}\n", disk_line(d));
    }
    let rows: Vec<Vec<Cell>> = picked
        .iter()
        .zip(&scans)
        .map(|(c, s)| {
            let size = size_text(s);
            let size = if s.unavailable { tone(size, Tone::Amber) } else { cell(size) };
            vec![cell(c.id), risk_cell(c.risk), size, cell(c.name)]
        })
        .collect();
    out.push_str(&table(ui, &["ID", "RISCO", "TAMANHO", "CATEGORIA"], &rows));
    let _ = writeln!(out, "\n{}", ui.paint(Tone::Bold, &format!("Total em arquivos  {}", format_bytes(total))));

    // Avisos que a janela mostra no cartão (ex.: falta de Acesso Total ao Disco).
    for (c, s) in picked.iter().zip(&scans) {
        if let (Some(note), Target::DirContents { .. }) = (&s.note, &c.target) {
            let _ = writeln!(out, "{}", ui.paint(Tone::Amber, &format!("! {}: {note}", c.name)));
        }
    }

    if a.details {
        for (c, s) in picked.iter().zip(&scans) {
            if s.entries.is_empty() {
                continue;
            }
            let _ = writeln!(
                out,
                "\n{}",
                ui.paint(Tone::Bold, &format!("{} ({}) — {} itens", c.name, c.id, s.entries.len()))
            );
            for e in s.entries.iter().take(a.top) {
                let _ = writeln!(out, "  {:>10}  {}", format_bytes(e.size), display_path(&e.path));
            }
            let hidden = s.entries.len().saturating_sub(a.top);
            if hidden > 0 {
                let _ = writeln!(out, "{}", ui.paint(Tone::Dim, &format!("  … e mais {hidden} itens menores")));
            }
        }
        let _ = writeln!(out, "\n{}", ui.paint(Tone::Dim, "Para abrir um item no Finder: epma-system-data reveal CAMINHO"));
    }
    emit(&out);
    Ok(0)
}

// ------------------------------------------------------------ reveal

fn cmd_reveal(raw: &str) -> Res {
    let path = expand_path(raw);
    if std::fs::symlink_metadata(&path).is_err() {
        return Err(CliError::Failed(format!("não existe: {}", path.display())));
    }
    match Proc::new("open").arg("-R").arg(&path).status() {
        Ok(s) if s.success() => Ok(0),
        Ok(s) => Err(CliError::Failed(format!("`open -R` saiu com {s}"))),
        Err(e) => Err(CliError::Failed(format!("não foi possível rodar `open`: {e}"))),
    }
}

// ------------------------------------------------------------- clean

struct Job<'a> {
    cat: &'a Category,
    scan: ScanResult,
    entries: Vec<Entry>,
}

impl Job<'_> {
    fn bytes(&self) -> u64 {
        self.entries.iter().map(|e| e.size).sum()
    }
}

#[derive(Serialize, Clone)]
struct PlannedJson {
    id: &'static str,
    name: &'static str,
    risk: &'static str,
    kind: &'static str,
    items: usize,
    bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    command: Option<String>,
}

#[derive(Serialize)]
struct FailureJson {
    path: String,
    error: String,
}

#[derive(Serialize)]
struct ResultJson {
    id: &'static str,
    removed: usize,
    freed_bytes: u64,
    failures: Vec<FailureJson>,
    #[serde(skip_serializing_if = "Option::is_none")]
    output: Option<String>,
}

#[derive(Serialize)]
struct SkippedJson {
    id: &'static str,
    reason: String,
}

#[derive(Serialize)]
struct CleanJson {
    dry_run: bool,
    mode: &'static str,
    planned: Vec<PlannedJson>,
    not_cleaned: Vec<SkippedJson>,
    results: Vec<ResultJson>,
    freed_bytes: u64,
    failures: usize,
    disk_before: Option<DiskJson>,
    disk_after: Option<DiskJson>,
}

fn planned_json(j: &Job) -> PlannedJson {
    let is_dir = matches!(j.cat.target, Target::DirContents { .. });
    PlannedJson {
        id: j.cat.id,
        name: j.cat.name,
        risk: risk_id(j.cat.risk),
        kind: kind_id(j.cat),
        items: if is_dir { j.entries.len() } else { j.scan.items.unwrap_or(0) },
        bytes: is_dir.then(|| j.bytes()),
        command: command_line(&j.cat.target),
    }
}

fn why_not(scan: &ScanResult) -> String {
    if scan.unavailable {
        scan.note.clone().unwrap_or_else(|| "indisponível".into())
    } else {
        "nada para limpar".into()
    }
}

fn plan_text(ui: &Ui, mode: Mode, jobs: &[Job], not_cleaned: &[(&Category, String)]) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Plano de limpeza — modo: {}\n", match mode {
        Mode::Trash => "Lixeira",
        Mode::Permanent => "exclusão permanente",
    });

    let rows: Vec<Vec<Cell>> = jobs
        .iter()
        .map(|j| match j.cat.target {
            Target::DirContents { .. } => vec![
                cell(j.cat.id),
                risk_cell(j.cat.risk),
                cell(j.entries.len().to_string()),
                cell(format_bytes(j.bytes())),
                cell(j.cat.name),
            ],
            Target::Command { .. } => vec![
                cell(j.cat.id),
                risk_cell(j.cat.risk),
                cell("comando"),
                cell(size_text(&j.scan)),
                cell(j.cat.name),
            ],
        })
        .collect();
    out.push_str(&table(ui, &["ID", "RISCO", "ITENS", "TAMANHO", "CATEGORIA"], &rows));

    for j in jobs {
        if let Some(cmd) = command_line(&j.cat.target) {
            let _ = writeln!(out, "{}", ui.paint(Tone::Dim, &format!("  {} executa: {cmd}", j.cat.id)));
        }
    }
    for (cat, reason) in not_cleaned {
        let _ = writeln!(out, "{}", ui.paint(Tone::Dim, &format!("  {} fora do plano: {reason}", cat.id)));
    }

    let bytes: u64 = jobs.iter().map(Job::bytes).sum();
    let _ = writeln!(out, "\n{} categoria(s), {} em arquivos.", jobs.len(), format_bytes(bytes));
    let _ = match mode {
        Mode::Trash => writeln!(out, "Os itens vão para a Lixeira. O espaço só é liberado quando você esvaziá-la."),
        Mode::Permanent => writeln!(
            out,
            "{}",
            ui.paint(Tone::Red, "Os itens serão excluídos permanentemente. Não dá para desfazer.")
        ),
    };
    let _ = writeln!(out, "{}", ui.paint(Tone::Dim, "Feche os apps abertos antes de limpar os caches."));
    out
}

fn confirm(mode: Mode) -> bool {
    let prompt = match mode {
        Mode::Trash => "Limpar agora? [s/N] ".to_string(),
        Mode::Permanent => format!("Para excluir de vez, digite \"{PERMANENT_WORD}\": "),
    };
    eprint!("{prompt}");
    let _ = io::stderr().flush();
    let mut answer = String::new();
    if io::stdin().lock().read_line(&mut answer).is_err() {
        return false;
    }
    let answer = answer.trim().to_lowercase();
    match mode {
        Mode::Trash => matches!(answer.as_str(), "s" | "sim" | "y" | "yes"),
        Mode::Permanent => answer == PERMANENT_WORD,
    }
}

fn result_json(cat: &Category, r: &CleanReport) -> ResultJson {
    ResultJson {
        id: cat.id,
        removed: r.removed,
        freed_bytes: r.freed,
        failures: r
            .failures
            .iter()
            .map(|(p, e)| FailureJson { path: p.display().to_string(), error: e.clone() })
            .collect(),
        output: r.output.clone().filter(|o| !o.is_empty()),
    }
}

fn cmd_clean(ui: &Ui, a: &CleanArgs) -> Res {
    let cats = categories::all();
    let selected = select_categories(&cats, &a.only, a.include_review, &a.exclude)?;
    if selected.is_empty() {
        return Err(CliError::Usage("nenhuma categoria selecionada".into()));
    }
    let mode = if a.permanent { Mode::Permanent } else { Mode::Trash };
    let disk_before = disk_json();

    // 1. Medir (a limpeza só age sobre o que acabou de ser medido).
    let picked: Vec<&Category> = selected.iter().map(|&i| &cats[i]).collect();
    ui.info(&format!("Escaneando {} categoria(s)…", picked.len()));
    let scans = scan_parallel(&picked);

    // 2. Itens que o usuário pediu para preservar.
    let skip: HashSet<PathBuf> = a.skip.iter().map(|p| expand_path(p)).collect();
    let known: HashSet<&Path> = scans
        .iter()
        .flat_map(|s| s.entries.iter().map(|e| e.path.as_path()))
        .collect();
    for p in skip.iter().filter(|p| !known.contains(p.as_path())) {
        ui.info(&format!("aviso: --skip {} não corresponde a nenhum item medido", display_path(p)));
    }

    // 3. Montar o plano.
    let mut jobs = Vec::new();
    let mut not_cleaned = Vec::new();
    for (cat, scan) in picked.iter().zip(scans) {
        if plan::has_work(cat, Some(&scan), true, &skip) {
            let entries = plan::chosen_entries(Some(&scan), &skip);
            jobs.push(Job { cat, scan, entries });
        } else {
            let reason = why_not(&scan);
            not_cleaned.push((*cat, reason));
        }
    }

    let planned: Vec<PlannedJson> = jobs.iter().map(planned_json).collect();
    let not_cleaned_json = || {
        not_cleaned
            .iter()
            .map(|(c, r)| SkippedJson { id: c.id, reason: r.clone() })
            .collect::<Vec<_>>()
    };
    let finish_json = |results: Vec<ResultJson>, disk_after| {
        let freed: u64 = results.iter().map(|r| r.freed_bytes).sum();
        let failures = results.iter().map(|r| r.failures.len()).sum();
        emit_json(&CleanJson {
            dry_run: a.dry_run,
            mode: mode_id(mode),
            planned: planned.clone(),
            not_cleaned: not_cleaned_json(),
            results,
            freed_bytes: freed,
            failures,
            disk_before: disk_before.clone(),
            disk_after,
        });
    };

    if jobs.is_empty() {
        if a.json {
            finish_json(vec![], disk_json());
        } else {
            emit(&plan_text_empty(ui, &not_cleaned));
        }
        return Ok(0);
    }

    // 4. Mostrar o plano; ensaio termina aqui.
    if !a.json {
        emit(&plan_text(ui, mode, &jobs, &not_cleaned));
    }
    if a.dry_run {
        if a.json {
            finish_json(vec![], disk_json());
        } else {
            emit(&format!("\n{}\n", ui.paint(Tone::Bold, "Ensaio: nada foi alterado.")));
        }
        return Ok(0);
    }

    // 5. Confirmar (a janela também pede confirmação).
    if !a.yes {
        if !io::stdin().is_terminal() {
            return Err(CliError::Refused(
                "a limpeza precisa de confirmação, mas não há terminal interativo. Use --yes ou --dry-run.".into(),
            ));
        }
        if !confirm(mode) {
            eprintln!("Cancelado. Nada foi alterado.");
            return Ok(0);
        }
    }

    // 6. Limpar, uma categoria por vez, como a janela.
    if !a.json {
        emit("\n");
    }
    let mut results = Vec::new();
    let mut failures = 0;
    let mut removed = 0;
    let mut freed = 0;
    for job in &jobs {
        ui.info(&format!("Limpando {}…", job.cat.name));
        let report = match job.cat.target {
            Target::DirContents { .. } => cleaner::clean_entries(&job.entries, mode),
            Target::Command { .. } => cleaner::run_command(&job.cat.target),
        };
        failures += report.failures.len();
        if matches!(job.cat.target, Target::DirContents { .. }) {
            removed += report.removed;
            freed += report.freed;
        }
        if !a.json {
            let lines = plan::report_lines(job.cat, &report, mode);
            let mut text = String::new();
            for (i, line) in lines.iter().enumerate() {
                let line = if i == 0 && !report.failures.is_empty() {
                    ui.paint(Tone::Red, line)
                } else {
                    line.clone()
                };
                let _ = writeln!(text, "{line}");
            }
            emit(&text);
        }
        results.push(result_json(job.cat, &report));
    }

    // 7. Resumo (a janela atualiza a barra do disco e avisa sobre a Lixeira).
    let disk_after = disk_json();
    if a.json {
        finish_json(results, disk_after);
    } else {
        let mut out = String::new();
        let what = if mode == Mode::Trash { "na Lixeira" } else { "liberados" };
        let _ = writeln!(
            out,
            "\n{}",
            ui.paint(Tone::Bold, &format!("Resumo: {removed} itens, {} {what}, {failures} falha(s).", format_bytes(freed)))
        );
        if let Some(d) = &disk_after {
            let _ = writeln!(out, "{}", disk_line(d));
        }
        if mode == Mode::Trash {
            let _ = writeln!(out, "{}", plan::TRASH_NOTICE);
        }
        emit(&out);
    }
    Ok(if failures > 0 { EXIT_FAILED } else { 0 })
}

fn plan_text_empty(ui: &Ui, not_cleaned: &[(&Category, String)]) -> String {
    let mut out = String::from("Nada para limpar.\n");
    for (cat, reason) in not_cleaned {
        let _ = writeln!(out, "{}", ui.paint(Tone::Dim, &format!("  {}: {reason}", cat.id)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cats() -> Vec<Category> {
        categories::build(Path::new("/Users/teste"))
    }

    fn ids(cats: &[Category], idx: &[usize]) -> Vec<&'static str> {
        idx.iter().map(|&i| cats[i].id).collect()
    }

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn default_selection_is_only_safe_categories() {
        let cats = cats();
        let sel = select_categories(&cats, &[], false, &[]).unwrap();
        assert!(!sel.is_empty());
        assert!(sel.iter().all(|&i| cats[i].risk == Risk::Safe));
    }

    #[test]
    fn include_review_adds_review_categories() {
        let cats = cats();
        let sel = select_categories(&cats, &[], true, &[]).unwrap();
        assert_eq!(sel.len(), cats.len());
    }

    #[test]
    fn only_picks_exactly_those_even_review() {
        let cats = cats();
        let sel = select_categories(&cats, &s(&["ios_backups", "user_logs"]), false, &[]).unwrap();
        assert_eq!(ids(&cats, &sel), ["user_logs", "ios_backups"]);
    }

    #[test]
    fn exclude_removes_from_selection() {
        let cats = cats();
        let sel = select_categories(&cats, &[], false, &s(&["user_caches"])).unwrap();
        assert!(!ids(&cats, &sel).contains(&"user_caches"));
    }

    #[test]
    fn unknown_id_is_a_usage_error_listing_valid_ids() {
        let cats = cats();
        let err = select_categories(&cats, &s(&["nao_existe"]), false, &[]).unwrap_err();
        match err {
            CliError::Usage(m) => assert!(m.contains("nao_existe") && m.contains("user_caches")),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn expand_path_handles_tilde_and_relative() {
        let home = dirs::home_dir().unwrap();
        assert_eq!(expand_path("~"), home);
        assert_eq!(expand_path("~/Library/x"), home.join("Library/x"));
        assert_eq!(expand_path("/abs/y"), PathBuf::from("/abs/y"));
        assert!(expand_path("rel/z").is_absolute());
    }

    #[test]
    fn table_aligns_columns_without_trailing_padding() {
        let ui = Ui { color: false, quiet: true };
        let rows = vec![vec![cell("a"), cell("xx")], vec![cell("bbb"), cell("y")]];
        let out = table(&ui, &["ID", "NOME"], &rows);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "ID   NOME");
        assert_eq!(lines[1], "a    xx");
        assert_eq!(lines[2], "bbb  y");
    }

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }
}
