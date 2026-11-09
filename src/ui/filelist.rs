use crate::app::{Action, TabexApp};
use crate::fs::entry::{format_date, format_size_kb, FileEntry};
use crate::fs::ops;
use crate::model::tab::{GroupBy, Row, SortKey, Tab};
use crate::ui::dnd::{self, FileDrag};
use crate::ui::theme::accent;
use egui::{
    vec2, Align, Align2, Button, Color32, CornerRadius, FontId, Key, Label, Layout, Modifiers, Pos2, Rect, RichText,
    Sense, Stroke, StrokeKind, TextEdit, Ui,
};
use egui_extras::{Column, TableBuilder};
use std::collections::HashSet;
use std::path::PathBuf;

pub enum RowEvent {
    Click(usize, Modifiers),
    Double(usize, Modifiers),
    Secondary(usize),
    Middle(usize),
    DragStart(usize),
}

/// Resultado da edição inline de nome.
enum RenameEvent {
    Commit,
    Cancel,
}

pub fn show(ui: &mut Ui, app: &mut TabexApp) {
    let mut actions: Vec<Action> = Vec::new();
    let (g, t) = app.session.active_pos();
    let dark = app.session.dark_theme;
    let cut_set: HashSet<PathBuf> = app
        .clipboard
        .as_ref()
        .filter(|c| c.cut)
        .map(|c| c.paths.iter().cloned().collect())
        .unwrap_or_default();
    let has_clip = app.clipboard.is_some() || app.sys_clip_has_files;
    let pinned: Vec<PathBuf> = app.session.pinned.clone();
    let native = app.native_menu_enabled();
    let view_icons = app.session.view_icons;
    let thumbs = app.thumbs.as_ref();
    let tab: &mut Tab = &mut app.session.groups[g].tabs[t];

    let bg_rect = ui.max_rect();
    let bg = ui.interact(
        bg_rect,
        ui.scope_id().with(("list_bg", tab.id)),
        Sense::click_and_drag(),
    );

    if let Some(err) = tab.error.clone() {
        ui.centered_and_justified(|ui| {
            ui.label(RichText::new(err).size(15.0).weak());
        });
        if native {
            if bg.secondary_clicked() {
                actions.push(Action::NativeMenu {
                    paths: Vec::new(),
                    folder: tab.path.clone(),
                });
            }
        } else {
            background_menu(&bg, tab, has_clip, &pinned, &mut actions);
        }
        app.pending.extend(actions);
        return;
    }

    let searching = tab.is_searching();
    let scroll_to = if tab.scroll_to_cursor {
        tab.scroll_to_cursor = false;
        tab.cursor_row()
    } else {
        None
    };

    // Estado da edição inline (copiado para dentro das closures da tabela).
    let mut rename_text: Option<(PathBuf, String, bool)> = tab.inline_rename.clone();
    let mut rename_event: Option<RenameEvent> = None;
    let mut events: Vec<RowEvent> = Vec::new();
    let mut row_rects: Vec<(usize, Rect)> = Vec::new();
    let rubber = tab.rubber.map(|start| {
        let now = ui.input(|i| i.pointer.interact_pos()).unwrap_or(start);
        Rect::from_two_pos(start, now)
    });

    if view_icons {
        crate::ui::gridview::show(
            ui,
            tab,
            dark,
            &cut_set,
            &mut events,
            &mut actions,
            &mut row_rects,
            app.thumbs.as_ref(),
        );
        if !native {
            // Menu de contexto por item na grade: reutiliza o menu de linha sobre a seleção atual.
            let _ = (&has_clip, &pinned);
        }
    } else {
        let tab_ref: &Tab = tab;
        let source = tab_ref.source();
        let row_h = 24.0;
        let mut builder = TableBuilder::new(ui)
            .id_salt(("filelist", tab_ref.id))
            .striped(false)
            .resizable(true)
            .auto_shrink([false, false])
            .drag_to_scroll(egui::scroll_area::DragScroll::Never)
            .sense(Sense::click_and_drag())
            .cell_layout(Layout::left_to_right(Align::Center))
            .column(Column::remainder().at_least(200.0).clip(true))
            .column(Column::initial(150.0).at_least(90.0).clip(true))
            .column(Column::initial(180.0).at_least(80.0).clip(true))
            .column(Column::initial(90.0).at_least(60.0).clip(true));
        if searching {
            builder = builder.column(Column::initial(280.0).at_least(100.0).clip(true));
        }
        if let Some(r) = scroll_to {
            builder = builder.scroll_to_row(r, Some(Align::Center));
        }

        let sort_key = tab_ref.sort_key;
        let sort_asc = tab_ref.sort_asc;
        let header_col = |ui: &mut Ui, key: SortKey, label: &str, actions: &mut Vec<Action>| {
            let arrow = if sort_key == key {
                if sort_asc {
                    " ▲"
                } else {
                    " ▼"
                }
            } else {
                ""
            };
            ui.add_space(6.0);
            let b = ui.add(
                Button::new(RichText::new(format!("{label}{arrow}")).color(ui.visuals().weak_text_color()))
                    .frame(false),
            );
            if b.clicked() {
                actions.push(Action::SetSort(key));
            }
        };

        builder
            .header(26.0, |mut header| {
                header.col(|ui| header_col(ui, SortKey::Name, "Nome", &mut actions));
                header.col(|ui| header_col(ui, SortKey::Modified, "Data de modificação", &mut actions));
                header.col(|ui| header_col(ui, SortKey::Kind, "Tipo", &mut actions));
                header.col(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        header_col(ui, SortKey::Size, "Tamanho", &mut actions);
                    });
                });
                if searching {
                    header.col(|ui| {
                        ui.add_space(6.0);
                        ui.label(RichText::new("Pasta").color(ui.visuals().weak_text_color()));
                    });
                }
            })
            .body(|body| {
                let rows = &tab_ref.rows;
                body.rows(row_h, rows.len(), |mut row| {
                    let ri = row.index();
                    match &rows[ri] {
                        Row::Header { label, count } => {
                            row.col(|ui| {
                                ui.add_space(6.0);
                                ui.label(
                                    RichText::new(format!("⌄ {label}  ({count})"))
                                        .strong()
                                        .color(ui.visuals().hyperlink_color),
                                );
                            });
                            row.col(|_| {});
                            row.col(|_| {});
                            row.col(|_| {});
                            if searching {
                                row.col(|_| {});
                            }
                            let r = row.response();
                            let y = r.rect.bottom() - 1.0;
                            r.ctx.layer_painter(r.layer_id).line_segment(
                                [egui::pos2(r.rect.left() + 8.0, y), egui::pos2(r.rect.right() - 8.0, y)],
                                Stroke::new(1.0, Color32::from_gray(if dark { 60 } else { 225 })),
                            );
                        }
                        Row::Entry(i) => {
                            let e: &FileEntry = &source[*i];
                            let selected = tab_ref.selected.contains(&e.path);
                            row.set_selected(selected);
                            let is_cut = cut_set.contains(&e.path);
                            let editing = rename_text.as_ref().map(|(p, _, _)| p == &e.path).unwrap_or(false);
                            row.col(|ui| {
                                let color = if is_cut {
                                    ui.visuals().weak_text_color()
                                } else {
                                    ui.visuals().text_color()
                                };
                                ui.add_space(6.0);
                                crate::ui::icons::entry(ui, e, 17.0, dark, thumbs);
                                ui.add_space(2.0);
                                if editing {
                                    if let Some((_, text, first)) = rename_text.as_mut() {
                                        let id = egui::Id::unique(("inline_rename", tab_ref.id));
                                        let resp = ui.add(
                                            TextEdit::singleline(text)
                                                .id(id)
                                                .desired_width(ui.available_width() - 8.0)
                                                .margin(egui::Margin::symmetric(4, 1)),
                                        );
                                        if *first {
                                            resp.request_focus();
                                            crate::ui::dialogs::select_stem(ui.ctx(), id, text, !e.is_dir);
                                            *first = false;
                                        }
                                        let enter = ui.input(|i| i.key_pressed(Key::Enter));
                                        let esc = ui.input(|i| i.key_pressed(Key::Escape));
                                        if esc {
                                            rename_event = Some(RenameEvent::Cancel);
                                        } else if enter || (resp.lost_focus() && !*first) {
                                            rename_event = Some(RenameEvent::Commit);
                                        }
                                    }
                                } else {
                                    ui.add(
                                        Label::new(RichText::new(&e.name).color(color))
                                            .truncate()
                                            .selectable(false),
                                    );
                                }
                            });
                            row.col(|ui| {
                                ui.add_space(6.0);
                                ui.label(RichText::new(format_date(e.modified)).color(ui.visuals().weak_text_color()));
                            });
                            row.col(|ui| {
                                ui.add_space(6.0);
                                ui.add(
                                    Label::new(RichText::new(&e.type_label).color(ui.visuals().weak_text_color()))
                                        .truncate()
                                        .selectable(false),
                                );
                            });
                            row.col(|ui| {
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.add_space(6.0);
                                    if !e.is_dir {
                                        ui.label(
                                            RichText::new(format_size_kb(e.size)).color(ui.visuals().weak_text_color()),
                                        );
                                    }
                                });
                            });
                            if searching {
                                row.col(|ui| {
                                    ui.add_space(6.0);
                                    ui.add(
                                        Label::new(
                                            RichText::new(e.parent_display()).color(ui.visuals().weak_text_color()),
                                        )
                                        .truncate()
                                        .selectable(false),
                                    );
                                });
                            }
                            let resp = row.response();
                            row_rects.push((*i, resp.rect));
                            if tab_ref.cursor.as_deref() == Some(e.path.as_path()) {
                                resp.ctx.layer_painter(resp.layer_id).rect_stroke(
                                    resp.rect.shrink(0.5),
                                    2,
                                    Stroke::new(1.0, accent(dark)),
                                    StrokeKind::Inside,
                                );
                            }
                            row_interactions(&resp, *i, e, tab_ref, editing, &mut events, &mut actions);
                            if !native && !editing {
                                resp.context_menu(|ui| item_menu(ui, e, tab_ref, has_clip, &pinned, &mut actions));
                            }
                        }
                    }
                });
            });
    }

    // Estados vazios / carregando
    if tab.rows.is_empty() {
        let msg = if tab.is_loading() {
            "Carregando…"
        } else if let Some(s) = &tab.search {
            if s.done {
                "Nenhum item corresponde à pesquisa."
            } else {
                "Pesquisando…"
            }
        } else if !tab.search_text.is_empty() || tab.quick_filter.is_active() {
            "Nenhum item corresponde ao filtro."
        } else {
            "Esta pasta está vazia."
        };
        ui.painter().text(
            bg_rect.center(),
            Align2::CENTER_CENTER,
            msg,
            FontId::proportional(15.0),
            ui.visuals().weak_text_color(),
        );
    }

    // Renomear inline: aplica resultado
    match rename_event {
        Some(RenameEvent::Commit) => {
            if let Some((path, text, _)) = rename_text.take() {
                let old = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if text.trim() != old && !text.trim().is_empty() {
                    match ops::rename(&path, &text) {
                        Ok(new_path) => {
                            actions.push(Action::PushUndo(ops::UndoEntry::new(
                                "renomear",
                                ops::UndoKind::Rename {
                                    from: path.clone(),
                                    to: new_path.clone(),
                                },
                            )));
                            tab.selected.clear();
                            tab.selected.insert(new_path.clone());
                            tab.cursor = Some(new_path);
                            tab.needs_reload = true;
                        }
                        Err(e) => actions.push(Action::Toast(format!("Não foi possível renomear: {e}"))),
                    }
                }
            }
            tab.inline_rename = None;
        }
        Some(RenameEvent::Cancel) => tab.inline_rename = None,
        None => tab.inline_rename = rename_text,
    }

    // Aplica eventos das linhas
    let mut clicked_row = false;
    for ev in events {
        match ev {
            RowEvent::Click(i, mods) => {
                clicked_row = true;
                if mods.command {
                    tab.toggle(i);
                } else if mods.shift {
                    tab.select_range(i);
                } else {
                    tab.select_only(i);
                }
            }
            RowEvent::Double(i, mods) => {
                clicked_row = true;
                if let Some(e) = tab.entry(i).cloned() {
                    if e.is_dir {
                        if mods.command {
                            actions.push(Action::OpenInNewTab(e.path));
                        } else {
                            actions.push(Action::Navigate(e.path));
                        }
                    } else {
                        actions.push(Action::Open(vec![e.path]));
                    }
                }
            }
            RowEvent::Secondary(i) => {
                clicked_row = true;
                if let Some(e) = tab.entry(i) {
                    if !tab.selected.contains(&e.path) {
                        tab.select_only(i);
                    }
                }
                if native {
                    let paths = tab.selected_paths();
                    if !paths.is_empty() {
                        actions.push(Action::NativeMenu {
                            paths,
                            folder: tab.path.clone(),
                        });
                    }
                }
            }
            RowEvent::Middle(i) => {
                if let Some(e) = tab.entry(i).cloned() {
                    if e.is_dir {
                        actions.push(Action::OpenInNewTab(e.path));
                    }
                }
            }
            RowEvent::DragStart(i) => {
                clicked_row = true;
                if let Some(e) = tab.entry(i) {
                    if !tab.selected.contains(&e.path) {
                        tab.select_only(i);
                    }
                }
                let paths = tab.selected_paths();
                if !paths.is_empty() {
                    egui::DragAndDrop::set_payload(
                        ui.ctx(),
                        FileDrag {
                            paths,
                            from: tab.path.clone(),
                        },
                    );
                }
            }
        }
    }

    // Seleção por retângulo (arrasto a partir do fundo)
    if bg.drag_started() && !egui::DragAndDrop::has_any_payload(ui.ctx()) {
        tab.rubber = ui.input(|i| i.pointer.interact_pos());
        tab.rubber_base = if ui.input(|i| i.modifiers.command) {
            tab.selected.clone()
        } else {
            HashSet::new()
        };
    }
    if let (Some(rect), true) = (rubber, tab.rubber.is_some()) {
        ui.painter().rect(
            rect,
            CornerRadius::ZERO,
            ui.visuals().selection.bg_fill.linear_multiply(0.5),
            Stroke::new(1.0, ui.visuals().hyperlink_color),
            StrokeKind::Inside,
        );
        let mut sel = tab.rubber_base.clone();
        for (i, r) in &row_rects {
            if r.intersects(rect) {
                if let Some(e) = tab.entry(*i) {
                    sel.insert(e.path.clone());
                }
            }
        }
        tab.selected = sel;
    }
    if bg.drag_stopped() || (tab.rubber.is_some() && !ui.input(|i| i.pointer.primary_down())) {
        tab.rubber = None;
    }

    if bg.clicked() && !clicked_row {
        tab.clear_selection();
        tab.inline_rename = None;
    }
    // Soltar arquivos no fundo da lista = colar nesta pasta
    dnd::drop_target(ui, &bg, &tab.path.clone(), &mut actions);

    if native {
        if bg.secondary_clicked() && !clicked_row {
            actions.push(Action::NativeMenu {
                paths: Vec::new(),
                folder: tab.path.clone(),
            });
        }
    } else {
        background_menu(&bg, tab, has_clip, &pinned, &mut actions);
    }

    app.pending.extend(actions);
}

