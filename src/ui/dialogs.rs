use crate::app::{plural, Action, TabexApp};
use crate::fs::entry::{format_date, format_size_human, thousands, FileEntry};
use crate::fs::ops::{self, PropertiesInfo};
use crate::model::color::GroupColor;
use crate::shortcuts::HELP;
use egui::{
    vec2, Align, Button, Context, CornerRadius, Grid, Id, Key, Layout, Modal, RichText, Sense, Stroke, TextEdit, Ui,
};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub enum Dialog {
    None,
    Rename {
        path: PathBuf,
        name: String,
        is_dir: bool,
        first: bool,
    },
    NewFolder {
        dir: PathBuf,
        name: String,
        first: bool,
    },
    NewFile {
        dir: PathBuf,
        name: String,
        first: bool,
    },
    ConfirmDelete {
        paths: Vec<PathBuf>,
        permanent: bool,
    },
    Conflict {
        clip: ops::Clipboard,
        dest: PathBuf,
        conflicts: Vec<ops::Conflict>,
    },
    Group {
        index: Option<usize>,
        name: String,
        color: GroupColor,
        first: bool,
    },
    Window {
        index: Option<usize>,
        name: String,
        color: GroupColor,
        first: bool,
    },
    Update,
    Properties {
        paths: Vec<PathBuf>,
        entry: Option<FileEntry>,
        info: Arc<Mutex<PropertiesInfo>>,
    },
    Error {
        title: String,
        message: String,
    },
    Shortcuts,
}

enum Outcome {
    Keep,
    Close,
}

