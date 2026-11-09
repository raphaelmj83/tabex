//! Arrastar arquivos do Tabex para outros programas (OLE `DoDragDrop` com objeto de dados do shell).

use std::path::{Path, PathBuf};
use windows::core::{BOOL, HRESULT, HSTRING, PCWSTR};
use windows::Win32::Foundation::{DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, DRAGDROP_S_USEDEFAULTCURSORS, S_OK};
use windows::Win32::System::Com::{IBindCtx, IDataObject};
use windows::Win32::System::Ole::{
    DoDragDrop, IDropSource, IDropSource_Impl, OleInitialize, DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_LINK,
    DROPEFFECT_MOVE, DROPEFFECT_NONE,
};
use windows::Win32::System::SystemServices::{MK_LBUTTON, MODIFIERKEYS_FLAGS};
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{IShellFolder, SHBindToParent, SHCreateDataObject, SHParseDisplayName};
use windows_core::implement;

#[implement(IDropSource)]
struct DropSource;

impl IDropSource_Impl for DropSource_Impl {
    fn QueryContinueDrag(&self, fescapepressed: BOOL, grfkeystate: MODIFIERKEYS_FLAGS) -> HRESULT {
        if fescapepressed.as_bool() {
            DRAGDROP_S_CANCEL
        } else if (grfkeystate & MK_LBUTTON) == MODIFIERKEYS_FLAGS(0) {
            DRAGDROP_S_DROP
        } else {
            S_OK
        }
    }

    fn GiveFeedback(&self, _dweffect: DROPEFFECT) -> HRESULT {
        DRAGDROP_S_USEDEFAULTCURSORS
    }
}

struct Pidl(*mut ITEMIDLIST);
impl Drop for Pidl {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { windows::Win32::System::Com::CoTaskMemFree(Some(self.0 as *const _)) };
        }
    }
}

/// Resultado do arrasto para fora.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragOutcome {
    Cancelled,
    Copied,
    Moved,
    Linked,
}

/// Inicia o arrasto OLE (bloqueia até soltar). Só itens da mesma pasta do primeiro são incluídos.
pub fn drag_out(paths: &[PathBuf]) -> Result<DragOutcome, String> {
    if paths.is_empty() {
        return Ok(DragOutcome::Cancelled);
    }
    unsafe {
        let _ = OleInitialize(None);
        let parent_dir = paths[0].parent().map(Path::to_path_buf);
        let mut pidls: Vec<Pidl> = Vec::new();
        let mut children: Vec<*const ITEMIDLIST> = Vec::new();
        let mut parent_pidl: Option<Pidl> = None;
        for p in paths.iter().filter(|p| p.parent().map(Path::to_path_buf) == parent_dir) {
            let w = HSTRING::from(p.as_os_str());
            let mut pidl: *mut ITEMIDLIST = std::ptr::null_mut();
            if SHParseDisplayName(PCWSTR(w.as_ptr()), None::<&IBindCtx>, &mut pidl, 0, None).is_err() {
                continue;
            }
            let pidl = Pidl(pidl);
            let mut child: *mut ITEMIDLIST = std::ptr::null_mut();
            let _sf: IShellFolder = SHBindToParent(pidl.0, Some(&mut child)).map_err(|e| e.to_string())?;
            children.push(child as *const ITEMIDLIST);
            pidls.push(pidl);
        }
        if children.is_empty() {
            return Err("Nenhum item válido para arrastar.".to_owned());
        }
        if let Some(dir) = &parent_dir {
            let w = HSTRING::from(dir.as_os_str());
            let mut pidl: *mut ITEMIDLIST = std::ptr::null_mut();
            if SHParseDisplayName(PCWSTR(w.as_ptr()), None::<&IBindCtx>, &mut pidl, 0, None).is_ok() {
                parent_pidl = Some(Pidl(pidl));
            }
        }
        let data: IDataObject = SHCreateDataObject(
            parent_pidl.as_ref().map(|p| p.0 as *const ITEMIDLIST),
            Some(&children),
            None::<&IDataObject>,
        )
        .map_err(|e| format!("Objeto de dados: {e}"))?;
        let source: IDropSource = DropSource.into();
        let mut effect = DROPEFFECT_NONE;
        let hr = DoDragDrop(
            &data,
            &source,
            DROPEFFECT_COPY | DROPEFFECT_MOVE | DROPEFFECT_LINK,
            &mut effect,
        );
        if hr == DRAGDROP_S_DROP {
            if effect & DROPEFFECT_MOVE != DROPEFFECT_NONE {
                Ok(DragOutcome::Moved)
            } else if effect & DROPEFFECT_LINK != DROPEFFECT_NONE {
                Ok(DragOutcome::Linked)
            } else {
                Ok(DragOutcome::Copied)
            }
        } else {
            Ok(DragOutcome::Cancelled)
        }
    }
}
