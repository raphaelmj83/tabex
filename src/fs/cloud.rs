//! Detecção de pastas sincronizadas de serviços de nuvem (OneDrive, Dropbox, Nextcloud...).

use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloudKind {
    OneDrive,
    Dropbox,
    Nextcloud,
    OwnCloud,
    GoogleDrive,
    ICloud,
}

impl CloudKind {
    pub fn label(self) -> &'static str {
        match self {
            CloudKind::OneDrive => "OneDrive",
            CloudKind::Dropbox => "Dropbox",
            CloudKind::Nextcloud => "Nextcloud",
            CloudKind::OwnCloud => "ownCloud",
            CloudKind::GoogleDrive => "Google Drive",
            CloudKind::ICloud => "iCloud Drive",
        }
    }

    pub fn color(self) -> egui::Color32 {
        match self {
            CloudKind::OneDrive => egui::Color32::from_rgb(0x0A, 0x64, 0xC8),
            CloudKind::Dropbox => egui::Color32::from_rgb(0x00, 0x61, 0xFF),
            CloudKind::Nextcloud => egui::Color32::from_rgb(0x00, 0x82, 0xC9),
            CloudKind::OwnCloud => egui::Color32::from_rgb(0x1D, 0x2D, 0x44),
            CloudKind::GoogleDrive => egui::Color32::from_rgb(0x1F, 0xA4, 0x63),
            CloudKind::ICloud => egui::Color32::from_rgb(0x3D, 0x9B, 0xE9),
        }
    }
}

#[derive(Clone, Debug)]
pub struct CloudFolder {
    pub kind: CloudKind,
    pub name: String,
    pub path: PathBuf,
}

fn push(out: &mut Vec<CloudFolder>, kind: CloudKind, name: impl Into<String>, path: PathBuf) {
    if path.is_dir() && !out.iter().any(|c| c.path == path) {
        out.push(CloudFolder {
            kind,
            name: name.into(),
            path,
        });
    }
}

fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_default()
}

/// Lê `info.json` do Dropbox: {"personal": {"path": "..."}, "business": {"path": "..."}}.
fn dropbox_from_info(out: &mut Vec<CloudFolder>, info: &Path) {
    let Ok(text) = std::fs::read_to_string(info) else {
        return;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
        return;
    };
    if let Some(obj) = v.as_object() {
        for (key, val) in obj {
            if let Some(p) = val.get("path").and_then(|p| p.as_str()) {
                let name = if key == "business" {
                    "Dropbox (Business)"
                } else {
                    "Dropbox"
                };
                push(out, CloudKind::Dropbox, name, PathBuf::from(p));
            }
        }
    }
}

/// Lê `nextcloud.cfg`/`owncloud.cfg`: linhas `...\localPath=C:/Users/x/Nextcloud/`.
fn nextcloud_from_cfg(out: &mut Vec<CloudFolder>, cfg: &Path, kind: CloudKind) {
    let Ok(text) = std::fs::read_to_string(cfg) else { return };
    let mut n = 0;
    for line in text.lines() {
        let line = line.trim();
        if let Some((k, v)) = line.split_once('=') {
            if k.trim_end().ends_with("localPath") {
                let p = PathBuf::from(v.trim().trim_end_matches(['/', '\\']));
                n += 1;
                let name = if n == 1 {
                    kind.label().to_owned()
                } else {
                    format!(
                        "{} ({})",
                        kind.label(),
                        p.file_name()
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_default()
                    )
                };
                push(out, kind, name, p);
            }
        }
    }
}

