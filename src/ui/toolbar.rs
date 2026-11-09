use crate::app::{Action, TabexApp};
use crate::fs::entry::FileKind;
use crate::fs::filter::{DateFilter, SizeFilter};
use crate::model::tab::{GroupBy, SortKey};
use crate::ui::icon_button;
use egui::{vec2, Align, Button, Frame, Key, Layout, Margin, RichText, Sense, Stroke, TextEdit, Ui};
use std::path::{Path, PathBuf};

pub fn show(ui: &mut Ui, app: &mut TabexApp) {
    let mut actions: Vec<Action> = Vec::new();
    command_bar(ui, app, &mut actions);
    ui.add_space(4.0);
    nav_bar(ui, app, &mut actions);
    app.pending.extend(actions);
}

fn command_bar(ui: &mut Ui, app: &mut TabexApp, actions: &mut Vec<Action>) {
    let tab = app.session.active_tab();
    let has_sel = !tab.selected.is_empty();
    let has_clip = app.clipboard.is_some();
    let sort_key = tab.sort_key;
    let sort_asc = tab.sort_asc;
    let group_by = tab.group_by;
    let sidebar = app.session.sidebar_visible;
    let hidden = app.session.show_hidden;
    let dark = app.session.dark_theme;
    let native_menu = app.session.use_native_menu;
    let native_supported = crate::platform::native_menu_supported();
    let update_available = matches!(app.updater.state(), crate::update::UpdateState::Available(_));

    ui.horizontal(|ui| {
        ui.menu_button("⊕ Novo", |ui| {
            if ui.add(Button::new("📁 Pasta").shortcut_text("Ctrl+Shift+N")).clicked() {
                actions.push(Action::NewFolder);
            }
            if ui.button("📝 Documento de texto").clicked() {
                actions.push(Action::NewFile);
            }
            ui.separator();
            if ui.add(Button::new("＋ Nova aba").shortcut_text("Ctrl+T")).clicked() {
                actions.push(Action::NewTab);
            }
            if ui
                .add(Button::new("＋ Novo grupo de abas").shortcut_text("Ctrl+G"))
                .clicked()
            {
                actions.push(Action::NewGroup);
            }
        });
        ui.separator();
        if cmd(ui, "✂", "Recortar", "Ctrl+X", has_sel) {
            actions.push(Action::Cut);
        }
        if cmd(ui, "📑", "Copiar", "Ctrl+C", has_sel) {
            actions.push(Action::Copy);
        }
        if cmd(ui, "📋", "Colar", "Ctrl+V", has_clip) {
            actions.push(Action::Paste);
        }
        if cmd(ui, "✏", "Renomear", "F2", has_sel) {
            actions.push(Action::Rename);
        }
        if cmd(ui, "🗑", "Excluir", "Delete", has_sel) {
            actions.push(Action::Delete { permanent: false });
        }
        ui.separator();

        ui.menu_button("↕ Classificar", |ui| {
            for k in [SortKey::Name, SortKey::Modified, SortKey::Kind, SortKey::Size] {
                if ui.radio(sort_key == k, k.label()).clicked() && sort_key != k {
                    actions.push(Action::SetSort(k));
                }
            }
            ui.separator();
            if ui.radio(sort_asc, "Crescente").clicked() {
                actions.push(Action::SetSortAsc(true));
            }
            if ui.radio(!sort_asc, "Decrescente").clicked() {
                actions.push(Action::SetSortAsc(false));
            }
            ui.separator();
            ui.menu_button("Agrupar por", |ui| {
                for g in [GroupBy::None, GroupBy::Date, GroupBy::Kind] {
                    if ui.radio(group_by == g, g.label()).clicked() {
                        actions.push(Action::SetGroupBy(g));
                    }
                }
            });
        });

        ui.menu_button("▦ Visualizar", |ui| {
            if ui
                .add(Button::new(check(sidebar, "Painel de navegação")).shortcut_text("Ctrl+B"))
                .clicked()
            {
                actions.push(Action::ToggleSidebar);
            }
            if ui
                .add(Button::new(check(hidden, "Itens ocultos")).shortcut_text("Ctrl+H"))
                .clicked()
            {
                actions.push(Action::ToggleHidden);
            }
            if ui.button(check(dark, "Tema escuro")).clicked() {
                actions.push(Action::ToggleTheme);
            }
            if native_supported
                && ui
                    .button(check(native_menu, "Menu de contexto do Windows"))
                    .on_hover_text("Botão direito abre o mesmo menu do Explorador (com os itens do Tabex no topo)")
                    .clicked()
            {
                actions.push(Action::ToggleNativeMenu);
            }
            ui.separator();
            ui.menu_button("Agrupar por", |ui| {
                for g in [GroupBy::None, GroupBy::Date, GroupBy::Kind] {
                    if ui.radio(group_by == g, g.label()).clicked() {
                        actions.push(Action::SetGroupBy(g));
                    }
                }
            });
            ui.separator();
            if ui
                .add(Button::new("⌨ Atalhos de teclado").shortcut_text("F1"))
                .clicked()
            {
                actions.push(Action::ShowShortcuts);
            }
        });

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.menu_button("⋯", |ui| {
                if ui
                    .add(Button::new("ℹ Propriedades").shortcut_text("Alt+Enter"))
                    .clicked()
                {
                    actions.push(Action::Properties);
                }
                if ui
                    .add(Button::new("📌 Fixar pasta atual no Acesso rápido").shortcut_text("Ctrl+D"))
                    .clicked()
                {
                    actions.push(Action::TogglePinCurrent);
                }
                if ui
                    .add(Button::new("🔗 Copiar caminho").shortcut_text("Ctrl+Shift+C"))
                    .clicked()
                {
                    actions.push(Action::CopyPath);
                }
                ui.separator();
                if ui.button("Selecionar tudo").clicked() {
                    actions.push(Action::SelectAll);
                }
                if ui.button("Inverter seleção").clicked() {
                    actions.push(Action::InvertSelection);
                }
                ui.separator();
                let upd_label = if update_available {
                    "⬆ Atualizações (nova versão!)"
                } else {
                    "⬆ Atualizações…"
                };
                if ui.button(upd_label).clicked() {
                    actions.push(Action::ShowUpdateDialog);
                }
                if ui.button("Abrir pasta de configuração do Tabex").clicked() {
                    if let Some(dir) = crate::model::session::Session::config_path().parent() {
                        let _ = std::fs::create_dir_all(dir);
                        actions.push(Action::OpenInNewTab(dir.to_path_buf()));
                    }
                }
                ui.label(
                    RichText::new(format!(
                        "Tabex v{}{}",
                        env!("CARGO_PKG_VERSION"),
                        if crate::model::session::Session::is_portable() {
                            " · portátil"
                        } else {
                            ""
                        }
                    ))
                    .weak()
                    .small(),
                );
            });
        });
    });
}

