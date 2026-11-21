//! Remoção dos itens selecionados: para a Lixeira ou definitiva.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;
#[cfg(target_os = "macos")]
use trash::macos::{DeleteMethod, TrashContextExtMacos};
use trash::TrashContext;

use crate::categories::Target;
use crate::scanner::Entry;
use crate::util::real_dir_inside_home;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// Move para a Lixeira: reversível, o espaço só volta ao esvaziá-la.
    Trash,
    /// Apaga de vez.
    Permanent,
}

/// Por que um item não foi removido. Vem do tipo do erro do sistema, nunca do texto dele,
/// que muda com o idioma e a versão do macOS.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    PermissionDenied,
    NotFound,
    /// A trava de segurança recusou: o caminho (ou o seu destino real) sai da pasta do usuário.
    OutsideHome,
    Other,
}

#[derive(Clone, Debug)]
pub struct Failure {
    pub path: PathBuf,
    pub kind: FailureKind,
    /// Mensagem original do sistema, para diagnóstico.
    pub message: String,
}

impl Failure {
    pub fn new(path: impl Into<PathBuf>, kind: FailureKind, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            kind,
            message: message.into(),
        }
    }

    pub fn from_io(path: &Path, e: &io::Error) -> Self {
        Self::new(path, classify_io(e), e.to_string())
    }

    pub fn from_trash(path: &Path, e: &trash::Error) -> Self {
        let kind = match e {
            trash::Error::Os { code, .. } if is_permission_code(*code) => {
                FailureKind::PermissionDenied
            }
            // "o alvo não existe ou falta permissão": o sistema de arquivos desempata.
            trash::Error::CouldNotAccess { .. } if fs::symlink_metadata(path).is_ok() => {
                FailureKind::PermissionDenied
            }
            trash::Error::CouldNotAccess { .. } => FailureKind::NotFound,
            other => classify_text(&other.to_string()),
        };
        Self::new(path, kind, e.to_string())
    }

    pub fn outside_home(path: &Path, why: &str) -> Self {
        Self::new(path, FailureKind::OutsideHome, why)
    }

    /// Texto curto para o usuário.
    pub fn friendly(&self) -> String {
        match self.kind {
            FailureKind::PermissionDenied if cfg!(target_os = "macos") => {
                "sem permissão (protegido pelo macOS)".into()
            }
            FailureKind::PermissionDenied => "sem permissão".into(),
            FailureKind::NotFound => "já não existe".into(),
            FailureKind::OutsideHome => "fora da área permitida".into(),
            FailureKind::Other => self.message.chars().take(120).collect(),
        }
    }
}

/// EPERM (1) e EACCES (13): "Operation not permitted" e "Permission denied".
fn is_permission_code(code: i32) -> bool {
    code == libc::EPERM || code == libc::EACCES
}

pub fn classify_io(e: &io::Error) -> FailureKind {
    match e.kind() {
        io::ErrorKind::PermissionDenied => FailureKind::PermissionDenied,
        io::ErrorKind::NotFound => FailureKind::NotFound,
        _ if e.raw_os_error().is_some_and(is_permission_code) => FailureKind::PermissionDenied,
        _ => FailureKind::Other,
    }
}

/// Último recurso, só para erros que chegam como texto (ex.: NSFileManager).
pub fn classify_text(text: &str) -> FailureKind {
    let t = text.to_lowercase();
    if t.contains("permission") || t.contains("not permitted") {
        FailureKind::PermissionDenied
    } else if t.contains("no such file") || t.contains("not found") {
        FailureKind::NotFound
    } else {
        FailureKind::Other
    }
}

#[derive(Clone, Debug, Default)]
pub struct CleanReport {
    pub removed: usize,
    pub freed: u64,
    pub failures: Vec<Failure>,
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
                .map(|e| {
                    Failure::new(
                        &e.path,
                        FailureKind::Other,
                        "pasta do usuário não encontrada",
                    )
                })
                .collect(),
            ..Default::default()
        },
    }
}

