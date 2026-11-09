//! Barra de "Janelas" (espaços de trabalho), acima dos grupos de abas.

use crate::app::{Action, TabexApp};
use crate::ui::tabstrip::{color_submenu, TabDrag};
use egui::{vec2, Align2, Button, Color32, CornerRadius, FontId, Rect, RichText, Sense, Stroke, StrokeKind, Ui};

pub const HEIGHT: f32 = 30.0;

pub fn show(ui: &mut Ui, app: &mut TabexApp) {
    let mut actions: Vec<Action> = Vec::new();
    let aw = app.session.active_window;
    let n = app.session.windows.len();

    egui::ScrollArea::horizontal()
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.label(RichText::new("Janelas").small().weak());
                ui.add_space(2.0);
                for wi in 0..n {
                    draw_chip(ui, app, wi, wi == aw, n, &mut actions);
                }
                let add = ui
                    .add(
                        Button::new(RichText::new("＋").size(14.0))
                            .frame(false)
                            .min_size(vec2(24.0, 22.0)),
                    )
                    .on_hover_text("Nova janela (Ctrl+Alt+T)\nArraste uma aba até aqui para abri-la numa janela nova");
                if add.clicked() {
                    actions.push(Action::NewWindow);
                }
                if add.dnd_hover_payload::<TabDrag>().is_some() {
                    ui.painter().rect_stroke(
                        add.rect,
                        4,
                        Stroke::new(1.5, ui.visuals().hyperlink_color),
                        StrokeKind::Inside,
                    );
                }
                if let Some(p) = add.dnd_release_payload::<TabDrag>() {
                    actions.push(Action::MoveTabToNewWindow(p.g, p.t));
                }
            });
        });

    app.pending.extend(actions);
}

fn draw_chip(ui: &mut Ui, app: &TabexApp, wi: usize, active: bool, n: usize, actions: &mut Vec<Action>) {
    let w = &app.session.windows[wi];
    let color = w.color.color();
    let groups = app.session.window_group_count(wi);
    let tabs = app.session.window_tab_count(wi);
    let text = w.name.clone();
    let font = FontId::proportional(13.0);
    let galley = ui
        .painter()
        .layout_no_wrap(text.clone(), font.clone(), Color32::PLACEHOLDER);
    let width = galley.rect.width() + 30.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 24.0), Sense::click());
    let visuals = ui.visuals().clone();

    let (fill, stroke, text_color) = if active {
        (
            w.color.tint(if visuals.dark_mode { 70 } else { 40 }),
            Stroke::new(1.5, color),
            visuals.strong_text_color(),
        )
    } else if resp.hovered() {
        (
            visuals.widgets.hovered.weak_bg_fill,
            Stroke::new(1.0, visuals.widgets.hovered.bg_stroke.color),
            visuals.text_color(),
        )
    } else {
        (
            Color32::TRANSPARENT,
            Stroke::new(1.0, visuals.widgets.noninteractive.bg_stroke.color),
            visuals.text_color(),
        )
    };
    ui.painter()
        .rect(rect, CornerRadius::same(12), fill, stroke, StrokeKind::Inside);
    ui.painter()
        .circle_filled(rect.left_center() + vec2(12.0, 0.0), 5.0, color);
    ui.painter().text(
        rect.left_center() + vec2(22.0, 0.0),
        Align2::LEFT_CENTER,
        &text,
        font,
        text_color,
    );

    if resp.dnd_hover_payload::<TabDrag>().is_some() && !active {
        ui.painter().rect_stroke(
            rect.expand(2.0),
            CornerRadius::same(14),
            Stroke::new(2.0, visuals.hyperlink_color),
            StrokeKind::Outside,
        );
    }
    if let Some(p) = resp.dnd_release_payload::<TabDrag>() {
        if !active {
            actions.push(Action::MoveTabToWindow { g: p.g, t: p.t, w: wi });
        }
    }

    if resp.double_clicked() {
        actions.push(Action::EditWindow(wi));
    } else if resp.clicked() && !active {
        actions.push(Action::SwitchWindow(wi));
    }
    if resp.middle_clicked() {
        actions.push(Action::CloseWindow(wi));
    }

    let resp = resp.on_hover_text(format!(
        "{}\n{groups} {} · {tabs} {}\nDuplo clique: renomear / cor",
        w.name,
        if groups == 1 { "grupo" } else { "grupos" },
        if tabs == 1 { "aba" } else { "abas" }
    ));
    resp.context_menu(|ui| {
        ui.set_min_width(220.0);
        if ui
            .add(Button::new("＋ Nova janela").shortcut_text("Ctrl+Alt+T"))
            .clicked()
        {
            actions.push(Action::NewWindow);
        }
        if ui.button("✏ Renomear janela…").clicked() {
            actions.push(Action::EditWindow(wi));
        }
        color_submenu(ui, w.color, |c| actions.push(Action::SetWindowColor(wi, c)));
        ui.separator();
        if ui.add_enabled(wi > 0, Button::new("◀ Mover para a esquerda")).clicked() {
            actions.push(Action::MoveWindow(wi, -1));
        }
        if ui
            .add_enabled(wi + 1 < n, Button::new("▶ Mover para a direita"))
            .clicked()
        {
            actions.push(Action::MoveWindow(wi, 1));
        }
        ui.separator();
        if ui
            .add(Button::new("✖ Fechar janela").shortcut_text("Ctrl+Alt+W"))
            .clicked()
        {
            actions.push(Action::CloseWindow(wi));
        }
    });
    let _ = Rect::NOTHING;
}
