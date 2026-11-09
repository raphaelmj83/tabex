//! Integrações específicas de plataforma (menu de contexto do shell do Windows).

use std::path::{Path, PathBuf};

#[cfg(windows)]
pub mod drag_out;
#[cfg(windows)]
pub mod shell_menu;

/// Arrastar arquivos para fora do Tabex (Windows). Bloqueia até soltar; `Ok(true)` se moveu.
pub fn drag_out(paths: &[PathBuf]) -> Result<bool, String> {
    #[cfg(windows)]
    {
        drag_out::drag_out(paths).map(|o| o == drag_out::DragOutcome::Moved)
    }
    #[cfg(not(windows))]
    {
        let _ = paths;
        Err("Arrastar para outros programas só está disponível no Windows.".to_owned())
    }
}

pub fn drag_out_supported() -> bool {
    cfg!(windows)
}

/// Item extra do Tabex inserido no topo do menu nativo.
#[cfg_attr(not(windows), allow(dead_code))]
pub struct MenuExtra {
    pub id: u32,
    pub label: String,
}

pub struct NativeMenuResult {
    /// Id do item extra escolhido (None se o usuário escolheu um item do shell ou cancelou).
    pub extra: Option<u32>,
}

pub fn native_menu_supported() -> bool {
    cfg!(windows)
}

/// Mostra o menu de contexto nativo para `paths` (ou o menu de fundo da pasta `folder` se vazio).
/// Bloqueia até o menu fechar.
pub fn show_native_menu(
    hwnd: Option<isize>,
    paths: &[PathBuf],
    folder: &Path,
    extras: &[MenuExtra],
) -> Result<NativeMenuResult, String> {
    #[cfg(windows)]
    {
        shell_menu::show(hwnd, paths, folder, extras)
    }
    #[cfg(not(windows))]
    {
        let _ = (hwnd, paths, folder, extras);
        Err("O menu de contexto nativo só está disponível no Windows.".to_owned())
    }
}
