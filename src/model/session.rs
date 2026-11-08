use super::color::GroupColor;
use super::group::TabGroup;
use super::tab::Tab;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Estado persistido entre execuções (grupos, abas, fixados e preferências).
#[derive(Serialize, Deserialize)]
pub struct Session {
    pub groups: Vec<TabGroup>,
    pub active_group: usize,
    pub pinned: Vec<PathBuf>,
    pub show_hidden: bool,
    pub dark_theme: bool,
    pub sidebar_visible: bool,
    pub next_id: u64,
    #[serde(default)]
    pub groups_created: usize,
}

impl Session {
    pub fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("tabex")
            .join("session.json")
    }

    pub fn load() -> Session {
        let path = Self::config_path();
        let loaded = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<Session>(&s).ok());
        let mut s = loaded.unwrap_or_else(Session::fresh);
        s.sanitize();
        s
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::config_path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    pub fn fresh() -> Session {
        let home = home_dir();
        let mut s = Session {
            groups: Vec::new(),
            active_group: 0,
            pinned: Vec::new(),
            show_hidden: false,
            dark_theme: false,
            sidebar_visible: true,
            next_id: 1,
            groups_created: 0,
        };
        let gid = s.alloc_id();
        let mut g = TabGroup::new(gid, "Principal", GroupColor::Blue);
        let tid = s.alloc_id();
        g.tabs.push(Tab::new(tid, home));
        s.groups.push(g);
        s.groups_created = 1;
        s
    }

    /// Garante invariantes após carregar (nenhum grupo vazio, índices válidos, caminhos existentes).
    fn sanitize(&mut self) {
        for g in &mut self.groups {
            for t in &mut g.tabs {
                if !t.path.is_dir() {
                    // Sobe até achar uma pasta existente.
                    let mut p = t.path.clone();
                    while !p.is_dir() {
                        match p.parent() {
                            Some(parent) => p = parent.to_path_buf(),
                            None => {
                                p = home_dir();
                                break;
                            }
                        }
                    }
                    t.path = p.clone();
                    t.history = vec![p];
                    t.history_idx = 0;
                }
                t.needs_reload = true;
            }
            g.tabs.retain(|_| true);
            g.clamp_active();
        }
        self.groups.retain(|g| !g.tabs.is_empty());
        if self.groups.is_empty() {
            *self = Session::fresh();
            return;
        }
        if self.active_group >= self.groups.len() {
            self.active_group = 0;
        }
        let max_id = self
            .groups
            .iter()
            .flat_map(|g| std::iter::once(g.id).chain(g.tabs.iter().map(|t| t.id)))
            .max()
            .unwrap_or(0);
        if self.next_id <= max_id {
            self.next_id = max_id + 1;
        }
    }

    pub fn alloc_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn active_group(&self) -> &TabGroup {
        &self.groups[self.active_group]
    }

    pub fn active_group_mut(&mut self) -> &mut TabGroup {
        let i = self.active_group;
        &mut self.groups[i]
    }

    pub fn active_tab(&self) -> &Tab {
        let g = self.active_group();
        &g.tabs[g.active]
    }

    pub fn active_tab_mut(&mut self) -> &mut Tab {
        let g = self.active_group_mut();
        let i = g.active;
        &mut g.tabs[i]
    }

    pub fn active_pos(&self) -> (usize, usize) {
        (self.active_group, self.groups[self.active_group].active)
    }

    /// Abre uma nova aba no grupo `g` (após a aba ativa) e a ativa.
    pub fn open_tab(&mut self, g: usize, path: PathBuf) -> (usize, usize) {
        let id = self.alloc_id();
        let group = &mut self.groups[g];
        let insert_at = if group.tabs.is_empty() { 0 } else { group.active + 1 };
        group.tabs.insert(insert_at, Tab::new(id, path));
        group.active = insert_at;
        group.collapsed = false;
        self.active_group = g;
        (g, insert_at)
    }

    /// Fecha a aba; devolve-a para possível reabertura. Nunca deixa a sessão sem abas.
    pub fn close_tab(&mut self, g: usize, t: usize) -> Option<Tab> {
        if g >= self.groups.len() || t >= self.groups[g].tabs.len() {
            return None;
        }
        let total: usize = self.groups.iter().map(|x| x.tabs.len()).sum();
        if total <= 1 {
            // Última aba: em vez de fechar, volta para a pasta pessoal.
            self.groups[g].tabs[t].navigate(home_dir());
            return None;
        }
        let tab = self.groups[g].tabs.remove(t);
        if self.groups[g].tabs.is_empty() {
            self.groups.remove(g);
            if self.active_group >= self.groups.len() {
                self.active_group = self.groups.len() - 1;
            } else if self.active_group > g {
                self.active_group -= 1;
            }
        } else {
            let group = &mut self.groups[g];
            if group.active > t || group.active >= group.tabs.len() {
                group.active = group.active.saturating_sub(1);
            }
        }
        Some(tab)
    }

    pub fn new_group(&mut self, name: Option<String>) -> usize {
        let id = self.alloc_id();
        self.groups_created += 1;
        let n = self.groups_created;
        let name = name.unwrap_or_else(|| format!("Grupo {n}"));
        let color = GroupColor::nth(n.saturating_sub(1));
        self.groups.push(TabGroup::new(id, name, color));
        self.groups.len() - 1
    }

    /// Move a aba (g, t) para o grupo `target` (ou um novo grupo se `None`), na posição `at`.
    pub fn move_tab(&mut self, g: usize, t: usize, target: Option<usize>, at: Option<usize>) {
        if g >= self.groups.len() || t >= self.groups[g].tabs.len() {
            return;
        }
        if let Some(tg) = target {
            if tg >= self.groups.len() {
                return;
            }
        }
        let tab = self.groups[g].tabs.remove(t);
        let mut at = at;
        if target == Some(g) {
            if let Some(a) = at.as_mut() {
                if *a > t {
                    *a -= 1;
                }
            }
        }
        let mut target_idx = match target {
            Some(tg) => tg,
            None => self.new_group(None),
        };
        if self.groups[g].tabs.is_empty() && target_idx != g {
            self.groups.remove(g);
            if target_idx > g {
                target_idx -= 1;
            }
        }
        let group = &mut self.groups[target_idx];
        let pos = at.map(|a| a.min(group.tabs.len())).unwrap_or(group.tabs.len());
        group.tabs.insert(pos, tab);
        group.active = pos;
        group.collapsed = false;
        self.active_group = target_idx;
        for grp in &mut self.groups {
            grp.clamp_active();
        }
    }

    /// Ativa a próxima/anterior aba percorrendo todos os grupos.
    pub fn cycle_tab(&mut self, delta: i32) {
        let mut flat: Vec<(usize, usize)> = Vec::new();
        for (gi, g) in self.groups.iter().enumerate() {
            for ti in 0..g.tabs.len() {
                flat.push((gi, ti));
            }
        }
        if flat.is_empty() {
            return;
        }
        let cur = self.active_pos();
        let pos = flat.iter().position(|&p| p == cur).unwrap_or(0) as i32;
        let n = flat.len() as i32;
        let next = ((pos + delta) % n + n) % n;
        let (g, t) = flat[next as usize];
        self.activate(g, t);
    }

    pub fn activate(&mut self, g: usize, t: usize) {
        if g < self.groups.len() && t < self.groups[g].tabs.len() {
            self.active_group = g;
            self.groups[g].active = t;
            self.groups[g].collapsed = false;
        }
    }

    pub fn cycle_group(&mut self, delta: i32) {
        let n = self.groups.len() as i32;
        if n == 0 {
            return;
        }
        let next = ((self.active_group as i32 + delta) % n + n) % n;
        self.active_group = next as usize;
        self.groups[self.active_group].collapsed = false;
    }

    pub fn move_tab_within_group(&mut self, delta: isize) {
        let (g, t) = self.active_pos();
        let len = self.groups[g].tabs.len() as isize;
        let nt = (t as isize + delta).clamp(0, len - 1) as usize;
        if nt != t {
            self.groups[g].tabs.swap(t, nt);
            self.groups[g].active = nt;
        }
    }

    pub fn move_group(&mut self, g: usize, delta: isize) {
        let len = self.groups.len() as isize;
        let ng = (g as isize + delta).clamp(0, len - 1) as usize;
        if ng != g {
            self.groups.swap(g, ng);
            if self.active_group == g {
                self.active_group = ng;
            } else if self.active_group == ng {
                self.active_group = g;
            }
        }
    }

    pub fn is_pinned(&self, p: &Path) -> bool {
        self.pinned.iter().any(|x| x == p)
    }

    pub fn toggle_pin(&mut self, p: PathBuf) {
        if let Some(i) = self.pinned.iter().position(|x| *x == p) {
            self.pinned.remove(i);
        } else {
            self.pinned.push(p);
        }
    }
}