fn check(on: bool, label: &str) -> String {
    format!("{} {label}", if on { "✔" } else { "    " })
}

fn cmd(ui: &mut Ui, icon: &str, label: &str, shortcut: &str, enabled: bool) -> bool {
    ui.add_enabled(enabled, Button::new(format!("{icon} {label}")).frame(false))
        .on_hover_text(format!("{label} ({shortcut})"))
        .clicked()
}

fn nav_bar(ui: &mut Ui, app: &mut TabexApp, actions: &mut Vec<Action>) {
    let (can_back, can_fwd, can_up) = {
        let t = app.session.active_tab();
        (
            t.can_back(),
            t.can_forward(),
            t.path.parent().map(|p| !p.as_os_str().is_empty()).unwrap_or(false),
        )
    };
    ui.horizontal(|ui| {
        if icon_button(ui, "◀", "Voltar (Alt+←)", can_back).clicked() {
            actions.push(Action::Back);
        }
        if icon_button(ui, "▶", "Avançar (Alt+→)", can_fwd).clicked() {
            actions.push(Action::Forward);
        }
        if icon_button(ui, "▲", "Subir um nível (Alt+↑)", can_up).clicked() {
            actions.push(Action::Up);
        }
        if icon_button(ui, "🔄", "Atualizar (F5)", true).clicked() {
            actions.push(Action::Refresh);
        }
        ui.add_space(4.0);

        let total = ui.available_width();
        let search_w = (total * 0.3).clamp(220.0, 360.0);
        let addr_w = (total - search_w - 12.0).max(160.0);

        ui.allocate_ui_with_layout(vec2(addr_w, 30.0), Layout::left_to_right(Align::Center), |ui| {
            address_bar(ui, app, addr_w, actions);
        });
        ui.allocate_ui_with_layout(vec2(search_w, 30.0), Layout::left_to_right(Align::Center), |ui| {
            search_box(ui, app, search_w, actions);
        });
    });
}

fn field_frame(ui: &Ui) -> Frame {
    Frame::new()
        .fill(ui.visuals().extreme_bg_color)
        .stroke(Stroke::new(1.0, ui.visuals().widgets.inactive.bg_stroke.color))
        .corner_radius(4)
        .inner_margin(Margin::symmetric(6, 2))
}

