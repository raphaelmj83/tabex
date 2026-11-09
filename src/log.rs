//! Registro em arquivo e captura de pânicos.
//!
//! O log fica em `tabex.log` na pasta de configuração (ex.: `C:\tabex\tabex.log`), com rotação
//! simples para `tabex.log.1` ao passar de 2 MB. Um pânico grava a mensagem, o local e o
//! *backtrace*, e deixa um marcador (`crash.flag`) que faz o Tabex avisar na próxima abertura.

use crate::model::session::Session;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;

static LOCK: Mutex<()> = Mutex::new(());

pub fn log_path() -> PathBuf {
    Session::config_dir().join("tabex.log")
}

fn crash_flag() -> PathBuf {
    Session::config_dir().join("crash.flag")
}

fn timestamp() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Acrescenta uma linha ao log (silencioso em caso de erro de disco).
pub fn log(level: &str, msg: &str) {
    let _guard = LOCK.lock();
    let path = log_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if std::fs::metadata(&path)
        .map(|m| m.len() > MAX_LOG_BYTES)
        .unwrap_or(false)
    {
        let _ = std::fs::rename(&path, path.with_extension("log.1"));
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{} [{level}] {msg}", timestamp());
    }
}

pub fn info(msg: impl AsRef<str>) {
    log("INFO", msg.as_ref());
}

pub fn warn(msg: impl AsRef<str>) {
    log("AVISO", msg.as_ref());
}

/// Instala o gancho de pânico: grava no log, cria o marcador e repassa ao gancho padrão.
pub fn init_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current().name().unwrap_or("?").to_owned();
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_default();
        let payload = if let Some(s) = info.payload().downcast_ref::<&str>() {
            (*s).to_owned()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "(sem mensagem)".to_owned()
        };
        let bt = std::backtrace::Backtrace::force_capture();
        log(
            "PÂNICO",
            &format!(
                "versão {} · thread {thread} · {location}\n{payload}\n{bt}",
                env!("CARGO_PKG_VERSION")
            ),
        );
        // Marcador lido na próxima abertura (só para a thread principal, que derruba o app).
        if thread == "main" {
            let _ = std::fs::write(crash_flag(), format!("{}\n{location}\n{payload}\n", timestamp()));
        }
        default(info);
    }));
}

/// Se o Tabex caiu na execução anterior, devolve o resumo e remove o marcador.
pub fn take_last_crash() -> Option<String> {
    let flag = crash_flag();
    let text = std::fs::read_to_string(&flag).ok()?;
    let _ = std::fs::remove_file(&flag);
    Some(text)
}
