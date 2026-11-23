//! Categorias de dados que compõem o "System Data" (Dados do Sistema) do macOS
//! e que podem ser limpas com segurança a partir da conta do usuário.

use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Risk {
    /// Dados regeneráveis: apps e o sistema recriam quando precisam.
    Safe,
    /// Pode conter algo que o usuário queira manter; revise antes de limpar.
    Review,
}

/// Comando de leitura usado para estimar quantos itens um comando de limpeza afeta.
#[derive(Clone, Debug)]
pub struct Probe {
    pub program: &'static str,
    pub args: &'static [&'static str],
    /// Conta as linhas da saída que contêm este texto.
    pub count_lines_containing: &'static str,
    pub unit: &'static str,
}

#[derive(Clone, Debug)]
pub enum Target {
    /// Remove o conteúdo destes diretórios, mantendo os diretórios em si.
    DirContents {
        dirs: Vec<PathBuf>,
        /// Nomes de itens de primeiro nível que nunca são tocados.
        exclude: &'static [&'static str],
        /// Caminhos específicos removidos por inteiro (arquivo ou pasta), além do conteúdo
        /// de `dirs`. Servem para pegar só o que interessa dentro de uma pasta que também
        /// guarda dados que nunca devem ser tocados (senhas, favoritos, preferências).
        paths: Vec<PathBuf>,
    },
    /// Executa um comando do sistema que faz a limpeza.
    Command {
        program: &'static str,
        args: &'static [&'static str],
        probe: Probe,
    },
}

#[derive(Clone, Debug)]
pub struct Category {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub risk: Risk,
    pub target: Target,
}

/// Caches da Apple ligados a iCloud, Casa e privacidade: recriá-los força
/// ressincronizações demoradas, então ficam de fora.
const PROTECTED_CACHES: &[&str] = &[
    "CloudKit",
    "FamilyCircle",
    "com.apple.bird",
    "com.apple.containermanagerd",
    "com.apple.HomeKit",
    "com.apple.homed",
    "com.apple.ap.adprivacyd",
    "com.apple.findmy.fmipcore",
    // Protegidos pelo TCC: a remoção sempre falha sem Acesso Total ao Disco.
    "familycircled",
    "com.apple.Safari",
    "com.apple.Safari.SafeBrowsing",
];

pub fn all() -> Vec<Category> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    build(&home)
}

