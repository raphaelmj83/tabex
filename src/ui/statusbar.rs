use crate::app::{plural, TabexApp};
use crate::fs::entry::{format_size_human, thousands};
use egui::{Align, Layout, ProgressBar, RichText, Ui};

pub fn show(ui: &mut Ui, app: &mut TabexApp) {
    let tab = app.session.active_tab();
    let mut clear_filters = false;
    let mut show_update = false;
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
                let frac = if p.total > 0 {
                    p.done as f32 / p.total as f32
                } else {
                    0.0
                };
                ui.add(
                    ProgressBar::new(frac)
                        .desired_width(140.0)
                        .desired_height(14.0)
                        .text(RichText::new(format!("{} {}", p.label, p.current)).small()),
                );
            }
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
    if show_update {
        app.pending.push(crate::app::Action::ShowUpdateDialog);
    }
}
