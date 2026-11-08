use egui::Color32;
use serde::{Deserialize, Serialize};

/// Cores disponíveis para grupos de abas (paleta inspirada nos grupos de abas do Chrome).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum GroupColor {
    Grey,
    #[default]
    Blue,
    Red,
    Yellow,
    Green,
    Pink,
    Purple,
    Cyan,
    Orange,
}

impl GroupColor {
    pub const ALL: [GroupColor; 9] = [
        GroupColor::Blue,
        GroupColor::Red,
        GroupColor::Yellow,
        GroupColor::Green,
        GroupColor::Pink,
        GroupColor::Purple,
        GroupColor::Cyan,
        GroupColor::Orange,
        GroupColor::Grey,
    ];

    pub fn label(self) -> &'static str {
        match self {
            GroupColor::Grey => "Cinza",
            GroupColor::Blue => "Azul",
            GroupColor::Red => "Vermelho",
            GroupColor::Yellow => "Amarelo",
            GroupColor::Green => "Verde",
            GroupColor::Pink => "Rosa",
            GroupColor::Purple => "Roxo",
            GroupColor::Cyan => "Ciano",
            GroupColor::Orange => "Laranja",
        }
    }

    pub fn color(self) -> Color32 {
        match self {
            GroupColor::Grey => Color32::from_rgb(0x5F, 0x63, 0x68),
            GroupColor::Blue => Color32::from_rgb(0x1A, 0x73, 0xE8),
            GroupColor::Red => Color32::from_rgb(0xD9, 0x30, 0x25),
            GroupColor::Yellow => Color32::from_rgb(0xF9, 0xAB, 0x00),
            GroupColor::Green => Color32::from_rgb(0x18, 0x80, 0x38),
            GroupColor::Pink => Color32::from_rgb(0xD0, 0x18, 0x84),
            GroupColor::Purple => Color32::from_rgb(0xA1, 0x42, 0xF4),
            GroupColor::Cyan => Color32::from_rgb(0x00, 0x7B, 0x83),
            GroupColor::Orange => Color32::from_rgb(0xFA, 0x90, 0x3E),
        }
    }

    /// Cor de texto legível sobre `color()`.
    pub fn text_color(self) -> Color32 {
        match self {
            GroupColor::Yellow | GroupColor::Orange => Color32::from_rgb(0x20, 0x20, 0x20),
            _ => Color32::WHITE,
        }
    }

    /// Versão translúcida usada como fundo de abas do grupo.
    pub fn tint(self, alpha: u8) -> Color32 {
        let c = self.color();
        Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), alpha)
    }

    /// Próxima cor da paleta (para alternar cores de novos grupos).
    pub fn nth(n: usize) -> Self {
        Self::ALL[n % Self::ALL.len()]
    }
}
