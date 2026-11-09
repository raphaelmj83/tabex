use super::color::GroupColor;
use super::group::TabGroup;
use serde::{Deserialize, Serialize};

/// "Janela" (espaço de trabalho): um conjunto de grupos de abas. É o nível acima dos grupos.
///
/// Enquanto a janela está ativa, seus grupos vivem em `Session::groups`; o campo `groups`
/// aqui só guarda os grupos das janelas inativas (e é sincronizado ao salvar/trocar).
#[derive(Serialize, Deserialize)]
pub struct Workspace {
    pub id: u64,
    pub name: String,
    pub color: GroupColor,
    #[serde(default)]
    pub groups: Vec<TabGroup>,
    #[serde(default)]
    pub active_group: usize,
    /// Aberta como janela separada do sistema (viewport).
    #[serde(default)]
    pub detached: bool,
}

impl Workspace {
    pub fn new(id: u64, name: impl Into<String>, color: GroupColor) -> Workspace {
        Workspace {
            id,
            name: name.into(),
            color,
            groups: Vec::new(),
            active_group: 0,
            detached: false,
        }
    }

    pub fn tab_count(&self) -> usize {
        self.groups.iter().map(|g| g.tabs.len()).sum()
    }
}
