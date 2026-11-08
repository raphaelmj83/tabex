use crate::app::{Action, TabexApp};
use crate::model::color::GroupColor;
use crate::model::group::TabGroup;
use crate::model::session::Session;
use crate::model::tab::Tab;
use egui::{
    pos2, vec2, Align2, Button, Color32, CornerRadius, CursorIcon, FontId, Id, Label, Rect, RichText, Sense, Stroke,
    StrokeKind, TextStyle, Ui,
};

/// Carga de arrastar-e-soltar: aba sendo arrastada.
#[derive(Clone, Copy, Debug)]
pub struct TabDrag {
    pub g: usize,
    pub t: usize,
}

const TAB_H: f32 = 32.0;

pub fn show(ui: &mut Ui, app: &mut TabexApp) {
    let mut actions: Vec<Action> = Vec::new();
    let (ag, at) = app.session.active_pos();
    let ngroups = app.session.groups.len();

    egui::ScrollArea::horizontal()
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for gi in 0..ngroups {
                    let group = &app.session.groups[gi];
                    draw_group_chip(ui, gi, group, gi == ag, ngroups, &mut actions);
                    if group.collapsed {
                        ui.add_space(4.0);
                        continue;
                    }
                    let start_x = ui.cursor().min.x;
                    for ti in 0..group.tabs.len() {
                        draw_tab(
                            ui,
                            gi,
                            ti,
                            &group.tabs[ti],
                            group,
                            gi == ag && ti == at,
                            &app.session,
                            &mut actions,
                        );
                    }
                    let end_x = ui.cursor().min.x;
                    let y = ui.max_rect().bottom() - 1.0;
                    ui.painter().line_segment(
                        [pos2(start_x, y), pos2(end_x, y)],
                        Stroke::new(2.0, group.color.color()),
                    );
                    ui.add_space(6.0);
                }

                let add = ui
                    .add(
                        Button::new(RichText::new("＋").size(16.0))
                            .frame(false)
                            .min_size(vec2(28.0, TAB_H - 4.0)),
                    )
                    .on_hover_text("Nova aba (Ctrl+T)");
                if add.clicked() {
                    actions.push(Action::NewTab);
                }
                let newg = ui
                    .add(
                        Button::new(RichText::new("＋ Grupo").size(12.5))
                            .frame(false)
                            .min_size(vec2(0.0, TAB_H - 4.0)),
                    )
                    .on_hover_text("Novo grupo de abas (Ctrl+G)\nArraste uma aba até aqui para criar um grupo com ela");
                if newg.clicked() {
                    actions.push(Action::NewGroup);
                }
                if newg.dnd_hover_payload::<TabDrag>().is_some() {
                    ui.painter().rect_stroke(
                        newg.rect,
                        4,
                        Stroke::new(1.5, ui.visuals().hyperlink_color),
                        StrokeKind::Inside,
                    );
                }
                if let Some(p) = newg.dnd_release_payload::<TabDrag>() {
                    actions.push(Action::MoveTabTo {
                        g: p.g,
                        t: p.t,
                        target: None,
                        at: None,
                    });
                }
            });
        });

    app.pending.extend(actions);
}

