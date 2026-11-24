use std::ffi::CString;
use std::fs;
use std::path::Path;

/// Formata bytes em base 1000, como o Finder ("1,5 GB").
pub fn format_bytes(bytes: u64) -> String {
    format_bytes_with(bytes, ',')
}

/// Como `format_bytes`, com o separador decimal do idioma (`,` em pt-BR, `.` em en-US).
pub fn format_bytes_with(bytes: u64, decimal: char) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if bytes < 1000 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit]).replace('.', &decimal.to_string())
}

/// Troca o prefixo da pasta do usuário por `~`.
pub fn display_path(path: &Path) -> String {
    match dirs::home_dir().and_then(|h| path.strip_prefix(&h).ok().map(Path::to_path_buf)) {
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

/// O caminho **real** de `dir` (com links simbólicos resolvidos) fica dentro de `home`,
/// pelo menos um nível abaixo dela? É a checagem que a trava de segurança usa: o texto do
/// caminho pode parecer seguro (`~/Library/Caches`) e ser um link para fora da sua pasta.
pub fn real_dir_inside_home(dir: &Path, home: &Path) -> bool {
    let (Ok(real_dir), Ok(real_home)) = (fs::canonicalize(dir), fs::canonicalize(home)) else {
        return false;
    };
    real_dir
        .strip_prefix(&real_home)
        .is_ok_and(|rest| rest.components().count() >= 1)
}

/// (total, disponível) em bytes do volume que contém `path`.
pub fn disk_space(path: &str) -> Option<(u64, u64)> {
    let c_path = CString::new(path).ok()?;
    let mut stat: libc::statfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statfs(c_path.as_ptr(), &mut stat) } != 0 {
        return None;
    }
    let block = stat.f_bsize as u64;
    Some((stat.f_blocks * block, stat.f_bavail * block))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(999), "999 B");
        assert_eq!(format_bytes(1_500), "1,5 KB");
        assert_eq!(format_bytes_with(1_500, '.'), "1.5 KB");
        assert_eq!(format_bytes_with(2_340_000_000, '.'), "2.3 GB");
        assert_eq!(format_bytes_with(999, '.'), "999 B");
        assert_eq!(format_bytes(2_340_000_000), "2,3 GB");
    }

    #[test]
    fn real_dir_inside_home_resolves_symlinks() {
        let home = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::create_dir_all(home.path().join("Library/Caches")).unwrap();
        fs::create_dir_all(home.path().join("real")).unwrap();
        std::os::unix::fs::symlink(outside.path(), home.path().join("Library/Fora")).unwrap();
        std::os::unix::fs::symlink(home.path().join("real"), home.path().join("Library/Dentro"))
            .unwrap();

        assert!(real_dir_inside_home(
            &home.path().join("Library/Caches"),
            home.path()
        ));
        // Link que continua dentro da home é aceito; para fora, não.
        assert!(real_dir_inside_home(
            &home.path().join("Library/Dentro"),
            home.path()
        ));
        assert!(!real_dir_inside_home(
            &home.path().join("Library/Fora"),
            home.path()
        ));
        // A própria home, ou algo que não existe, nunca vale.
        assert!(!real_dir_inside_home(home.path(), home.path()));
        assert!(!real_dir_inside_home(
            &home.path().join("nao/existe"),
            home.path()
        ));
    }

    #[test]
    fn reads_root_disk_space() {
        let (total, free) = disk_space("/").unwrap();
        assert!(total > 0 && free <= total);
    }
}
