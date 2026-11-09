//! Integração com a área de transferência do sistema para arquivos.
//!
//! No Windows usa `CF_HDROP` + `Preferred DropEffect`, os mesmos formatos do Explorador, para
//! copiar/recortar aqui e colar lá (e vice-versa). Em outros sistemas só o texto é usado.

use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct SysClip {
    pub paths: Vec<PathBuf>,
    pub cut: bool,
}

#[cfg(windows)]
mod imp {
    use super::SysClip;
    use clipboard_win::{formats, raw, Clipboard, Getter};
    use std::path::PathBuf;

    const DROPEFFECT_COPY: u32 = 1;
    const DROPEFFECT_MOVE: u32 = 2;
    const DROPEFFECT_LINK: u32 = 4;

    fn drop_effect_format() -> Option<u32> {
        raw::register_format("Preferred DropEffect").map(|n| n.get())
    }

    pub fn write(paths: &[PathBuf], cut: bool) -> Result<(), String> {
        let _clip = Clipboard::new_attempts(10).map_err(|e| format!("Área de transferência ocupada: {e}"))?;
        raw::empty().map_err(|e| e.to_string())?;
        let strs: Vec<String> = paths.iter().map(|p| p.to_string_lossy().into_owned()).collect();
        raw::set_file_list(&strs).map_err(|e| format!("CF_HDROP: {e}"))?;
        if let Some(fmt) = drop_effect_format() {
            let effect: u32 = if cut {
                DROPEFFECT_MOVE
            } else {
                DROPEFFECT_COPY | DROPEFFECT_LINK
            };
            let _ = raw::set_without_clear(fmt, &effect.to_le_bytes());
        }
        // Texto também, para colar caminhos em editores.
        let _ = raw::set_without_clear(formats::CF_UNICODETEXT, &utf16(&strs.join("\r\n")));
        Ok(())
    }

    fn utf16(s: &str) -> Vec<u8> {
        s.encode_utf16()
            .chain(std::iter::once(0))
            .flat_map(|u| u.to_le_bytes())
            .collect()
    }

    pub fn has_files() -> bool {
        raw::is_format_avail(formats::CF_HDROP)
    }

    pub fn read() -> Option<SysClip> {
        if !has_files() {
            return None;
        }
        let _clip = Clipboard::new_attempts(10).ok()?;
        let mut paths: Vec<PathBuf> = Vec::new();
        formats::FileList.read_clipboard(&mut paths).ok()?;
        if paths.is_empty() {
            return None;
        }
        let mut cut = false;
        if let Some(fmt) = drop_effect_format() {
            let mut bytes: Vec<u8> = Vec::new();
            if formats::RawData(fmt).read_clipboard(&mut bytes).is_ok() && bytes.len() >= 4 {
                let effect = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                cut = effect & DROPEFFECT_MOVE != 0 && effect & DROPEFFECT_COPY == 0;
            }
        }
        Some(SysClip { paths, cut })
    }

    pub fn sequence() -> Option<u32> {
        raw::seq_num().map(|n| n.get())
    }
}

#[cfg(not(windows))]
mod imp {
    use super::SysClip;
    use std::path::PathBuf;

    pub fn write(_paths: &[PathBuf], _cut: bool) -> Result<(), String> {
        Err("não suportado".to_owned())
    }
    pub fn has_files() -> bool {
        false
    }
    pub fn read() -> Option<SysClip> {
        None
    }
    pub fn sequence() -> Option<u32> {
        None
    }
}

/// Coloca os arquivos na área de transferência do sistema (Windows). Em outros sistemas retorna Err.
pub fn write(paths: &[PathBuf], cut: bool) -> Result<(), String> {
    imp::write(paths, cut)
}

/// Há uma lista de arquivos na área de transferência do sistema?
pub fn has_files() -> bool {
    imp::has_files()
}

/// Lê a lista de arquivos (e se foi "recortar") da área de transferência do sistema.
pub fn read() -> Option<SysClip> {
    imp::read()
}

/// Número de sequência da área de transferência (muda a cada alteração).
pub fn sequence() -> Option<u32> {
    imp::sequence()
}
