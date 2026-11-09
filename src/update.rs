//! Verificação e instalação de atualizações a partir das releases do GitHub.

use crate::model::session::Session;
use std::io::{Cursor, Read};
use std::sync::{Arc, Mutex};

pub const REPO: &str = "raphaelmj83/tabex";
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Debug)]
pub struct ReleaseInfo {
    pub tag: String,
    pub version: String,
    pub notes: String,
    pub html_url: String,
    pub asset_name: Option<String>,
    /// URL da API do asset (funciona em repositórios privados com token).
    pub asset_url: Option<String>,
    /// URL do arquivo SHA256SUMS.txt da release, se publicado.
    pub sums_url: Option<String>,
}

#[derive(Clone, Debug)]
pub enum UpdateState {
    Idle,
    Checking,
    UpToDate,
    Available(ReleaseInfo),
    Downloading(ReleaseInfo),
    Installed(ReleaseInfo),
    /// Versão anterior restaurada a partir do backup; precisa reiniciar.
    RolledBack,
    Error(String),
}

pub struct Updater {
    state: Arc<Mutex<UpdateState>>,
    /// Já avisou o usuário sobre a versão disponível nesta execução.
    pub notified: bool,
}

impl Default for Updater {
    fn default() -> Self {
        Self::new()
    }
}

impl Updater {
    pub fn new() -> Updater {
        Updater {
            state: Arc::new(Mutex::new(UpdateState::Idle)),
            notified: false,
        }
    }

    pub fn state(&self) -> UpdateState {
        self.state.lock().map(|s| s.clone()).unwrap_or(UpdateState::Idle)
    }

    pub fn is_busy(&self) -> bool {
        matches!(self.state(), UpdateState::Checking | UpdateState::Downloading(_))
    }

    /// Token opcional para repositórios privados: `github_token.txt` na pasta de configuração
    /// ou a variável de ambiente `TABEX_GITHUB_TOKEN`.
    pub fn token() -> Option<String> {
        if let Ok(t) = std::env::var("TABEX_GITHUB_TOKEN") {
            let t = t.trim().to_owned();
            if !t.is_empty() {
                return Some(t);
            }
        }
        std::fs::read_to_string(Self::token_path())
            .ok()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
    }

    pub fn token_path() -> std::path::PathBuf {
        Session::config_dir().join("github_token.txt")
    }

    /// Caminho do backup do executável anterior (`tabex.exe.bak`).
    pub fn backup_path() -> Option<std::path::PathBuf> {
        let exe = std::env::current_exe().ok()?;
        let mut name = exe.file_name()?.to_os_string();
        name.push(".bak");
        Some(exe.with_file_name(name))
    }

    pub fn has_backup() -> bool {
        Self::backup_path().map(|p| p.is_file()).unwrap_or(false)
    }

    /// Volta para o executável anterior guardado em `tabex.exe.bak`.
    pub fn rollback(&mut self) {
        if self.is_busy() {
            return;
        }
        let result = (|| -> Result<(), String> {
            let bak = Self::backup_path().ok_or("Sem backup.")?;
            if !bak.is_file() {
                return Err("Não há backup da versão anterior.".to_owned());
            }
            let bytes = std::fs::read(&bak).map_err(|e| e.to_string())?;
            install_bytes(&bytes, false)
        })();
        *self.state.lock().unwrap() = match result {
            Ok(()) => UpdateState::RolledBack,
            Err(e) => UpdateState::Error(e),
        };
    }

    pub fn check(&mut self) {
        if self.is_busy() {
            return;
        }
        *self.state.lock().unwrap() = UpdateState::Checking;
        let state = self.state.clone();
        std::thread::Builder::new()
            .name("tabex-update-check".into())
            .spawn(move || {
                let result = fetch_latest(Self::token().as_deref());
                let new_state = match result {
                    Ok(info) => {
                        if is_newer(&info.version, CURRENT) {
                            UpdateState::Available(info)
                        } else {
                            UpdateState::UpToDate
                        }
                    }
                    Err(e) => UpdateState::Error(e),
                };
                *state.lock().unwrap() = new_state;
            })
            .ok();
    }

    pub fn install(&mut self, info: ReleaseInfo) {
        if self.is_busy() {
            return;
        }
        *self.state.lock().unwrap() = UpdateState::Downloading(info.clone());
        let state = self.state.clone();
        std::thread::Builder::new()
            .name("tabex-update-install".into())
            .spawn(move || {
                let new_state = match download_and_install(&info, Self::token().as_deref()) {
                    Ok(()) => UpdateState::Installed(info),
                    Err(e) => UpdateState::Error(e),
                };
                *state.lock().unwrap() = new_state;
            })
            .ok();
    }
}

/// `a` é mais novo que `b`? (semver; tags podem ter o prefixo "v")
pub fn is_newer(a: &str, b: &str) -> bool {
    let pa = semver::Version::parse(a.trim_start_matches('v'));
    let pb = semver::Version::parse(b.trim_start_matches('v'));
    match (pa, pb) {
        (Ok(a), Ok(b)) => a > b,
        _ => false,
    }
}

