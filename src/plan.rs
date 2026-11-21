//! Lógica de seleção e relatório compartilhada pela interface gráfica e pelo CLI,
//! para que os dois se comportem de forma idêntica.

use std::collections::HashSet;
use std::path::PathBuf;

use crate::categories::{Category, Target};
use crate::cleaner::{CleanReport, Mode};
use crate::scanner::{Entry, ScanResult};
use crate::util::{display_path, format_bytes};

/// Aviso exibido depois de limpar para a Lixeira.
pub const TRASH_NOTICE: &str =
    "Itens movidos para a Lixeira. Esvazie a Lixeira para liberar o espaço.";

/// Itens de primeiro nível que serão removidos, descontados os que o usuário desmarcou.
pub fn chosen_entries(scan: Option<&ScanResult>, deselected: &HashSet<PathBuf>) -> Vec<Entry> {
    scan.iter()
        .flat_map(|s| &s.entries)
        .filter(|e| !deselected.contains(&e.path))
        .cloned()
        .collect()
}

/// A categoria tem algo selecionado que de fato será limpo?
pub fn has_work(
    cat: &Category,
    scan: Option<&ScanResult>,
    selected: bool,
    deselected: &HashSet<PathBuf>,
) -> bool {
    let available = scan.is_none_or(|s| !s.unavailable);
    if !selected || !available {
        return false;
    }
    match &cat.target {
        Target::DirContents { .. } => !chosen_entries(scan, deselected).is_empty(),
        Target::Command { .. } => scan.and_then(|s| s.items).unwrap_or(0) > 0,
    }
}

/// Traduz o erro bruto do sistema para algo que o usuário entende.
pub fn friendly_error(err: &str) -> String {
    if err.contains("permission") {
        "sem permissão (protegido pelo macOS)".into()
    } else {
        err.chars().take(120).collect()
    }
}

/// Linhas de histórico de uma categoria limpa (mesmo texto na GUI e no CLI).
pub fn report_lines(cat: &Category, report: &CleanReport, mode: Mode) -> Vec<String> {
    let name = cat.name;
    let mut head = match cat.target {
        Target::Command { .. } if report.failures.is_empty() => format!("{name}: concluído"),
        Target::Command { .. } => format!("{name}: falhou"),
        Target::DirContents { .. } => format!(
            "{name}: {} itens, {} {}",
            report.removed,
            format_bytes(report.freed),
            if mode == Mode::Trash { "na Lixeira" } else { "liberados" },
        ),
    };
    if !report.failures.is_empty() {
        head.push_str(&format!(" — {} falha(s)", report.failures.len()));
    }

    let mut lines = vec![head];
    for (path, err) in report.failures.iter().take(5) {
        lines.push(format!("    {}: {}", display_path(path), friendly_error(err)));
    }
    if let Some(out) = report.output.as_deref().filter(|o| !o.is_empty()) {
        lines.extend(out.lines().take(5).map(|l| format!("    {l}")));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::categories::{self, Risk};

    fn entry(path: &str, size: u64) -> Entry {
        Entry { path: PathBuf::from(path), size }
    }

    fn scan_with(entries: Vec<Entry>) -> ScanResult {
        ScanResult { total: entries.iter().map(|e| e.size).sum(), entries, ..Default::default() }
    }

    fn cat(id: &str) -> Category {
        categories::build(std::path::Path::new("/Users/teste"))
            .into_iter()
            .find(|c| c.id == id)
            .unwrap()
    }

    #[test]
    fn chosen_entries_skips_deselected() {
        let scan = scan_with(vec![entry("/a", 3), entry("/b", 2)]);
        let off: HashSet<PathBuf> = [PathBuf::from("/a")].into();
        let chosen = chosen_entries(Some(&scan), &off);
        assert_eq!(chosen.len(), 1);
        assert_eq!(chosen[0].path, PathBuf::from("/b"));
    }

    #[test]
    fn has_work_rules() {
        let none = HashSet::new();
        let dirs = cat("user_caches");
        let filled = scan_with(vec![entry("/a", 1)]);
        assert!(has_work(&dirs, Some(&filled), true, &none));
        assert!(!has_work(&dirs, Some(&filled), false, &none));
        assert!(!has_work(&dirs, Some(&scan_with(vec![])), true, &none));
        let all_off: HashSet<PathBuf> = [PathBuf::from("/a")].into();
        assert!(!has_work(&dirs, Some(&filled), true, &all_off));
        let unavailable = ScanResult { unavailable: true, ..filled.clone() };
        assert!(!has_work(&dirs, Some(&unavailable), true, &none));

        let cmd = cat("tm_snapshots");
        assert_eq!(cmd.risk, Risk::Safe);
        let two = ScanResult { items: Some(2), ..Default::default() };
        let zero = ScanResult { items: Some(0), ..Default::default() };
        assert!(has_work(&cmd, Some(&two), true, &none));
        assert!(!has_work(&cmd, Some(&zero), true, &none));
    }

    #[test]
    fn report_lines_summarize_and_translate_errors() {
        let report = CleanReport {
            removed: 2,
            freed: 1_500,
            failures: vec![(PathBuf::from("/x/y"), "Operation not permitted: permission".into())],
            output: None,
        };
        let lines = report_lines(&cat("user_caches"), &report, Mode::Trash);
        assert_eq!(lines[0], "Caches de apps: 2 itens, 1,5 KB na Lixeira — 1 falha(s)");
        assert!(lines[1].ends_with("sem permissão (protegido pelo macOS)"));
    }
}