/// Cliques, arrasto e alvo de soltura de uma linha/célula de item.
pub fn row_interactions(
    resp: &egui::Response,
    i: usize,
    e: &FileEntry,
    tab: &Tab,
    editing: bool,
    events: &mut Vec<RowEvent>,
    actions: &mut Vec<Action>,
) {
    if editing {
        return;
    }
    let mods = resp.ctx.input(|i| i.modifiers);
    if resp.double_clicked() {
        events.push(RowEvent::Double(i, mods));
    } else if resp.clicked() {
        events.push(RowEvent::Click(i, mods));
    }
    if resp.secondary_clicked() {
        events.push(RowEvent::Secondary(i));
    }
    if resp.middle_clicked() {
        events.push(RowEvent::Middle(i));
    }
    if resp.drag_started() {
        events.push(RowEvent::DragStart(i));
    }
    // Pastas aceitam soltar arquivos.
    if e.is_dir && !tab.selected.contains(&e.path) {
        if let Some(p) = resp.dnd_hover_payload::<FileDrag>() {
            if !p.paths.iter().any(|x| x == &e.path) {
                resp.ctx.layer_painter(resp.layer_id).rect_stroke(
                    resp.rect.shrink(1.0),
                    CornerRadius::same(4),
                    Stroke::new(2.0, resp.ctx.global_style().visuals.hyperlink_color),
                    StrokeKind::Inside,
                );
            }
        }
        if let Some(p) = resp.dnd_release_payload::<FileDrag>() {
            if !p.paths.iter().any(|x| x == &e.path) {
                let mods = resp.ctx.input(|i| i.modifiers);
                let cut = if mods.shift {
                    true
                } else if mods.command {
                    false
                } else {
                    same_volume(&p.from, &e.path)
                };
                actions.push(Action::DropFiles {
                    paths: p.paths.clone(),
                    dest: e.path.clone(),
                    cut,
                });
            }
        }
    }
}