pub fn show(ctx: &Context, app: &mut TabexApp) {
    if matches!(app.dialog, Dialog::None) {
        return;
    }
    let mut dialog = std::mem::replace(&mut app.dialog, Dialog::None);
    let mut actions: Vec<Action> = Vec::new();
    let outcome = match &mut dialog {
        Dialog::None => Outcome::Close,
        Dialog::Rename {
            path,
            name,
            is_dir,
            first,
        } => {
            let path = path.clone();
            let is_dir = *is_dir;
            text_dialog(
                ctx,
                "Renomear",
                if is_dir {
                    "Novo nome da pasta:"
                } else {
                    "Novo nome do arquivo:"
                },
                name,
                first,
                !is_dir,
                |name| match ops::rename(&path, name) {
                    Ok(new_path) => {
                        let parent = path.parent().map(|p| p.to_path_buf());
                        if let Some(p) = parent {
                            actions.push(Action::Navigate(p));
                        }
                        if new_path != path {
                            actions.push(Action::PushUndo(ops::UndoEntry::new(
                                "renomear",
                                ops::UndoKind::Rename {
                                    from: path.clone(),
                                    to: new_path.clone(),
                                },
                            )));
                        }
                        actions.push(Action::Toast(format!(
                            "Renomeado para \"{}\"",
                            new_path
                                .file_name()
                                .map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or_default()
                        )));
                        Ok(())
                    }
                    Err(e) => Err(e.to_string()),
                },
            )
        }
        Dialog::NewFolder { dir, name, first } => {
            let dir = dir.clone();
            text_dialog(ctx, "Nova pasta", "Nome da pasta:", name, first, false, |name| {
                ops::create_folder(&dir, name)
                    .map(|p| {
                        actions.push(Action::PushUndo(ops::UndoEntry::new(
                            "criar pasta",
                            ops::UndoKind::Create { path: p, is_dir: true },
                        )));
                        actions.push(Action::Refresh)
                    })
                    .map_err(|e| e.to_string())
            })
        }
        Dialog::NewFile { dir, name, first } => {
            let dir = dir.clone();
            text_dialog(
                ctx,
                "Novo documento de texto",
                "Nome do arquivo:",
                name,
                first,
                true,
                |name| {
                    ops::create_file(&dir, name)
                        .map(|p| {
                            actions.push(Action::PushUndo(ops::UndoEntry::new(
                                "criar arquivo",
                                ops::UndoKind::Create { path: p, is_dir: false },
                            )));
                            actions.push(Action::Refresh)
                        })
                        .map_err(|e| e.to_string())
                },
            )
        }
        Dialog::ConfirmDelete { paths, permanent } => confirm_delete(ctx, paths, *permanent, &mut actions),
        Dialog::Conflict { clip, dest, conflicts } => conflict_dialog(ctx, clip, dest, conflicts, &mut actions),
        Dialog::Group {
            index,
            name,
            color,
            first,
        } => {
            let is_edit = index.is_some();
            let (outcome, picked) = name_color_dialog(
                ctx,
                "dlg_group",
                if is_edit {
                    "Editar grupo de abas"
                } else {
                    "Novo grupo de abas"
                },
                "Nome do grupo:",
                if is_edit { "Salvar" } else { "Criar" },
                name,
                color,
                first,
            );
            if let Some((final_name, c)) = picked {
                match index {
                    Some(gi) => {
                        if let Some(g) = app.session.groups.get_mut(*gi) {
                            g.name = final_name;
                            g.color = c;
                        }
                    }
                    None => {
                        let p = app.session.active_tab().path.clone();
                        let gi = app.session.new_group(Some(final_name));
                        app.session.groups[gi].color = c;
                        app.session.open_tab(gi, p);
                    }
                }
                actions.push(Action::Toast("Grupo salvo".into()));
            }
            outcome
        }
        Dialog::Window {
            index,
            name,
            color,
            first,
        } => {
            let is_edit = index.is_some();
            let (outcome, picked) = name_color_dialog(
                ctx,
                "dlg_window",
                if is_edit { "Editar janela" } else { "Nova janela" },
                "Nome da janela:",
                if is_edit { "Salvar" } else { "Criar" },
                name,
                color,
                first,
            );
            if let Some((final_name, c)) = picked {
                match index {
                    Some(wi) => {
                        if let Some(w) = app.session.windows.get_mut(*wi) {
                            w.name = final_name;
                            w.color = c;
                        }
                    }
                    None => {
                        let p = app.session.active_tab().path.clone();
                        app.session.new_window(Some(final_name), Some(c), p);
                    }
                }
                actions.push(Action::Toast("Janela salva".into()));
            }
            outcome
        }
        Dialog::Update => update_dialog(ctx, app, &mut actions),
        Dialog::Properties { paths, entry, info } => properties(ctx, paths, entry.as_ref(), info),
        Dialog::Error { title, message } => {
            let m = Modal::new(Id::unique("dlg_error")).show(ctx, |ui| {
                ui.set_width(420.0);
                ui.heading(title.as_str());
                ui.add_space(6.0);
                ui.label(message.as_str());
                ui.add_space(12.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| ui.button("OK").clicked())
                    .inner
            });
            if m.inner || m.should_close() || ctx.input(|i| i.key_pressed(Key::Enter)) {
                Outcome::Close
            } else {
                Outcome::Keep
            }
        }
        Dialog::Shortcuts => shortcuts_dialog(ctx),
    };
    match outcome {
        Outcome::Keep => app.dialog = dialog,
        Outcome::Close => app.dialog = Dialog::None,
    }
    app.pending.extend(actions);
}

/// Seleciona o nome sem a extensão (como o Explorador faz ao renomear).
pub fn select_stem(ctx: &Context, id: Id, text: &str, keep_ext: bool) {
    if let Some(mut state) = TextEdit::load_state(ctx, id) {
        let end = if keep_ext {
            match text.rfind('.') {
                Some(i) if i > 0 => text[..i].chars().count(),
                _ => text.chars().count(),
            }
        } else {
            text.chars().count()
        };
        state.cursor.set_char_range(Some(egui::text::CCursorRange::two(
            egui::text::CCursor::new(0),
            egui::text::CCursor::new(end),
        )));
        state.store(ctx, id);
    }
}

