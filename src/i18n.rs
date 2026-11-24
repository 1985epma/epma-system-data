//! Textos da janela em pt-BR e en-US.
//!
//! Cada texto é uma `Key`, e `keys!` gera as duas tabelas juntas: esquecer uma tradução
//! não compila. A CLI não usa este módulo: sua saída continua em pt-BR para não quebrar
//! scripts nem o contrato testado em `tests/cli.rs`.

use crate::categories::Category;
use crate::cleaner::{Failure, FailureKind};
use crate::util::format_bytes_with;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    PtBr,
    EnUs,
}

impl Lang {
    /// Pelo locale do sistema: `en*` vira en-US; qualquer outro (inclusive `pt*`) vira pt-BR.
    pub fn from_locale(locale: &str) -> Self {
        if locale.trim().to_ascii_lowercase().starts_with("en") {
            Lang::EnUs
        } else {
            Lang::PtBr
        }
    }

    /// `EPMA_LANG` > idioma preferido do macOS > `LC_ALL`/`LANG`.
    pub fn detect() -> Self {
        if let Ok(v) = std::env::var("EPMA_LANG") {
            if !v.trim().is_empty() {
                return Self::from_locale(&v);
            }
        }
        #[cfg(target_os = "macos")]
        if let Some(l) = macos_preferred_language() {
            return Self::from_locale(&l);
        }
        for var in ["LC_ALL", "LANG"] {
            if let Ok(v) = std::env::var(var) {
                if !v.is_empty() {
                    return Self::from_locale(&v);
                }
            }
        }
        Lang::PtBr
    }

    pub fn code(self) -> &'static str {
        match self {
            Lang::PtBr => "PT",
            Lang::EnUs => "EN",
        }
    }

    /// Separador decimal dos tamanhos (`767,8 MB` / `767.8 MB`).
    pub fn decimal(self) -> char {
        match self {
            Lang::PtBr => ',',
            Lang::EnUs => '.',
        }
    }
}

/// Primeiro idioma de `defaults read -g AppleLanguages` (ex.: `pt-BR`, `en-US`).
#[cfg(target_os = "macos")]
fn macos_preferred_language() -> Option<String> {
    let out = std::process::Command::new("defaults")
        .args(["read", "-g", "AppleLanguages"])
        .output()
        .ok()?;
    parse_apple_languages(&String::from_utf8_lossy(&out.stdout))
}

/// A saída é uma lista no formato de plist: `(\n    "pt-BR",\n    "en-US"\n)`.
pub fn parse_apple_languages(text: &str) -> Option<String> {
    text.lines()
        .map(|l| l.trim().trim_end_matches(',').trim_matches('"'))
        .find(|l| !l.is_empty() && *l != "(" && *l != ")")
        .map(str::to_string)
}

macro_rules! keys {
    ($($name:ident => $pt:expr, $en:expr;)*) => {
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        pub enum Key { $($name),* }

        pub const ALL_KEYS: &[Key] = &[$(Key::$name),*];

        pub fn t(lang: Lang, key: Key) -> &'static str {
            match (lang, key) {
                $( (Lang::PtBr, Key::$name) => $pt, (Lang::EnUs, Key::$name) => $en, )*
            }
        }
    };
}

