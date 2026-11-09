pub mod dialogs;
pub mod dnd;
pub mod filelist;
pub mod gridview;
pub mod icons;
pub mod preview;
pub mod sidebar;
pub mod statusbar;
pub mod tabstrip;
pub mod theme;
pub mod toolbar;
pub mod windowbar;

/// Pequeno botão de ícone sem moldura, com dica.
pub fn icon_button(ui: &mut egui::Ui, icon: &str, tip: &str, enabled: bool) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(egui::RichText::new(icon).size(16.0))
            .frame(false)
            .min_size(egui::vec2(28.0, 26.0)),
    )
    .on_hover_text(tip)
}