fn draw_group_chip(
    ui: &mut Ui,
    gi: usize,
    group: &TabGroup,
    is_active: bool,
    ngroups: usize,
    actions: &mut Vec<Action>,
) {
    let color = group.color.color();
    let text = if group.collapsed {
        format!("{} ({})", group.name, group.tabs.len())
    } else {
        group.name.clone()
    };
    let font = FontId::proportional(12.5);
    let galley = ui
        .painter()
        .layout_no_wrap(text.clone(), font.clone(), Color32::PLACEHOLDER);
    let w = galley.rect.width() + 18.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(w, TAB_H - 4.0), Sense::click());
    let chip = Rect::from_center_size(rect.center(), vec2(w, 22.0));
    let fill = if is_active || resp.hovered() {
        color
    } else {
        group.color.tint(200)
    };
    ui.painter().rect_filled(chip, CornerRadius::same(11), fill);
    ui.painter().text(
        chip.center(),
        Align2::CENTER_CENTER,
        &text,
        font,
        group.color.text_color(),
    );

    if resp.dnd_hover_payload::<TabDrag>().is_some() {
        ui.painter().rect_stroke(
            chip.expand(2.0),
            CornerRadius::same(13),
            Stroke::new(2.0, ui.visuals().hyperlink_color),
            StrokeKind::Outside,
        );
    }
    if let Some(p) = resp.dnd_release_payload::<TabDrag>() {
        actions.push(Action::MoveTabTo {
            g: p.g,
            t: p.t,
            target: Some(gi),
            at: None,
        });
    }

    if resp.double_clicked() {
        actions.push(Action::EditGroup(gi));
    } else if resp.clicked() {
        if is_active {
            actions.push(Action::ToggleCollapse(gi));
        } else {
            actions.push(Action::ActivateGroup(gi));
        }
    }
    let resp = resp.on_hover_text(format!(
        "{} — {} {}\nClique: {} · Duplo clique: editar",
        group.name,
        group.tabs.len(),
        if group.tabs.len() == 1 { "aba" } else { "abas" },
        if is_active { "recolher/expandir" } else { "ativar grupo" }
    ));
    resp.context_menu(|ui| {
        ui.set_min_width(220.0);
        if ui.button("＋ Nova aba neste grupo").clicked() {
            actions.push(Action::ActivateGroup(gi));
            actions.push(Action::NewTab);
        }
        if ui.button("✏ Renomear grupo…").clicked() {
            actions.push(Action::EditGroup(gi));
        }
        color_submenu(ui, group.color, |c| actions.push(Action::SetGroupColor(gi, c)));
        ui.separator();
        if ui
            .button(if group.collapsed {
                "▸ Expandir grupo"
            } else {
                "▾ Recolher grupo"
            })
            .clicked()
        {
            actions.push(Action::ToggleCollapse(gi));
        }
        if ui
            .add_enabled(gi > 0, Button::new("◀ Mover grupo para a esquerda"))
            .clicked()
        {
            actions.push(Action::MoveGroup(gi, -1));
        }
        if ui
            .add_enabled(gi + 1 < ngroups, Button::new("▶ Mover grupo para a direita"))
            .clicked()
        {
            actions.push(Action::MoveGroup(gi, 1));
        }
        ui.separator();
        if ui.button("✖ Fechar grupo").clicked() {
            actions.push(Action::CloseGroup(gi));
        }
    });
}

