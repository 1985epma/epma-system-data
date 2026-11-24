//! Validação da senha do usuário antes de uma limpeza, como porta de confirmação
//! ("é você, e você assume a responsabilidade"). **Não eleva privilégio**: a limpeza
//! continua rodando como o próprio usuário — as operações não precisam de root, e rodá-las
//! como root seria mais perigoso. A senha serve só para validar a identidade.
//!
//! No macOS usamos `dscl . -authonly`, passando a senha pelo **stdin** (não como argumento,
//! que apareceria em `ps` e, pior, retorna código de saída 0 mesmo quando a senha está errada).
//! A senha nunca é registrada em log nem guardada.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AuthOutcome {
    /// Senha confere.
    Ok,
    /// Senha incorreta.
    Wrong,
    /// Não foi possível validar (ferramenta ausente, usuário desconhecido).
    Unavailable,
}

#[cfg(target_os = "macos")]
pub fn validate_password(password: &str) -> AuthOutcome {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let Some(user) = std::env::var("USER").ok().filter(|u| !u.is_empty()) else {
        return AuthOutcome::Unavailable;
    };

    // `dscl . -authonly <user>` lê a senha de uma linha do stdin e sai com 0 se confere.
    let mut child = match Command::new("/usr/bin/dscl")
        .args([".", "-authonly", &user])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return AuthOutcome::Unavailable,
    };

    if let Some(mut stdin) = child.stdin.take() {
        // Fechar o stdin (drop) manda EOF; o dscl então avalia o que recebeu.
        let _ = stdin.write_all(password.as_bytes());
    }

    match child.wait() {
        Ok(status) if status.success() => AuthOutcome::Ok,
        Ok(_) => AuthOutcome::Wrong,
        Err(_) => AuthOutcome::Unavailable,
    }
}

/// Fora do macOS não há `dscl`; a janela existe só para macOS, então aqui liberamos.
#[cfg(not(target_os = "macos"))]
pub fn validate_password(_password: &str) -> AuthOutcome {
    AuthOutcome::Ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn wrong_password_is_rejected() {
        // Uma senha claramente errada nunca deve validar (não usamos a senha real em teste).
        assert_eq!(
            validate_password("senha-propositalmente-errada-para-teste-1290387"),
            AuthOutcome::Wrong
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn empty_password_does_not_authenticate() {
        assert_ne!(validate_password(""), AuthOutcome::Ok);
    }
}
