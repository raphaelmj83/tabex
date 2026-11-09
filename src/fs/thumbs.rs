//! Ícones do sistema e miniaturas, carregados em segundo plano e guardados como texturas.
//!
//! Windows: `IShellItemImageFactory` (miniaturas e ícones reais do shell) e `SHGetFileInfoW`
//! (ícone pequeno por extensão). Demais sistemas: miniaturas de imagens com o crate `image`.

use crate::fs::entry::FileEntry;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Mutex;
use std::time::SystemTime;

/// Imagem RGBA crua pronta para virar textura.
pub struct Rgba {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Key {
    /// Ícone pequeno por tipo (extensão, ou "<dir>" / caminho completo para .exe/.lnk/.ico).
    Icon(String),
    /// Miniatura/ícone grande de um arquivo específico (caminho, data de modificação, tamanho em px).
    Thumb(PathBuf, u64, u32),
}

enum Slot {
    Loading,
    Ready(egui::TextureHandle),
    Missing,
}

struct Req {
    key: Key,
    path: PathBuf,
    is_dir: bool,
    size: u32,
}

pub struct ThumbService {
    tx: Sender<Req>,
    rx: Mutex<Receiver<(Key, Option<Rgba>)>>,
    cache: Mutex<HashMap<Key, Slot>>,
}

impl Default for ThumbService {
    fn default() -> Self {
        Self::new()
    }
}

impl ThumbService {
    pub fn new() -> ThumbService {
        let (tx, worker_rx) = std::sync::mpsc::channel::<Req>();
        let (done_tx, rx) = std::sync::mpsc::channel::<(Key, Option<Rgba>)>();
        std::thread::Builder::new()
            .name("tabex-thumbs".into())
            .spawn(move || {
                platform::init_thread();
                while let Ok(req) = worker_rx.recv() {
                    let img = match &req.key {
                        Key::Icon(_) => platform::small_icon(&req.path, req.is_dir, req.size),
                        Key::Thumb(..) => platform::thumbnail(&req.path, req.is_dir, req.size),
                    };
                    if done_tx.send((req.key, img)).is_err() {
                        break;
                    }
                }
            })
            .ok();
        ThumbService {
            tx,
            rx: Mutex::new(rx),
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// Chave do ícone pequeno de uma entrada.
    pub fn icon_key(e: &FileEntry) -> Key {
        if e.is_dir {
            Key::Icon("<dir>".to_owned())
        } else if matches!(e.ext.as_str(), "exe" | "lnk" | "ico" | "url" | "appref-ms") {
            Key::Icon(e.path.to_string_lossy().into_owned())
        } else {
            Key::Icon(format!(".{}", e.ext))
        }
    }

    pub fn thumb_key(e: &FileEntry, size: u32) -> Key {
        let mtime = e
            .modified
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Key::Thumb(e.path.clone(), mtime, size)
    }

    /// Textura do ícone pequeno (pede ao worker se ainda não existir).
    pub fn icon(&self, e: &FileEntry, size: u32) -> Option<egui::TextureHandle> {
        if !platform::has_system_icons() {
            return None;
        }
        self.lookup(Self::icon_key(e), e, size)
    }

    /// Textura da miniatura (ou ícone grande) de um arquivo.
    pub fn thumb(&self, e: &FileEntry, size: u32) -> Option<egui::TextureHandle> {
        if !platform::has_thumbnails(e) {
            return None;
        }
        self.lookup(Self::thumb_key(e, size), e, size)
    }

    fn lookup(&self, key: Key, e: &FileEntry, size: u32) -> Option<egui::TextureHandle> {
        let mut cache = self.cache.lock().ok()?;
        match cache.get(&key) {
            Some(Slot::Ready(t)) => Some(t.clone()),
            Some(_) => None,
            None => {
                cache.insert(key.clone(), Slot::Loading);
                drop(cache);
                let req = Req {
                    key,
                    path: e.path.clone(),
                    is_dir: e.is_dir,
                    size,
                };
                if self.tx.send(req).is_err() {
                    // worker morto: nada a fazer
                }
                None
            }
        }
    }

    /// Converte resultados prontos em texturas. Chamar uma vez por quadro. Retorna `true` se algo chegou.
    pub fn poll(&self, ctx: &egui::Context) -> bool {
        let mut any = false;
        let Ok(rx) = self.rx.lock() else { return false };
        let Ok(mut cache) = self.cache.lock() else { return false };
        let mut n = 0;
        while let Ok((key, img)) = rx.try_recv() {
            any = true;
            let slot = match img {
                Some(rgba) => {
                    let ci = egui::ColorImage::from_rgba_unmultiplied([rgba.width, rgba.height], &rgba.pixels);
                    let name = format!("thumb-{}", cache.len());
                    Slot::Ready(ctx.load_texture(name, ci, egui::TextureOptions::LINEAR))
                }
                None => Slot::Missing,
            };
            cache.insert(key, slot);
            n += 1;
            if n > 64 {
                break;
            }
        }
        any
    }

    /// Esquece miniaturas (ex.: arquivo alterado).
    #[allow(dead_code)]
    pub fn forget(&self, path: &Path) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.retain(|k, _| !matches!(k, Key::Thumb(p, _, _) if p == path));
        }
    }
}

#[cfg(windows)]
mod platform {
    use super::Rgba;
    use crate::fs::entry::FileEntry;
    use std::path::Path;
    use windows::core::{Interface, HSTRING, PCWSTR};
    use windows::Win32::Foundation::SIZE;
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO, BITMAPINFOHEADER,
        BI_RGB, DIB_RGB_COLORS, HBITMAP, HGDIOBJ,
    };
    use windows::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL};
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
    use windows::Win32::UI::Shell::{
        IShellItemImageFactory, SHCreateItemFromParsingName, SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON,
        SHGFI_SMALLICON, SHGFI_USEFILEATTRIBUTES, SIIGBF_BIGGERSIZEOK, SIIGBF_RESIZETOFIT,
    };
    use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, ICONINFO};

    pub fn init_thread() {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
    }

    pub fn has_system_icons() -> bool {
        true
    }

    pub fn has_thumbnails(_e: &FileEntry) -> bool {
        true
    }

    /// Lê um HBITMAP (32 bpp) como RGBA top-down.
    unsafe fn bitmap_to_rgba(hbm: HBITMAP) -> Option<Rgba> {
        unsafe {
            let mut bm = BITMAP::default();
            if GetObjectW(
                HGDIOBJ(hbm.0),
                std::mem::size_of::<BITMAP>() as i32,
                Some(&mut bm as *mut _ as *mut _),
            ) == 0
            {
                return None;
            }
            let (w, h) = (bm.bmWidth.max(1) as usize, bm.bmHeight.unsigned_abs() as usize);
            let hdc = CreateCompatibleDC(None);
            let mut info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w as i32,
                    biHeight: -(h as i32), // top-down
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut buf = vec![0u8; w * h * 4];
            let lines = GetDIBits(
                hdc,
                hbm,
                0,
                h as u32,
                Some(buf.as_mut_ptr() as *mut _),
                &mut info,
                DIB_RGB_COLORS,
            );
            let _ = DeleteDC(hdc);
            if lines == 0 {
                return None;
            }
            // BGRA -> RGBA. Se não houver canal alfa (tudo 0), considera opaco.
            let any_alpha = buf.chunks_exact(4).any(|p| p[3] != 0);
            for p in buf.chunks_exact_mut(4) {
                p.swap(0, 2);
                if !any_alpha {
                    p[3] = 255;
                }
            }
            Some(Rgba {
                width: w,
                height: h,
                pixels: buf,
            })
        }
    }

    unsafe fn icon_to_rgba(hicon: windows::Win32::UI::WindowsAndMessaging::HICON) -> Option<Rgba> {
        unsafe {
            let mut info = ICONINFO::default();
            GetIconInfo(hicon, &mut info).ok()?;
            let mut rgba = bitmap_to_rgba(info.hbmColor);
            // Ícones sem alfa usam a máscara: pixel da máscara = 1 -> transparente.
            if let Some(img) = rgba.as_mut() {
                if let Some(mask) = bitmap_to_rgba(info.hbmMask) {
                    if mask.width == img.width && mask.height >= img.height {
                        let all_opaque = img.pixels.chunks_exact(4).all(|p| p[3] == 255);
                        if all_opaque {
                            for (p, m) in img.pixels.chunks_exact_mut(4).zip(mask.pixels.chunks_exact(4)) {
                                if m[0] > 127 {
                                    p[3] = 0;
                                }
                            }
                        }
                    }
                }
            }
            let _ = DeleteObject(HGDIOBJ(info.hbmColor.0));
            let _ = DeleteObject(HGDIOBJ(info.hbmMask.0));
            rgba
        }
    }

    /// Ícone pequeno via SHGetFileInfo (por extensão quando possível, sem tocar no disco).
    pub fn small_icon(path: &Path, is_dir: bool, size: u32) -> Option<Rgba> {
        unsafe {
            let by_attr = is_dir
                || !matches!(
                    path.extension()
                        .and_then(|e| e.to_str())
                        .map(|e| e.to_lowercase())
                        .as_deref(),
                    Some("exe" | "lnk" | "ico" | "url" | "appref-ms")
                );
            let name: HSTRING = if by_attr && !is_dir {
                HSTRING::from(format!("x.{}", path.extension().and_then(|e| e.to_str()).unwrap_or("")))
            } else if by_attr {
                HSTRING::from("x")
            } else {
                HSTRING::from(path.as_os_str())
            };
            let mut sfi = SHFILEINFOW::default();
            let mut flags = SHGFI_ICON | if size <= 20 { SHGFI_SMALLICON } else { SHGFI_LARGEICON };
            let attrs = if is_dir {
                FILE_ATTRIBUTE_DIRECTORY
            } else {
                FILE_ATTRIBUTE_NORMAL
            };
            if by_attr {
                flags |= SHGFI_USEFILEATTRIBUTES;
            }
            let r = SHGetFileInfoW(
                PCWSTR(name.as_ptr()),
                attrs,
                Some(&mut sfi),
                std::mem::size_of::<SHFILEINFOW>() as u32,
                flags,
            );
            if r == 0 || sfi.hIcon.is_invalid() {
                return None;
            }
            let out = icon_to_rgba(sfi.hIcon);
            let _ = DestroyIcon(sfi.hIcon);
            out
        }
    }

    /// Miniatura (ou ícone grande) via IShellItemImageFactory.
    pub fn thumbnail(path: &Path, _is_dir: bool, size: u32) -> Option<Rgba> {
        unsafe {
            let w = HSTRING::from(path.as_os_str());
            let item: IShellItemImageFactory =
                SHCreateItemFromParsingName(PCWSTR(w.as_ptr()), None::<&windows::Win32::System::Com::IBindCtx>).ok()?;
            let hbm = item
                .GetImage(
                    SIZE {
                        cx: size as i32,
                        cy: size as i32,
                    },
                    SIIGBF_RESIZETOFIT | SIIGBF_BIGGERSIZEOK,
                )
                .ok()?;
            let out = bitmap_to_rgba(hbm);
            let _ = DeleteObject(HGDIOBJ(hbm.0));
            let _ = item.cast::<IShellItemImageFactory>();
            out
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::Rgba;
    use crate::fs::entry::{FileEntry, FileKind};
    use std::path::Path;

    pub fn init_thread() {}

    pub fn has_system_icons() -> bool {
        false
    }

    pub fn has_thumbnails(e: &FileEntry) -> bool {
        e.kind == FileKind::Image && e.size < 64 * 1024 * 1024
    }

    pub fn small_icon(_path: &Path, _is_dir: bool, _size: u32) -> Option<Rgba> {
        None
    }

    pub fn thumbnail(path: &Path, _is_dir: bool, size: u32) -> Option<Rgba> {
        super::decode_image_thumbnail(path, size)
    }
}

/// Miniatura de imagem com o crate `image` (usada fora do Windows e como reserva).
pub fn decode_image_thumbnail(path: &Path, size: u32) -> Option<Rgba> {
    let img = image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?;
    let thumb = img.thumbnail(size, size).to_rgba8();
    let (w, h) = thumb.dimensions();
    Some(Rgba {
        width: w as usize,
        height: h as usize,
        pixels: thumb.into_raw(),
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn decodes_image_thumbnail_with_image_crate() {
        let dir = std::env::temp_dir().join(format!("tabex_thumb_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.png");
        let img = image::RgbaImage::from_fn(64, 32, |x, _| image::Rgba([x as u8 * 4, 0, 0, 255]));
        img.save(&path).unwrap();
        let t = super::decode_image_thumbnail(&path, 16).expect("miniatura");
        assert_eq!((t.width, t.height), (16, 8));
        assert_eq!(t.pixels.len(), 16 * 8 * 4);
        assert!(super::decode_image_thumbnail(&dir.join("nao-existe.png"), 16).is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