keys! {
    // Navegação e barra superior
    AppName => "EPMA System Data", "EPMA System Data";
    NavCleanup => "Limpeza", "Cleanup";
    NavDisks => "Discos", "Disks";
    NavHistory => "Histórico", "History";
    NavSettings => "Ajustes", "Settings";
    FilterHint => "Filtrar categorias…", "Filter categories…";
    ScanningProgress => "Escaneando {} de {}…", "Scanning {} of {}…";
    ScannedJustNow => "Escaneado agora", "Scanned just now";
    ScannedMinutes => "Escaneado há {} min", "Scanned {} min ago";
    ScannedHours => "Escaneado há {} h", "Scanned {} h ago";
    ToggleThemeTip => "Alternar tema (claro / escuro)", "Toggle theme (light / dark)";
    ToggleLangTip => "Idioma / Language", "Language / Idioma";

    // Resumo
    CanBeFreed => "Pode ser liberado agora", "Can be freed now";
    NothingSelected => "Nada selecionado", "Nothing selected";
    CatsSelectedOne => "{} categoria selecionada", "{} category selected";
    CatsSelectedMany => "{} categorias selecionadas", "{} categories selected";
    GoesToTrash => "vão para a Lixeira", "going to Trash";
    PermanentDeletion => "exclusão permanente", "permanent deletion";
    StatSafe => "Seguro", "Safe";
    StatReview => "Revisar", "Review";
    StatItems => "Itens encontrados", "Items found";
    SystemDisk => "Disco do sistema", "System disk";
    DiskCritical => "Crítico · {}%", "Critical · {}%";
    DiskWarning => "Aviso · {}%", "Warning · {}%";
    DiskOk => "{}% usado", "{}% used";
    Free => "livres", "free";
    UsedOf => "{} usados", "{} used";
    DiskThresholds => "Aviso a partir de 90%, crítico a partir de 95% — os mesmos limites do diag.", "Warning from 90%, critical from 95% — the same thresholds as diag.";

    // Lista de categorias
    Categories => "Categorias", "Categories";
    SelectBrowsers => "Marcar navegadores", "Select browsers";
    SelectBrowsersTip => "Marca todas as categorias de Safari e Edge, inclusive as de revisar", "Selects every Safari and Edge category, including the review ones";
    SectionSafe => "Seguro", "Safe";
    SectionReview => "Revisar", "Review";
    HintChecked => "marcado por padrão", "checked by default";
    HintUnchecked => "desmarcado por padrão", "unchecked by default";
    EmptyGroup => "Nada para limpar ou indisponível", "Nothing to clean or unavailable";
    ItemSingular => "item", "item";
    ItemPlural => "itens", "items";
    CategorySingular => "categoria", "category";
    CategoryPlural => "categorias", "categories";
    SelectAll => "Marcar todos", "Select all";
    SelectNone => "Nenhum", "None";
    ViewAll => "Ver todos os {} itens", "View all {} items";
    MenuShowInFinder => "Mostrar no Finder", "Show in Finder";
    MenuCopyPath => "Copiar caminho", "Copy path";
    MenuExclude => "Não incluir na limpeza", "Exclude from cleanup";
    MenuInclude => "Incluir na limpeza", "Include in cleanup";
    PermissionDeniedRow => "Permissão negada. Habilite o Acesso Total ao Disco e reescaneie.", "Permission denied. Enable Full Disk Access and rescan.";
    UnavailableRow => "Indisponível", "Unavailable";

    // Barra de ações
    SelectedWord => "selecionados", "selected";
    WaitingForScan => "aguardando escaneamento", "waiting for scan";
    Destination => "Destino:", "Destination:";
    DestTrash => "Lixeira", "Trash";
    DestPermanent => "Permanente", "Permanent";
    Rescan => "Reescanear", "Rescan";
    CleanWord => "Limpar", "Clean";
    DeleteWord => "Excluir", "Delete";

    // Avisos
    PermissionTitle => "Faltou permissão para ler “{}”", "Missing permission to read “{}”";
    PermissionBody => "O macOS bloqueia essas pastas sem Acesso Total ao Disco. Nada foi alterado nelas.", "macOS blocks these folders without Full Disk Access. Nothing was changed in them.";
    OpenSystemSettings => "Abrir Ajustes do Sistema", "Open System Settings";
    DoneTrash => "Limpeza concluída: {} movidos para a Lixeira", "Cleanup complete: {} moved to Trash";
    DonePermanent => "Limpeza concluída: {} liberados", "Cleanup complete: {} freed";
    DoneWithFailures => "Limpeza concluída com {} falha(s)", "Cleanup finished with {} failure(s)";
    DoneTrashHint => "O espaço só volta ao esvaziar a Lixeira.", "Space only comes back once you empty the Trash.";
    OpenTrash => "Abrir Lixeira", "Open Trash";
    ViewDetails => "Ver detalhes", "View details";
    BusyTitle => "Outra limpeza está em andamento", "Another cleanup is in progress";

    // Detalhes
    DetailsTitle => "Detalhes da limpeza", "Cleanup details";
    TabSummary => "Resumo", "Summary";
    TabFailures => "Falhas ({})", "Failures ({})";
    ResultDone => "concluído", "done";
    ResultFailed => "falhou", "failed";
    MovedToTrash => "movidos para a Lixeira", "moved to Trash";
    Removed => "removidos", "removed";
    NoCleanups => "Nenhuma limpeza nesta sessão.", "No cleanups in this session yet.";
    NoFailures => "Sem falhas.", "No failures.";
    TrashNote => "No modo Lixeira o espaço só volta depois de esvaziá-la. Itens permanecem recuperáveis até lá.", "In Trash mode, space only comes back after you empty it. Items stay recoverable until then.";
    RunLabel => "Limpeza {}", "Cleanup {}";

    // Estado vazio
    EmptyTitle => "Seu Mac está em dia", "Your Mac is all caught up";
    EmptyBody => "Não há nada para limpar agora. Reescaneie quando quiser.", "There is nothing to clean right now. Rescan whenever you like.";

    // Confirmação
    ConfirmTitle => "Confirmar limpeza", "Confirm cleanup";
    Total => "Total", "Total";
    TrashInfo => "Os itens vão para a Lixeira. O espaço só é liberado quando você esvaziá-la.", "Items go to the Trash. Space is only freed once you empty it.";
    CloseApps => "Feche os apps abertos antes de limpar os caches.", "Close open apps before cleaning their caches.";
    Cancel => "Cancelar", "Cancel";
    MoveToTrash => "Mover para a Lixeira", "Move to Trash";
    DeletePermTitle => "Excluir permanentemente?", "Delete permanently?";
    DeletePermWarn => "Não dá para desfazer. Os itens não passam pela Lixeira.", "This can't be undone. Items skip the Trash.";
    TypeToConfirm => "Digite {} para confirmar", "Type {} to confirm";
    ConfirmWord => "apagar", "delete";
    PasswordPrompt => "Digite a senha do seu usuário do Mac para confirmar", "Type your Mac user password to confirm";
    PasswordNote => "Validação de identidade. A limpeza roda como você, sem privilégio de administrador.", "Identity check. The cleanup runs as you, without administrator privileges.";
    Validating => "Validando…", "Checking…";
    WrongPassword => "Senha incorreta. Tente de novo.", "Wrong password. Try again.";
    AuthUnavailable => "Não foi possível validar a senha nesta máquina.", "Could not validate the password on this machine.";
    DeletePermanently => "Excluir permanentemente", "Delete permanently";

    // Discos
    DisksTitle => "Discos e volumes", "Disks and volumes";
    DisksSubtitle => "Somente leitura: nada é alterado nesta tela.", "Read-only: nothing is changed on this screen.";
    DisksReading => "Lendo discos…", "Reading disks…";
    Refresh => "Atualizar", "Refresh";
    PhysicalDisks => "Discos físicos", "Physical disks";
    Volumes => "Volumes", "Volumes";
    Internal => "interno", "internal";
    External => "externo", "external";
    Removable => "removível", "removable";
    ReadOnly => "somente leitura", "read-only";
    SmartPassed => "SMART: OK", "SMART: passed";
    SmartFailing => "SMART: falha", "SMART: failing";
    SmartUnsupported => "SMART: não suportado", "SMART: not supported";
    SmartUnknown => "SMART: não lido", "SMART: not read";
    VolumeFreeOfTotal => "{} livres de {}", "{} free of {}";
    CollectionNotes => "Limitações da leitura", "Reading limitations";

    // Ajustes
    SettingsTitle => "Ajustes", "Settings";
    LanguageLabel => "Idioma", "Language";
    ThemeLabel => "Tema", "Theme";
    OptAuto => "Automático (sistema)", "Automatic (system)";
    OptLight => "Claro", "Light";
    OptDark => "Escuro", "Dark";
    LanguageNote => "A linha de comando continua em português.", "The command line stays in Portuguese.";
    AboutLabel => "Sobre", "About";
    Version => "Versão {}", "Version {}";
}

