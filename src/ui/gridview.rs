//! Visualização em ícones grandes (grade com miniaturas).

use crate::app::Action;
use crate::fs::entry::FileEntry;
use crate::fs::thumbs::ThumbService;
use crate::model::tab::{Row, Tab};
use crate::ui::theme::accent;
use egui::{vec2, Align2, Color32, CornerRadius, FontId, Rect, Sense, Stroke, StrokeKind, Ui};
use std::collections::HashSet;
use std::path::PathBuf;

use crate::ui::filelist::RowEvent;

pub const THUMB: u32 = 96;
const TILE_W: f32 = 128.0;
const TILE_H: f32 = 142.0;

#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut Ui,
    tab: &Tab,
    dark: bool,
    cut_set: &HashSet<PathBuf>,
    events: &mut Vec<RowEvent>,
    actions: &mut Vec<Action>,
    rects: &mut Vec<(usize, Rect)>,
    thumbs: Option<&ThumbService>,
) {
    let source = tab.source();
    let idxs: Vec<usize> = tab
        .rows
        .iter()
        .filter_map(|r| match r {
            Row::Entry(i) => Some(*i),
            _ => None,
        })
        .collect();
    let avail = ui.available_width();
    let cols = ((avail - 8.0) / TILE_W).floor().max(1.0) as usize;
    let nrows = idxs.len().div_ceil(cols);
    let scroll_to_row = tab
        .cursor
        .as_ref()
        .and_then(|c| idxs.iter().position(|&i| source[i].path == *c))
        .map(|p| p / cols);

    let mut area =
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .scroll_source(egui::scroll_area::ScrollSource {
                drag: egui::scroll_area::DragScroll::Never,
                ..Default::default()
            });
    if tab.scroll_to_cursor {
        if let Some(r) = scroll_to_row {
            area = area.vertical_scroll_offset(r as f32 * TILE_H);
        }
    }
    area.show_rows(ui, TILE_H, nrows, |ui, range| {
        for r in range {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.add_space(4.0);
                for c in 0..cols {
                    let Some(&i) = idxs.get(r * cols + c) else { break };
                    let e = &source[i];
                    tile(
                        ui,
                        i,
                        e,
                        tab,
                        dark,
                        cut_set.contains(&e.path),
                        thumbs,
                        events,
                        actions,
                        rects,
                    );
                }
            });
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn tile(
    ui: &mut Ui,
    i: usize,
    e: &FileEntry,
    tab: &Tab,
    dark: bool,
    is_cut: bool,
    thumbs: Option<&ThumbService>,
    events: &mut Vec<RowEvent>,
    actions: &mut Vec<Action>,
    rects: &mut Vec<(usize, Rect)>,
) {
    let (rect, resp) = ui.allocate_exact_size(vec2(TILE_W, TILE_H), Sense::click_and_drag());
    rects.push((i, rect));
    let selected = tab.selected.contains(&e.path);
    let visuals = ui.visuals();
    let inner = rect.shrink(3.0);
    if selected {
        ui.painter()
            .rect_filled(inner, CornerRadius::same(6), visuals.selection.bg_fill);
    } else if resp.hovered() {
        ui.painter()
            .rect_filled(inner, CornerRadius::same(6), visuals.widgets.hovered.weak_bg_fill);
    }
    if tab.cursor.as_deref() == Some(e.path.as_path()) {
        ui.painter().rect_stroke(
            inner,
            CornerRadius::same(6),
            Stroke::new(1.0, accent(dark)),
            StrokeKind::Inside,
        );
    }
    // Miniatura / ícone
    let img_rect = Rect::from_center_size(
        egui::pos2(rect.center().x, rect.top() + 10.0 + THUMB as f32 / 2.0),
        vec2(THUMB as f32, THUMB as f32),
    );
    let tex = thumbs.and_then(|t| t.thumb(e, THUMB));
    match tex {
        Some(t) => {
            let size = t.size_vec2();
            let scale = (THUMB as f32 / size.x).min(THUMB as f32 / size.y).min(1.0);
            let draw = Rect::from_center_size(img_rect.center(), size * scale);
            let tint = if is_cut {
                Color32::from_white_alpha(120)
            } else {
                Color32::WHITE
            };
            ui.painter().image(
                t.id(),
                draw,
                Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                tint,
            );
            if e.kind == crate::fs::entry::FileKind::Image {
                ui.painter().rect_stroke(
                    draw,
                    CornerRadius::same(2),
                    Stroke::new(1.0, Color32::from_black_alpha(40)),
                    StrokeKind::Outside,
                );
            }
        }
        None => {
            let icon_rect = Rect::from_center_size(img_rect.center(), vec2(56.0, 56.0));
            crate::ui::icons::paint(ui.painter(), icon_rect, e.kind, dark);
        }
    }
    // Nome (até duas linhas)
    let color = if is_cut {
        visuals.weak_text_color()
    } else if selected {
        visuals.selection.stroke.color
    } else {
        visuals.text_color()
    };
    let font = FontId::proportional(12.5);
    let galley = ui.painter().layout(e.name.clone(), font, color, TILE_W - 12.0);
    let text_rect = Rect::from_min_size(
        egui::pos2(rect.left() + 6.0, img_rect.bottom() + 6.0),
        vec2(TILE_W - 12.0, 32.0),
    );
    let clip = ui.painter().with_clip_rect(text_rect);
    let pos = egui::pos2(
        text_rect.center().x - galley.rect.width().min(TILE_W - 12.0) / 2.0,
        text_rect.top(),
    );
    clip.galley(pos, galley, color);
    let _ = Align2::CENTER_TOP;
    let resp = resp.on_hover_text(format!("{}\n{}", e.name, e.type_label));
    crate::ui::filelist::row_interactions(&resp, i, e, tab, false, events, actions);
}
