//! Menu de contexto do shell do Windows (o mesmo do Explorador), via `IContextMenu`.
//!
//! Fluxo: caminho -> PIDL -> `IShellFolder` pai -> `IContextMenu` -> `QueryContextMenu` num
//! `HMENU` -> `TrackPopupMenuEx` -> `InvokeCommand`. Mensagens de menu (`WM_INITMENUPOPUP`,
//! `WM_DRAWITEM`, `WM_MEASUREITEM`, `WM_MENUCHAR`) são encaminhadas para `IContextMenu2/3`
//! por um *subclass* temporário da janela, para que submenus como "Enviar para" e "Abrir com"
//! funcionem.

use super::{MenuExtra, NativeMenuResult};
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use windows::core::{Interface, HSTRING, PCSTR, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::Com::{
    CoInitializeEx, CoTaskMemFree, IBindCtx, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_SHIFT};
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    DefSubclassProc, IContextMenu, IContextMenu2, IContextMenu3, IShellFolder, RemoveWindowSubclass, SHBindToObject,
    SHBindToParent, SHParseDisplayName, SetWindowSubclass, CMF_EXPLORE, CMF_EXTENDEDVERBS, CMF_NORMAL,
    CMIC_MASK_CONTROL_DOWN, CMIC_MASK_PTINVOKE, CMIC_MASK_SHIFT_DOWN, CMINVOKECOMMANDINFO, CMINVOKECOMMANDINFOEX,
    SEE_MASK_UNICODE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreatePopupMenu, DestroyMenu, GetCursorPos, GetForegroundWindow, InsertMenuW, PostMessageW, SetForegroundWindow,
    TrackPopupMenuEx, MF_BYPOSITION, MF_SEPARATOR, MF_STRING, SW_SHOWNORMAL, TPM_LEFTALIGN, TPM_RETURNCMD,
    TPM_RIGHTBUTTON, WM_DRAWITEM, WM_INITMENUPOPUP, WM_MEASUREITEM, WM_MENUCHAR, WM_NULL,
};

const ID_EXTRA_FIRST: u32 = 1;
const ID_EXTRA_LAST: u32 = 0x0FFF;
const ID_SHELL_FIRST: u32 = 0x1000;
const ID_SHELL_LAST: u32 = 0x7FFF;
const SUBCLASS_ID: usize = 0x5441_4258; // "TABX"

thread_local! {
    static COM_READY: Cell<bool> = const { Cell::new(false) };
    static MENU2: RefCell<Option<IContextMenu2>> = const { RefCell::new(None) };
    static MENU3: RefCell<Option<IContextMenu3>> = const { RefCell::new(None) };
}

/// PIDL absoluto, liberado com `CoTaskMemFree`.
struct Pidl(*mut ITEMIDLIST);

impl Drop for Pidl {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CoTaskMemFree(Some(self.0 as *const _)) };
        }
    }
}

struct Menu(windows::Win32::UI::WindowsAndMessaging::HMENU);

impl Drop for Menu {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyMenu(self.0);
        }
    }
}

fn ensure_com() {
    COM_READY.with(|c| {
        if !c.get() {
            // Ignora RPC_E_CHANGED_MODE: o winit já inicializa OLE/COM na thread principal.
            unsafe {
                let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
            }
            c.set(true);
        }
    });
}

fn parse(path: &Path) -> Result<Pidl, String> {
    let w = HSTRING::from(path.as_os_str());
    let mut pidl: *mut ITEMIDLIST = std::ptr::null_mut();
    unsafe { SHParseDisplayName(PCWSTR(w.as_ptr()), None::<&IBindCtx>, &mut pidl, 0, None) }
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Pidl(pidl))
}

unsafe extern "system" fn subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    unsafe {
        match msg {
            WM_MENUCHAR => {
                if let Some(m3) = MENU3.with(|m| m.borrow().clone()) {
                    let mut r = LRESULT(0);
                    if m3.HandleMenuMsg2(msg, wparam, lparam, Some(&mut r)).is_ok() {
                        return r;
                    }
                }
            }
            WM_INITMENUPOPUP | WM_DRAWITEM | WM_MEASUREITEM => {
                let ret = LRESULT(if msg == WM_INITMENUPOPUP { 0 } else { 1 });
                if let Some(m3) = MENU3.with(|m| m.borrow().clone()) {
                    if m3.HandleMenuMsg2(msg, wparam, lparam, None).is_ok() {
                        return ret;
                    }
                }
                if let Some(m2) = MENU2.with(|m| m.borrow().clone()) {
                    if m2.HandleMenuMsg(msg, wparam, lparam).is_ok() {
                        return ret;
                    }
                }
            }
            _ => {}
        }
        DefSubclassProc(hwnd, msg, wparam, lparam)
    }
}

fn key_down(vk: i32) -> bool {
    unsafe { GetKeyState(vk) < 0 }
}