fn address_bar(ui: &mut Ui, app: &mut TabexApp, width: f32, actions: &mut Vec<Action>) {
    let path = app.session.active_tab().path.clone();
    field_frame(ui).show(ui, |ui| {
        ui.set_min_size(vec2(width - 14.0, 26.0));
        ui.set_max_width(width - 14.0);
        ui.spacing_mut().item_spacing.x = 0.0;
        if let Some(text) = app.address_edit.as_mut() {
            let resp = ui.add(
                TextEdit::singleline(text)
                    .id(egui::Id::unique("address_edit"))
                    .desired_width(f32::INFINITY)
                    .frame(Frame::NONE)
                    .align(egui::Align2::LEFT_CENTER),
            );
            if app.focus_address {
                resp.request_focus();
                select_all_text(ui, resp.id, text.chars().count());
                app.focus_address = false;
            }
            let enter = ui.input(|i| i.key_pressed(Key::Enter));
            let esc = ui.input(|i| i.key_pressed(Key::Escape));
            if resp.lost_focus() || esc {
                if enter && !esc {
                    actions.push(Action::Navigate(PathBuf::from(text.trim())));
                }
                app.address_edit = None;
            }
            return;
        }

        // Breadcrumb
        let mut parts: Vec<&Path> = path.ancestors().collect();
        parts.reverse();
        let n = parts.len();
        let max_visible = 6;
        for (i, p) in parts.iter().enumerate() {
            if n > max_visible && i > 0 && i < n - (max_visible - 1) {
                if i == 1 {
                    ui.label(RichText::new(" … ").weak());
                    crumb_menu(ui, parts[i - 1], actions);
                }
                continue;
            }
            let label = crumb_label(p);
            let is_last = i == n - 1;
            let text = if is_last {
                RichText::new(label).strong()
            } else {
                RichText::new(label)
            };
            let b = ui.add(Button::new(text).frame(false).min_size(vec2(0.0, 24.0)));
            if b.clicked() {
                actions.push(Action::Navigate(p.to_path_buf()));
            }
            if b.middle_clicked() {
                actions.push(Action::OpenInNewTab(p.to_path_buf()));
            }
            b.context_menu(|ui| {
                if ui.button("Abrir em nova aba").clicked() {
                    actions.push(Action::OpenInNewTab(p.to_path_buf()));
                }
                if ui.button("Copiar endereço").clicked() {
                    app.pending_copy_text = Some(p.to_string_lossy().into_owned());
                    actions.push(Action::Toast("Endereço copiado".into()));
                }
                if ui.button("Editar endereço").clicked() {
                    actions.push(Action::FocusAddress);
                }
            });
            crumb_menu(ui, p, actions);
        }
        // Área vazia: clique para editar.
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width().max(8.0), 24.0), Sense::click());
        let _ = r;
        if resp.clicked() {
            actions.push(Action::FocusAddress);
        }
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
        }
    });
}

/// Separador "›" com menu das subpastas (como no Explorador).
fn crumb_menu(ui: &mut Ui, p: &Path, actions: &mut Vec<Action>) {
    let r = ui.menu_button(RichText::new("›").weak().size(15.0), |ui| {
        ui.set_min_width(200.0);
        let mut dirs: Vec<(String, PathBuf)> = std::fs::read_dir(p)
            .map(|rd| {
                rd.flatten()
                    .filter(|e| e.path().is_dir())
                    .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
                    .filter(|(n, _)| !n.starts_with('.'))
                    .collect()
            })
            .unwrap_or_default();
        dirs.sort_by(|a, b| crate::fs::entry::natural_cmp(&a.0.to_lowercase(), &b.0.to_lowercase()));
        if dirs.is_empty() {
            ui.weak("(sem subpastas)");
        }
        egui::ScrollArea::vertical().max_height(400.0).show(ui, |ui| {
            for (name, path) in dirs {
                if ui.button(format!("📁 {name}")).clicked() {
                    actions.push(Action::Navigate(path));
                }
            }
        });
    });
    let _ = r;
}

fn crumb_label(p: &Path) -> String {
    match p.file_name() {
        Some(n) => n.to_string_lossy().into_owned(),
        None => {
            let s = p.to_string_lossy();
            let s = s.trim_end_matches(['\\', '/']);
            if s.is_empty() {
                "Sistema".to_owned()
            } else {
                s.to_owned()
            }
        }
    }
}

