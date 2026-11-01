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

    vec![
        Category {
            id: "user_caches",
            name: "Caches de apps",
            description: "~/Library/Caches — arquivos temporários de apps e navegadores. Feche os apps antes de limpar.",
            risk: Risk::Safe,
            target: Target::DirContents {
                dirs: vec![lib.join("Caches")],
                exclude: PROTECTED_CACHES,
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
            },
        },
    ]
}