/// Substitui o primeiro `{}` de `template`.
pub fn fmt1(lang: Lang, key: Key, a: &str) -> String {
    t(lang, key).replacen("{}", a, 1)
}

/// Substitui os dois primeiros `{}` de `template`, em ordem.
pub fn fmt2(lang: Lang, key: Key, a: &str, b: &str) -> String {
    t(lang, key).replacen("{}", a, 1).replacen("{}", b, 1)
}

/// `1 item` / `47 itens`.
pub fn n_items(lang: Lang, n: usize) -> String {
    let word = if n == 1 {
        Key::ItemSingular
    } else {
        Key::ItemPlural
    };
    format!("{n} {}", t(lang, word))
}

/// `1 categoria` / `2 categorias`.
pub fn n_categories(lang: Lang, n: usize) -> String {
    let word = if n == 1 {
        Key::CategorySingular
    } else {
        Key::CategoryPlural
    };
    format!("{n} {}", t(lang, word))
}

/// `2 categorias selecionadas` / `1 category selected`.
pub fn n_selected(lang: Lang, n: usize) -> String {
    let key = if n == 1 {
        Key::CatsSelectedOne
    } else {
        Key::CatsSelectedMany
    };
    fmt1(lang, key, &n.to_string())
}

/// "Escaneado há 2 min" a partir de segundos decorridos.
pub fn scanned_ago(lang: Lang, secs: u64) -> String {
    match secs {
        0..=59 => t(lang, Key::ScannedJustNow).to_string(),
        60..=3599 => fmt1(lang, Key::ScannedMinutes, &(secs / 60).to_string()),
        _ => fmt1(lang, Key::ScannedHours, &(secs / 3600).to_string()),
    }
}