fn select_all_text(ui: &Ui, id: egui::Id, len: usize) {
    if let Some(mut state) = TextEdit::load_state(ui.ctx(), id) {
        state.cursor.set_char_range(Some(egui::text::CCursorRange::two(
            egui::text::CCursor::new(0),
            egui::text::CCursor::new(len),
        )));
        state.store(ui.ctx(), id);
    }
}

fn search_box(ui: &mut Ui, app: &mut TabexApp, width: f32, actions: &mut Vec<Action>) {
    let folder = app.session.active_tab().title();
    let show_hidden = app.session.show_hidden;
    let tab = app.session.active_tab_mut();
    let filter_active = tab.quick_filter.is_active();
    let frame = field_frame(ui);
    frame.show(ui, |ui| {
        ui.set_min_size(vec2(width - 14.0, 26.0));
        ui.set_max_width(width - 14.0);
        ui.spacing_mut().item_spacing.x = 2.0;
        ui.label(RichText::new("🔍").weak());
        let filter_w = 74.0;
        let clear_w = if tab.search_text.is_empty() && tab.search.is_none() {
            0.0
        } else {
            22.0
        };
        let edit_w = (ui.available_width() - filter_w - clear_w - 8.0).max(40.0);
        let resp = ui.add(
            TextEdit::singleline(&mut tab.search_text)
                .id(egui::Id::unique(("search_edit", tab.id)))
                .hint_text(format!("Pesquisar em {folder}"))
                .desired_width(edit_w)
                .frame(Frame::NONE)
                .align(egui::Align2::LEFT_CENTER),
        );
        if app.focus_search {
            resp.request_focus();
            select_all_text(ui, resp.id, tab.search_text.chars().count());
            app.focus_search = false;
        }
        if resp.changed() {
            // Filtro instantâneo na pasta atual; pesquisa recursiva só com Enter.
            if tab.search.is_some() {
                tab.search = None;
            }
            tab.rebuild_rows();
        }
        if resp.has_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
            if tab.search_text.trim().is_empty() {
                tab.clear_search();
            } else {
                tab.start_search(show_hidden);
            }
        }
        let _ = resp.on_hover_text(
            "Filtra enquanto digita. Enter pesquisa nas subpastas.\n\
             Prefixos: ext:pdf  tipo:pasta  tipo:imagem  tamanho:>1mb  data:hoje",
        );
        if clear_w > 0.0
            && ui
                .add(Button::new("✕").frame(false).small())
                .on_hover_text("Limpar pesquisa (Esc)")
                .clicked()
        {
            actions.push(Action::ClearSearch);
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let label = if filter_active {
                RichText::new("Filtrar ●").color(ui.visuals().hyperlink_color)
            } else {
                RichText::new("Filtrar ▾")
            };
            let mut changed = false;
            egui::containers::menu::MenuButton::new(label)
                .config(
                    egui::containers::menu::MenuConfig::new()
                        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside),
                )
                .ui(ui, |ui| {
                    ui.set_min_width(260.0);
                    ui.strong("Tipo");
                    egui::Grid::new("kind_filter")
                        .num_columns(2)
                        .spacing(vec2(12.0, 2.0))
                        .show(ui, |ui| {
                            for (i, k) in FileKind::FILTERABLE.iter().enumerate() {
                                let mut on = tab.quick_filter.kinds.contains(k);
                                if ui.checkbox(&mut on, format!("{} {}", k.icon(), k.label())).changed() {
                                    if on {
                                        tab.quick_filter.kinds.insert(*k);
                                    } else {
                                        tab.quick_filter.kinds.remove(k);
                                    }
                                    changed = true;
                                }
                                if i % 2 == 1 {
                                    ui.end_row();
                                }
                            }
                        });
                    ui.separator();
                    ui.strong("Data de modificação");
                    for d in DateFilter::ALL {
                        if ui.radio(tab.quick_filter.date == d, d.label()).clicked() {
                            tab.quick_filter.date = d;
                            changed = true;
                        }
                    }
                    ui.separator();
                    ui.strong("Tamanho");
                    for s in SizeFilter::ALL {
                        if ui.radio(tab.quick_filter.size == s, s.label()).clicked() {
                            tab.quick_filter.size = s;
                            changed = true;
                        }
                    }
                    ui.separator();
                    if ui
                        .add_enabled(tab.quick_filter.is_active(), Button::new("Limpar filtros"))
                        .clicked()
                    {
                        tab.quick_filter.clear();
                        changed = true;
                        ui.close();
                    }
                });
            if changed {
                tab.rebuild_rows();
            }
        });
    });
}