/// Sufixo do zip para esta plataforma (igual ao gerado pelo workflow).
pub fn asset_suffix() -> &'static str {
    if cfg!(all(windows, target_arch = "x86_64")) {
        "windows-x64"
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        "linux-x64"
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "macos-arm64"
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        "macos-x64"
    } else {
        "unknown"
    }
}

/// Agente HTTP com o provedor TLS certo: nativo (Schannel) no Windows, rustls nos demais.
/// O padrão do `ureq` é rustls, que não está compilado no Windows e causava pânico.
fn agent() -> ureq::Agent {
    use ureq::tls::{TlsConfig, TlsProvider};
    let provider = if cfg!(windows) {
        TlsProvider::NativeTls
    } else {
        TlsProvider::Rustls
    };
    let config = ureq::Agent::config_builder()
        .tls_config(TlsConfig::builder().provider(provider).build())
        .build();
    ureq::Agent::new_with_config(config)
}

fn user_agent() -> String {
    format!("tabex/{CURRENT} (+https://github.com/{REPO})")
}

fn friendly(e: ureq::Error) -> String {
    match e {
        ureq::Error::StatusCode(404) => {
            "Release não encontrada (HTTP 404). Se o repositório for privado, salve um token do GitHub em github_token.txt na pasta de configuração.".to_owned()
        }
        ureq::Error::StatusCode(401) | ureq::Error::StatusCode(403) => {
            "Acesso negado pelo GitHub (HTTP 401/403). Verifique o token em github_token.txt.".to_owned()
        }
        other => format!("Falha de rede: {other}"),
    }
}

fn fetch_latest(token: Option<&str>) -> Result<ReleaseInfo, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let mut req = agent()
        .get(&url)
        .header("User-Agent", user_agent())
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28");
    if let Some(t) = token {
        req = req.header("Authorization", format!("Bearer {t}"));
    }
    let mut resp = req.call().map_err(friendly)?;
    let text = resp.body_mut().read_to_string().map_err(|e| e.to_string())?;
    parse_release(&text)
}

pub fn parse_release(json: &str) -> Result<ReleaseInfo, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("Resposta inválida do GitHub: {e}"))?;
    let tag = v
        .get("tag_name")
        .and_then(|t| t.as_str())
        .ok_or("Resposta sem tag_name")?
        .to_owned();
    let suffix = asset_suffix();
    let mut asset_name = None;
    let mut asset_url = None;
    let mut sums_url = None;
    if let Some(assets) = v.get("assets").and_then(|a| a.as_array()) {
        for a in assets {
            let name = a.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let url = a.get("url").and_then(|u| u.as_str()).map(str::to_owned);
            if name.eq_ignore_ascii_case("SHA256SUMS.txt") {
                sums_url = url;
            } else if asset_name.is_none() && name.contains(suffix) && name.ends_with(".zip") {
                asset_name = Some(name.to_owned());
                asset_url = url;
            }
        }
    }
    Ok(ReleaseInfo {
        version: tag.trim_start_matches('v').to_owned(),
        tag,
        notes: v.get("body").and_then(|b| b.as_str()).unwrap_or("").to_owned(),
        html_url: v.get("html_url").and_then(|u| u.as_str()).unwrap_or("").to_owned(),
        asset_name,
        asset_url,
        sums_url,
    })
}

fn download(url: &str, token: Option<&str>) -> Result<Vec<u8>, String> {
    let mut req = agent()
        .get(url)
        .header("User-Agent", user_agent())
        .header("Accept", "application/octet-stream")
        .header("X-GitHub-Api-Version", "2022-11-28");
    if let Some(t) = token {
        req = req.header("Authorization", format!("Bearer {t}"));
    }
    let mut resp = req.call().map_err(friendly)?;
    resp.body_mut()
        .with_config()
        .limit(512 * 1024 * 1024)
        .read_to_vec()
        .map_err(|e| format!("Falha ao baixar: {e}"))
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Procura o hash de `name` num SHA256SUMS.txt ("<hex>  <nome>" por linha).
pub fn find_sum(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let mut it = line.split_whitespace();
        let hash = it.next()?;
        let file = it.next()?.trim_start_matches('*');
        let file = file.rsplit('/').next().unwrap_or(file);
        (file == name && hash.len() == 64).then(|| hash.to_ascii_lowercase())
    })
}