fn text_dialog(
    ctx: &Context,
    title: &str,
    label: &str,
    name: &mut String,
    first: &mut bool,
    keep_ext: bool,
    mut on_ok: impl FnMut(&str) -> Result<(), String>,
) -> Outcome {
    let id = Id::unique(("dlg_text", title));
    let mut result = Outcome::Keep;
    let m = Modal::new(Id::unique(("modal", title))).show(ctx, |ui| {
        ui.set_width(420.0);
        ui.heading(title);
        ui.add_space(6.0);
        ui.label(label);
        let resp = ui.add(TextEdit::singleline(name).id(id).desired_width(f32::INFINITY));
        if *first {
            resp.request_focus();
            select_stem(ctx, id, name, keep_ext);
            *first = false;
        }
        let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
        let err_id = id.with("err");
        let mut err: Option<String> = ui.memory(|m| m.data.get_temp(err_id));
        if let Some(e) = &err {
            ui.label(RichText::new(e).color(ui.visuals().error_fg_color));
        }
        ui.add_space(10.0);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let ok = ui.button("OK").clicked() || enter;
            let cancel = ui.button("Cancelar").clicked();
            if ok {
                match on_ok(name) {
                    Ok(()) => {
                        result = Outcome::Close;
                        err = None;
                    }
                    Err(e) => {
                        err = Some(e);
                        resp.request_focus();
                    }
                }
            }
            if cancel {
                result = Outcome::Close;
                err = None;
            }
        });
        ui.memory_mut(|m| match err {
            Some(e) => {
                m.data.insert_temp(err_id, e);
            }
            None => {
                m.data.remove::<String>(err_id);
            }
        });
    });
    if m.should_close() {
        ctx.memory_mut(|m| m.data.remove::<String>(id.with("err")));
        return Outcome::Close;
    }
    result
}

fn confirm_delete(ctx: &Context, paths: &[PathBuf], permanent: bool, actions: &mut Vec<Action>) -> Outcome {
    let n = paths.len();
    let what = if n == 1 {
        format!(
            "\"{}\"",
            paths[0]
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default()
        )
    } else {
        format!("estes {} itens", n)
    };
    let msg = if permanent {
        format!("Tem certeza de que deseja excluir permanentemente {what}?")
    } else {
        format!("Tem certeza de que deseja mover {what} para a Lixeira?")
    };
    let mut result = Outcome::Keep;
    let m = Modal::new(Id::unique("dlg_delete")).show(ctx, |ui| {
        ui.set_width(440.0);
        ui.heading(if permanent {
            "Excluir permanentemente"
        } else {
            "Excluir"
        });
        ui.add_space(6.0);
        ui.label(msg);
        if n > 1 {
            ui.add_space(4.0);
            egui::ScrollArea::vertical().max_height(140.0).show(ui, |ui| {
                for p in paths.iter().take(200) {
                    ui.weak(
                        p.file_name()
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                    );
                }
            });
        }
        ui.add_space(12.0);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let yes = ui.button("Sim").clicked() || ui.input(|i| i.key_pressed(Key::Enter));
            let no = ui.button("Não").clicked();
            if yes {
                actions.push(Action::Enqueue(ops::QueuedOp::Delete {
                    paths: paths.to_vec(),
                    permanent,
                }));
                result = Outcome::Close;
            } else if no {
                result = Outcome::Close;
            }
        });
    });
    if m.should_close() {
        return Outcome::Close;
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn name_color_dialog(
    ctx: &Context,
    id_salt: &str,
    title: &str,
    label: &str,
    ok_label: &str,
    name: &mut String,
    color: &mut GroupColor,
    first: &mut bool,
) -> (Outcome, Option<(String, GroupColor)>) {
    let mut result = Outcome::Keep;
    let mut picked = None;
    let id = Id::unique((id_salt, "name"));
    let m = Modal::new(Id::unique(id_salt)).show(ctx, |ui| {
        ui.set_width(400.0);
        ui.heading(title);
        ui.add_space(6.0);
        ui.label(label);
        let resp = ui.add(TextEdit::singleline(name).id(id).desired_width(f32::INFINITY));
        if *first {
            resp.request_focus();
            select_stem(ctx, id, name, false);
            *first = false;
        }
        let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
        ui.add_space(8.0);
        ui.label("Cor:");
        ui.horizontal_wrapped(|ui| {
            for c in GroupColor::ALL {
                let (rect, r) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::click());
                let center = rect.center();
                ui.painter().circle_filled(center, 11.0, c.color());
                if *color == c {
                    ui.painter().circle_stroke(center, 14.0, Stroke::new(2.0, c.color()));
                }
                if r.hovered() {
                    ui.painter()
                        .circle_stroke(center, 13.0, Stroke::new(1.0, ui.visuals().text_color()));
                }
                let r = r.on_hover_text(c.label());
                if r.clicked() {
                    *color = c;
                }
            }
        });
        ui.add_space(6.0);
        let preview = Button::new(RichText::new(name.as_str()).color(color.text_color()).size(12.5))
            .fill(color.color())
            .corner_radius(CornerRadius::same(11))
            .stroke(Stroke::NONE);
        ui.add_enabled(false, preview);
        ui.add_space(12.0);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let ok = ui.button(ok_label).clicked() || enter;
            if ui.button("Cancelar").clicked() {
                result = Outcome::Close;
            }
            if ok {
                let final_name = if name.trim().is_empty() {
                    "Sem nome".to_owned()
                } else {
                    name.trim().to_owned()
                };
                picked = Some((final_name, *color));
                result = Outcome::Close;
            }
        });
    });
    if m.should_close() {
        return (Outcome::Close, None);
    }
    (result, picked)
}

