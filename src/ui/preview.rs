//! Painel de visualização (Alt+P): imagem, texto, PDF (miniatura do shell no Windows) e informações.

use crate::app::TabexApp;
use crate::fs::entry::{format_date, format_size_human, FileEntry, FileKind};
use egui::{vec2, Color32, CornerRadius, Rect, RichText, Stroke, StrokeKind, Ui};
use std::path::PathBuf;

pub const THUMB: u32 = 512;
const TEXT_LIMIT: usize = 64 * 1024;

/// Cache do texto pré-visualizado (caminho, modificação, conteúdo).
#[derive(Default)]
pub struct PreviewCache {
    pub text: Option<(PathBuf, Option<std::time::SystemTime>, String, bool)>,
}

pub fn show(ui: &mut Ui, app: &mut TabexApp) {
    let tab = app.session.active_tab();
    let sel = tab.selected_entries();
    let dark = app.session.dark_theme;
    ui.add_space(4.0);
    match sel.len() {
        0 => {
            let folder = FileEntry::from_path(&tab.path);
            ui.heading(RichText::new(tab.title()).size(15.0));
            ui.weak(format!("{} itens nesta pasta", tab.visible_count()));
            if let Some(f) = folder {
                info_grid(ui, &f);
            }
        }
        1 => {
            let e = sel[0].clone();
            let thumbs = app.thumbs.as_ref();
            ui.horizontal(|ui| {
                crate::ui::icons::entry(ui, &e, 18.0, dark, thumbs);
                ui.add(egui::Label::new(RichText::new(&e.name).strong().size(14.0)).wrap());
            });
            ui.add_space(6.0);
            let avail = ui.available_width();
            // Imagem / miniatura
            if let Some(tex) = thumbs.and_then(|t| t.thumb(&e, THUMB)) {
                let size = tex.size_vec2();
                let scale = (avail / size.x).min(1.0).min(360.0 / size.y);
                let draw = size * scale;
                let (rect, _) = ui.allocate_exact_size(vec2(avail, draw.y), egui::Sense::hover());
                let img = Rect::from_center_size(rect.center(), draw);
                ui.painter().image(
                    tex.id(),
                    img,
                    Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
                ui.painter().rect_stroke(
                    img,
                    CornerRadius::same(2),
                    Stroke::new(1.0, Color32::from_black_alpha(40)),
                    StrokeKind::Outside,
                );
                ui.add_space(6.0);
            } else if e.kind == FileKind::Image || e.kind == FileKind::Pdf {
                ui.weak("Gerando visualização…");
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
            }
            // Texto
            if !e.is_dir && is_text_like(&e) {
                let cache = &mut app.preview_cache;
                let fresh = cache
                    .text
                    .as_ref()
                    .map(|(p, m, _, _)| *p == e.path && *m == e.modified)
                    .unwrap_or(false);
                if !fresh {
                    cache.text = Some((
                        e.path.clone(),
                        e.modified,
                        read_text_head(&e),
                        e.size as usize > TEXT_LIMIT,
                    ));
                }
                if let Some((_, _, text, truncated)) = &cache.text {
                    egui::Frame::new()
                        .fill(ui.visuals().extreme_bg_color)
                        .stroke(Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color))
                        .corner_radius(4)
                        .inner_margin(6)
                        .show(ui, |ui| {
                            egui::ScrollArea::both()
                                .max_height(360.0)
                                .auto_shrink([false, true])
                                .show(ui, |ui| {
                                    ui.add(egui::Label::new(RichText::new(text).monospace().size(12.0)).wrap());
                                });
                        });
                    if *truncated {
                        ui.weak("(mostrando os primeiros 64 KB)");
                    }
                }
            }
            ui.add_space(6.0);
            info_grid(ui, &e);
        }
        n => {
            ui.heading(RichText::new(format!("{n} itens selecionados")).size(15.0));
            let total: u64 = sel.iter().map(|e| e.size).sum();
            ui.label(format!("Tamanho: {}", format_size_human(total)));
            let dirs = sel.iter().filter(|e| e.is_dir).count();
            ui.weak(format!("{} pastas, {} arquivos", dirs, n - dirs));
        }
    }
}

fn is_text_like(e: &FileEntry) -> bool {
    matches!(e.kind, FileKind::Code | FileKind::Document if e.ext != "doc" && e.ext != "docx" && e.ext != "odt" && e.ext != "rtf" && e.ext != "pages")
        || matches!(
            e.ext.as_str(),
            "txt" | "md" | "csv" | "tsv" | "log" | "ini" | "cfg" | "conf" | "env" | "gitignore" | "srt" | "sub"
        )
        || (e.ext.is_empty() && e.size > 0 && e.size < 512 * 1024)
}

fn read_text_head(e: &FileEntry) -> String {
    use std::io::Read;
    let Ok(mut f) = std::fs::File::open(&e.path) else {
        return String::new();
    };
    let mut buf = vec![0u8; TEXT_LIMIT];
    let n = f.read(&mut buf).unwrap_or(0);
    buf.truncate(n);
    if buf.contains(&0) {
        return "(arquivo binário)".to_owned();
    }
    String::from_utf8_lossy(&buf).into_owned()
}

fn info_grid(ui: &mut Ui, e: &FileEntry) {
    egui::Grid::new(("preview_info", &e.path))
        .num_columns(2)
        .spacing(vec2(10.0, 3.0))
        .show(ui, |ui| {
            ui.weak("Tipo");
            ui.add(egui::Label::new(&e.type_label).wrap());
            ui.end_row();
            if !e.is_dir {
                ui.weak("Tamanho");
                ui.label(format_size_human(e.size));
                ui.end_row();
            }
            ui.weak("Modificado");
            ui.label(format_date(e.modified));
            ui.end_row();
            if let Ok(md) = std::fs::metadata(&e.path) {
                ui.weak("Criado");
                ui.label(format_date(md.created().ok()));
                ui.end_row();
            }
            ui.weak("Local");
            ui.add(egui::Label::new(RichText::new(e.parent_display()).small()).wrap());
            ui.end_row();
        });
}