pub fn build(home: &Path) -> Vec<Category> {
    let lib = home.join("Library");
    let xcode = lib.join("Developer/Xcode");

    let mut cats = vec![
        Category {
            id: "user_caches",
            name: "Caches de apps",
            description: "~/Library/Caches — arquivos temporários de apps e navegadores. Feche os apps antes de limpar.",
            risk: Risk::Safe,
            target: Target::DirContents {
                dirs: vec![lib.join("Caches")],
                exclude: PROTECTED_CACHES,
                paths: vec![],
            },
        },
        Category {
            id: "user_logs",
            name: "Logs e relatórios de falha",
            description: "~/Library/Logs — logs de apps e DiagnosticReports.",
            risk: Risk::Safe,
            target: Target::DirContents {
                dirs: vec![lib.join("Logs")],
                exclude: &[],
                paths: vec![],
            },
        },
        Category {
            id: "xcode_derived",
            name: "Xcode DerivedData",
            description: "Builds intermediários e índices do Xcode. Recriados no próximo build.",
            risk: Risk::Safe,
            target: Target::DirContents {
                dirs: vec![xcode.join("DerivedData")],
                exclude: &[],
                paths: vec![],
            },
        },
        Category {
            id: "device_support",
            name: "Device Support (iOS/watchOS/tvOS)",
            description: "Símbolos de depuração de dispositivos. Baixados de novo ao conectar o aparelho.",
            risk: Risk::Safe,
            target: Target::DirContents {
                dirs: ["iOS", "watchOS", "tvOS", "visionOS"]
                    .iter()
                    .map(|os| xcode.join(format!("{os} DeviceSupport")))
                    .collect(),
                exclude: &[],
                paths: vec![],
            },
        },
        Category {
            id: "sim_caches",
            name: "Caches do Simulator",
            description: "~/Library/Developer/CoreSimulator/Caches — caches dyld dos simuladores.",
            risk: Risk::Safe,
            target: Target::DirContents {
                dirs: vec![lib.join("Developer/CoreSimulator/Caches")],
                exclude: &[],
                paths: vec![],
            },
        },
        Category {
            id: "sim_unavailable",
            name: "Simuladores indisponíveis",
            description: "Simuladores de runtimes que não estão mais instalados (xcrun simctl delete unavailable).",
            risk: Risk::Safe,
            target: Target::Command {
                program: "xcrun",
                args: &["simctl", "delete", "unavailable"],
                probe: Probe {
                    program: "xcrun",
                    args: &["simctl", "list", "devices", "unavailable"],
                    count_lines_containing: "(unavailable",
                    unit: "simulador(es)",
                },
            },
        },
        Category {
            id: "tm_snapshots",
            name: "Snapshots locais do Time Machine",
            description: "Snapshots APFS guardados no disco interno. O macOS costuma contá-los como System Data.",
            risk: Risk::Safe,
            target: Target::Command {
                program: "tmutil",
                args: &["thinlocalsnapshots", "/", "999999999999999", "4"],
                probe: Probe {
                    program: "tmutil",
                    args: &["listlocalsnapshots", "/"],
                    count_lines_containing: "com.apple.TimeMachine",
                    unit: "snapshot(s)",
                },
            },
        },
        Category {
            id: "mail_downloads",
            name: "Anexos baixados do Mail",
            description: "Cópias locais de anexos abertos no Mail. Continuam disponíveis no servidor.",
            risk: Risk::Safe,
            target: Target::DirContents {
                dirs: vec![lib.join("Containers/com.apple.mail/Data/Library/Mail Downloads")],
                exclude: &[],
                paths: vec![],
            },
        },
        Category {
            id: "dev_caches",
            name: "Caches de ferramentas de desenvolvimento",
            description: "npm, Gradle, Cargo e ~/.cache. Podem incluir modelos ou pacotes grandes que serão baixados de novo.",
            risk: Risk::Review,
            target: Target::DirContents {
                dirs: vec![
                    home.join(".npm/_cacache"),
                    home.join(".gradle/caches"),
                    home.join(".cargo/registry/cache"),
                    home.join(".cache"),
                ],
                exclude: &[],
                paths: vec![],
            },
        },
        Category {
            id: "xcode_archives",
            name: "Xcode Archives",
            description: "Builds arquivados para distribuição. Necessários para simbolizar crashes de versões publicadas.",
            risk: Risk::Review,
            target: Target::DirContents {
                dirs: vec![xcode.join("Archives")],
                exclude: &[],
                paths: vec![],
            },
        },
        Category {
            id: "ios_backups",
            name: "Backups de iPhone/iPad",
            description: "~/Library/Application Support/MobileSync/Backup. Apague só backups que você não precisa mais.",
            risk: Risk::Review,
            target: Target::DirContents {
                dirs: vec![lib.join("Application Support/MobileSync/Backup")],
                exclude: &[],
                paths: vec![],
            },
        },
    ];
    cats.extend(browsers(home));
    cats
}

/// Categorias de navegador (Safari e Edge): ids com estes prefixos.
const BROWSER_PREFIXES: &[&str] = &["safari_", "edge_"];

/// A categoria limpa dados de navegador? Base do atalho `--only browsers`.
pub fn is_browser(cat: &Category) -> bool {
    BROWSER_PREFIXES.iter().any(|p| cat.id.starts_with(p))
}

/// Perfis do Edge: a raiz (caches de shader ficam nela) e cada pasta de perfil
/// (`Default`, `Profile 1`, `Guest Profile`...). Só pastas que existem.
fn edge_profiles(home: &Path) -> Vec<PathBuf> {
    let root = home.join("Library/Application Support/Microsoft Edge");
    let mut out = vec![root.clone()];
    if let Ok(read) = std::fs::read_dir(&root) {
        let mut found: Vec<PathBuf> = read
            .flatten()
            .filter(|e| {
                let n = e.file_name();
                let n = n.to_string_lossy();
                n == "Default" || n.starts_with("Profile ") || n == "Guest Profile"
            })
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .map(|e| e.path())
            .collect();
        found.sort();
        out.extend(found);
    }
    out
}

/// Cada nome de `names` dentro de cada perfil. Quem não existir é ignorado na medição.
fn in_each(profiles: &[PathBuf], names: &[&str]) -> Vec<PathBuf> {
    profiles
        .iter()
        .flat_map(|p| names.iter().map(move |n| p.join(n)))
        .collect()
}