fn update_dialog(ctx: &Context, app: &mut TabexApp, actions: &mut Vec<Action>) -> Outcome {
    use crate::update::{UpdateState, Updater, CURRENT};
    let state = app.updater.state();
    if app.updater.is_busy() {
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
    }
    let mut close = false;
    let m = Modal::new(Id::unique("dlg_update")).show(ctx, |ui| {
        ui.set_width(480.0);
        ui.heading("Atualizações do Tabex");
        ui.add_space(4.0);
        ui.label(format!("Versão instalada: {CURRENT}"));
        ui.weak(format!("Fonte: github.com/{}", crate::update::REPO));
        ui.add_space(8.0);
        match &state {
            UpdateState::Idle => {
                ui.label("Clique em \"Verificar agora\" para procurar uma versão nova.");
            }
            UpdateState::Checking => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Verificando…");
                });
            }
            UpdateState::UpToDate => {
                ui.label(RichText::new("✔ Você já está na versão mais recente.").color(ui.visuals().hyperlink_color));
            }
            UpdateState::Available(info) => {
                ui.label(RichText::new(format!("Nova versão disponível: {}", info.tag)).strong());
                if info.asset_name.is_none() {
                    ui.label(
                        RichText::new(
                            "A release não tem um pacote para esta plataforma; abra a página para baixar manualmente.",
                        )
                        .color(ui.visuals().warn_fg_color),
                    );
                }
                if !info.notes.trim().is_empty() {
                    ui.add_space(4.0);
                    egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                        ui.label(RichText::new(info.notes.trim()).size(12.5));
                    });
                }
            }
            UpdateState::Downloading(info) => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(format!("Baixando e instalando {}…", info.tag));
                });
            }
            UpdateState::Installed(info) => {
                ui.label(
                    RichText::new(format!("✔ Versão {} instalada.", info.tag)).color(ui.visuals().hyperlink_color),
                );
                ui.label("Reinicie o Tabex para usar a nova versão. Suas janelas, grupos e abas são mantidos.");
                ui.weak("A versão anterior ficou guardada como backup (.bak) ao lado do executável.");
            }
            UpdateState::RolledBack => {
                ui.label(RichText::new("✔ Versão anterior restaurada.").color(ui.visuals().hyperlink_color));
                ui.label("Reinicie o Tabex para concluir.");
            }
            UpdateState::Error(e) => {
                ui.label(RichText::new(e).color(ui.visuals().error_fg_color));
                ui.add_space(4.0);
                ui.weak(format!(
                    "Repositório privado? Salve um token do GitHub (permissão de leitura de releases) em:\n{}",
                    Updater::token_path().display()
                ));
            }
        }
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            let mut auto = app.session.auto_update;
            if ui
                .checkbox(&mut auto, "Verificar automaticamente ao iniciar (1x por dia)")
                .changed()
            {
                actions.push(Action::ToggleAutoUpdate);
            }
        });
        ui.add_space(8.0);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.button("Fechar").clicked() {
                close = true;
            }
            match &state {
                UpdateState::Available(info) => {
                    if info.asset_url.is_some() && ui.button(RichText::new("⬇ Baixar e instalar").strong()).clicked()
                    {
                        actions.push(Action::InstallUpdate);
                    }
                    if !info.html_url.is_empty() && ui.button("Abrir página da release").clicked() {
                        let _ = open::that_detached(&info.html_url);
                    }
                }
                UpdateState::Installed(_) | UpdateState::RolledBack => {
                    if ui.button(RichText::new("🔄 Reiniciar agora").strong()).clicked() {
                        actions.push(Action::RestartApp);
                    }
                }
                UpdateState::Checking | UpdateState::Downloading(_) => {}
                _ => {
                    if ui.button("Verificar agora").clicked() {
                        actions.push(Action::CheckUpdates { manual: true });
                    }
                    if Updater::has_backup()
                        && ui
                            .button("↩ Restaurar versão anterior")
                            .on_hover_text("Volta para o executável guardado em .bak")
                            .clicked()
                    {
                        actions.push(Action::RollbackUpdate);
                    }
                }
            }
        });
    });
    if close || m.should_close() {
        Outcome::Close
    } else {
        Outcome::Keep
    }
}

