//! Arrastar e soltar arquivos dentro do Tabex (entre lista, abas, grupos, janelas e painel lateral).

use crate::app::Action;
use egui::{Align2, Color32, CornerRadius, FontId, Response, Stroke, StrokeKind, Ui};
use std::path::{Path, PathBuf};

/// Carga de arrastar: arquivos selecionados e a pasta de origem.
#[derive(Clone, Debug)]
pub struct FileDrag {
    pub paths: Vec<PathBuf>,
    pub from: PathBuf,
}

/// Raiz do volume (letra da unidade no Windows; "/" nos demais).
fn volume_root(p: &Path) -> String {
    let mut comps = p.components();
    match comps.next() {
        Some(std::path::Component::Prefix(pr)) => pr.as_os_str().to_string_lossy().to_lowercase(),
        _ => "/".to_owned(),
    }
}

/// Mover ou copiar? Como no Explorador: Shift força mover, Ctrl força copiar,
/// sem modificador move dentro do mesmo volume e copia entre volumes.
pub fn decide_cut(ui: &Ui, from: &Path, dest: &Path) -> bool {
    let mods = ui.input(|i| i.modifiers);
    if mods.shift {
        return true;
    }
    if mods.command {
        return false;
    }
    volume_root(from) == volume_root(dest)
}

/// Trata `resp` como alvo de soltura para a pasta `dest`: realça ao passar por cima e
/// gera `DropFiles` ao soltar. Retorna `true` se há um arrasto sobre o alvo.
pub fn drop_target(ui: &Ui, resp: &Response, dest: &Path, actions: &mut Vec<Action>) -> bool {
    let hovering = match resp.dnd_hover_payload::<FileDrag>() {
        Some(p) => !p.paths.iter().any(|x| x == dest),
        None => false,
    };
    if hovering {
        ui.painter().rect_stroke(
            resp.rect.shrink(1.0),
            CornerRadius::same(4),
            Stroke::new(2.0, ui.visuals().hyperlink_color),
            StrokeKind::Inside,
        );
    }
    if let Some(p) = resp.dnd_release_payload::<FileDrag>() {
        if !p.paths.iter().any(|x| x == dest) && p.from != dest {
            let cut = decide_cut(ui, &p.from, dest);
            actions.push(Action::DropFiles {
                paths: p.paths.clone(),
                dest: dest.to_path_buf(),
                cut,
            });
        }
    }
    hovering
}

/// Desenha a "sombra" do arrasto junto ao ponteiro (quantidade e nome do primeiro item).
pub fn ghost(ctx: &egui::Context) {
    let Some(p) = egui::DragAndDrop::payload::<FileDrag>(ctx) else {
        return;
    };
    let Some(pos) = ctx.pointer_hover_pos() else { return };
    let text = if p.paths.len() == 1 {
        p.paths[0]
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    } else {
        format!("{} itens", p.paths.len())
    };
    let mods = ctx.input(|i| i.modifiers);
    let hint = if mods.command {
        "  (copiar)"
    } else if mods.shift {
        "  (mover)"
    } else {
        ""
    };
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Tooltip,
        egui::Id::unique("file_drag_ghost"),
    ));
    let font = FontId::proportional(13.0);
    let galley = painter.layout_no_wrap(format!("{text}{hint}"), font.clone(), Color32::WHITE);
    let rect = egui::Rect::from_min_size(pos + egui::vec2(14.0, 10.0), galley.rect.size() + egui::vec2(16.0, 8.0));
    painter.rect_filled(
        rect,
        CornerRadius::same(6),
        Color32::from_rgba_unmultiplied(0x1A, 0x73, 0xE8, 230),
    );
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        format!("{text}{hint}"),
        font,
        Color32::WHITE,
    );
}
