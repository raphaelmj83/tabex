//! Unidades e volumes: letra/caminho, rótulo, tipo, espaço livre e, para unidades de rede,
//! o caminho UNC. A enumeração roda em segundo plano porque unidades de rede fora do ar
//! podem demorar segundos para responder.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DriveKind {
    Fixed,
    Removable,
    Network,
    Optical,
    Ram,
    Unknown,
}

impl DriveKind {
    pub fn label(self) -> &'static str {
        match self {
            DriveKind::Fixed => "Disco Local",
            DriveKind::Removable => "Unidade removível",
            DriveKind::Network => "Unidade de rede",
            DriveKind::Optical => "Unidade de CD/DVD",
            DriveKind::Ram => "Disco em memória",
            DriveKind::Unknown => "Unidade",
        }
    }
}

#[derive(Clone, Debug)]
pub struct DriveInfo {
    /// Nome exibido: "Documentos (D:)", "projetos (\\\\servidor\\projetos) (Z:)".
    pub name: String,
    pub path: PathBuf,
    pub kind: DriveKind,
    pub total: u64,
    pub free: u64,
    /// Caminho UNC de uma unidade de rede mapeada.
    pub unc: Option<String>,
    /// Volume acessível no momento (unidades de rede podem estar fora do ar).
    pub ready: bool,
}

impl DriveInfo {
    pub fn used_fraction(&self) -> Option<f32> {
        if self.total == 0 {
            None
        } else {
            Some(((self.total - self.free.min(self.total)) as f64 / self.total as f64) as f32)
        }
    }
}

/// Serviço que mantém a lista de unidades atualizada em segundo plano.
pub struct DriveService {
    rx: Receiver<Vec<DriveInfo>>,
    tx_req: Sender<()>,
    pub drives: Vec<DriveInfo>,
    last_request: Instant,
}

impl Default for DriveService {
    fn default() -> Self {
        Self::new()
    }
}

impl DriveService {
    pub fn new() -> DriveService {
        let (tx, rx) = std::sync::mpsc::channel::<Vec<DriveInfo>>();
        let (tx_req, rx_req) = std::sync::mpsc::channel::<()>();
        std::thread::Builder::new()
            .name("tabex-drives".into())
            .spawn(move || {
                // Primeira enumeração imediata; depois, a pedido.
                let _ = tx.send(enumerate());
                while rx_req.recv().is_ok() {
                    // Agrupa pedidos acumulados.
                    while rx_req.try_recv().is_ok() {}
                    if tx.send(enumerate()).is_err() {
                        break;
                    }
                }
            })
            .ok();
        DriveService {
            rx,
            tx_req,
            drives: Vec::new(),
            last_request: Instant::now(),
        }
    }

    /// Recebe listas prontas e pede uma nova a cada 15 s. Retorna `true` se a lista mudou.
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        while let Ok(list) = self.rx.try_recv() {
            self.drives = list;
            changed = true;
        }
        if self.last_request.elapsed() > Duration::from_secs(15) {
            self.last_request = Instant::now();
            let _ = self.tx_req.send(());
        }
        changed
    }

    pub fn refresh_now(&mut self) {
        self.last_request = Instant::now();
        let _ = self.tx_req.send(());
    }

    pub fn network(&self) -> impl Iterator<Item = &DriveInfo> {
        self.drives.iter().filter(|d| d.kind == DriveKind::Network)
    }

    pub fn local(&self) -> impl Iterator<Item = &DriveInfo> {
        self.drives.iter().filter(|d| d.kind != DriveKind::Network)
    }
}

fn space(path: &std::path::Path) -> (u64, u64) {
    let total = fs4::total_space(path).unwrap_or(0);
    let free = fs4::available_space(path).unwrap_or(0);
    (total, free)
}

