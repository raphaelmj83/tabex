use crate::app::{Action, TabexApp};
use crate::fs::entry::standard_folders;
use egui::{vec2, Button, CollapsingHeader, RichText, Sense, Ui};
use std::path::{Path, PathBuf};

pub fn show(ui: &mut Ui, app: &mut TabexApp) {
    let mut actions: Vec<Action> = Vec::new();
    let current = app.session.active_tab().path.clone();
    let pinned = app.session.pinned.clone();
    let drives = app.drives.clone();
    let ag = app.session.active_group;

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        CollapsingHeader::new(RichText::new("Acesso rápido").strong())
            .default_open(true)
            .show(ui, |ui| {
                for p in &pinned {
                    let name = p
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| p.to_string_lossy().into_owned());
                    item(ui, "📌", &name, p, &current, true, &mut actions);
                }
                if !pinned.is_empty() {
                    ui.separator();
                }
                for (label, p) in standard_folders() {
                    let icon = match label {
                        "Área de Trabalho" => "🖥",
                        "Downloads" => "⬇",
                        "Documentos" => "📄",
                        "Imagens" => "🖼",
                        "Músicas" => "🎵",
                        "Vídeos" => "🎬",
                        _ => "🏠",
                    };
                    item(ui, icon, label, &p, &current, false, &mut actions);
                }
            });

        ui.add_space(6.0);
        CollapsingHeader::new(RichText::new("Grupos de abas").strong())
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
                    item(ui, "💽", label, p, &current, false, &mut actions);
                }
            });
    });

    app.pending.extend(actions);
}

fn item(ui: &mut Ui, icon: &str, label: &str, path: &Path, current: &Path, is_pinned: bool, actions: &mut Vec<Action>) {
    let selected = current == path;
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
    let visuals = ui.visuals();
    if selected {
        ui.painter().rect_filled(rect, 4, visuals.selection.bg_fill);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, 4, visuals.widgets.hovered.weak_bg_fill);
    }
    let icon_rect = egui::Rect::from_min_size(rect.min + vec2(8.0, 5.0), vec2(16.0, 16.0));
    if icon == "📁" || icon == "💽" || icon == "📌" {
        crate::ui::icons::folder(ui.painter(), icon_rect);
        if icon == "📌" {
            ui.painter().text(
                icon_rect.right_top() + vec2(1.0, 0.0),
                egui::Align2::CENTER_CENTER,
                "📌",
                egui::FontId::proportional(9.0),
                visuals.text_color(),
            );
        }
    } else {
        ui.painter().text(
            icon_rect.center(),
            egui::Align2::CENTER_CENTER,
            icon,
            egui::FontId::proportional(13.0),
            visuals.text_color(),
        );
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
