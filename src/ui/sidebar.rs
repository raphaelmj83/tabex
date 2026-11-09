use crate::app::{Action, TabexApp};
use crate::fs::entry::standard_folders;
use egui::{vec2, Button, CollapsingHeader, Color32, RichText, Sense, Ui};
use std::path::{Path, PathBuf};

pub fn show(ui: &mut Ui, app: &mut TabexApp) {
    let mut actions: Vec<Action> = Vec::new();
    let current = app.session.active_tab().path.clone();
    let pinned = app.session.pinned.clone();
    let drives = app.drives.clone();
    let clouds = app.clouds.clone();
    let ag = app.session.active_group;
    let aw = app.session.active_window;

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        CollapsingHeader::new(RichText::new("Acesso rápido").strong())
            .default_open(true)
            .show(ui, |ui| {
                for p in &pinned {
                    let name = p
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| p.to_string_lossy().into_owned());
                    item(ui, Icon::Pinned, &name, p, &current, true, &mut actions);
                }
                if !pinned.is_empty() {
                    ui.separator();
                }
                for (label, p) in standard_folders() {
                    let icon = match label {
                        "Área de Trabalho" => Icon::Glyph("🖥"),
                        "Downloads" => Icon::Glyph("⬇"),
                        "Documentos" => Icon::Glyph("📄"),
                        "Imagens" => Icon::Glyph("🖼"),
                        "Músicas" => Icon::Glyph("🎵"),
                        "Vídeos" => Icon::Glyph("🎬"),
                        _ => Icon::Glyph("🏠"),
                    };
                    item(ui, icon, label, &p, &current, false, &mut actions);
                }
            });

        if !clouds.is_empty() {
            ui.add_space(6.0);
            CollapsingHeader::new(RichText::new("Nuvem").strong())
                .default_open(true)
                .show(ui, |ui| {
                    for c in &clouds {
                        item(
                            ui,
                            Icon::Cloud(c.kind.color()),
                            &c.name,
                            &c.path,
                            &current,
                            false,
                            &mut actions,
                        );
                    }
                });
        }

        ui.add_space(6.0);
        CollapsingHeader::new(RichText::new("Janelas").strong())
            .default_open(true)
            .show(ui, |ui| {
                for wi in 0..app.session.windows.len() {
                    let w = &app.session.windows[wi];
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
                        ui.painter().circle_filled(r.center(), 5.0, w.color.color());
                        let text = format!("{}  ({})", w.name, app.session.window_tab_count(wi));
                        let resp = ui.selectable_label(wi == aw, text);
                        if resp.clicked() && wi != aw {
                            actions.push(Action::SwitchWindow(wi));
                        }
                        if resp.double_clicked() {
                            actions.push(Action::EditWindow(wi));
                        }
                        resp.context_menu(|ui| {
                            if ui.button("✏ Renomear / cor…").clicked() {
                                actions.push(Action::EditWindow(wi));
                            }
                            if ui.button("✖ Fechar janela").clicked() {
                                actions.push(Action::CloseWindow(wi));
                            }
                        });
                    });
                }
                if ui
                    .add(Button::new(RichText::new("＋ Nova janela").small()).frame(false))
                    .clicked()
                {
                    actions.push(Action::NewWindow);
                }
            });

        ui.add_space(6.0);
        let title = format!("Grupos de abas · {}", app.session.active_workspace().name);
        CollapsingHeader::new(RichText::new(title).strong())
            .default_open(true)
            .show(ui, |ui| {
                for (gi, g) in app.session.groups.iter().enumerate() {
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
                        ui.painter().circle_filled(r.center(), 5.0, g.color.color());
                        let text = format!("{}  ({})", g.name, g.tabs.len());
                        let resp = ui.selectable_label(gi == ag, text);
                        if resp.clicked() {
                            actions.push(Action::ActivateGroup(gi));
                        }
                        if resp.double_clicked() {
                            actions.push(Action::EditGroup(gi));
                        }
                        resp.context_menu(|ui| {
                            if ui.button("✏ Renomear / cor…").clicked() {
                                actions.push(Action::EditGroup(gi));
                            }
                            if ui.button("＋ Nova aba neste grupo").clicked() {
                                actions.push(Action::ActivateGroup(gi));
                                actions.push(Action::NewTab);
                            }
                            if app.session.windows.len() > 1 {
                                ui.menu_button("➡ Mover grupo para a janela", |ui| {
                                    for (wi, w) in app.session.windows.iter().enumerate() {
                                        if wi == aw {
                                            continue;
                                        }
                                        let label = RichText::new(format!("●  {}", w.name)).color(w.color.color());
                                        if ui.button(label).clicked() {
                                            actions.push(Action::MoveGroupToWindow { g: gi, w: wi });
                                        }
                                    }
                                });
                            }
                            if ui.button("✖ Fechar grupo").clicked() {
                                actions.push(Action::CloseGroup(gi));
                            }
                        });
                    });
                }
                if ui
                    .add(Button::new(RichText::new("＋ Novo grupo").small()).frame(false))
                    .clicked()
                {
                    actions.push(Action::NewGroup);
                }
            });

        ui.add_space(6.0);
        CollapsingHeader::new(RichText::new("Este Computador").strong())
            .default_open(true)
            .show(ui, |ui| {
                for (label, p) in &drives {
                    item(ui, Icon::Drive, label, p, &current, false, &mut actions);
                }
            });
    });

    app.pending.extend(actions);
}

