//! Preferências da janela (idioma e tema), guardadas num arquivo de texto simples
//! (`chave=valor`) na pasta de configuração do usuário. Sem dependências extras e sem
//! guardar nada que afete a segurança: o destino da limpeza (Lixeira ou permanente)
//! nunca é persistido; a janela sempre abre em "Lixeira".

use std::fs;
use std::path::{Path, PathBuf};

/// Escolha de idioma: seguir o sistema ou fixar um.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum LangPref {
    #[default]
    Auto,
    PtBr,
    EnUs,
}

/// Escolha de tema: seguir o sistema ou fixar um.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ThemePref {
    #[default]
    Auto,
    Light,
    Dark,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Prefs {
    pub lang: LangPref,
    pub theme: ThemePref,
}

impl Prefs {
    /// `~/Library/Application Support/epma-system-data/prefs` no macOS.
    pub fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("epma-system-data").join("prefs"))
    }

    pub fn load() -> Self {
        Self::path()
            .map(|p| Self::load_from(&p))
            .unwrap_or_default()
    }

    pub fn load_from(path: &Path) -> Self {
        fs::read_to_string(path)
            .map(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    /// Falha ao salvar não é erro para o usuário: a escolha só vale até fechar o app.
    pub fn save(&self) {
        if let Some(path) = Self::path() {
            self.save_to(&path);
        }
    }

    pub fn save_to(&self, path: &Path) {
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = fs::write(path, self.serialize());
    }

    /// Linhas desconhecidas ou valores inválidos viram o padrão (seguir o sistema).
    pub fn parse(text: &str) -> Self {
        let mut prefs = Self::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            match (key.trim(), value.trim()) {
                ("lang", "pt-BR") => prefs.lang = LangPref::PtBr,
                ("lang", "en-US") => prefs.lang = LangPref::EnUs,
                ("theme", "light") => prefs.theme = ThemePref::Light,
                ("theme", "dark") => prefs.theme = ThemePref::Dark,
                _ => {}
            }
        }
        prefs
    }

    pub fn serialize(&self) -> String {
        let lang = match self.lang {
            LangPref::Auto => "auto",
            LangPref::PtBr => "pt-BR",
            LangPref::EnUs => "en-US",
        };
        let theme = match self.theme {
            ThemePref::Auto => "auto",
            ThemePref::Light => "light",
            ThemePref::Dark => "dark",
        };
        format!("lang={lang}\ntheme={theme}\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_every_combination() {
        for lang in [LangPref::Auto, LangPref::PtBr, LangPref::EnUs] {
            for theme in [ThemePref::Auto, ThemePref::Light, ThemePref::Dark] {
                let prefs = Prefs { lang, theme };
                assert_eq!(Prefs::parse(&prefs.serialize()), prefs);
            }
        }
    }

    #[test]
    fn garbage_falls_back_to_following_the_system() {
        let prefs = Prefs::parse("lang=klingon\ntheme=\nnada\n=\ntheme = purple");
        assert_eq!(prefs, Prefs::default());
    }

    #[test]
    fn missing_file_is_the_default() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            Prefs::load_from(&dir.path().join("nao-existe")),
            Prefs::default()
        );
    }

    #[test]
    fn saves_and_loads_from_disk_creating_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/pasta/prefs");
        let prefs = Prefs {
            lang: LangPref::EnUs,
            theme: ThemePref::Dark,
        };
        prefs.save_to(&path);
        assert_eq!(Prefs::load_from(&path), prefs);
    }
}
