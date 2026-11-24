//! Medição do espaço ocupado por cada categoria.

use std::collections::HashSet;
use std::fs;
use std::io::ErrorKind;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use walkdir::WalkDir;

use crate::categories::{Category, Probe, Target};
use crate::util::real_dir_inside_home;

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
    /// O macOS negou a leitura (falta Acesso Total ao Disco).
    pub denied: bool,
}

pub fn scan(cat: &Category) -> ScanResult {
    match &cat.target {
        Target::DirContents {
            dirs,
            exclude,
            paths,
        } => scan_targets(dirs, exclude, paths, dirs::home_dir().as_deref()),
        Target::Command { probe, .. } => run_probe(probe),
    }
}

pub fn scan_dirs(dirs: &[PathBuf], exclude: &[&str]) -> ScanResult {
    scan_dirs_within(dirs, exclude, None)
}

/// Com `home`, ignora (e avisa) diretórios que sejam links simbólicos para fora dela:
/// a limpeza os recusaria de qualquer jeito, e medi-los mostraria um número enganoso.
pub fn scan_dirs_within(dirs: &[PathBuf], exclude: &[&str], home: Option<&Path>) -> ScanResult {
    scan_targets(dirs, exclude, &[], home)
}

/// Como `scan_dirs_within`, e ainda mede cada caminho de `paths` que existir como um item
/// inteiro. Um caminho dentro de pasta protegida (TCC) conta como negado, não como ausente.
pub fn scan_targets(
    dirs: &[PathBuf],
    exclude: &[&str],
    paths: &[PathBuf],
    home: Option<&Path>,
) -> ScanResult {
    let mut res = ScanResult::default();
    let mut denied = false;
    let mut notes: Vec<String> = Vec::new();

    for dir in dirs {
        if let Some(home) = home {
            if fs::canonicalize(dir).is_ok() && !real_dir_inside_home(dir, home) {
                res.errors += 1;
                notes.push(format!(
                    "{} aponta (link simbólico) para fora da sua pasta de usuário e foi ignorada.",
                    crate::util::display_path(dir)
                ));
                continue;
            }
        }
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

    for path in paths {
        if let Some(home) = home {
            let parent_ok = path.parent().is_some_and(|p| real_dir_inside_home(p, home));
            if fs::canonicalize(path.parent().unwrap_or(path)).is_ok() && !parent_ok {
                res.errors += 1;
                notes.push(format!(
                    "{} fica fora da sua pasta de usuário (link simbólico) e foi ignorado.",
                    crate::util::display_path(path)
                ));
                continue;
            }
        }
        match fs::symlink_metadata(path) {
            Ok(_) => {
                if res.entries.iter().any(|e| &e.path == path) {
                    continue;
                }
                let size = dir_size(path, &mut res.errors);
                res.entries.push(Entry {
                    path: path.clone(),
                    size,
                });
            }
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => {
                denied |= e.kind() == ErrorKind::PermissionDenied;
                res.errors += 1;
            }
        }
    }

    res.entries.sort_by_key(|e| std::cmp::Reverse(e.size));
    res.total = res.entries.iter().map(|e| e.size).sum();
    res.denied = denied;
    if denied {
        notes.push(
            "Sem permissão de leitura. Conceda Acesso Total ao Disco em Ajustes > Privacidade e Segurança."
                .into(),
        );
    }
    if !notes.is_empty() {
        res.note = Some(notes.join(" "));
    }
    res
}

/// Soma o tamanho em disco de um arquivo ou árvore, sem seguir links simbólicos.
///
/// Arquivo com vários hard links (comum em `node_modules` de pnpm e em caches do Xcode)
/// conta uma vez só por árvore. Clones do APFS (`clonefile`) não aparecem no `stat` e
/// continuam somando por inteiro: o número pode ficar acima do espaço realmente liberado.
pub fn dir_size(path: &Path, errors: &mut usize) -> u64 {
    let mut total = 0;
    let mut seen_links: HashSet<(u64, u64)> = HashSet::new();
    for entry in WalkDir::new(path).follow_links(false) {
        match entry.and_then(|e| e.metadata()) {
            Ok(meta) => {
                if meta.nlink() > 1
                    && !meta.is_dir()
                    && !seen_links.insert((meta.dev(), meta.ino()))
                {
                    continue;
                }
                total += meta.blocks() * 512;
            }
            Err(_) => *errors += 1,
        }
    }
    total
}

fn run_probe(probe: &Probe) -> ScanResult {
    let mut res = ScanResult::default();
    match Command::new(probe.program)
        .args(probe.args)
        .env("PATH", crate::util::tool_path())
        .output()
    {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout);
            // `count_lines_containing` vazio conta toda linha não vazia (ex.: um ID por linha).
            let n = text
                .lines()
                .filter(|l| !l.trim().is_empty() && l.contains(probe.count_lines_containing))
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
    fn hard_links_are_counted_once_per_tree() {
        let tmp = tempfile::tempdir().unwrap();
        let single = tmp.path().join("single");
        let linked = tmp.path().join("linked");
        fs::create_dir_all(&single).unwrap();
        fs::create_dir_all(&linked).unwrap();
        fs::write(single.join("big.bin"), vec![1u8; 256 * 1024]).unwrap();
        fs::write(linked.join("big.bin"), vec![1u8; 256 * 1024]).unwrap();
        fs::hard_link(linked.join("big.bin"), linked.join("copia.bin")).unwrap();
        fs::hard_link(linked.join("big.bin"), linked.join("outra.bin")).unwrap();

        let mut errors = 0;
        let one = dir_size(&single, &mut errors);
        let three = dir_size(&linked, &mut errors);
        assert_eq!(errors, 0);
        // Três nomes para o mesmo arquivo: o mesmo espaço de um só (sobra só o diretório).
        assert!(
            three < one + 64 * 1024,
            "{three} vs {one}: hard links foram somados"
        );
    }

    #[test]
    fn symlinked_category_dir_outside_home_is_ignored_with_a_note() {
        let home = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::create_dir_all(outside.path().join("alvo/sub")).unwrap();
        fs::write(outside.path().join("alvo/sub/a.bin"), vec![1u8; 4096]).unwrap();
        fs::create_dir_all(home.path().join("Library")).unwrap();
        let caches = home.path().join("Library/Caches");
        std::os::unix::fs::symlink(outside.path().join("alvo"), &caches).unwrap();

        let res = scan_dirs_within(std::slice::from_ref(&caches), &[], Some(home.path()));
        assert!(res.entries.is_empty());
        assert_eq!(res.total, 0);
        assert!(res.note.as_deref().unwrap().contains("link simbólico"));

        // Sem `home` (uso interno e testes) o comportamento antigo continua.
        assert_eq!(scan_dirs_within(&[caches], &[], None).entries.len(), 1);
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