pub fn detect() -> Vec<CloudFolder> {
    let mut out = Vec::new();
    let home = home();

    // --- OneDrive ---
    if let Some(p) = std::env::var_os("OneDriveConsumer") {
        push(&mut out, CloudKind::OneDrive, "OneDrive - Pessoal", PathBuf::from(p));
    }
    if let Some(p) = std::env::var_os("OneDriveCommercial") {
        let p = PathBuf::from(p);
        let name = p
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "OneDrive - Trabalho".into());
        push(&mut out, CloudKind::OneDrive, name, p);
    }
    if let Some(p) = std::env::var_os("OneDrive") {
        push(&mut out, CloudKind::OneDrive, "OneDrive", PathBuf::from(p));
    }
    if let Ok(rd) = std::fs::read_dir(&home) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with("OneDrive") && e.path().is_dir() {
                push(&mut out, CloudKind::OneDrive, name.clone(), e.path());
            }
        }
    }

    // --- Dropbox ---
    for base in [dirs::data_local_dir(), dirs::config_dir(), Some(home.join(".dropbox"))]
        .into_iter()
        .flatten()
    {
        let info = if base.ends_with(".dropbox") {
            base.join("info.json")
        } else {
            base.join("Dropbox").join("info.json")
        };
        dropbox_from_info(&mut out, &info);
    }
    push(&mut out, CloudKind::Dropbox, "Dropbox", home.join("Dropbox"));

    // --- Nextcloud / ownCloud ---
    if let Some(cfg) = dirs::config_dir() {
        nextcloud_from_cfg(
            &mut out,
            &cfg.join("Nextcloud").join("nextcloud.cfg"),
            CloudKind::Nextcloud,
        );
        nextcloud_from_cfg(
            &mut out,
            &cfg.join("ownCloud").join("owncloud.cfg"),
            CloudKind::OwnCloud,
        );
    }
    push(&mut out, CloudKind::Nextcloud, "Nextcloud", home.join("Nextcloud"));
    push(&mut out, CloudKind::OwnCloud, "ownCloud", home.join("ownCloud"));

    // --- Google Drive / iCloud ---
    push(
        &mut out,
        CloudKind::GoogleDrive,
        "Google Drive",
        home.join("Google Drive"),
    );
    push(&mut out, CloudKind::GoogleDrive, "Google Drive", home.join("Meu Drive"));
    push(&mut out, CloudKind::ICloud, "iCloud Drive", home.join("iCloudDrive"));
    push(
        &mut out,
        CloudKind::ICloud,
        "iCloud Drive",
        home.join("Library/Mobile Documents/com~apple~CloudDocs"),
    );

    // macOS moderno: ~/Library/CloudStorage/<Provedor>-<conta>
    if let Ok(rd) = std::fs::read_dir(home.join("Library/CloudStorage")) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let kind = if name.starts_with("OneDrive") {
                CloudKind::OneDrive
            } else if name.starts_with("Dropbox") {
                CloudKind::Dropbox
            } else if name.starts_with("GoogleDrive") {
                CloudKind::GoogleDrive
            } else if name.to_lowercase().contains("nextcloud") {
                CloudKind::Nextcloud
            } else {
                continue;
            };
            push(&mut out, kind, name.replace('-', " - "), e.path());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_dropbox_info_and_nextcloud_cfg() {
        let dir = std::env::temp_dir().join(format!("tabex_cloud_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Dropbox")).unwrap();
        std::fs::create_dir_all(dir.join("Nuvem")).unwrap();
        let info = dir.join("info.json");
        std::fs::write(
            &info,
            format!(
                r#"{{"personal": {{"path": "{}"}}}}"#,
                dir.join("Dropbox").display().to_string().replace('\\', "\\\\")
            ),
        )
        .unwrap();
        let cfg = dir.join("nextcloud.cfg");
        std::fs::write(
            &cfg,
            format!(
                "[Accounts]\n0\\Folders\\1\\localPath={}/\n0\\Folders\\1\\journalPath=.sync.db\n",
                dir.join("Nuvem").display()
            ),
        )
        .unwrap();
        let mut out = Vec::new();
        dropbox_from_info(&mut out, &info);
        nextcloud_from_cfg(&mut out, &cfg, CloudKind::Nextcloud);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].kind, CloudKind::Dropbox);
        assert_eq!(out[1].kind, CloudKind::Nextcloud);
        assert_eq!(out[1].path, dir.join("Nuvem"));
        // duplicatas e pastas inexistentes são ignoradas
        push(&mut out, CloudKind::Dropbox, "x", dir.join("Dropbox"));
        push(&mut out, CloudKind::Dropbox, "y", dir.join("nao-existe"));
        assert_eq!(out.len(), 2);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