pub fn clean_entries_within(entries: &[Entry], mode: Mode, home: &Path) -> CleanReport {
    let mut report = CleanReport::default();
    #[allow(unused_mut)] // só o macOS ajusta o método de remoção
    let mut ctx = TrashContext::default();
    // NSFileManager evita o prompt de automação do Finder.
    #[cfg(target_os = "macos")]
    ctx.set_delete_method(DeleteMethod::NsFileManager);

    for entry in entries {
        if let Some(refusal) = refusal(&entry.path, home) {
            report.failures.push(refusal);
            continue;
        }
        let result = match mode {
            Mode::Trash => ctx
                .delete(&entry.path)
                .map_err(|e| Failure::from_trash(&entry.path, &e)),
            Mode::Permanent => {
                remove_permanently(&entry.path).map_err(|e| Failure::from_io(&entry.path, &e))
            }
        };
        match result {
            Ok(()) => {
                report.removed += 1;
                report.freed += entry.size;
            }
            Err(failure) => report.failures.push(failure),
        }
    }
    report
}

/// Duas travas: o texto do caminho (`is_safe_target`) e o caminho real do diretório que
/// contém o item. Só a segunda pega um `~/Library/Caches` que seja link para fora da home.
fn refusal(path: &Path, home: &Path) -> Option<Failure> {
    if !is_safe_target(path, home) {
        return Some(Failure::outside_home(path, "fora da área permitida"));
    }
    // O próprio item pode ser um link (é removido como link, sem seguir o destino);
    // o que não pode é o diretório que o contém levar para fora da home.
    let parent_ok = path.parent().is_some_and(|p| real_dir_inside_home(p, home));
    if !parent_ok {
        return Some(match fs::symlink_metadata(path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Failure::from_io(path, &e),
            _ => Failure::outside_home(
                path,
                "o diretório real fica fora da sua pasta de usuário (link simbólico?)",
            ),
        });
    }
    None
}

/// Só aceita caminhos pelo menos dois níveis abaixo de `home`
/// (ex.: `~/Library/Caches/app`), nunca a própria `home` nem `~/Library`.
/// Checa só o texto do caminho; veja `refusal` para a checagem do caminho real.
pub fn is_safe_target(path: &Path, home: &Path) -> bool {
    if !path.is_absolute() || path.components().any(|c| c.as_os_str() == "..") {
        return false;
    }
    match path.strip_prefix(home) {
        Ok(rest) => rest.components().count() >= 2,
        Err(_) => false,
    }
}

