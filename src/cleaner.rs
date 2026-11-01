//! Remoção dos itens selecionados: para a Lixeira ou definitiva.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use trash::macos::{DeleteMethod, TrashContextExtMacos};
use trash::TrashContext;

use crate::categories::Target;
use crate::scanner::Entry;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// Move para a Lixeira: reversível, o espaço só volta ao esvaziá-la.
    Trash,
    /// Apaga de vez.
    Permanent,
}

#[derive(Clone, Debug, Default)]
pub struct CleanReport {
    pub removed: usize,
    pub freed: u64,
    pub failures: Vec<(PathBuf, String)>,
    /// Saída de categorias de comando.
    pub output: Option<String>,
}

/// Remove `entries`, recusando qualquer caminho fora da pasta do usuário.
pub fn clean_entries(entries: &[Entry], mode: Mode) -> CleanReport {
    match dirs::home_dir() {
        Some(home) => clean_entries_within(entries, mode, &home),
        None => CleanReport {
            failures: entries
                .iter()
                .map(|e| (e.path.clone(), "pasta do usuário não encontrada".into()))
                .collect(),
            ..Default::default()
        },
    }
}

pub fn clean_entries_within(entries: &[Entry], mode: Mode, home: &Path) -> CleanReport {
    let mut report = CleanReport::default();
    let mut ctx = TrashContext::default();
    // NSFileManager evita o prompt de automação do Finder.
    ctx.set_delete_method(DeleteMethod::NsFileManager);

    for entry in entries {
        if !is_safe_target(&entry.path, home) {
            report
                .failures
                .push((entry.path.clone(), "fora da área permitida".into()));
            continue;
        }
        let result = match mode {
            Mode::Trash => ctx.delete(&entry.path).map_err(|e| e.to_string()),
            Mode::Permanent => remove_permanently(&entry.path).map_err(|e| e.to_string()),
        };
        match result {
            Ok(()) => {
                report.removed += 1;
                report.freed += entry.size;
            }
            Err(e) => report.failures.push((entry.path.clone(), e)),
        }
    }
    report
}

/// Só aceita caminhos pelo menos dois níveis abaixo de `home`
/// (ex.: `~/Library/Caches/app`), nunca a própria `home` nem `~/Library`.
pub fn is_safe_target(path: &Path, home: &Path) -> bool {
    if !path.is_absolute() || path.components().any(|c| c.as_os_str() == "..") {
        return false;
    }
    match path.strip_prefix(home) {
        Ok(rest) => rest.components().count() >= 2,
        Err(_) => false,
    }
}

fn remove_permanently(path: &Path) -> std::io::Result<()> {
    // symlink_metadata: um link é removido como link, sem seguir o destino.
    let meta = fs::symlink_metadata(path)?;
    if meta.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

pub fn run_command(target: &Target) -> CleanReport {
    let Target::Command { program, args, .. } = target else {
        return CleanReport::default();
    };
    let mut report = CleanReport::default();
    match Command::new(program).args(*args).output() {
        Ok(out) => {
            let mut text = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let err = String::from_utf8_lossy(&out.stderr);
            if !err.trim().is_empty() {
                text.push('\n');
                text.push_str(err.trim());
            }
            if out.status.success() {
                report.removed = 1;
            } else {
                report
                    .failures
                    .push((PathBuf::from(program), format!("saiu com {}", out.status)));
            }
            report.output = Some(text);
        }
        Err(e) => report.failures.push((PathBuf::from(program), e.to_string())),
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_target_requires_depth_under_home() {
        let home = Path::new("/Users/teste");
        assert!(is_safe_target(Path::new("/Users/teste/Library/Caches/app"), home));
        assert!(is_safe_target(Path::new("/Users/teste/.cache/pip"), home));
        assert!(!is_safe_target(Path::new("/Users/teste/Library"), home));
        assert!(!is_safe_target(Path::new("/Users/teste"), home));
        assert!(!is_safe_target(Path::new("/System/Library/Caches/x"), home));
        assert!(!is_safe_target(Path::new("/Users/teste/Library/../../etc"), home));
        assert!(!is_safe_target(Path::new("Library/Caches/app"), home));
    }

    #[test]
    fn permanent_clean_removes_dirs_files_and_links_only() {
        let home = tempfile::tempdir().unwrap();
        let caches = home.path().join("Library/Caches");
        fs::create_dir_all(caches.join("app/sub")).unwrap();
        fs::write(caches.join("app/sub/f"), b"data").unwrap();
        fs::write(caches.join("file.tmp"), b"data").unwrap();

        // Um link para fora não pode levar o destino junto.
        let outside = home.path().join("Documentos");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("importante.txt"), b"nao apagar").unwrap();
        std::os::unix::fs::symlink(&outside, caches.join("link")).unwrap();

        let entries: Vec<Entry> = ["app", "file.tmp", "link"]
            .iter()
            .map(|n| Entry { path: caches.join(n), size: 10 })
            .collect();
        let report = clean_entries_within(&entries, Mode::Permanent, home.path());

        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert_eq!(report.removed, 3);
        assert_eq!(report.freed, 30);
        assert!(caches.exists());
        assert_eq!(fs::read_dir(&caches).unwrap().count(), 0);
        assert!(outside.join("importante.txt").exists());
    }

    #[test]
    fn refuses_paths_outside_home() {
        let home = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let victim = other.path().join("a/b");
        fs::create_dir_all(&victim).unwrap();

        let report = clean_entries_within(
            &[Entry { path: victim.clone(), size: 1 }],
            Mode::Permanent,
            home.path(),
        );
        assert_eq!(report.removed, 0);
        assert_eq!(report.failures.len(), 1);
        assert!(victim.exists());
    }
}