pub fn show(
    hwnd: Option<isize>,
    paths: &[PathBuf],
    folder: &Path,
    extras: &[MenuExtra],
) -> Result<NativeMenuResult, String> {
    ensure_com();
    let hwnd = match hwnd {
        Some(h) if h != 0 => HWND(h as *mut core::ffi::c_void),
        _ => unsafe { GetForegroundWindow() },
    };

    unsafe {
        // ---- objeto IContextMenu ----
        // Mantém os PIDLs vivos durante toda a chamada (os filhos apontam para dentro deles).
        let mut pidls: Vec<Pidl> = Vec::new();
        let cm: IContextMenu = if paths.is_empty() {
            let pidl = parse(folder)?;
            let sf: IShellFolder =
                SHBindToObject(None::<&IShellFolder>, pidl.0, None::<&IBindCtx>).map_err(|e| format!("Pasta: {e}"))?;
            pidls.push(pidl);
            sf.CreateViewObject(hwnd).map_err(|e| format!("Menu da pasta: {e}"))?
        } else {
            // Só itens da mesma pasta (requisito do IShellFolder pai).
            let parent_dir = paths[0].parent().map(Path::to_path_buf);
            let same: Vec<&PathBuf> = paths
                .iter()
                .filter(|p| p.parent().map(Path::to_path_buf) == parent_dir)
                .collect();
            let mut children: Vec<*const ITEMIDLIST> = Vec::new();
            let mut parent: Option<IShellFolder> = None;
            for p in same {
                let Ok(pidl) = parse(p) else { continue };
                let mut child: *mut ITEMIDLIST = std::ptr::null_mut();
                let sf: IShellFolder = SHBindToParent(pidl.0, Some(&mut child)).map_err(|e| format!("Item: {e}"))?;
                if parent.is_none() {
                    parent = Some(sf);
                }
                children.push(child as *const ITEMIDLIST);
                pidls.push(pidl);
            }
            let parent = parent.ok_or_else(|| "Nenhum item válido para o menu.".to_owned())?;
            parent
                .GetUIObjectOf(hwnd, &children, None)
                .map_err(|e| format!("Menu dos itens: {e}"))?
        };

        // ---- HMENU com os extras do Tabex no topo ----
        let menu = Menu(CreatePopupMenu().map_err(|e| e.to_string())?);
        let mut pos = 0u32;
        let labels: Vec<HSTRING> = extras.iter().map(|e| HSTRING::from(e.label.as_str())).collect();
        for (ex, label) in extras.iter().zip(labels.iter()) {
            let id = ex.id.clamp(ID_EXTRA_FIRST, ID_EXTRA_LAST) as usize;
            InsertMenuW(menu.0, pos, MF_BYPOSITION | MF_STRING, id, PCWSTR(label.as_ptr()))
                .map_err(|e| e.to_string())?;
            pos += 1;
        }
        if !extras.is_empty() {
            InsertMenuW(menu.0, pos, MF_BYPOSITION | MF_SEPARATOR, 0, PCWSTR::null()).map_err(|e| e.to_string())?;
            pos += 1;
        }
        let shift = key_down(VK_SHIFT.0 as i32);
        let mut flags = CMF_NORMAL | CMF_EXPLORE;
        if shift {
            flags |= CMF_EXTENDEDVERBS;
        }
        let hr = cm.QueryContextMenu(menu.0, pos, ID_SHELL_FIRST, ID_SHELL_LAST, flags);
        if hr.is_err() {
            return Err(format!("QueryContextMenu falhou: {hr}"));
        }

        // ---- subclass para IContextMenu2/3 ----
        MENU2.with(|m| *m.borrow_mut() = cm.cast::<IContextMenu2>().ok());
        MENU3.with(|m| *m.borrow_mut() = cm.cast::<IContextMenu3>().ok());
        let subclassed = SetWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID, 0).as_bool();

        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let _ = SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenuEx(
            menu.0,
            TPM_LEFTALIGN.0 | TPM_RIGHTBUTTON.0 | TPM_RETURNCMD.0,
            pt.x,
            pt.y,
            hwnd,
            None,
        )
        .0 as u32;
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));

        if subclassed {
            let _ = RemoveWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID);
        }
        MENU2.with(|m| *m.borrow_mut() = None);
        MENU3.with(|m| *m.borrow_mut() = None);

        // ---- resultado ----
        if (ID_EXTRA_FIRST..=ID_EXTRA_LAST).contains(&cmd) {
            return Ok(NativeMenuResult { extra: Some(cmd) });
        }
        if (ID_SHELL_FIRST..=ID_SHELL_LAST).contains(&cmd) {
            let verb = (cmd - ID_SHELL_FIRST) as usize;
            let ctrl = key_down(VK_CONTROL.0 as i32);
            let dir = HSTRING::from(folder.as_os_str());
            let mut mask = SEE_MASK_UNICODE | CMIC_MASK_PTINVOKE; // CMIC_MASK_UNICODE == SEE_MASK_UNICODE
            if shift {
                mask |= CMIC_MASK_SHIFT_DOWN;
            }
            if ctrl {
                mask |= CMIC_MASK_CONTROL_DOWN;
            }
            let info = CMINVOKECOMMANDINFOEX {
                cbSize: std::mem::size_of::<CMINVOKECOMMANDINFOEX>() as u32,
                fMask: mask,
                hwnd,
                lpVerb: PCSTR(verb as *const u8),
                lpVerbW: PCWSTR(verb as *const u16),
                lpDirectoryW: PCWSTR(dir.as_ptr()),
                nShow: SW_SHOWNORMAL.0,
                ptInvoke: pt,
                ..Default::default()
            };
            cm.InvokeCommand(&info as *const CMINVOKECOMMANDINFOEX as *const CMINVOKECOMMANDINFO)
                .map_err(|e| format!("Comando do menu: {e}"))?;
        }
        Ok(NativeMenuResult { extra: None })
    }
}
