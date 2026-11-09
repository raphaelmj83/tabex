use crate::app::{plural, TabexApp};
use crate::fs::entry::{format_size_human, thousands};
use egui::{Align, Layout, ProgressBar, RichText, Ui};

/// "2 min", "45 s", "1 h 10 min".
pub fn human_eta(secs: u64) -> String {
    if secs < 60 {
        format!("{secs} s")
    } else if secs < 3600 {
        format!("{} min", secs.div_ceil(60))
    } else {
        format!("{} h {} min", secs / 3600, (secs % 3600) / 60)
    }
}

pub fn show(ui: &mut Ui, app: &mut TabexApp) {
    let tab = app.session.active_tab();
    let mut clear_filters = false;
    let mut show_update = false;
    let mut cancel = false;
    let mut toggle_pause = false;
    let mut clear_queue = false;
    ui.horizontal(|ui| {
        let n = tab.visible_count();
        ui.label(format!("{} {}", thousands(n as u64), plural(n, "item", "itens")));
        let sel = tab.selected.len();
        if sel > 0 {
            ui.separator();
            let size = tab.selected_size();
            ui.label(format!(
                "{} {} selecionado{}  {}",
                thousands(sel as u64),
                plural(sel, "item", "itens"),
                if sel == 1 { "" } else { "s" },
                format_size_human(size)
            ));
        }
        if let Some(s) = &tab.search {
            ui.separator();
            if s.done {
                ui.label(format!(
                    "Pesquisa concluída: {} {}",
                    thousands(s.hits.len() as u64),
                    plural(s.hits.len(), "resultado", "resultados")
                ));
            } else {
                ui.spinner();
                ui.label(format!(
                    "Pesquisando… {} encontrados · {} verificados",
                    thousands(s.hits.len() as u64),
                    thousands(s.scanned() as u64)
                ));
            }
        }
        if tab.quick_filter.is_active() {
            ui.separator();
            ui.label(RichText::new("Filtro ativo").color(ui.visuals().hyperlink_color));
            if ui
                .add(egui::Button::new(RichText::new("✕").small()).frame(false))
                .on_hover_text("Limpar filtros")
                .clicked()
            {
                clear_filters = true;
            }
        }

        for op in &app.ops {
            if let Ok(p) = op.progress.lock() {
                ui.separator();
                let paused = op.is_paused();
                let text = if p.counting {
                    format!("{}: calculando…", p.label)
                } else if p.total_bytes > 0 {
                    let mut t = format!(
                        "{} {} de {} ({:.0}%)",
                        p.label,
                        format_size_human(p.done_bytes),
                        format_size_human(p.total_bytes),
                        p.fraction() * 100.0
                    );
                    if paused {
                        t.push_str(" · pausado");
                    } else {
                        if let Some(sp) = p.speed() {
                            t.push_str(&format!(" · {}/s", format_size_human(sp as u64)));
                        }
                        if let Some(eta) = p.eta_secs() {
                            t.push_str(&format!(" · {}", human_eta(eta)));
                        }
                    }
                    t
                } else {
                    format!("{} {}/{}", p.label, p.done, p.total)
                };
                let bar = ProgressBar::new(p.fraction())
                    .desired_width(260.0)
                    .desired_height(14.0)
                    .animate(p.counting)
                    .text(RichText::new(text).small());
                ui.add(bar).on_hover_text(if p.current.is_empty() {
                    p.label.clone()
                } else {
                    p.current.clone()
                });
                if ui
                    .add(egui::Button::new(RichText::new(if paused { "▶" } else { "⏸" }).small()).frame(false))
                    .on_hover_text(if paused { "Continuar" } else { "Pausar" })
                    .clicked()
                {
                    toggle_pause = true;
                }
                if ui
                    .add(egui::Button::new(RichText::new("✕").small()).frame(false))
                    .on_hover_text("Cancelar operação")
                    .clicked()
                {
                    cancel = true;
                }
            }
        }
        if !app.op_queue.is_empty() {
            let n = app.op_queue.len();
            let r = ui.label(RichText::new(format!("+{n} na fila")).small().weak());
            r.on_hover_text(app.op_queue.iter().map(|q| q.label()).collect::<Vec<_>>().join("\n"));
            if ui
                .add(egui::Button::new(RichText::new("limpar").small()).frame(false))
                .clicked()
            {
                clear_queue = true;
            }
        }
        if tab.is_loading_slow() {
            ui.separator();
            ui.spinner();
            ui.label(RichText::new("Carregando…").small());
        }

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if let crate::update::UpdateState::Available(info) = app.updater.state() {
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new(format!("⬆ Atualizar para {}", info.tag))
                                .color(ui.visuals().hyperlink_color)
                                .small(),
                        )
                        .frame(false),
                    )
                    .clicked()
                {
                    show_update = true;
                }
                ui.separator();
            }
            if let Some((msg, _)) = &app.toast {
                ui.label(RichText::new(msg).color(ui.visuals().hyperlink_color));
                ui.separator();
            }
            if app.session.show_hidden {
                ui.label(RichText::new("Itens ocultos visíveis").weak().small());
            }
            if let Some(c) = &app.clipboard {
                ui.label(
                    RichText::new(format!(
                        "{} {} {}",
                        c.paths.len(),
                        plural(c.paths.len(), "item", "itens"),
                        if c.cut { "para mover" } else { "para colar" }
                    ))
                    .weak()
                    .small(),
                );
            }
        });
    });
    if clear_filters {
        app.pending.push(crate::app::Action::ClearFilters);
    }
    if cancel {
        app.pending.push(crate::app::Action::CancelCurrentOp);
    }
    if toggle_pause {
        app.pending.push(crate::app::Action::TogglePauseOp);
    }
    if clear_queue {
        app.pending.push(crate::app::Action::ClearQueue);
    }
    if show_update {
        app.pending.push(crate::app::Action::ShowUpdateDialog);
    }
}