fn remove_permanently(path: &Path) -> io::Result<()> {
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
                report.failures.push(Failure::new(
                    *program,
                    FailureKind::Other,
                    format!("saiu com {}", out.status),
                ));
            }
            report.output = Some(text);
        }
        Err(e) => report
            .failures
            .push(Failure::new(*program, FailureKind::Other, e.to_string())),
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_target_requires_depth_under_home() {
        let home = Path::new("/Users/teste");
        assert!(is_safe_target(
            Path::new("/Users/teste/Library/Caches/app"),
            home
        ));
        assert!(is_safe_target(Path::new("/Users/teste/.cache/pip"), home));
        assert!(!is_safe_target(Path::new("/Users/teste/Library"), home));
        assert!(!is_safe_target(Path::new("/Users/teste"), home));
        assert!(!is_safe_target(Path::new("/System/Library/Caches/x"), home));
        assert!(!is_safe_target(
            Path::new("/Users/teste/Library/../../etc"),
            home
        ));
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
            .map(|n| Entry {
                path: caches.join(n),
                size: 10,
            })
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
            &[Entry {
                path: victim.clone(),
                size: 1,
            }],
            Mode::Permanent,
            home.path(),
        );
        assert_eq!(report.removed, 0);
        assert_eq!(report.failures.len(), 1);
        assert_eq!(report.failures[0].kind, FailureKind::OutsideHome);
        assert!(victim.exists());
    }

    /// Regressão: `~/Library/Caches` virando link para fora da home passava na checagem
    /// de texto e o conteúdo de fora era apagado.
    #[test]
    fn refuses_items_inside_a_directory_symlinked_outside_home() {
        let home = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::create_dir_all(outside.path().join("alvo/sub")).unwrap();
        fs::write(outside.path().join("alvo/sub/arquivo.txt"), b"nao apagar").unwrap();
        fs::create_dir_all(home.path().join("Library")).unwrap();
        // Lexicamente ~/Library/Caches/sub é seguro; na prática é /fora/alvo/sub.
        std::os::unix::fs::symlink(
            outside.path().join("alvo"),
            home.path().join("Library/Caches"),
        )
        .unwrap();

        let item = home.path().join("Library/Caches/sub");
        assert!(
            is_safe_target(&item, home.path()),
            "o texto do caminho parece seguro"
        );

        for mode in [Mode::Permanent, Mode::Trash] {
            let report = clean_entries_within(
                &[Entry {
                    path: item.clone(),
                    size: 1,
                }],
                mode,
                home.path(),
            );
            assert_eq!(report.removed, 0, "{mode:?}");
            assert_eq!(
                report.failures[0].kind,
                FailureKind::OutsideHome,
                "{mode:?}"
            );
        }
        assert!(outside.path().join("alvo/sub/arquivo.txt").exists());
    }

    #[test]
    fn a_symlink_to_a_directory_inside_home_is_still_cleaned() {
        let home = tempfile::tempdir().unwrap();
        fs::create_dir_all(home.path().join("dados/cache/app")).unwrap();
        fs::write(home.path().join("dados/cache/app/f"), b"x").unwrap();
        fs::create_dir_all(home.path().join("Library")).unwrap();
        std::os::unix::fs::symlink(
            home.path().join("dados/cache"),
            home.path().join("Library/Caches"),
        )
        .unwrap();

        let item = home.path().join("Library/Caches/app");
        let report = clean_entries_within(
            &[Entry {
                path: item,
                size: 1,
            }],
            Mode::Permanent,
            home.path(),
        );
        assert!(report.failures.is_empty(), "{:?}", report.failures);
        assert!(!home.path().join("dados/cache/app").exists());
    }

    /// Regressão: a tradução procurava "permission" em minúsculo, e o macOS escreve
    /// "Permission denied" e "Operation not permitted".
    #[test]
    fn classifies_real_os_errors_regardless_of_message_text() {
        for code in [libc::EACCES, libc::EPERM] {
            let e = io::Error::from_raw_os_error(code);
            let f = Failure::from_io(Path::new("/x"), &e);
            assert_eq!(f.kind, FailureKind::PermissionDenied, "{e}");
            assert!(f.friendly().starts_with("sem permissão"), "{e}");
        }
        let missing = io::Error::from_raw_os_error(libc::ENOENT);
        assert_eq!(
            Failure::from_io(Path::new("/x"), &missing).kind,
            FailureKind::NotFound
        );
        let other = io::Error::from_raw_os_error(libc::EIO);
        assert_eq!(
            Failure::from_io(Path::new("/x"), &other).kind,
            FailureKind::Other
        );
    }

    #[test]
    fn classifies_text_only_errors_case_insensitively() {
        assert_eq!(
            classify_text("Permission denied"),
            FailureKind::PermissionDenied
        );
        assert_eq!(
            classify_text("Operation not permitted"),
            FailureKind::PermissionDenied
        );
        assert_eq!(
            classify_text("You don’t have permission to access it."),
            FailureKind::PermissionDenied
        );
        assert_eq!(
            classify_text("No such file or directory"),
            FailureKind::NotFound
        );
        assert_eq!(classify_text("disk on fire"), FailureKind::Other);
    }

    #[test]
    fn trash_errors_are_classified_by_code_not_text() {
        let os = trash::Error::Os {
            code: libc::EPERM,
            description: "qualquer coisa".into(),
        };
        assert_eq!(
            Failure::from_trash(Path::new("/x"), &os).kind,
            FailureKind::PermissionDenied
        );
        let blocked = trash::Error::CouldNotAccess {
            target: "/nao/existe/mesmo".into(),
        };
        assert_eq!(
            Failure::from_trash(Path::new("/nao/existe/mesmo"), &blocked).kind,
            FailureKind::NotFound
        );
    }
}
