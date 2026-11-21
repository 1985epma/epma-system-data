//! Trava entre processos: impede duas limpezas ao mesmo tempo (janela, terminal, agendamento).
//!
//! Usa `flock` num arquivo de trava. O sistema libera a trava sozinho quando o processo
//! termina, mesmo que ele morra no meio, então não existe "trava velha" para limpar à mão.

use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};

/// Enquanto este valor existir, a trava está segurada. Soltá-lo (drop) libera.
#[derive(Debug)]
pub struct LockGuard {
    _file: File,
}

#[derive(Debug)]
pub enum LockError {
    /// Outro processo está limpando agora.
    Busy {
        pid: Option<u32>,
    },
    Io(io::Error),
}

impl fmt::Display for LockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LockError::Busy { pid: Some(pid) } => write!(
                f,
                "outra limpeza está em andamento (pid {pid}). Espere terminar e tente de novo."
            ),
            LockError::Busy { pid: None } => {
                write!(
                    f,
                    "outra limpeza está em andamento. Espere terminar e tente de novo."
                )
            }
            LockError::Io(e) => write!(f, "não foi possível criar a trava de limpeza: {e}"),
        }
    }
}

impl std::error::Error for LockError {}

/// Onde fica o arquivo de trava, dado o HOME. Sem depender de variáveis de ambiente.
pub fn lock_path(home: &Path) -> PathBuf {
    #[cfg(target_os = "macos")]
    let base = home.join("Library/Application Support");
    #[cfg(not(target_os = "macos"))]
    let base = home.join(".local/state");
    base.join("epma-system-data/clean.lock")
}

/// Caminho usado de verdade; no Linux respeita `XDG_STATE_HOME`.
pub fn default_path() -> Option<PathBuf> {
    #[cfg(not(target_os = "macos"))]
    if let Some(xdg) = std::env::var_os("XDG_STATE_HOME").map(PathBuf::from) {
        if xdg.is_absolute() {
            return Some(xdg.join("epma-system-data/clean.lock"));
        }
    }
    dirs::home_dir().map(|home| lock_path(&home))
}

/// Pega a trava de limpeza do usuário atual.
pub fn acquire() -> Result<LockGuard, LockError> {
    let path = default_path().ok_or_else(|| {
        LockError::Io(io::Error::new(
            io::ErrorKind::NotFound,
            "pasta do usuário não encontrada",
        ))
    })?;
    acquire_at(&path)
}

pub fn acquire_at(path: &Path) -> Result<LockGuard, LockError> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(LockError::Io)?;
    }
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(LockError::Io)?;

    // SAFETY: `fd` pertence a `file`, que continua aberto durante a chamada.
    let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if rc != 0 {
        let err = io::Error::last_os_error();
        return if err.kind() == io::ErrorKind::WouldBlock {
            let pid = fs::read_to_string(path)
                .ok()
                .and_then(|s| s.trim().parse().ok());
            Err(LockError::Busy { pid })
        } else {
            Err(LockError::Io(err))
        };
    }

    // Quem segura a trava anota o próprio pid, só para a mensagem de quem esperar.
    let _ = file.set_len(0);
    let _ = write!(file, "{}", std::process::id());
    let _ = file.flush();
    Ok(LockGuard { _file: file })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_acquire_is_busy_until_the_first_is_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/clean.lock");

        let first = acquire_at(&path).unwrap();
        match acquire_at(&path) {
            Err(LockError::Busy { pid }) => assert_eq!(pid, Some(std::process::id())),
            other => panic!("esperava Busy, veio {other:?}"),
        }
        drop(first);
        // Outros testes criam subprocessos em paralelo (fork): o filho herda o arquivo até o
        // `exec`, e a trava só solta depois disso. Na prática a janela dura microssegundos.
        let reacquired = (0..50).any(|_| {
            acquire_at(&path).is_ok() || {
                std::thread::sleep(std::time::Duration::from_millis(20));
                false
            }
        });
        assert!(reacquired, "a trava devia ter sido liberada");
    }

    #[test]
    fn busy_message_names_the_other_process() {
        let msg = LockError::Busy { pid: Some(4242) }.to_string();
        assert!(msg.contains("4242") && msg.contains("em andamento"));
    }

    #[test]
    fn lock_path_lives_under_the_user_home() {
        let home = Path::new("/Users/teste");
        let p = lock_path(home);
        assert!(p.starts_with(home));
        assert!(p.ends_with("epma-system-data/clean.lock"));
    }
}