#[cfg(windows)]
pub fn enumerate() -> Vec<DriveInfo> {
    use windows::core::{HSTRING, PCWSTR, PWSTR};
    use windows::Win32::NetworkManagement::WNet::WNetGetConnectionW;
    use windows::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW};

    let mut out = Vec::new();
    let mask = unsafe { GetLogicalDrives() };
    for i in 0..26u32 {
        if mask & (1 << i) == 0 {
            continue;
        }
        let letter = (b'A' + i as u8) as char;
        let root = format!("{letter}:\\");
        let w = HSTRING::from(root.as_str());
        let kind = match unsafe { GetDriveTypeW(PCWSTR(w.as_ptr())) } {
            2 => DriveKind::Removable,
            3 => DriveKind::Fixed,
            4 => DriveKind::Network,
            5 => DriveKind::Optical,
            6 => DriveKind::Ram,
            1 => continue, // sem raiz
            _ => DriveKind::Unknown,
        };
        // Rótulo do volume (falha em unidades sem mídia ou de rede fora do ar).
        let mut name_buf = [0u16; 261];
        let ready =
            unsafe { GetVolumeInformationW(PCWSTR(w.as_ptr()), Some(&mut name_buf), None, None, None, None) }.is_ok();
        let label = {
            let n = name_buf.iter().position(|&c| c == 0).unwrap_or(name_buf.len());
            String::from_utf16_lossy(&name_buf[..n])
        };
        // Caminho UNC para unidades de rede.
        let mut unc = None;
        if kind == DriveKind::Network {
            let local = HSTRING::from(format!("{letter}:"));
            let mut buf = vec![0u16; 1024];
            let mut len = buf.len() as u32;
            let r = unsafe { WNetGetConnectionW(PCWSTR(local.as_ptr()), Some(PWSTR(buf.as_mut_ptr())), &mut len) };
            if r.0 == 0 {
                let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
                unc = Some(String::from_utf16_lossy(&buf[..n]));
            }
        }
        let (total, free) = if ready {
            space(std::path::Path::new(&root))
        } else {
            (0, 0)
        };
        let name = match (&unc, label.is_empty()) {
            (Some(u), true) => format!(
                "{} ({letter}:)",
                u.trim_start_matches('\\').split('\\').next_back().unwrap_or(u)
            ),
            (Some(u), false) => format!("{label} ({u}) ({letter}:)"),
            (None, true) => format!("{} ({letter}:)", kind.label()),
            (None, false) => format!("{label} ({letter}:)"),
        };
        out.push(DriveInfo {
            name,
            path: PathBuf::from(root),
            kind,
            total,
            free,
            unc,
            ready,
        });
    }
    out
}

#[cfg(not(windows))]
pub fn enumerate() -> Vec<DriveInfo> {
    let mut out = Vec::new();
    let mut push = |name: String, path: PathBuf, kind: DriveKind| {
        if !path.is_dir() || out.iter().any(|d: &DriveInfo| d.path == path) {
            return;
        }
        let (total, free) = space(&path);
        out.push(DriveInfo {
            name,
            path,
            kind,
            total,
            free,
            unc: None,
            ready: true,
        });
    };
    push(
        "Sistema de arquivos (/)".to_owned(),
        PathBuf::from("/"),
        DriveKind::Fixed,
    );
    // Montagens reais (exclui pseudo-sistemas de arquivos).
    if let Ok(text) = std::fs::read_to_string("/proc/mounts") {
        let user = std::env::var("USER").unwrap_or_default();
        for line in text.lines() {
            let mut it = line.split_whitespace();
            let (Some(dev), Some(mnt), Some(fstype)) = (it.next(), it.next(), it.next()) else {
                continue;
            };
            let mnt = mnt.replace("\\040", " ");
            let pseudo = matches!(
                fstype,
                "proc"
                    | "sysfs"
                    | "devtmpfs"
                    | "devpts"
                    | "tmpfs"
                    | "cgroup"
                    | "cgroup2"
                    | "pstore"
                    | "securityfs"
                    | "debugfs"
                    | "tracefs"
                    | "configfs"
                    | "fusectl"
                    | "mqueue"
                    | "hugetlbfs"
                    | "bpf"
                    | "autofs"
                    | "binfmt_misc"
                    | "overlay"
                    | "squashfs"
                    | "nsfs"
                    | "rpc_pipefs"
                    | "efivarfs"
                    | "ramfs"
            );
            if pseudo
                || mnt == "/"
                || mnt.starts_with("/boot")
                || mnt.starts_with("/snap")
                || mnt.starts_with("/var/lib")
            {
                continue;
            }
            let network = matches!(
                fstype,
                "nfs" | "nfs4" | "cifs" | "smb3" | "sshfs" | "fuse.sshfs" | "davfs" | "fuse.rclone"
            );
            let removable = mnt.starts_with("/media/") || mnt.starts_with("/run/media/");
            let interesting =
                network || removable || mnt.starts_with("/mnt/") || mnt == "/home" || dev.starts_with("/dev/");
            if !interesting {
                continue;
            }
            let base = mnt
                .rsplit('/')
                .next()
                .filter(|s| !s.is_empty())
                .unwrap_or(&mnt)
                .to_owned();
            let name = if network { format!("{base} ({dev})") } else { base };
            let kind = if network {
                DriveKind::Network
            } else if removable {
                DriveKind::Removable
            } else {
                DriveKind::Fixed
            };
            let _ = &user;
            push(name, PathBuf::from(&mnt), kind);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enumerates_at_least_one_drive_with_space() {
        let drives = enumerate();
        assert!(!drives.is_empty());
        let root = &drives[0];
        assert!(root.total > 0, "espaço total de {}", root.path.display());
        assert!(root.used_fraction().is_some());
    }
}