fn conflict_dialog(
    ctx: &Context,
    clip: &ops::Clipboard,
    dest: &std::path::Path,
    conflicts: &[ops::Conflict],
    actions: &mut Vec<Action>,
) -> Outcome {
    use crate::fs::entry::{format_date, format_size_human};
    let mut result = Outcome::Keep;
    let n = conflicts.len();
    let m = Modal::new(Id::unique("dlg_conflict")).show(ctx, |ui| {
        ui.set_width(560.0);
        ui.heading(if n == 1 {
            "O destino já tem um item com este nome".to_owned()
        } else {
            format!("O destino já tem {n} itens com os mesmos nomes")
        });
        ui.add_space(4.0);
        ui.weak(format!("Destino: {}", dest.display()));
        ui.add_space(6.0);
        egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
            Grid::new("conflicts")
                .num_columns(3)
                .striped(true)
                .spacing(vec2(14.0, 4.0))
                .show(ui, |ui| {
                    ui.strong("Nome");
                    ui.strong("Origem");
                    ui.strong("Já existe no destino");
                    ui.end_row();
                    for c in conflicts.iter().take(200) {
                        let info = |p: &std::path::Path| {
                            std::fs::metadata(p)
                                .map(|m| {
                                    if m.is_dir() {
                                        format!("pasta · {}", format_date(m.modified().ok()))
                                    } else {
                                        format!("{} · {}", format_size_human(m.len()), format_date(m.modified().ok()))
                                    }
                                })
                                .unwrap_or_default()
                        };
                        ui.add(egui::Label::new(&c.name).truncate());
                        ui.label(RichText::new(info(&c.src)).small());
                        ui.label(RichText::new(info(&c.dst)).small());
                        ui.end_row();
                    }
                });
        });
        ui.add_space(10.0);
        ui.label("O que fazer com os itens em conflito? (os demais são copiados normalmente)");
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            let mut pick = |policy: ops::ConflictPolicy| {
                actions.push(Action::PasteWith {
                    clip: clip.clone(),
                    dest: dest.to_path_buf(),
                    policy,
                });
                result = Outcome::Close;
            };
            if ui
                .add(Button::new(RichText::new("Substituir").strong()))
                .on_hover_text("Sobrescreve os arquivos existentes; pastas são mescladas. Não pode ser desfeito.")
                .clicked()
            {
                pick(ops::ConflictPolicy::Replace);
            }
            if ui
                .button("Pular")
                .on_hover_text("Mantém os existentes e não copia os itens em conflito")
                .clicked()
            {
                pick(ops::ConflictPolicy::Skip);
            }
            if ui
                .button("Manter ambos")
                .on_hover_text("Copia com outro nome: \"Nome (2)\"")
                .clicked()
            {
                pick(ops::ConflictPolicy::KeepBoth);
            }
            if ui.button("Cancelar").clicked() {
                result = Outcome::Close;
            }
        });
    });
    if m.should_close() {
        return Outcome::Close;
    }
    result
}

