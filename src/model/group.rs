use super::color::GroupColor;
use super::tab::Tab;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct TabGroup {
    pub id: u64,
    pub name: String,
    pub color: GroupColor,
    pub tabs: Vec<Tab>,
    pub active: usize,
    pub collapsed: bool,
}

impl TabGroup {
    pub fn new(id: u64, name: impl Into<String>, color: GroupColor) -> TabGroup {
        TabGroup {
            id,
            name: name.into(),
            color,
            tabs: Vec::new(),
            active: 0,
            collapsed: false,
        }
    }

    pub fn clamp_active(&mut self) {
        if self.tabs.is_empty() {
            self.active = 0;
        } else if self.active >= self.tabs.len() {
            self.active = self.tabs.len() - 1;
        }
    }
}