pub fn color_submenu(ui: &mut Ui, current: GroupColor, mut on_pick: impl FnMut(GroupColor)) {
    ui.menu_button("🎨 Cor do grupo", |ui| {
        for c in GroupColor::ALL {
            let mark = if c == current { "●" } else { "○" };
            let label = RichText::new(format!("{mark}  {}", c.label())).color(c.color());
            if ui.button(label).clicked() {
                on_pick(c);
            }
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn draw_tab(
    ui: &mut Ui,
    gi: usize,
    ti: usize,
    tab: &Tab,
    group: &TabGroup,
    is_active: bool,
    session: &Session,
    actions: &mut Vec<Action>,
) {
    let title = tab.title();
    let font = TextStyle::Body.resolve(ui.style());
    let galley = ui.painter().layout_no_wrap(title.clone(), font, Color32::PLACEHOLDER);
    let w = (galley.rect.width() + 46.0).clamp(110.0, 230.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(w, TAB_H - 4.0), Sense::click_and_drag());
    let id = Id::unique(("tab", tab.id));
    let color = group.color.color();
    let visuals = ui.visuals().clone();

    // Arrastar para reordenar / mover de grupo.
    if resp.drag_started() {
        resp.dnd_set_drag_payload(TabDrag { g: gi, t: ti });
    }
    if resp.dragged() {
        ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
    }

    let painter = ui.painter();
    let top = CornerRadius {
        nw: 6,
        ne: 6,
        sw: 0,
        se: 0,
    };
    if is_active {
        painter.rect_filled(rect, top, visuals.window_fill);
        painter.rect_filled(
            Rect::from_min_size(rect.min, vec2(rect.width(), 3.0)),
            CornerRadius {
                nw: 6,
                ne: 6,
                sw: 0,
                se: 0,
            },
            color,
        );
    } else if resp.hovered() && !resp.dragged() {
        painter.rect_filled(rect, top, visuals.widgets.hovered.weak_bg_fill);
    }
    if let Some(p) = resp.dnd_hover_payload::<TabDrag>() {
        if !(p.g == gi && p.t == ti) {
            painter.line_segment(
                [rect.left_top() + vec2(1.0, 4.0), rect.left_bottom() + vec2(1.0, -2.0)],
                Stroke::new(3.0, visuals.hyperlink_color),
            );
        }
    }
    if let Some(p) = resp.dnd_release_payload::<TabDrag>() {
        if !(p.g == gi && p.t == ti) {
            actions.push(Action::MoveTabTo {
                g: p.g,
                t: p.t,
                target: Some(gi),
                at: Some(ti),
            });
        }
    }

    // Texto
    let text_color = if is_active {
        visuals.strong_text_color()
    } else {
        visuals.text_color()
    };
    let text_rect = Rect::from_min_max(rect.min + vec2(10.0, 0.0), rect.max - vec2(26.0, 0.0));
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(text_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    child.add(
        Label::new(RichText::new(&title).color(text_color))
            .truncate()
            .selectable(false),
    );

    // Botão fechar
    let close_rect = Rect::from_center_size(pos2(rect.right() - 14.0, rect.center().y), vec2(18.0, 18.0));
    let close = ui.interact(close_rect, id.with("close"), Sense::click());
    if close.hovered() {
        ui.painter()
            .rect_filled(close_rect, CornerRadius::same(4), visuals.widgets.hovered.bg_fill);
    }
    let show_close = is_active || resp.hovered() || close.hovered();
    if show_close {
        ui.painter().text(
            close_rect.center(),
            Align2::CENTER_CENTER,
            "✕",
            FontId::proportional(11.0),
            visuals.text_color(),
        );
    }

    if close.clicked() || resp.middle_clicked() {
        actions.push(Action::CloseTab(gi, ti));
    } else if resp.clicked() {
        actions.push(Action::ActivateTab(gi, ti));
    }

    let resp = resp.on_hover_text(tab.path.to_string_lossy());
    resp.context_menu(|ui| {
        ui.set_min_width(230.0);
        if ui
            .add(Button::new("＋ Nova aba à direita").shortcut_text("Ctrl+T"))
            .clicked()
        {
            actions.push(Action::ActivateTab(gi, ti));
            actions.push(Action::NewTab);
        }
        if ui.button("⧉ Duplicar aba").clicked() {
            actions.push(Action::DuplicateTab(gi, ti));
        }
        ui.separator();
        ui.menu_button("➡ Mover para o grupo", |ui| {
            for (oi, og) in session.groups.iter().enumerate() {
                if oi == gi {
                    continue;
                }
                let label = RichText::new(format!("●  {}", og.name)).color(og.color.color());
                if ui.button(label).clicked() {
                    actions.push(Action::MoveTabTo {
                        g: gi,
                        t: ti,
                        target: Some(oi),
                        at: None,
                    });
                }
            }
            if session.groups.len() > 1 {
                ui.separator();
            }
            if ui.button("＋ Novo grupo").clicked() {
                actions.push(Action::MoveTabTo {
                    g: gi,
                    t: ti,
                    target: None,
                    at: None,
                });
            }
        });
        if ui.add_enabled(ti > 0, Button::new("◀ Mover para a esquerda")).clicked() {
            actions.push(Action::ActivateTab(gi, ti));
            actions.push(Action::MoveTab(-1));
        }
        if ui
            .add_enabled(ti + 1 < group.tabs.len(), Button::new("▶ Mover para a direita"))
            .clicked()
        {
            actions.push(Action::ActivateTab(gi, ti));
            actions.push(Action::MoveTab(1));
        }
        ui.separator();
        if ui.add(Button::new("✖ Fechar aba").shortcut_text("Ctrl+W")).clicked() {
            actions.push(Action::CloseTab(gi, ti));
        }
        if ui
            .add_enabled(group.tabs.len() > 1, Button::new("Fechar outras abas"))
            .clicked()
        {
            actions.push(Action::CloseOtherTabs(gi, ti));
        }
        if ui
            .add_enabled(ti + 1 < group.tabs.len(), Button::new("Fechar abas à direita"))
            .clicked()
        {
            actions.push(Action::CloseTabsRight(gi, ti));
        }
    });
}
