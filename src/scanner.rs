//! Medição do espaço ocupado por cada categoria.

use std::fs;
use std::io::ErrorKind;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use walkdir::WalkDir;

use crate::categories::{Category, Probe, Target};

#[derive(Clone, Debug)]
pub struct Entry {
    pub path: PathBuf,
    /// Tamanho real em disco (blocos alocados), em bytes.
    pub size: u64,
}

#[derive(Clone, Debug, Default)]
pub struct ScanResult {
    pub total: u64,
    /// Itens de primeiro nível, do maior para o menor.
    pub entries: Vec<Entry>,
    /// Para categorias de comando: quantos itens o comando afetaria.
    pub items: Option<usize>,
    pub errors: usize,
    pub note: Option<String>,
    /// A ferramenta necessária não existe nesta máquina.
    pub unavailable: bool,
}

pub fn scan(cat: &Category) -> ScanResult {
    match &cat.target {
        Target::DirContents { dirs, exclude } => scan_dirs(dirs, exclude),
        Target::Command { probe, .. } => run_probe(probe),
    }
}

pub fn scan_dirs(dirs: &[PathBuf], exclude: &[&str]) -> ScanResult {
    let mut res = ScanResult::default();
    let mut denied = false;

    for dir in dirs {
        let read = match fs::read_dir(dir) {
            Ok(read) => read,
            Err(e) if e.kind() == ErrorKind::NotFound => continue,
            Err(e) => {
                denied |= e.kind() == ErrorKind::PermissionDenied;
                res.errors += 1;
                continue;
            }
        };
        for child in read.flatten() {
            let name = child.file_name();
            if exclude.iter().any(|x| name == *x) {
                continue;
            }
            let path = child.path();
            let size = dir_size(&path, &mut res.errors);
            res.entries.push(Entry { path, size });
        }
    }

    res.entries.sort_by_key(|e| std::cmp::Reverse(e.size));
    res.total = res.entries.iter().map(|e| e.size).sum();
    if denied {
        res.note = Some(
            "Sem permissão de leitura. Conceda Acesso Total ao Disco em Ajustes > Privacidade e Segurança."
                .into(),
        );
    }
    res
}

/// Soma o tamanho em disco de um arquivo ou árvore, sem seguir links simbólicos.
pub fn dir_size(path: &Path, errors: &mut usize) -> u64 {
    let mut total = 0;
    for entry in WalkDir::new(path).follow_links(false) {
        match entry.and_then(|e| e.metadata()) {
            Ok(meta) => total += meta.blocks() * 512,
            Err(_) => *errors += 1,
        }
    }
    total
}

fn run_probe(probe: &Probe) -> ScanResult {
    let mut res = ScanResult::default();
    match Command::new(probe.program).args(probe.args).output() {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout);
            let n = text
                .lines()
                .filter(|l| l.contains(probe.count_lines_containing))
                .count();
            res.items = Some(n);
            res.note = Some(if n == 0 {
                "Nada para limpar".into()
            } else {
                format!("{n} {}", probe.unit)
            });
        }
        Ok(out) => {
            res.unavailable = true;
            let err = String::from_utf8_lossy(&out.stderr);
            res.note = Some(format!(
                "Indisponível: {}",
                err.lines().next().unwrap_or("o comando falhou").trim()
            ));
        }
        Err(_) => {
            res.unavailable = true;
            res.note = Some(format!("Indisponível: `{}` não encontrado", probe.program));
        }
    }
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_dirs_measures_children_and_honors_exclusions() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("Caches");
        fs::create_dir_all(root.join("big/nested")).unwrap();
        fs::write(root.join("big/nested/a.bin"), vec![1u8; 64 * 1024]).unwrap();
        fs::write(root.join("small.txt"), b"oi").unwrap();
        fs::create_dir_all(root.join("CloudKit")).unwrap();
        fs::write(root.join("CloudKit/keep.db"), vec![1u8; 4096]).unwrap();

        let res = scan_dirs(&[root.clone(), tmp.path().join("missing")], &["CloudKit"]);

        assert_eq!(res.entries.len(), 2);
        assert_eq!(res.entries[0].path, root.join("big"));
        assert!(res.entries[0].size >= 64 * 1024);
        assert_eq!(res.total, res.entries.iter().map(|e| e.size).sum::<u64>());
        assert!(res.note.is_none());
    }

    #[test]
    fn missing_probe_program_is_unavailable() {
        let probe = Probe {
            program: "epma-programa-inexistente",
            args: &[],
            count_lines_containing: "x",
            unit: "itens",
        };
        let res = run_probe(&probe);
        assert!(res.unavailable);
        assert!(res.items.is_none());
    }
}