enum Icon {
    Pinned,
    Drive,
    Cloud(Color32),
    Glyph(&'static str),
}

fn item(ui: &mut Ui, icon: Icon, label: &str, path: &Path, current: &Path, is_pinned: bool, actions: &mut Vec<Action>) {
    let selected = current == path;
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
    let visuals = ui.visuals();
    if selected {
        ui.painter().rect_filled(rect, 4, visuals.selection.bg_fill);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, 4, visuals.widgets.hovered.weak_bg_fill);
    }
    let icon_rect = egui::Rect::from_min_size(rect.min + vec2(8.0, 5.0), vec2(16.0, 16.0));
    match icon {
        Icon::Pinned => {
            crate::ui::icons::folder(ui.painter(), icon_rect);
            ui.painter().text(
                icon_rect.right_top() + vec2(1.0, 0.0),
                egui::Align2::CENTER_CENTER,
                "📌",
                egui::FontId::proportional(9.0),
                visuals.text_color(),
            );
        }
        Icon::Drive => crate::ui::icons::folder(ui.painter(), icon_rect),
        Icon::Cloud(color) => crate::ui::icons::cloud_folder(ui.painter(), icon_rect, color),
        Icon::Glyph(g) => {
            ui.painter().text(
                icon_rect.center(),
                egui::Align2::CENTER_CENTER,
                g,
                egui::FontId::proportional(13.0),
                visuals.text_color(),
            );
        }
    }
    let text_color = if selected {
        visuals.selection.stroke.color
    } else {
        visuals.text_color()
    };
    let text_rect = egui::Rect::from_min_max(rect.min + vec2(32.0, 0.0), rect.max - vec2(4.0, 0.0));
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(text_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    child.add(
        egui::Label::new(RichText::new(label).color(text_color))
            .truncate()
            .selectable(false),
    );
    let resp = resp.on_hover_text(path.to_string_lossy());
    if resp.clicked() {
        actions.push(Action::Navigate(path.to_path_buf()));
    }
    if resp.middle_clicked() {
        actions.push(Action::OpenInNewTab(path.to_path_buf()));
    }
    let p: PathBuf = path.to_path_buf();
    resp.context_menu(|ui| {
        if ui.button("Abrir em nova aba").clicked() {
            actions.push(Action::OpenInNewTab(p.clone()));
        }
        if ui.button("Abrir em novo grupo").clicked() {
            actions.push(Action::OpenInNewGroup(p.clone()));
        }
        if ui.button("Abrir em nova janela").clicked() {
            actions.push(Action::OpenInNewWindow(p.clone()));
        }
        ui.separator();
        if is_pinned {
            if ui.button("Desafixar do Acesso rápido").clicked() {
                actions.push(Action::TogglePin(p.clone()));
            }
        } else if ui.button("📌 Fixar no Acesso rápido").clicked() {
            actions.push(Action::TogglePin(p.clone()));
        }
    });
}