fn download_and_install(info: &ReleaseInfo, token: Option<&str>) -> Result<(), String> {
    let url = info.asset_url.as_deref().ok_or_else(|| {
        format!(
            "A release {} não tem um pacote para esta plataforma ({}).",
            info.tag,
            asset_suffix()
        )
    })?;
    let bytes = download(url, token)?;

    // Integridade: confere o SHA-256 publicado na release (obrigatório quando o arquivo existe).
    if let (Some(sums_url), Some(name)) = (info.sums_url.as_deref(), info.asset_name.as_deref()) {
        let sums = String::from_utf8_lossy(&download(sums_url, token)?).into_owned();
        let expected = find_sum(&sums, name).ok_or_else(|| {
            format!("SHA256SUMS.txt da release não lista {name}; instalação cancelada por segurança.")
        })?;
        let actual = sha256_hex(&bytes);
        if actual != expected {
            return Err(format!(
                "O pacote baixado não confere com o hash publicado (esperado {}…, obtido {}…). Instalação cancelada.",
                &expected[..12],
                &actual[..12]
            ));
        }
    }

    let exe_bytes = extract_executable(&bytes)?;
    install_bytes(&exe_bytes, true)
}

/// Extrai `tabex.exe`/`tabex` do zip da release.
pub fn extract_executable(zip_bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(zip_bytes)).map_err(|e| format!("Zip inválido: {e}"))?;
    let wanted = if cfg!(windows) { "tabex.exe" } else { "tabex" };
    let mut idx = None;
    for i in 0..archive.len() {
        let f = archive.by_index(i).map_err(|e| e.to_string())?;
        let full = f.name().map(|n| n.into_owned()).unwrap_or_default();
        let name = full.rsplit(['/', '\\']).next().unwrap_or("").to_owned();
        if name.eq_ignore_ascii_case(wanted) && !f.is_dir() {
            idx = Some(i);
            break;
        }
    }
    let i = idx.ok_or_else(|| format!("O pacote não contém {wanted}."))?;
    let mut f = archive.by_index(i).map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(f.size() as usize);
    f.read_to_end(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

fn install_bytes(exe_bytes: &[u8], keep_backup: bool) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    if keep_backup {
        if let Some(bak) = Updater::backup_path() {
            std::fs::copy(&exe, &bak).map_err(|e| format!("Não foi possível criar o backup {}: {e}", bak.display()))?;
        }
    }
    let tmp = exe.with_extension("new");
    std::fs::write(&tmp, exe_bytes).map_err(|e| format!("Não foi possível gravar {}: {e}", tmp.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755));
    }
    let r = self_replace::self_replace(&tmp).map_err(|e| format!("Não foi possível substituir o executável: {e}"));
    let _ = std::fs::remove_file(&tmp);
    r
}

/// Reinicia o aplicativo (inicia uma nova instância; quem chama deve fechar a atual).
pub fn spawn_new_instance() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    std::process::Command::new(exe)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_compare() {
        assert!(is_newer("v0.2.0", "0.1.0"));
        assert!(is_newer("1.0.0", "v0.9.9"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.2.0"));
        assert!(!is_newer("abc", "0.1.0"));
    }

    #[test]
    fn parses_release_json_and_picks_platform_asset() {
        let json = format!(
            r#"{{"tag_name":"v9.9.9","html_url":"https://github.com/x/y/releases/tag/v9.9.9","body":"notas",
            "assets":[{{"name":"tabex-{}-v9.9.9.zip","url":"https://api.github.com/repos/x/y/releases/assets/1"}},
                      {{"name":"SHA256SUMS.txt","url":"https://api.github.com/repos/x/y/releases/assets/3"}},
                      {{"name":"outro.zip","url":"https://api.github.com/repos/x/y/releases/assets/2"}}]}}"#,
            asset_suffix()
        );
        let r = parse_release(&json).unwrap();
        assert_eq!(r.version, "9.9.9");
        assert_eq!(r.tag, "v9.9.9");
        assert_eq!(
            r.asset_url.as_deref(),
            Some("https://api.github.com/repos/x/y/releases/assets/1")
        );
        assert!(parse_release("{}").is_err());
    }

    #[test]
    fn sha256_and_sums_parsing() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let sums = "BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD  tabex-windows-x64-v1.zip\n\
                    0000000000000000000000000000000000000000000000000000000000000000 *dist/outro.zip\n";
        assert_eq!(
            find_sum(sums, "tabex-windows-x64-v1.zip").as_deref(),
            Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
        );
        assert_eq!(
            find_sum(sums, "outro.zip").as_deref(),
            Some("0000000000000000000000000000000000000000000000000000000000000000")
        );
        assert!(find_sum(sums, "nada.zip").is_none());
    }

    #[test]
    fn extracts_executable_from_zip() {
        let mut buf = Cursor::new(Vec::new());
        {
            let mut w = zip::ZipWriter::new(&mut buf);
            let opts = zip::write::SimpleFileOptions::default();
            w.start_file("README.txt", opts).unwrap();
            std::io::Write::write_all(&mut w, b"x").unwrap();
            let name = if cfg!(windows) { "tabex.exe" } else { "tabex" };
            w.start_file(name, opts).unwrap();
            std::io::Write::write_all(&mut w, b"BINARIO").unwrap();
            w.finish().unwrap();
        }
        let out = extract_executable(buf.get_ref()).unwrap();
        assert_eq!(out, b"BINARIO");
        assert!(extract_executable(b"nao e zip").is_err());
    }
}