fn same_volume(a: &std::path::Path, b: &std::path::Path) -> bool {
    let root = |p: &std::path::Path| match p.components().next() {
        Some(std::path::Component::Prefix(pr)) => pr.as_os_str().to_string_lossy().to_lowercase(),
        _ => "/".to_owned(),
    };
    root(a) == root(b)
}

pub fn item_menu(ui: &mut Ui, e: &FileEntry, tab: &Tab, has_clip: bool, pinned: &[PathBuf], actions: &mut Vec<Action>) {
    ui.set_min_width(240.0);
    let multi = tab.selected.len() > 1 && tab.selected.contains(&e.path);
    if ui.add(Button::new("Abrir").shortcut_text("Enter")).clicked() {
        actions.push(Action::OpenSelected);
    }
    if e.is_dir && !multi {
        if ui.button("Abrir em nova aba").clicked() {
            actions.push(Action::OpenInNewTab(e.path.clone()));
        }
        if ui.button("Abrir em novo grupo").clicked() {
            actions.push(Action::OpenInNewGroup(e.path.clone()));
        }
        if ui.button("Abrir em nova janela").clicked() {
            actions.push(Action::OpenInNewWindow(e.path.clone()));
        }
    }
    ui.separator();
    if ui.add(Button::new("✂ Recortar").shortcut_text("Ctrl+X")).clicked() {
        actions.push(Action::Cut);
    }
    if ui.add(Button::new("📑 Copiar").shortcut_text("Ctrl+C")).clicked() {
        actions.push(Action::Copy);
    }
    if e.is_dir && !multi && ui.add_enabled(has_clip, Button::new("📋 Colar nesta pasta")).clicked() {
        actions.push(Action::PasteInto(e.path.clone()));
    }
    if ui
        .add_enabled(!multi, Button::new("✏ Renomear").shortcut_text("F2"))
        .clicked()
    {
        actions.push(Action::Rename);
    }
    if ui.add(Button::new("🗑 Excluir").shortcut_text("Delete")).clicked() {
        actions.push(Action::Delete { permanent: false });
    }
    if ui
        .add(Button::new("Excluir permanentemente").shortcut_text("Shift+Delete"))
        .clicked()
    {
        actions.push(Action::Delete { permanent: true });
    }
    ui.separator();
    if ui
        .add(Button::new("🔗 Copiar como caminho").shortcut_text("Ctrl+Shift+C"))
        .clicked()
    {
        actions.push(Action::CopyPath);
    }
    if e.is_dir && !multi {
        let is_pinned = pinned.contains(&e.path);
        let label = if is_pinned {
            "Desafixar do Acesso rápido"
        } else {
            "📌 Fixar no Acesso rápido"
        };
        if ui.button(label).clicked() {
            actions.push(Action::TogglePin(e.path.clone()));
        }
    }
    ui.separator();
    if ui
        .add(Button::new("ℹ Propriedades").shortcut_text("Alt+Enter"))
        .clicked()
    {
        actions.push(Action::Properties);
    }
}