/// Tamanho com o separador decimal do idioma.
pub fn bytes(lang: Lang, n: u64) -> String {
    format_bytes_with(n, lang.decimal())
}

/// Nome da categoria no idioma (pt-BR é o texto original de `categories.rs`).
pub fn cat_name(lang: Lang, cat: &Category) -> &'static str {
    if lang == Lang::PtBr {
        return cat.name;
    }
    match cat.id {
        "user_caches" => "App caches",
        "user_logs" => "Logs and crash reports",
        "xcode_derived" => "Xcode DerivedData",
        "device_support" => "Device Support (iOS/watchOS/tvOS)",
        "sim_caches" => "Simulator caches",
        "sim_unavailable" => "Unavailable simulators",
        "tm_snapshots" => "Local Time Machine snapshots",
        "mail_downloads" => "Mail Downloads",
        "dev_caches" => "Developer tool caches",
        "xcode_archives" => "Xcode Archives",
        "ios_backups" => "iPhone/iPad backups",
        "tf_plugin_cache" => "Terraform plugin cache",
        "docker_prune" => "Docker: unused build cache, images and containers",
        "docker_volumes" => "Docker: unused volumes",
        "safari_cache" => "Safari: caches",
        "safari_history" => "Safari: history and downloads",
        "safari_site_data" => "Safari: cookies and site data",
        "safari_extensions" => "Safari: extension data",
        "edge_cache" => "Edge: caches",
        "edge_history" => "Edge: history and downloads",
        "edge_site_data" => "Edge: cookies and site data",
        "edge_extensions" => "Edge: extensions and their data",
        _ => cat.name,
    }
}