pub fn home_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session_with(groups: &[&[&str]]) -> Session {
        let mut s = Session::fresh();
        s.groups.clear();
        s.groups_created = 0;
        for tabs in groups {
            let gi = s.new_group(None);
            for t in tabs.iter() {
                s.open_tab(gi, PathBuf::from(t));
            }
        }
        s.active_group = 0;
        s
    }

    #[test]
    fn open_and_close_tabs() {
        let mut s = session_with(&[&["/a", "/b"]]);
        assert_eq!(s.groups[0].tabs.len(), 2);
        assert_eq!(s.active_pos(), (0, 1));
        s.close_tab(0, 1);
        assert_eq!(s.groups[0].tabs.len(), 1);
        assert_eq!(s.active_pos(), (0, 0));
        // A última aba nunca é fechada: volta para a pasta pessoal.
        s.close_tab(0, 0);
        assert_eq!(s.groups.len(), 1);
        assert_eq!(s.groups[0].tabs.len(), 1);
    }

    #[test]
    fn closing_last_tab_of_group_removes_group() {
        let mut s = session_with(&[&["/a"], &["/b"]]);
        s.active_group = 1;
        s.close_tab(1, 0);
        assert_eq!(s.groups.len(), 1);
        assert_eq!(s.active_group, 0);
    }

    #[test]
    fn move_tab_between_groups_and_to_new_group() {
        let mut s = session_with(&[&["/a", "/b"], &["/c"]]);
        s.move_tab(0, 0, Some(1), None);
        assert_eq!(s.groups[0].tabs.len(), 1);
        assert_eq!(s.groups[1].tabs.len(), 2);
        assert_eq!(s.groups[1].tabs[1].path, PathBuf::from("/a"));
        assert_eq!(s.active_pos(), (1, 1));

        // Para um grupo novo
        s.move_tab(1, 0, None, None);
        assert_eq!(s.groups.len(), 3);
        assert_eq!(s.groups[2].tabs[0].path, PathBuf::from("/c"));

        // Esvaziar o grupo de origem remove-o e corrige o índice do destino
        let mut s = session_with(&[&["/a"], &["/b"]]);
        s.move_tab(0, 0, Some(1), Some(0));
        assert_eq!(s.groups.len(), 1);
        assert_eq!(s.groups[0].tabs[0].path, PathBuf::from("/a"));
        assert_eq!(s.groups[0].tabs[1].path, PathBuf::from("/b"));
    }

    #[test]
    fn reorder_within_group_by_drop() {
        let mut s = session_with(&[&["/a", "/b", "/c"]]);
        // arrasta /a para antes de /c
        s.move_tab(0, 0, Some(0), Some(2));
        let paths: Vec<_> = s.groups[0].tabs.iter().map(|t| t.path.clone()).collect();
        assert_eq!(
            paths,
            vec![PathBuf::from("/b"), PathBuf::from("/a"), PathBuf::from("/c")]
        );
    }

    #[test]
    fn cycle_tabs_across_groups() {
        let mut s = session_with(&[&["/a", "/b"], &["/c"]]);
        s.activate(0, 1);
        s.cycle_tab(1);
        assert_eq!(s.active_pos(), (1, 0));
        s.cycle_tab(1);
        assert_eq!(s.active_pos(), (0, 0));
        s.cycle_tab(-1);
        assert_eq!(s.active_pos(), (1, 0));
    }

    #[test]
    fn session_roundtrip_json() {
        let s = session_with(&[&["/a"], &["/b", "/c"]]);
        let json = serde_json::to_string(&s).unwrap();
        let back: Session = serde_json::from_str(&json).unwrap();
        assert_eq!(back.groups.len(), 2);
        assert_eq!(back.groups[1].tabs.len(), 2);
        assert_eq!(back.groups[1].color, s.groups[1].color);
    }

    #[test]
    fn pin_toggle() {
        let mut s = Session::fresh();
        s.toggle_pin(PathBuf::from("/x"));
        assert!(s.is_pinned(Path::new("/x")));
        s.toggle_pin(PathBuf::from("/x"));
        assert!(!s.is_pinned(Path::new("/x")));
    }
}