pub fn background_menu(bg: &egui::Response, tab: &Tab, has_clip: bool, pinned: &[PathBuf], actions: &mut Vec<Action>) {
    let path = tab.path.clone();
    let sort_key = tab.sort_key;
    let group_by = tab.group_by;
    bg.context_menu(|ui| {
        ui.set_min_width(240.0);
        if ui.add(Button::new("🔄 Atualizar").shortcut_text("F5")).clicked() {
            actions.push(Action::Refresh);
        }
        ui.separator();
        ui.menu_button("⊕ Novo", |ui| {
            if ui.add(Button::new("📁 Pasta").shortcut_text("Ctrl+Shift+N")).clicked() {
                actions.push(Action::NewFolder);
            }
            if ui.button("📝 Documento de texto").clicked() {
                actions.push(Action::NewFile);
            }
        });
        if ui
            .add_enabled(has_clip, Button::new("📋 Colar").shortcut_text("Ctrl+V"))
            .clicked()
        {
            actions.push(Action::Paste);
        }
        ui.separator();
        ui.menu_button("↕ Classificar por", |ui| {
            for k in [SortKey::Name, SortKey::Modified, SortKey::Kind, SortKey::Size] {
                if ui.radio(sort_key == k, k.label()).clicked() {
                    actions.push(Action::SetSort(k));
                }
            }
        });
        ui.menu_button("▤ Agrupar por", |ui| {
            for g in [GroupBy::None, GroupBy::Date, GroupBy::Kind] {
                if ui.radio(group_by == g, g.label()).clicked() {
                    actions.push(Action::SetGroupBy(g));
                }
            }
        });
        ui.separator();
        if ui.add(Button::new("Selecionar tudo").shortcut_text("Ctrl+A")).clicked() {
            actions.push(Action::SelectAll);
        }
        let is_pinned = pinned.contains(&path);
        if ui
            .button(if is_pinned {
                "Desafixar esta pasta"
            } else {
                "📌 Fixar esta pasta no Acesso rápido"
            })
            .clicked()
        {
            actions.push(Action::TogglePin(path.clone()));
        }
        if ui.add(Button::new("🔗 Copiar caminho da pasta")).clicked() {
            actions.push(Action::ClearSelection);
            actions.push(Action::CopyPath);
        }
        ui.separator();
        if ui.button("ℹ Propriedades da pasta").clicked() {
            actions.push(Action::PropertiesOfFolder);
        }
    });
    let _ = (vec2(0.0, 0.0), Pos2::ZERO);
}
