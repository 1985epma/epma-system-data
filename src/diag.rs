//! Diagnóstico de discos para macOS e Linux. Somente leitura: não altera nada.
//!
//! Reúne três coisas e as cruza em uma lista de achados:
//! - **volumes** montados: espaço, inodes e se estão somente leitura;
//! - **discos físicos**: modelo, tamanho, SSD ou HDD, interno ou externo;
//! - **saúde (SMART)**: `diskutil` no macOS, `smartctl` no Linux (precisa de root).
//!
//! As funções de leitura de texto (`parse_*`) e a análise (`analyze`) não dependem do
//! sistema, então são testadas nos dois sistemas. Só a coleta (`collect`) é específica.

use std::collections::HashSet;
use std::ffi::CString;
use std::path::Path;
use std::process::Command;

use serde::Serialize;

/// A partir daqui o volume é um aviso; a partir de `FULL_CRITICAL`, crítico.
pub const FULL_WARNING: f64 = 90.0;
pub const FULL_CRITICAL: f64 = 95.0;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SmartStatus {
    Passed,
    Failing,
    /// O disco ou a ponte USB não expõe SMART.
    NotSupported,
    /// Não deu para ler (ferramenta ausente, falta de permissão).
    Unknown,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DiskKind {
    Ssd,
    Hdd,
    Unknown,
}

#[derive(Clone, Debug, Serialize)]
pub struct Disk {
    /// `disk0` no macOS, `sda` ou `nvme0n1` no Linux.
    pub name: String,
    pub model: Option<String>,
    pub size_bytes: Option<u64>,
    pub kind: DiskKind,
    pub removable: bool,
    /// `None` quando o sistema não informa.
    pub internal: Option<bool>,
    pub protocol: Option<String>,
    pub smart: SmartStatus,
    pub smart_note: Option<String>,
}

impl Disk {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            model: None,
            size_bytes: None,
            kind: DiskKind::Unknown,
            removable: false,
            internal: None,
            protocol: None,
            smart: SmartStatus::Unknown,
            smart_note: None,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Volume {
    pub device: String,
    pub mount: String,
    pub fstype: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    /// Espaço que um usuário comum consegue usar (como o `df`).
    pub free_bytes: u64,
    pub inodes_total: Option<u64>,
    pub inodes_free: Option<u64>,
    pub read_only: bool,
}

impl Volume {
    /// Percentual usado, calculado como o `df` (usado / (usado + livre)).
    pub fn used_percent(&self) -> f64 {
        let denom = self.used_bytes + self.free_bytes;
        if denom == 0 {
            0.0
        } else {
            self.used_bytes as f64 * 100.0 / denom as f64
        }
    }

    pub fn inodes_used_percent(&self) -> Option<f64> {
        let total = self.inodes_total.filter(|t| *t > 0)?;
        let free = self.inodes_free?;
        Some(total.saturating_sub(free) as f64 * 100.0 / total as f64)
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Finding {
    pub severity: Severity,
    /// Volume (ponto de montagem) ou disco a que o achado se refere.
    pub subject: String,
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Report {
    pub os: &'static str,
    pub disks: Vec<Disk>,
    pub volumes: Vec<Volume>,
    pub findings: Vec<Finding>,
    /// Limitações da coleta (ferramenta ausente, sem permissão).
    pub notes: Vec<String>,
}

impl Report {
    fn new(os: &'static str) -> Self {
        Self { os, disks: vec![], volumes: vec![], findings: vec![], notes: vec![] }
    }

    pub fn worst(&self) -> Option<Severity> {
        self.findings.iter().map(|f| f.severity).max()
    }
}

/// Coleta tudo e já preenche os achados.
pub fn collect() -> Report {
    #[cfg(target_os = "macos")]
    let mut report = macos::collect();
    #[cfg(target_os = "linux")]
    let mut report = linux::collect();
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let mut report = {
        let mut r = Report::new("outro");
        r.notes.push("diagnóstico de discos só existe para macOS e Linux".into());
        r
    };
    report.findings = analyze(&report);
    report
}

// ------------------------------------------------------------ análise

/// Volumes que são somente leitura por desenho (não merecem aviso).
///
/// No macOS o sistema não remonta disco em somente leitura por erro (a raiz é selada, e
/// imagens de disco e cartões travados são comuns), então lá nunca vira aviso.
fn read_only_is_expected(os: &str, v: &Volume) -> bool {
    const READ_ONLY_FS: [&str; 5] = ["iso9660", "cd9660", "udf", "squashfs", "erofs"];
    os == "macos" || READ_ONLY_FS.contains(&v.fstype.as_str())
}

/// Volumes APFS do mesmo container dividem o mesmo espaço: devolve o container
/// (`/dev/disk3s5` -> `disk3`). Para os demais, o próprio dispositivo.
fn space_group(v: &Volume) -> String {
    if v.fstype == "apfs" {
        if let Some(rest) = v.device.strip_prefix("/dev/disk") {
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            return format!("apfs:disk{digits}");
        }
    }
    v.device.clone()
}

/// Regras de diagnóstico sobre os dados coletados. Ordenado do mais grave ao menos.
pub fn analyze(report: &Report) -> Vec<Finding> {
    let mut out = Vec::new();

    let mut seen_groups = HashSet::new();
    for v in &report.volumes {
        let pct = v.used_percent();
        // Cheio só importa onde dá para gravar, e uma vez por grupo que divide espaço.
        // (só um volume gravável "consome" o grupo: a raiz selada do macOS não conta)
        let report_full = !v.read_only && seen_groups.insert(space_group(v));
        if !report_full {
            // sem aviso de espaço
        } else if pct >= FULL_CRITICAL {
            out.push(Finding {
                severity: Severity::Critical,
                subject: v.mount.clone(),
                message: format!("{pct:.0}% cheio: o sistema pode falhar ao gravar. Libere espaço agora."),
            });
        } else if pct >= FULL_WARNING {
            out.push(Finding {
                severity: Severity::Warning,
                subject: v.mount.clone(),
                message: format!("{pct:.0}% cheio: libere espaço antes que falte."),
            });
        }

        if let Some(ipct) = v.inodes_used_percent().filter(|p| *p >= FULL_WARNING) {
            out.push(Finding {
                severity: if ipct >= FULL_CRITICAL { Severity::Critical } else { Severity::Warning },
                subject: v.mount.clone(),
                message: format!("{ipct:.0}% dos inodes em uso: dá para ficar sem criar arquivos mesmo com espaço livre."),
            });
        }

        if v.read_only && !read_only_is_expected(report.os, v) {
            out.push(Finding {
                severity: Severity::Warning,
                subject: v.mount.clone(),
                message: "montado somente leitura. Se não foi de propósito, pode indicar erro no sistema de arquivos.".into(),
            });
        }
    }

    for d in &report.disks {
        match d.smart {
            SmartStatus::Failing => out.push(Finding {
                severity: Severity::Critical,
                subject: d.name.clone(),
                message: "o SMART reporta falha: faça backup agora e planeje trocar o disco.".into(),
            }),
            SmartStatus::Unknown => {
                if let Some(note) = &d.smart_note {
                    out.push(Finding {
                        severity: Severity::Info,
                        subject: d.name.clone(),
                        message: format!("saúde (SMART) não verificada: {note}"),
                    });
                }
            }
            SmartStatus::Passed | SmartStatus::NotSupported => {}
        }
    }

    out.sort_by_key(|f| std::cmp::Reverse(f.severity));
    out
}

// ---------------------------------------------------------- parsers

/// Lê o número de bytes de `251.0 GB (251000193024 Bytes) (exactly ...)`.
fn bytes_in_parens(value: &str) -> Option<u64> {
    let start = value.find('(')? + 1;
    let rest = &value[start..];
    let end = rest.find(" Bytes")?;
    rest[..end].trim().parse().ok()
}

/// Identificadores dos discos inteiros em `diskutil list physical`.
pub fn parse_diskutil_list(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| l.strip_prefix("/dev/"))
        .filter_map(|l| l.split_whitespace().next())
        .map(str::to_string)
        .collect()
}

/// Lê a saída de `diskutil info /dev/diskN` de um disco inteiro.
pub fn parse_diskutil_info(name: &str, text: &str) -> Disk {
    let mut disk = Disk::new(name);
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else { continue };
        let (key, value) = (key.trim(), value.trim());
        match key {
            "Device / Media Name" if !value.is_empty() => disk.model = Some(value.to_string()),
            "Disk Size" => disk.size_bytes = bytes_in_parens(value),
            "Solid State" => {
                disk.kind = match value {
                    "Yes" => DiskKind::Ssd,
                    "No" => DiskKind::Hdd,
                    _ => DiskKind::Unknown,
                }
            }
            "Removable Media" => disk.removable = value == "Removable",
            "Device Location" => {
                disk.internal = match value {
                    "Internal" => Some(true),
                    "External" => Some(false),
                    _ => None,
                }
            }
            "Protocol" if !value.is_empty() => disk.protocol = Some(value.to_string()),
            "SMART Status" => {
                let lower = value.to_lowercase();
                disk.smart = if lower.contains("verified") {
                    SmartStatus::Passed
                } else if lower.contains("fail") {
                    SmartStatus::Failing
                } else if lower.contains("not supported") {
                    SmartStatus::NotSupported
                } else {
                    SmartStatus::Unknown
                };
            }
            _ => {}
        }
    }
    disk
}

/// Uma linha de `/proc/self/mounts`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MountEntry {
    pub device: String,
    pub mount: String,
    pub fstype: String,
    pub read_only: bool,
}

/// `\040` (espaço), `\011` (tab), `\012` e `\134` (barra) vêm escapados em octal.
fn unescape_mount(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 4 <= bytes.len() && bytes[i + 1..i + 4].iter().all(|b| (b'0'..=b'7').contains(b)) {
            let value = (bytes[i + 1] - b'0') * 64 + (bytes[i + 2] - b'0') * 8 + (bytes[i + 3] - b'0');
            out.push(value);
            i += 4;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn parse_proc_mounts(text: &str) -> Vec<MountEntry> {
    text.lines()
        .filter_map(|line| {
            let mut f = line.split_whitespace();
            let (device, mount, fstype, opts) = (f.next()?, f.next()?, f.next()?, f.next()?);
            Some(MountEntry {
                device: unescape_mount(device),
                mount: unescape_mount(mount),
                fstype: fstype.to_string(),
                read_only: opts.split(',').any(|o| o == "ro"),
            })
        })
        .collect()
}

/// Uma montagem por dispositivo. Bind mounts e subvolumes repetem os mesmos números, e o
/// ponto mais curto (`/`) é o que o usuário reconhece. Mantém a ordem da primeira aparição.
pub fn pick_mounts(entries: Vec<MountEntry>) -> Vec<MountEntry> {
    let mut picked: Vec<MountEntry> = Vec::new();
    for e in entries {
        match picked.iter_mut().find(|p| p.device == e.device) {
            Some(p) if e.mount.len() < p.mount.len() => *p = e,
            Some(_) => {}
            None => picked.push(e),
        }
    }
    picked
}

/// Quais montagens do Linux interessam: discos reais e compartilhamentos de rede.
pub fn keep_linux_mount(m: &MountEntry) -> bool {
    const NETWORK_OR_POOL: [&str; 5] = ["nfs", "nfs4", "cifs", "smb3", "zfs"];
    if m.fstype == "squashfs" || m.device.starts_with("/dev/loop") {
        return false;
    }
    m.device.starts_with("/dev/") || NETWORK_OR_POOL.contains(&m.fstype.as_str())
}

/// Quais montagens do macOS interessam (esconde volumes técnicos do sistema).
pub fn keep_macos_mount(device: &str, fstype: &str, mount: &str) -> bool {
    const NETWORK: [&str; 4] = ["nfs", "smbfs", "afpfs", "webdav"];
    const NOISE: [&str; 10] = [
        "/private/var/folders",
        "/var/folders",
        "/System/Volumes/VM",
        "/System/Volumes/Preboot",
        "/System/Volumes/Update",
        "/System/Volumes/xarts",
        "/System/Volumes/iSCPreboot",
        "/System/Volumes/Hardware",
        "/Library/Developer/CoreSimulator",
        "/System/Library/AssetsV2",
    ];
    if NOISE.iter().any(|n| mount.starts_with(n)) || mount.contains("/Cryptexes") {
        return false;
    }
    device.starts_with("/dev/") || NETWORK.contains(&fstype)
}

/// Resultado de `smartctl -H` (a saída vale mesmo com código de saída diferente de zero).
pub fn parse_smartctl_health(text: &str) -> (SmartStatus, Option<String>) {
    let lower = text.to_lowercase();
    if lower.contains("permission denied") {
        return (SmartStatus::Unknown, Some("sem permissão; rode com sudo".into()));
    }
    // Só o veredito do disco conta como falha: "failed" solto aparece em erros de abertura
    // do dispositivo (ex.: controladora RAID) e não pode virar um alarme falso.
    if lower.contains("test result: failed") || lower.contains("health status: fail") {
        return (SmartStatus::Failing, None);
    }
    if lower.contains("test result: passed") || lower.contains("health status: ok") {
        return (SmartStatus::Passed, None);
    }
    if lower.contains("unable to detect device type") {
        return (SmartStatus::Unknown, Some("o smartctl não detectou o tipo do disco (controladora RAID ou ponte USB? tente `smartctl -d`)".into()));
    }
    if lower.contains("smart support is: unavailable") || lower.contains("not supported") {
        return (SmartStatus::NotSupported, None);
    }
    (SmartStatus::Unknown, Some("resposta do smartctl não reconhecida".into()))
}

// ----------------------------------------------------------- sysfs

/// Lê os discos de `/sys/block` (ou de uma cópia dela, nos testes).
pub fn read_sysfs_disks(root: &Path) -> Vec<Disk> {
    const LOGICAL: [&str; 8] = ["loop", "ram", "zram", "dm-", "nbd", "sr", "fd", "md"];
    let read = |p: std::path::PathBuf| std::fs::read_to_string(p).ok().map(|s| s.trim().to_string());

    let mut names: Vec<String> = match std::fs::read_dir(root) {
        Ok(rd) => rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect(),
        Err(_) => return vec![],
    };
    names.sort();

    names
        .iter()
        .filter(|n| !LOGICAL.iter().any(|p| n.starts_with(p)))
        .map(|name| {
            let dir = root.join(name);
            let mut d = Disk::new(name);
            d.size_bytes = read(dir.join("size")).and_then(|s| s.parse::<u64>().ok()).map(|s| s * 512);
            d.model = read(dir.join("device/model")).filter(|m| !m.is_empty());
            d.kind = match read(dir.join("queue/rotational")).as_deref() {
                Some("1") => DiskKind::Hdd,
                Some("0") => DiskKind::Ssd,
                _ => DiskKind::Unknown,
            };
            d.removable = read(dir.join("removable")).as_deref() == Some("1");
            d.protocol = if name.starts_with("nvme") {
                Some("NVMe".into())
            } else if name.starts_with("mmcblk") {
                Some("MMC".into())
            } else if name.starts_with("vd") {
                Some("VirtIO".into())
            } else if name.starts_with("xvd") {
                Some("Xen".into())
            } else {
                None
            };
            // Disco virtual: o hipervisor costuma declarar "rotacional" sem ser verdade, e
            // não há SMART para ler.
            if d.protocol.as_deref().is_some_and(|p| p == "VirtIO" || p == "Xen") {
                d.kind = DiskKind::Unknown;
                d.smart = SmartStatus::NotSupported;
            }
            d
        })
        .collect()
}

// ---------------------------------------------------------- estatísticas

/// Mede um ponto de montagem com `statvfs` (vale para Linux e macOS).
pub fn statvfs_volume(device: &str, mount: &str, fstype: &str, read_only: bool) -> Option<Volume> {
    let c_path = CString::new(mount).ok()?;
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c_path.as_ptr(), &mut s) } != 0 {
        return None;
    }
    let block = if s.f_frsize > 0 { s.f_frsize as u64 } else { s.f_bsize as u64 };
    let total = s.f_blocks as u64 * block;
    let free_all = s.f_bfree as u64 * block;
    Some(Volume {
        device: device.to_string(),
        mount: mount.to_string(),
        fstype: fstype.to_string(),
        total_bytes: total,
        used_bytes: total.saturating_sub(free_all),
        free_bytes: s.f_bavail as u64 * block,
        inodes_total: Some(s.f_files as u64).filter(|n| *n > 0),
        inodes_free: Some(s.f_ffree as u64),
        read_only,
    })
}

// ------------------------------------------------------------- macOS

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use std::ffi::CStr;

    /// Roda um programa e devolve o stdout; o erro é a mensagem para o usuário.
    fn run(program: &str, args: &[&str]) -> Result<String, String> {
        match Command::new(program).args(args).output() {
            Ok(out) => Ok(String::from_utf8_lossy(&out.stdout).into_owned()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(format!("`{program}` não encontrado")),
            Err(e) => Err(format!("`{program}`: {e}")),
        }
    }

    pub fn collect() -> Report {
        let mut report = Report::new("macos");
        report.volumes = volumes();

        match run("diskutil", &["list", "physical"]) {
            Ok(list) => {
                for id in parse_diskutil_list(&list) {
                    match run("diskutil", &["info", &format!("/dev/{id}")]) {
                        Ok(info) => report.disks.push(parse_diskutil_info(&id, &info)),
                        Err(e) => report.notes.push(format!("{id}: {e}")),
                    }
                }
            }
            Err(e) => report.notes.push(format!("lista de discos indisponível: {e}")),
        }
        report
    }

    fn cstr(raw: &[libc::c_char]) -> String {
        unsafe { CStr::from_ptr(raw.as_ptr()) }.to_string_lossy().into_owned()
    }

    fn volumes() -> Vec<Volume> {
        let mut ptr: *mut libc::statfs = std::ptr::null_mut();
        // O buffer pertence à libc: não liberar.
        let n = unsafe { libc::getmntinfo(&mut ptr, libc::MNT_NOWAIT) };
        if n <= 0 || ptr.is_null() {
            return vec![];
        }
        let list = unsafe { std::slice::from_raw_parts(ptr, n as usize) };
        list.iter()
            .filter_map(|s| {
                let (device, mount, fstype) =
                    (cstr(&s.f_mntfromname), cstr(&s.f_mntonname), cstr(&s.f_fstypename));
                if !keep_macos_mount(&device, &fstype, &mount) {
                    return None;
                }
                let block = s.f_bsize as u64;
                let total = s.f_blocks * block;
                Some(Volume {
                    device,
                    mount,
                    fstype,
                    total_bytes: total,
                    used_bytes: total.saturating_sub(s.f_bfree * block),
                    free_bytes: s.f_bavail * block,
                    inodes_total: Some(s.f_files).filter(|n| *n > 0),
                    inodes_free: Some(s.f_ffree),
                    read_only: s.f_flags & (libc::MNT_RDONLY as u32) != 0,
                })
            })
            .collect()
    }
}

// ------------------------------------------------------------- Linux

#[cfg(target_os = "linux")]
mod linux {
    use super::*;

    pub fn collect() -> Report {
        let mut report = Report::new("linux");
        report.volumes = volumes(&std::fs::read_to_string("/proc/self/mounts").unwrap_or_default());
        report.disks = read_sysfs_disks(Path::new("/sys/block"));
        if report.disks.is_empty() {
            report.notes.push("nenhum disco encontrado em /sys/block (contêiner?)".into());
        }
        read_smart(&mut report);
        report
    }

    fn volumes(mounts: &str) -> Vec<Volume> {
        let wanted: Vec<MountEntry> = parse_proc_mounts(mounts).into_iter().filter(keep_linux_mount).collect();
        pick_mounts(wanted)
            .into_iter()
            .filter_map(|m| statvfs_volume(&m.device, &m.mount, &m.fstype, m.read_only))
            .collect()
    }

    fn read_smart(report: &mut Report) {
        if report.disks.is_empty() {
            return;
        }
        let is_root = unsafe { libc::geteuid() } == 0;
        for disk in report.disks.iter_mut().filter(|d| d.smart != SmartStatus::NotSupported) {
            match Command::new("smartctl").args(["-H", &format!("/dev/{}", disk.name)]).output() {
                Ok(out) => {
                    let (status, note) = parse_smartctl_health(&String::from_utf8_lossy(&out.stdout));
                    disk.smart = status;
                    disk.smart_note = note;
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    report.notes.push(
                        "saúde (SMART) não verificada: instale o smartmontools (`smartctl`)".into(),
                    );
                    return;
                }
                Err(e) => {
                    disk.smart_note = Some(e.to_string());
                }
            }
        }
        if !is_root && report.disks.iter().any(|d| d.smart == SmartStatus::Unknown) {
            report.notes.push("para ler o SMART no Linux rode com sudo".into());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const DISKUTIL_LIST: &str = "\
/dev/disk0 (internal, physical):
   #:                       TYPE NAME                    SIZE       IDENTIFIER
   0:      GUID_partition_scheme                        *251.0 GB   disk0
   1:             Apple_APFS_ISC Container disk1         524.3 MB   disk0s1

/dev/disk4 (external, physical):
   #:                       TYPE NAME                    SIZE       IDENTIFIER
   0:      GUID_partition_scheme                        *1.0 TB     disk4
";

    const DISKUTIL_INFO: &str = "\
   Device Identifier:         disk0
   Device / Media Name:       APPLE SSD AP0256Z

   Protocol:                  Apple Fabric
   SMART Status:              Verified

   Disk Size:                 251.0 GB (251000193024 Bytes) (exactly 490234752 512-Byte-Units)
   Device Location:           Internal
   Removable Media:           Fixed

   Solid State:               Yes
";

    fn vol(mount: &str, fstype: &str, used: u64, free: u64) -> Volume {
        Volume {
            device: format!("/dev/{}", mount.trim_start_matches('/')),
            mount: mount.into(),
            fstype: fstype.into(),
            total_bytes: used + free,
            used_bytes: used,
            free_bytes: free,
            inodes_total: None,
            inodes_free: None,
            read_only: false,
        }
    }

    fn report(os: &'static str, volumes: Vec<Volume>, disks: Vec<Disk>) -> Report {
        Report { os, disks, volumes, findings: vec![], notes: vec![] }
    }

    #[test]
    fn parses_diskutil_list_whole_disks() {
        assert_eq!(parse_diskutil_list(DISKUTIL_LIST), ["disk0", "disk4"]);
    }

    #[test]
    fn parses_diskutil_info() {
        let d = parse_diskutil_info("disk0", DISKUTIL_INFO);
        assert_eq!(d.model.as_deref(), Some("APPLE SSD AP0256Z"));
        assert_eq!(d.size_bytes, Some(251_000_193_024));
        assert_eq!(d.kind, DiskKind::Ssd);
        assert_eq!(d.internal, Some(true));
        assert!(!d.removable);
        assert_eq!(d.protocol.as_deref(), Some("Apple Fabric"));
        assert_eq!(d.smart, SmartStatus::Passed);
    }

    #[test]
    fn diskutil_smart_variants() {
        let smart = |v: &str| parse_diskutil_info("d", &format!("SMART Status: {v}")).smart;
        assert_eq!(smart("Verified"), SmartStatus::Passed);
        assert_eq!(smart("Failing"), SmartStatus::Failing);
        assert_eq!(smart("Not Supported"), SmartStatus::NotSupported);
        assert_eq!(smart("???"), SmartStatus::Unknown);
    }

    #[test]
    fn parses_proc_mounts_with_escapes_and_ro() {
        let text = "\
/dev/sda1 / ext4 rw,relatime 0 0
/dev/sdb1 /mnt/meu\\040disco ext4 ro,noatime 0 0
proc /proc proc rw 0 0
";
        let m = parse_proc_mounts(text);
        assert_eq!(m.len(), 3);
        assert_eq!(m[1].mount, "/mnt/meu disco");
        assert!(m[1].read_only && !m[0].read_only);
        assert!(keep_linux_mount(&m[0]) && !keep_linux_mount(&m[2]));
    }

    #[test]
    fn pick_mounts_prefers_shortest_mount_per_device() {
        let m = |d: &str, p: &str| MountEntry { device: d.into(), mount: p.into(), fstype: "ext4".into(), read_only: false };
        let picked = pick_mounts(vec![m("/dev/a", "/etc/hosts"), m("/dev/b", "/data"), m("/dev/a", "/"), m("/dev/a", "/etc/resolv.conf")]);
        assert_eq!(picked, [m("/dev/a", "/"), m("/dev/b", "/data")]);
    }

    #[test]
    fn linux_mount_filter_drops_snaps_and_loops_but_keeps_network() {
        let e = |d: &str, f: &str| MountEntry { device: d.into(), mount: "/m".into(), fstype: f.into(), read_only: false };
        assert!(!keep_linux_mount(&e("/dev/loop3", "squashfs")));
        assert!(!keep_linux_mount(&e("tmpfs", "tmpfs")));
        assert!(!keep_linux_mount(&e("overlay", "overlay")));
        assert!(keep_linux_mount(&e("srv:/export", "nfs4")));
        assert!(keep_linux_mount(&e("tank/home", "zfs")));
    }

    #[test]
    fn macos_mount_filter_hides_technical_volumes() {
        assert!(keep_macos_mount("/dev/disk3s1s1", "apfs", "/"));
        assert!(keep_macos_mount("/dev/disk3s5", "apfs", "/System/Volumes/Data"));
        assert!(!keep_macos_mount("/dev/disk3s6", "apfs", "/System/Volumes/VM"));
        assert!(!keep_macos_mount("/dev/disk4s2", "hfs", "/private/var/folders/z2/abc/T/com.docker.install/Docker"));
        assert!(!keep_macos_mount("map auto_home", "autofs", "/System/Volumes/Data/home"));
        assert!(!keep_macos_mount("/dev/disk5s1", "apfs", "/Library/Developer/CoreSimulator/Volumes/iOS_1"));
        assert!(keep_macos_mount("//u@srv/share", "smbfs", "/Volumes/share"));
    }

    #[test]
    fn smartctl_output_variants() {
        let passed = "SMART overall-health self-assessment test result: PASSED";
        let failed = "SMART overall-health self-assessment test result: FAILED!";
        let scsi = "SMART Health Status: OK";
        let none = "SMART support is: Unavailable - device lacks SMART capability.";
        let denied = "Smartctl open device: /dev/sda failed: Permission denied";
        let raid = "Smartctl open device: /dev/sda failed: DELL or MegaRaid controller, please try adding '-d megaraid,N'";
        assert_eq!(parse_smartctl_health(passed).0, SmartStatus::Passed);
        assert_eq!(parse_smartctl_health(failed).0, SmartStatus::Failing);
        assert_eq!(parse_smartctl_health(scsi).0, SmartStatus::Passed);
        assert_eq!(parse_smartctl_health(none).0, SmartStatus::NotSupported);
        let (status, note) = parse_smartctl_health(denied);
        assert_eq!(status, SmartStatus::Unknown);
        assert!(note.unwrap().contains("sudo"));
        // Erro de abertura não pode virar "disco falhando".
        assert_eq!(parse_smartctl_health(raid).0, SmartStatus::Unknown);
        let undetected = "/dev/vda: Unable to detect device type\nPlease specify device type with the -d option.";
        let (status, note) = parse_smartctl_health(undetected);
        assert_eq!(status, SmartStatus::Unknown);
        assert!(note.unwrap().contains("-d"));
    }

    #[test]
    fn reads_sysfs_like_tree() {
        let root = tempfile::tempdir().unwrap();
        let mk = |name: &str, files: &[(&str, &str)]| {
            for (rel, content) in files {
                let p = root.path().join(name).join(rel);
                fs::create_dir_all(p.parent().unwrap()).unwrap();
                fs::write(p, content).unwrap();
            }
        };
        mk("sda", &[("size", "1953525168\n"), ("device/model", "ST1000DM010-2EP1  \n"), ("queue/rotational", "1\n"), ("removable", "0\n")]);
        mk("nvme0n1", &[("size", "1000215216\n"), ("device/model", "Samsung SSD 970\n"), ("queue/rotational", "0\n"), ("removable", "0\n")]);
        mk("vda", &[("size", "2048\n"), ("queue/rotational", "1\n")]);
        mk("loop0", &[("size", "100\n")]);
        mk("dm-0", &[("size", "100\n")]);

        let disks = read_sysfs_disks(root.path());
        let names: Vec<&str> = disks.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["nvme0n1", "sda", "vda"]);
        // Disco virtual: o "rotacional" do hipervisor não vale e não há SMART.
        assert_eq!(disks[2].kind, DiskKind::Unknown);
        assert_eq!(disks[2].smart, SmartStatus::NotSupported);
        assert_eq!(disks[2].protocol.as_deref(), Some("VirtIO"));
        let sda = &disks[1];
        assert_eq!(sda.size_bytes, Some(1_953_525_168 * 512));
        assert_eq!(sda.model.as_deref(), Some("ST1000DM010-2EP1"));
        assert_eq!(sda.kind, DiskKind::Hdd);
        assert_eq!(disks[0].kind, DiskKind::Ssd);
        assert_eq!(disks[0].protocol.as_deref(), Some("NVMe"));
        assert!(read_sysfs_disks(Path::new("/caminho/que/nao/existe")).is_empty());
    }

    #[test]
    fn used_percent_matches_df_semantics() {
        assert_eq!(vol("/", "ext4", 90, 10).used_percent(), 90.0);
        assert_eq!(vol("/", "ext4", 0, 0).used_percent(), 0.0);
        let mut v = vol("/", "ext4", 1, 1);
        v.inodes_total = Some(100);
        v.inodes_free = Some(5);
        assert_eq!(v.inodes_used_percent(), Some(95.0));
    }

    #[test]
    fn analyze_flags_full_volumes_by_severity_and_sorts() {
        let r = report("linux", vec![vol("/ok", "ext4", 50, 50), vol("/quase", "ext4", 91, 9), vol("/cheio", "ext4", 96, 4)], vec![]);
        let f = analyze(&r);
        assert_eq!(f.len(), 2);
        assert_eq!((f[0].severity, f[0].subject.as_str()), (Severity::Critical, "/cheio"));
        assert_eq!((f[1].severity, f[1].subject.as_str()), (Severity::Warning, "/quase"));
    }

    #[test]
    fn full_read_only_volume_is_not_reported() {
        // Imagem de disco ou ISO cheia é normal.
        let full_ro = Volume { read_only: true, ..vol("/mnt/iso", "iso9660", 100, 0) };
        assert!(analyze(&report("linux", vec![full_ro], vec![])).is_empty());
    }

    #[test]
    fn apfs_volumes_of_one_container_are_reported_once() {
        let apfs = |dev: &str, mount: &str| Volume { device: dev.into(), ..vol(mount, "apfs", 92, 8) };
        let r = report("macos", vec![apfs("/dev/disk3s3s1", "/"), apfs("/dev/disk3s1", "/System/Volumes/Data"), apfs("/dev/disk5s1", "/Volumes/Outro")], vec![]);
        let f = analyze(&r);
        assert_eq!(f.len(), 2, "{f:?}");
        assert_eq!(f[0].subject, "/");
        assert_eq!(f[1].subject, "/Volumes/Outro");
    }

    #[test]
    fn analyze_flags_inode_exhaustion() {
        let mut v = vol("/dados", "ext4", 10, 90);
        v.inodes_total = Some(1000);
        v.inodes_free = Some(20);
        let f = analyze(&report("linux", vec![v], vec![]));
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].severity, Severity::Critical);
        assert!(f[0].message.contains("inodes"));
    }

    #[test]
    fn analyze_read_only_expected_vs_unexpected() {
        let ro = |mount: &str, fstype: &str| Volume { read_only: true, ..vol(mount, fstype, 10, 90) };
        // macOS: raiz selada, imagens de disco e iso são normais.
        let mac = report("macos", vec![ro("/", "apfs"), ro("/Volumes/CD", "cd9660"), ro("/Volumes/Img", "hfs")], vec![]);
        assert!(analyze(&mac).is_empty());
        // Linux: ext4 só leitura é suspeito; a mesma raiz no macOS não.
        let f = analyze(&report("linux", vec![ro("/", "ext4")], vec![]));
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].severity, Severity::Warning);
    }

    #[test]
    fn analyze_smart_failing_is_critical_unknown_is_info_only_with_note() {
        let mut failing = Disk::new("sda");
        failing.smart = SmartStatus::Failing;
        let mut unknown = Disk::new("sdb");
        unknown.smart_note = Some("sem permissão".into());
        let silent = Disk::new("sdc"); // Unknown sem nota: a nota geral do relatório já explica.
        let mut ok = Disk::new("sdd");
        ok.smart = SmartStatus::Passed;

        let f = analyze(&report("linux", vec![], vec![failing, unknown, silent, ok]));
        assert_eq!(f.len(), 2);
        assert_eq!((f[0].severity, f[0].subject.as_str()), (Severity::Critical, "sda"));
        assert_eq!((f[1].severity, f[1].subject.as_str()), (Severity::Info, "sdb"));
    }

    #[test]
    fn statvfs_measures_a_real_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let v = statvfs_volume("dev", tmp.path().to_str().unwrap(), "x", false).unwrap();
        assert!(v.total_bytes > 0 && v.free_bytes <= v.total_bytes);
        assert!(statvfs_volume("dev", "/caminho/que/nao/existe", "x", false).is_none());
    }

    #[test]
    fn collect_runs_on_this_machine() {
        let r = collect();
        assert!(["macos", "linux"].contains(&r.os));
        assert!(!r.volumes.is_empty(), "esperava ao menos um volume");
    }
}