fn browsers(home: &Path) -> Vec<Category> {
    let lib = home.join("Library");
    let safari = lib.join("Safari");
    let safari_box = lib.join("Containers/com.apple.Safari/Data/Library");
    let edge = edge_profiles(home);

    // Só os nomes abaixo são tocados. `Login Data`, `Web Data`, `Bookmarks`, `Preferences`
    // e `Local State` do Edge, e o iCloud Keychain/favoritos do Safari, nunca entram.
    vec![
        Category {
            id: "safari_cache",
            name: "Safari: caches",
            description: "Cache de páginas, ícones e prévias do Safari. O Safari recria. Feche-o antes e conceda Acesso Total ao Disco.",
            risk: Risk::Safe,
            target: Target::DirContents {
                dirs: vec![lib.join("Caches/com.apple.Safari"), safari_box.join("Caches")],
                exclude: &[],
                paths: ["Favicon Cache", "Touch Icons Cache", "Template Icons"]
                    .iter()
                    .map(|n| safari.join(n))
                    .collect(),
            },
        },
        Category {
            id: "safari_history",
            name: "Safari: histórico e downloads",
            description: "Histórico, lista de downloads, abas fechadas recentemente e última sessão. Não apaga favoritos nem senhas.",
            risk: Risk::Review,
            target: Target::DirContents {
                dirs: vec![],
                exclude: &[],
                paths: [
                    "History.db",
                    "History.db-lock",
                    "History.db-shm",
                    "History.db-wal",
                    "Downloads.plist",
                    "LastSession.plist",
                    "RecentlyClosedTabs.plist",
                    "TopSites.plist",
                ]
                .iter()
                .map(|n| safari.join(n))
                .collect(),
            },
        },
        Category {
            id: "safari_site_data",
            name: "Safari: cookies e dados de sites",
            description: "Cookies, localStorage e bancos de sites. Desconecta você dos sites.",
            risk: Risk::Review,
            target: Target::DirContents {
                dirs: vec![],
                exclude: &[],
                paths: vec![
                    lib.join("Cookies/Cookies.binarycookies"),
                    safari_box.join("Cookies/Cookies.binarycookies"),
                    safari.join("LocalStorage"),
                    safari.join("Databases"),
                    safari_box.join("WebKit/WebsiteData"),
                    lib.join("WebKit/com.apple.Safari"),
                ],
            },
        },
        Category {
            id: "safari_extensions",
            name: "Safari: dados de extensões",
            description: "Dados e configurações salvos pelas extensões do Safari. As extensões instaladas pelo app continuam; só perdem o que guardaram.",
            risk: Risk::Review,
            target: Target::DirContents {
                dirs: vec![],
                exclude: &[],
                paths: vec![
                    safari.join("Extensions"),
                    safari_box.join("Safari/WebExtensions"),
                    safari_box.join("Safari/AppExtensions"),
                ],
            },
        },
        Category {
            id: "edge_cache",
            name: "Edge: caches",
            description: "Cache de páginas, código compilado, GPU e Service Workers de todos os perfis do Edge. Feche o Edge antes.",
            risk: Risk::Safe,
            target: Target::DirContents {
                dirs: vec![],
                exclude: &[],
                paths: in_each(
                    &edge,
                    &[
                        "Cache",
                        "Code Cache",
                        "GPUCache",
                        "DawnCache",
                        "GraphiteDawnCache",
                        "ShaderCache",
                        "GrShaderCache",
                        "Service Worker/CacheStorage",
                        "Service Worker/ScriptCache",
                    ],
                ),
            },
        },
        Category {
            id: "edge_history",
            name: "Edge: histórico e downloads",
            description: "Histórico, downloads, sites mais visitados, ícones e sessões de todos os perfis. Não apaga favoritos, senhas nem preenchimento automático.",
            risk: Risk::Review,
            target: Target::DirContents {
                dirs: vec![],
                exclude: &[],
                paths: in_each(
                    &edge,
                    &[
                        "History",
                        "History-journal",
                        "Top Sites",
                        "Top Sites-journal",
                        "Visited Links",
                        "Favicons",
                        "Favicons-journal",
                        "Shortcuts",
                        "Shortcuts-journal",
                        "Network Action Predictor",
                        "Sessions",
                    ],
                ),
            },
        },
        Category {
            id: "edge_site_data",
            name: "Edge: cookies e dados de sites",
            description: "Cookies, localStorage, IndexedDB e bancos de sites de todos os perfis. Desconecta você dos sites.",
            risk: Risk::Review,
            target: Target::DirContents {
                dirs: vec![],
                exclude: &[],
                paths: in_each(
                    &edge,
                    &[
                        "Network/Cookies",
                        "Network/Cookies-journal",
                        "Local Storage",
                        "IndexedDB",
                        "Session Storage",
                        "File System",
                        "databases",
                        "Service Worker/Database",
                    ],
                ),
            },
        },
        Category {
            id: "edge_extensions",
            name: "Edge: extensões e seus dados",
            description: "Extensões instaladas e as configurações delas, em todos os perfis. Extensões sincronizadas voltam sozinhas; as outras precisam ser reinstaladas.",
            risk: Risk::Review,
            target: Target::DirContents {
                dirs: vec![],
                exclude: &[],
                paths: in_each(
                    &edge,
                    &[
                        "Extensions",
                        "Local Extension Settings",
                        "Sync Extension Settings",
                        "Extension State",
                        "Extension Rules",
                        "Extension Scripts",
                    ],
                ),
            },
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths_of<'a>(cats: &'a [Category], id: &str) -> &'a [PathBuf] {
        match &cats.iter().find(|c| c.id == id).unwrap().target {
            Target::DirContents { paths, .. } => paths,
            Target::Command { .. } => panic!("{id} não é de pasta"),
        }
    }

    #[test]
    fn ids_are_unique() {
        let cats = build(Path::new("/Users/teste"));
        let mut ids: Vec<_> = cats.iter().map(|c| c.id).collect();
        ids.sort_unstable();
        let n = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), n);
    }

    #[test]
    fn browser_categories_are_found_by_prefix_and_caches_are_safe() {
        let cats = build(Path::new("/Users/teste"));
        let browsers: Vec<_> = cats.iter().filter(|c| is_browser(c)).collect();
        assert_eq!(browsers.len(), 8);
        for c in &browsers {
            let expect = if c.id.ends_with("_cache") {
                Risk::Safe
            } else {
                Risk::Review
            };
            assert_eq!(c.risk, expect, "{}", c.id);
        }
        assert!(!is_browser(
            cats.iter().find(|c| c.id == "user_caches").unwrap()
        ));
    }

    #[test]
    fn browser_paths_never_reach_passwords_bookmarks_or_prefs() {
        let cats = build(Path::new("/Users/teste"));
        let forbidden = [
            "Login Data",
            "Web Data",
            "Bookmarks",
            "Preferences",
            "Local State",
            "Secure Preferences",
            "Keychains",
        ];
        for c in cats.iter().filter(|c| is_browser(c)) {
            let Target::DirContents { dirs, paths, .. } = &c.target else {
                continue;
            };
            for p in paths.iter().chain(dirs) {
                let name = p.file_name().unwrap().to_string_lossy();
                assert!(!forbidden.contains(&&*name), "{} toca {name}", c.id);
            }
        }
    }

    #[test]
    fn browser_paths_are_deep_enough_for_the_safety_lock() {
        let home = Path::new("/Users/teste");
        for c in build(home).iter().filter(|c| is_browser(c)) {
            let Target::DirContents { paths, .. } = &c.target else {
                continue;
            };
            for p in paths {
                assert!(crate::cleaner::is_safe_target(p, home), "{}: {p:?}", c.id);
            }
        }
    }

    #[test]
    fn edge_profiles_are_discovered_and_junk_dirs_ignored() {
        let home = tempfile::tempdir().unwrap();
        let root = home
            .path()
            .join("Library/Application Support/Microsoft Edge");
        for d in [
            "Default",
            "Profile 2",
            "Guest Profile",
            "Crashpad",
            "extensions_crx_cache",
        ] {
            std::fs::create_dir_all(root.join(d)).unwrap();
        }
        let cats = build(home.path());
        let cache = paths_of(&cats, "edge_cache");
        assert!(cache.contains(&root.join("Default/Cache")));
        assert!(cache.contains(&root.join("Profile 2/Service Worker/CacheStorage")));
        assert!(cache.contains(&root.join("Guest Profile/GPUCache")));
        assert!(cache.contains(&root.join("ShaderCache")), "raiz do Edge");
        assert!(!cache.iter().any(|p| p.starts_with(root.join("Crashpad"))));
    }
}