/// Descrição da categoria no idioma.
pub fn cat_desc(lang: Lang, cat: &Category) -> &'static str {
    if lang == Lang::PtBr {
        return cat.description;
    }
    match cat.id {
        "user_caches" => "~/Library/Caches — temporary files from apps and browsers. Close apps before cleaning.",
        "user_logs" => "~/Library/Logs — app logs and DiagnosticReports.",
        "xcode_derived" => "Intermediate builds and indexes from Xcode. Recreated on the next build.",
        "device_support" => "Debug symbols for devices. Downloaded again when you connect the device.",
        "sim_caches" => "~/Library/Developer/CoreSimulator/Caches — dyld caches for the simulators.",
        "sim_unavailable" => "Simulators for runtimes that are no longer installed (xcrun simctl delete unavailable).",
        "tm_snapshots" => "Local APFS snapshots kept on the internal disk. macOS usually counts them as System Data.",
        "mail_downloads" => "Local copies of attachments opened in Mail. They remain available on the server.",
        "dev_caches" => "npm, Gradle, Cargo and ~/.cache. May include large models or packages that will be downloaded again.",
        "xcode_archives" => "Archived builds for distribution. Needed to symbolicate crashes from published versions.",
        "ios_backups" => "~/Library/Application Support/MobileSync/Backup. Only delete backups you no longer need.",
        "tf_plugin_cache" => "~/.tf-plugin-cache — Terraform/OpenTofu providers. Re-downloaded on the next init.",
        "docker_prune" => "docker system prune: removes build cache, dangling images, stopped containers and unused networks. Does not touch volumes. Permanent. Disk space only returns after compacting the Docker disk image (Docker Desktop > Resources > Clean / Purge data).",
        "docker_volumes" => "docker volume prune: deletes volumes not used by any container. ⚠️ May delete database data from stopped projects. Permanent, no undo.",
        "safari_cache" => "Safari page, icon and preview cache. Safari recreates it. Quit Safari first and grant Full Disk Access.",
        "safari_history" => "History, downloads list, recently closed tabs and last session. Does not delete bookmarks or passwords.",
        "safari_site_data" => "Cookies, localStorage and site databases. Signs you out of websites.",
        "safari_extensions" => "Data and settings saved by Safari extensions. Installed extensions stay; they only lose what they stored.",
        "edge_cache" => "Page cache, compiled code, GPU and Service Workers for all Edge profiles. Quit Edge first.",
        "edge_history" => "History, downloads, top sites, icons and sessions for all profiles. Does not delete bookmarks, passwords or autofill.",
        "edge_site_data" => "Cookies, localStorage, IndexedDB and site databases for all profiles. Signs you out of websites.",
        "edge_extensions" => "Installed extensions and their settings, across all profiles. Synced extensions come back on their own; the others must be reinstalled.",
        _ => cat.description,
    }
}

/// Notas do scanner chegam em pt-BR; em en-US trocamos as frases conhecidas.
pub fn scan_note(lang: Lang, note: &str) -> String {
    if lang == Lang::PtBr {
        return note.to_string();
    }
    const PAIRS: &[(&str, &str)] = &[
        (
            "Sem permissão de leitura. Conceda Acesso Total ao Disco em Ajustes > Privacidade e Segurança.",
            "Read permission denied. Grant Full Disk Access in System Settings > Privacy & Security.",
        ),
        (
            "aponta (link simbólico) para fora da sua pasta de usuário e foi ignorada.",
            "points (symlink) outside your user folder and was ignored.",
        ),
        (
            "fica fora da sua pasta de usuário (link simbólico) e foi ignorado.",
            "is outside your user folder (symlink) and was ignored.",
        ),
        ("Nada para limpar", "Nothing to clean"),
        ("Indisponível:", "Unavailable:"),
        ("não encontrado", "not found"),
    ];
    PAIRS
        .iter()
        .fold(note.to_string(), |acc, (pt, en)| acc.replace(pt, en))
}

