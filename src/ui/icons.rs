//! Ícones vetoriais simples (pasta e arquivo) desenhados com o painter do egui,
//! para um visual consistente em qualquer plataforma/fonte.

use crate::fs::entry::FileKind;
use egui::{pos2, vec2, Color32, CornerRadius, Painter, Rect, Sense, Stroke, StrokeKind, Ui};

pub fn kind_color(kind: FileKind) -> Color32 {
    match kind {
        FileKind::Folder => Color32::from_rgb(0xF5, 0xC2, 0x42),
        FileKind::Document => Color32::from_rgb(0x2B, 0x57, 0x9A),
        FileKind::Spreadsheet => Color32::from_rgb(0x21, 0x73, 0x46),
        FileKind::Presentation => Color32::from_rgb(0xC4, 0x3E, 0x1C),
        FileKind::Pdf => Color32::from_rgb(0xD9, 0x30, 0x25),
        FileKind::Image => Color32::from_rgb(0x1E, 0x8E, 0x3E),
        FileKind::Video => Color32::from_rgb(0x8E, 0x24, 0xAA),
        FileKind::Audio => Color32::from_rgb(0xE6, 0x7E, 0x22),
        FileKind::Archive => Color32::from_rgb(0x8D, 0x6E, 0x63),
        FileKind::Code => Color32::from_rgb(0x00, 0x7B, 0x83),
        FileKind::Executable => Color32::from_rgb(0x5F, 0x63, 0x68),
        FileKind::Font => Color32::from_rgb(0x45, 0x4A, 0x52),
        FileKind::Other => Color32::from_rgb(0x9A, 0xA0, 0xA6),
    }
}

/// Desenha o ícone de `kind` dentro de `rect` (ideal: 16x16 a 20x20).
pub fn paint(painter: &Painter, rect: Rect, kind: FileKind, dark: bool) {
    if kind == FileKind::Folder {
        folder(painter, rect);
    } else {
        file(painter, rect, kind_color(kind), dark);
    }
}

/// Aloca espaço e desenha o ícone, sem interação.
#[allow(dead_code)]
pub fn show(ui: &mut Ui, kind: FileKind, size: f32, dark: bool) {
    let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    paint(ui.painter(), rect, kind, dark);
}

/// Ícone de uma entrada: o do sistema (Windows), se disponível, senão o desenhado.
pub fn entry(
    ui: &mut Ui,
    e: &crate::fs::entry::FileEntry,
    size: f32,
    dark: bool,
    thumbs: Option<&crate::fs::thumbs::ThumbService>,
) {
    let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    if let Some(tex) = thumbs.and_then(|t| t.icon(e, 16)) {
        ui.painter().image(
            tex.id(),
            rect,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    } else {
        paint(ui.painter(), rect, e.kind, dark);
    }
}

pub fn folder(painter: &Painter, rect: Rect) {
    let r = rect.shrink2(vec2(rect.width() * 0.04, rect.height() * 0.12));
    let back = Color32::from_rgb(0xE3, 0xA9, 0x1B);
    let front = Color32::from_rgb(0xF7, 0xC9, 0x4C);
    // Aba superior
    let tab = Rect::from_min_size(r.min, vec2(r.width() * 0.45, r.height() * 0.28));
    painter.rect_filled(
        tab,
        CornerRadius {
            nw: 2,
            ne: 2,
            sw: 0,
            se: 0,
        },
        back,
    );
    // Corpo de trás
    let body = Rect::from_min_max(pos2(r.min.x, r.min.y + r.height() * 0.18), r.max);
    painter.rect_filled(body, CornerRadius::same(2), back);
    // Frente
    let frente = Rect::from_min_max(pos2(r.min.x, r.min.y + r.height() * 0.40), r.max);
    painter.rect_filled(frente, CornerRadius::same(2), front);
}

pub fn file(painter: &Painter, rect: Rect, accent: Color32, dark: bool) {
    let r = rect.shrink2(vec2(rect.width() * 0.16, rect.height() * 0.06));
    let paper = if dark {
        Color32::from_rgb(0xE8, 0xE8, 0xE8)
    } else {
        Color32::WHITE
    };
    let border = if dark {
        Color32::from_rgb(0x90, 0x90, 0x90)
    } else {
        Color32::from_rgb(0xB0, 0xB0, 0xB0)
    };
    painter.rect(
        r,
        CornerRadius::same(1),
        paper,
        Stroke::new(1.0, border),
        StrokeKind::Inside,
    );
    // Dobra do canto
    let fold = r.width() * 0.34;
    let a = pos2(r.max.x - fold, r.min.y);
    let b = pos2(r.max.x, r.min.y + fold);
    painter.line_segment([a, b], Stroke::new(1.0, border));
    painter.line_segment([a, pos2(a.x, b.y)], Stroke::new(1.0, border));
    painter.line_segment([pos2(a.x, b.y), b], Stroke::new(1.0, border));
    // Faixa colorida indicando o tipo
    let strip = Rect::from_min_max(
        pos2(r.min.x + 1.0, r.max.y - r.height() * 0.34),
        pos2(r.max.x - 1.0, r.max.y - 1.0),
    );
    painter.rect_filled(
        strip,
        CornerRadius {
            nw: 0,
            ne: 0,
            sw: 1,
            se: 1,
        },
        accent,
    );
}

/// Pasta na cor do serviço de nuvem, com uma pequena nuvem branca sobreposta.
pub fn cloud_folder(painter: &Painter, rect: Rect, color: Color32) {
    let r = rect.shrink2(vec2(rect.width() * 0.04, rect.height() * 0.12));
    let back = color.linear_multiply(0.85);
    let tab = Rect::from_min_size(r.min, vec2(r.width() * 0.45, r.height() * 0.28));
    painter.rect_filled(
        tab,
        CornerRadius {
            nw: 2,
            ne: 2,
            sw: 0,
            se: 0,
        },
        back,
    );
    let body = Rect::from_min_max(pos2(r.min.x, r.min.y + r.height() * 0.18), r.max);
    painter.rect_filled(body, CornerRadius::same(2), back);
    let front = Rect::from_min_max(pos2(r.min.x, r.min.y + r.height() * 0.40), r.max);
    painter.rect_filled(front, CornerRadius::same(2), color);
    // nuvem: três círculos + base
    let c = front.center() + vec2(0.0, r.height() * 0.05);
    let w = Color32::from_rgba_unmultiplied(255, 255, 255, 230);
    let s = r.width() * 0.11;
    painter.circle_filled(c + vec2(-s * 1.1, s * 0.3), s * 0.9, w);
    painter.circle_filled(c + vec2(0.0, -s * 0.3), s * 1.2, w);
    painter.circle_filled(c + vec2(s * 1.2, s * 0.3), s * 0.9, w);
    painter.rect_filled(
        Rect::from_center_size(c + vec2(0.0, s * 0.6), vec2(s * 4.2, s * 1.2)),
        CornerRadius::same(2),
        w,
    );
}