fn properties(
    ctx: &Context,
    paths: &[PathBuf],
    entry: Option<&FileEntry>,
    info: &Arc<Mutex<PropertiesInfo>>,
) -> Outcome {
    let snapshot = info.lock().map(|i| i.clone()).unwrap_or_default();
    if !snapshot.finished {
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
    }
    let mut close = false;
    let m = Modal::new(Id::unique("dlg_props")).show(ctx, |ui| {
        ui.set_width(460.0);
        match entry {
            Some(e) => {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(e.kind.icon()).size(28.0));
                    ui.heading(&e.name);
                });
                ui.separator();
                Grid::new("props_grid")
                    .num_columns(2)
                    .spacing(vec2(16.0, 6.0))
                    .show(ui, |ui| {
                        ui.weak("Tipo:");
                        ui.label(&e.type_label);
                        ui.end_row();
                        ui.weak("Local:");
                        ui.add(egui::Label::new(e.parent_display()).wrap());
                        ui.end_row();
                        ui.weak("Tamanho:");
                        let bytes = if e.is_dir { snapshot.bytes } else { e.size };
                        ui.label(format!(
                            "{} ({} bytes){}",
                            format_size_human(bytes),
                            thousands(bytes),
                            if e.is_dir && !snapshot.finished {
                                " — calculando…"
                            } else {
                                ""
                            }
                        ));
                        ui.end_row();
                        if e.is_dir {
                            ui.weak("Contém:");
                            ui.label(format!(
                                "{} {}, {} {}",
                                thousands(snapshot.files as u64),
                                plural(snapshot.files, "arquivo", "arquivos"),
                                thousands(snapshot.folders as u64),
                                plural(snapshot.folders, "pasta", "pastas")
                            ));
                            ui.end_row();
                        }
                        if let Ok(md) = std::fs::metadata(&e.path) {
                            ui.weak("Criado em:");
                            ui.label(format_date(md.created().ok()));
                            ui.end_row();
                            ui.weak("Modificado em:");
                            ui.label(format_date(md.modified().ok()));
                            ui.end_row();
                            ui.weak("Acessado em:");
                            ui.label(format_date(md.accessed().ok()));
                            ui.end_row();
                            ui.weak("Somente leitura:");
                            ui.label(if md.permissions().readonly() { "Sim" } else { "Não" });
                            ui.end_row();
                        }
                        ui.weak("Oculto:");
                        ui.label(if e.is_hidden { "Sim" } else { "Não" });
                        ui.end_row();
                    });
            }
            None => {
                ui.heading(format!("{} itens selecionados", paths.len()));
                ui.separator();
                Grid::new("props_grid_multi")
                    .num_columns(2)
                    .spacing(vec2(16.0, 6.0))
                    .show(ui, |ui| {
                        ui.weak("Tamanho total:");
                        ui.label(format!(
                            "{} ({} bytes){}",
                            format_size_human(snapshot.bytes),
                            thousands(snapshot.bytes),
                            if snapshot.finished { "" } else { " — calculando…" }
                        ));
                        ui.end_row();
                        ui.weak("Contém:");
                        ui.label(format!(
                            "{} {}, {} {}",
                            thousands(snapshot.files as u64),
                            plural(snapshot.files, "arquivo", "arquivos"),
                            thousands(snapshot.folders as u64),
                            plural(snapshot.folders, "pasta", "pastas")
                        ));
                        ui.end_row();
                        ui.weak("Local:");
                        ui.add(
                            egui::Label::new(
                                paths[0]
                                    .parent()
                                    .map(|p| p.to_string_lossy().into_owned())
                                    .unwrap_or_default(),
                            )
                            .wrap(),
                        );
                        ui.end_row();
                    });
            }
        }
        ui.add_space(12.0);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.button("Fechar").clicked() || ui.input(|i| i.key_pressed(Key::Enter)) {
                close = true;
            }
        });
    });
    if close || m.should_close() {
        Outcome::Close
    } else {
        Outcome::Keep
    }
}

fn shortcuts_dialog(ctx: &Context) -> Outcome {
    let mut close = false;
    let m = Modal::new(Id::unique("dlg_shortcuts")).show(ctx, |ui| {
        ui.set_width(760.0);
        ui.heading("Atalhos de teclado");
        ui.add_space(6.0);
        egui::ScrollArea::vertical().max_height(520.0).show(ui, |ui| {
            ui.columns(3, |cols| {
                for (col, (section, items)) in cols.iter_mut().zip(HELP.iter()) {
                    col.strong(*section);
                    col.add_space(4.0);
                    Grid::new(("sc", section))
                        .num_columns(2)
                        .spacing(vec2(10.0, 3.0))
                        .striped(true)
                        .show(col, |ui| {
                            for (keys, desc) in items.iter() {
                                ui.label(RichText::new(*keys).monospace().size(12.0));
                                ui.label(RichText::new(*desc).size(12.5));
                                ui.end_row();
                            }
                        });
                }
            });
        });
        ui.add_space(10.0);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.button("Fechar").clicked() {
                close = true;
            }
        });
    });
    if close || m.should_close() {
        Outcome::Close
    } else {
        Outcome::Keep
    }
}

#[allow(dead_code)]
fn _keep(_: &mut Ui) {}