/// Mensagem de uma falha de limpeza no idioma.
pub fn failure_text(lang: Lang, f: &Failure) -> String {
    if lang == Lang::PtBr {
        return f.friendly();
    }
    match f.kind {
        FailureKind::PermissionDenied if cfg!(target_os = "macos") => {
            "no permission (protected by macOS)".to_string()
        }
        FailureKind::PermissionDenied => "no permission".to_string(),
        FailureKind::NotFound => "no longer exists".to_string(),
        FailureKind::OutsideHome => "outside the allowed area".to_string(),
        FailureKind::Other => f.message.chars().take(120).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::categories;

    #[test]
    fn every_key_is_filled_in_both_languages() {
        for key in ALL_KEYS {
            assert!(!t(Lang::PtBr, *key).is_empty(), "{key:?} vazio em pt-BR");
            assert!(!t(Lang::EnUs, *key).is_empty(), "{key:?} vazio em en-US");
        }
    }

    #[test]
    fn languages_differ_except_for_words_that_are_the_same() {
        // Nomes próprios e palavras iguais nos dois idiomas.
        let same = [Key::AppName, Key::Total, Key::Volumes, Key::ItemSingular];
        for key in ALL_KEYS {
            if same.contains(key) {
                continue;
            }
            assert_ne!(
                t(Lang::PtBr, *key),
                t(Lang::EnUs, *key),
                "{key:?} sem tradução"
            );
        }
    }

    #[test]
    fn placeholders_match_between_languages() {
        for key in ALL_KEYS {
            let pt = t(Lang::PtBr, *key).matches("{}").count();
            let en = t(Lang::EnUs, *key).matches("{}").count();
            assert_eq!(pt, en, "{key:?}: número de {{}} difere entre os idiomas");
        }
    }

    #[test]
    fn every_category_has_an_english_name_and_description() {
        let home = std::path::Path::new("/Users/teste");
        for cat in categories::build(home) {
            // Nomes técnicos que são iguais nos dois idiomas.
            let same_name = [
                "xcode_derived",
                "device_support",
                "xcode_archives",
                "safari_cache",
                "edge_cache",
            ];
            if !same_name.contains(&cat.id) {
                assert_ne!(
                    cat_name(Lang::EnUs, &cat),
                    cat.name,
                    "{} sem nome en",
                    cat.id
                );
            }
            assert_ne!(
                cat_desc(Lang::EnUs, &cat),
                cat.description,
                "{} sem descrição en",
                cat.id
            );
            assert_eq!(cat_name(Lang::PtBr, &cat), cat.name);
        }
    }

    #[test]
    fn locale_detection() {
        assert_eq!(Lang::from_locale("en-US"), Lang::EnUs);
        assert_eq!(Lang::from_locale("en_GB.UTF-8"), Lang::EnUs);
        assert_eq!(Lang::from_locale("pt-BR"), Lang::PtBr);
        assert_eq!(Lang::from_locale("pt_BR.UTF-8"), Lang::PtBr);
        assert_eq!(Lang::from_locale("fr-FR"), Lang::PtBr);
        assert_eq!(Lang::from_locale(""), Lang::PtBr);
    }

    #[test]
    fn apple_languages_output_is_parsed() {
        let out = "(\n    \"en-US\",\n    \"pt-BR\"\n)\n";
        assert_eq!(parse_apple_languages(out).as_deref(), Some("en-US"));
        assert_eq!(parse_apple_languages("(\n)\n"), None);
    }

    #[test]
    fn formatting_helpers() {
        assert_eq!(n_items(Lang::PtBr, 1), "1 item");
        assert_eq!(n_items(Lang::PtBr, 47), "47 itens");
        assert_eq!(n_items(Lang::EnUs, 47), "47 items");
        assert_eq!(n_categories(Lang::EnUs, 1), "1 category");
        assert_eq!(n_categories(Lang::PtBr, 2), "2 categorias");
        assert_eq!(n_selected(Lang::PtBr, 1), "1 categoria selecionada");
        assert_eq!(n_selected(Lang::PtBr, 2), "2 categorias selecionadas");
        assert_eq!(n_selected(Lang::EnUs, 1), "1 category selected");
        assert_eq!(bytes(Lang::PtBr, 767_800_000), "767,8 MB");
        assert_eq!(bytes(Lang::EnUs, 767_800_000), "767.8 MB");
        assert_eq!(scanned_ago(Lang::PtBr, 5), "Escaneado agora");
        assert_eq!(scanned_ago(Lang::EnUs, 125), "Scanned 2 min ago");
        assert_eq!(scanned_ago(Lang::PtBr, 7300), "Escaneado há 2 h");
        assert_eq!(
            fmt2(Lang::EnUs, Key::VolumeFreeOfTotal, "7.2 GB", "245.1 GB"),
            "7.2 GB free of 245.1 GB"
        );
    }

    #[test]
    fn scanner_notes_are_translated_but_pt_is_untouched() {
        let note = "Indisponível: `xcrun` não encontrado";
        assert_eq!(scan_note(Lang::PtBr, note), note);
        assert_eq!(
            scan_note(Lang::EnUs, note),
            "Unavailable: `xcrun` not found"
        );
        assert_eq!(
            scan_note(Lang::EnUs, "Nada para limpar"),
            "Nothing to clean"
        );
    }
}
