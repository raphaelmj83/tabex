use super::color::GroupColor;
use super::group::TabGroup;
use super::tab::Tab;
use super::window::Workspace;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Estado em execução: janelas, grupos, abas, fixados e preferências.
///
/// Os grupos da janela ativa ficam em `groups`/`active_group` (acesso direto e rápido);
/// `windows[active_window].groups` fica vazio enquanto ela está ativa.
pub struct Session {
    pub windows: Vec<Workspace>,
    pub active_window: usize,
    pub groups: Vec<TabGroup>,
    pub active_group: usize,
    pub pinned: Vec<PathBuf>,
    pub show_hidden: bool,
    pub dark_theme: bool,
    pub sidebar_visible: bool,
    pub next_id: u64,
    pub groups_created: usize,
    pub windows_created: usize,
    /// Usa o menu de contexto do Windows (shell) em vez do menu interno.
    pub use_native_menu: bool,
    /// Verifica novas versões no GitHub ao iniciar (uma vez por dia).
    pub auto_update: bool,
    /// Época (segundos) da última verificação de atualização.
    pub last_update_check: u64,
}

/// Formato gravado em disco (versão 2: com janelas).
#[derive(Serialize)]
struct SessionFileRef<'a> {
    version: u32,
    windows: &'a [Workspace],
    active_window: usize,
    pinned: &'a [PathBuf],
    show_hidden: bool,
    dark_theme: bool,
    sidebar_visible: bool,
    next_id: u64,
    groups_created: usize,
    windows_created: usize,
    use_native_menu: bool,
    auto_update: bool,
    last_update_check: u64,
}

#[derive(Deserialize)]
struct SessionFile {
    #[serde(default)]
    windows: Vec<Workspace>,
    #[serde(default)]
    active_window: usize,
    // Formato antigo (versão 1): grupos no nível superior.
    #[serde(default)]
    groups: Vec<TabGroup>,
    #[serde(default)]
    active_group: usize,
    #[serde(default)]
    pinned: Vec<PathBuf>,
    #[serde(default)]
    show_hidden: bool,
    #[serde(default)]
    dark_theme: bool,
    #[serde(default = "yes")]
    sidebar_visible: bool,
    #[serde(default = "one")]
    next_id: u64,
    #[serde(default)]
    groups_created: usize,
    #[serde(default)]
    windows_created: usize,
    #[serde(default = "default_native_menu")]
    use_native_menu: bool,
    #[serde(default = "yes")]
    auto_update: bool,
    #[serde(default)]
    last_update_check: u64,
}

fn yes() -> bool {
    true
}
fn one() -> u64 {
    1
}
fn default_native_menu() -> bool {
    cfg!(windows)
}

impl Session {
    // ------------------------------------------------------------------ arquivos

    /// Pasta de configuração. Em instalações "portáteis" (ex.: `C:\tabex`) fica ao lado do
    /// executável, para que cada computador tenha a sua; caso contrário usa a pasta do usuário.
    pub fn config_dir() -> PathBuf {
        if let Some(dir) = portable_dir() {
            return dir;
        }
        user_config_dir()
    }

    pub fn config_path() -> PathBuf {
        Self::config_dir().join("session.json")
    }

    pub fn is_portable() -> bool {
        portable_dir().is_some()
    }

    pub fn load() -> Session {
        let path = Self::config_path();
        // Migra uma sessão antiga da pasta do usuário para a pasta portátil, se for o caso.
        if !path.exists() {
            let legacy = user_config_dir().join("session.json");
            if legacy.exists() && legacy != path {
                if let Some(dir) = path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                let _ = std::fs::copy(&legacy, &path);
            }
        }
        let loaded = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<SessionFile>(&s).ok())
            .map(Session::from_file);
        let mut s = loaded.unwrap_or_else(Session::fresh);
        s.sanitize();
        s
    }

    fn from_file(f: SessionFile) -> Session {
        let mut windows = f.windows;
        let mut windows_created = f.windows_created;
        if windows.is_empty() {
            // Formato antigo: embrulha os grupos numa única janela.
            let mut w = Workspace::new(0, "Principal", GroupColor::Blue);
            w.groups = f.groups;
            w.active_group = f.active_group;
            windows.push(w);
            windows_created = windows_created.max(1);
        }
        let mut s = Session {
            windows,
            active_window: f.active_window,
            groups: Vec::new(),
            active_group: 0,
            pinned: f.pinned,
            show_hidden: f.show_hidden,
            dark_theme: f.dark_theme,
            sidebar_visible: f.sidebar_visible,
            next_id: f.next_id,
            groups_created: f.groups_created,
            windows_created,
            use_native_menu: f.use_native_menu,
            auto_update: f.auto_update,
            last_update_check: f.last_update_check,
        };
        if s.active_window >= s.windows.len() {
            s.active_window = 0;
        }
        // Carrega a janela ativa para o estado "vivo".
        let aw = s.active_window;
        s.groups = std::mem::take(&mut s.windows[aw].groups);
        s.active_group = s.windows[aw].active_group;
        s
    }

    pub fn save(&mut self) -> std::io::Result<()> {
        let path = Self::config_path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        self.stash_active();
        let file = SessionFileRef {
            version: 2,
            windows: &self.windows,
            active_window: self.active_window,
            pinned: &self.pinned,
            show_hidden: self.show_hidden,
            dark_theme: self.dark_theme,
            sidebar_visible: self.sidebar_visible,
            next_id: self.next_id,
            groups_created: self.groups_created,
            windows_created: self.windows_created,
            use_native_menu: self.use_native_menu,
            auto_update: self.auto_update,
            last_update_check: self.last_update_check,
        };
        let json = serde_json::to_string_pretty(&file).map_err(std::io::Error::other);
        self.unstash_active();
        let json = json?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    /// Move os grupos vivos para dentro de `windows[active_window]` (para salvar/trocar).
    fn stash_active(&mut self) {
        let aw = self.active_window;
        std::mem::swap(&mut self.windows[aw].groups, &mut self.groups);
        self.windows[aw].active_group = self.active_group;
    }

    fn unstash_active(&mut self) {
        let aw = self.active_window;
        std::mem::swap(&mut self.windows[aw].groups, &mut self.groups);
        self.active_group = self.windows[aw].active_group;
    }

    pub fn fresh() -> Session {
        let mut s = Session {
            windows: Vec::new(),
            active_window: 0,
            groups: Vec::new(),
            active_group: 0,
            pinned: Vec::new(),
            show_hidden: false,
            dark_theme: false,
            sidebar_visible: true,
            next_id: 1,
            groups_created: 0,
            windows_created: 0,
            use_native_menu: cfg!(windows),
            auto_update: true,
            last_update_check: 0,
        };
        let wid = s.alloc_id();
        s.windows.push(Workspace::new(wid, "Principal", GroupColor::Blue));
        s.windows_created = 1;
        s.active_window = 0;
        let gid = s.alloc_id();
        let mut g = TabGroup::new(gid, "Principal", GroupColor::Blue);
        let tid = s.alloc_id();
        g.tabs.push(Tab::new(tid, home_dir()));
        s.groups.push(g);
        s.groups_created = 1;
        s
    }

    /// Garante invariantes após carregar (nenhuma janela/grupo vazio, índices válidos, caminhos existentes).
    fn sanitize(&mut self) {
        self.stash_active();
        for w in &mut self.windows {
            for g in &mut w.groups {
                for t in &mut g.tabs {
                    if !t.path.is_dir() {
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
                g.clamp_active();
            }
            w.groups.retain(|g| !g.tabs.is_empty());
            if w.active_group >= w.groups.len() {
                w.active_group = 0;
            }
        }
        self.windows.retain(|w| !w.groups.is_empty());
        if self.windows.is_empty() {
            *self = Session::fresh();
            return;
        }
        if self.active_window >= self.windows.len() {
            self.active_window = 0;
        }
        let max_id = self
            .windows
            .iter()
            .flat_map(|w| {
                std::iter::once(w.id).chain(
                    w.groups
                        .iter()
                        .flat_map(|g| std::iter::once(g.id).chain(g.tabs.iter().map(|t| t.id))),
                )
            })
            .max()
            .unwrap_or(0);
        if self.next_id <= max_id {
            self.next_id = max_id + 1;
        }
        self.unstash_active();
    }

    pub fn alloc_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    // ------------------------------------------------------------------ acesso

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

    pub fn active_workspace(&self) -> &Workspace {
        &self.windows[self.active_window]
    }

    /// Número de abas de uma janela (a ativa é contada pelos grupos vivos).
    pub fn window_tab_count(&self, w: usize) -> usize {
        if w == self.active_window {
            self.groups.iter().map(|g| g.tabs.len()).sum()
        } else {
            self.windows.get(w).map(|x| x.tab_count()).unwrap_or(0)
        }
    }

    pub fn window_group_count(&self, w: usize) -> usize {
        if w == self.active_window {
            self.groups.len()
        } else {
            self.windows.get(w).map(|x| x.groups.len()).unwrap_or(0)
        }
    }

    // ------------------------------------------------------------------ abas

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

    /// Fecha a aba; devolve-a para possível reabertura. Nunca deixa a janela sem abas.
    pub fn close_tab(&mut self, g: usize, t: usize) -> Option<Tab> {
        if g >= self.groups.len() || t >= self.groups[g].tabs.len() {
            return None;
        }
        let total: usize = self.groups.iter().map(|x| x.tabs.len()).sum();
        if total <= 1 {
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

    /// Ativa a próxima/anterior aba percorrendo todos os grupos da janela.
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

    // ------------------------------------------------------------------ janelas

    /// Cria uma janela nova (com um grupo e uma aba na pasta `path`) e a ativa.
    pub fn new_window(&mut self, name: Option<String>, color: Option<GroupColor>, path: PathBuf) -> usize {
        let id = self.alloc_id();
        self.windows_created += 1;
        let n = self.windows_created;
        let name = name.unwrap_or_else(|| format!("Janela {n}"));
        let color = color.unwrap_or_else(|| GroupColor::nth(n.saturating_sub(1)));
        let mut w = Workspace::new(id, name, color);
        let gid = self.alloc_id();
        self.groups_created += 1;
        let mut g = TabGroup::new(gid, "Principal", color);
        let tid = self.alloc_id();
        g.tabs.push(Tab::new(tid, path));
        w.groups.push(g);
        self.windows.push(w);
        let idx = self.windows.len() - 1;
        self.switch_window(idx);
        idx
    }

    pub fn switch_window(&mut self, w: usize) {
        if w >= self.windows.len() || w == self.active_window {
            return;
        }
        self.stash_active();
        self.active_window = w;
        self.unstash_active();
        if self.groups.is_empty() {
            self.ensure_live_group();
        }
        if self.active_group >= self.groups.len() {
            self.active_group = 0;
        }
    }

    pub fn cycle_window(&mut self, delta: i32) {
        let n = self.windows.len() as i32;
        if n <= 1 {
            return;
        }
        let next = ((self.active_window as i32 + delta) % n + n) % n;
        self.switch_window(next as usize);
    }

    /// Fecha a janela `w`, devolvendo suas abas (para reabertura). Nunca deixa a sessão sem janela.
    pub fn close_window(&mut self, w: usize) -> Vec<Tab> {
        if w >= self.windows.len() {
            return Vec::new();
        }
        if self.windows.len() == 1 {
            // Última janela: reinicia com uma aba na pasta pessoal, devolvendo as abas antigas.
            let old: Vec<Tab> = self.groups.drain(..).flat_map(|g| g.tabs).collect();
            self.ensure_live_group();
            self.active_group = 0;
            return old;
        }
        if w == self.active_window {
            let target = if w + 1 < self.windows.len() { w + 1 } else { w - 1 };
            self.switch_window(target);
        }
        let removed = self.windows.remove(w);
        if self.active_window > w {
            self.active_window -= 1;
        }
        removed.groups.into_iter().flat_map(|g| g.tabs).collect()
    }

    pub fn move_window(&mut self, w: usize, delta: isize) {
        let len = self.windows.len() as isize;
        let nw = (w as isize + delta).clamp(0, len - 1) as usize;
        if nw != w {
            self.windows.swap(w, nw);
            if self.active_window == w {
                self.active_window = nw;
            } else if self.active_window == nw {
                self.active_window = w;
            }
        }
    }

    /// Move o grupo `g` da janela ativa para a janela `w`.
    pub fn move_group_to_window(&mut self, g: usize, w: usize) {
        if g >= self.groups.len() || w >= self.windows.len() || w == self.active_window {
            return;
        }
        let group = self.groups.remove(g);
        self.windows[w].groups.push(group);
        self.windows[w].active_group = self.windows[w].groups.len() - 1;
        if self.groups.is_empty() {
            self.ensure_live_group();
        }
        if self.active_group >= self.groups.len() {
            self.active_group = self.groups.len() - 1;
        }
    }

    /// Move a aba (g, t) da janela ativa para a janela `w` (no grupo ativo dela).
    pub fn move_tab_to_window(&mut self, g: usize, t: usize, w: usize) {
        if g >= self.groups.len()
            || t >= self.groups[g].tabs.len()
            || w >= self.windows.len()
            || w == self.active_window
        {
            return;
        }
        let total: usize = self.groups.iter().map(|x| x.tabs.len()).sum();
        let tab = self.groups[g].tabs.remove(t);
        if self.groups[g].tabs.is_empty() {
            self.groups.remove(g);
        } else {
            self.groups[g].clamp_active();
        }
        if total <= 1 || self.groups.is_empty() {
            self.ensure_live_group();
        }
        if self.active_group >= self.groups.len() {
            self.active_group = self.groups.len() - 1;
        }
        let win = &mut self.windows[w];
        if win.groups.is_empty() {
            let gid = self.next_id;
            self.next_id += 1;
            win.groups.push(TabGroup::new(gid, "Principal", win.color));
        }
        let ag = win.active_group.min(win.groups.len() - 1);
        win.groups[ag].tabs.push(tab);
        win.groups[ag].active = win.groups[ag].tabs.len() - 1;
        win.active_group = ag;
    }

    fn ensure_live_group(&mut self) {
        if self.groups.is_empty() {
            let color = self.windows[self.active_window].color;
            let gid = self.alloc_id();
            self.groups_created += 1;
            let mut g = TabGroup::new(gid, "Principal", color);
            let tid = self.alloc_id();
            g.tabs.push(Tab::new(tid, home_dir()));
            self.groups.push(g);
            self.active_group = 0;
        }
    }

    // ------------------------------------------------------------------ fixados

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

fn user_config_dir() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("tabex")
}

/// Pasta do executável quando a instalação é portátil (ex.: `C:\tabex`).
fn portable_dir() -> Option<PathBuf> {
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    if std::env::var_os("TABEX_PORTABLE").is_some()
        || exe_dir.join("portable").exists()
        || exe_dir.join("portable.txt").exists()
    {
        return Some(exe_dir);
    }
    #[cfg(windows)]
    {
        let lower = exe_dir.to_string_lossy().to_lowercase();
        let dev_build = lower.contains("\\target\\debug") || lower.contains("\\target\\release");
        let program_files = lower.contains("\\program files");
        if !dev_build && !program_files && dir_is_writable(&exe_dir) {
            return Some(exe_dir);
        }
    }
    None
}

#[cfg(windows)]
fn dir_is_writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".tabex-write-test-{}", std::process::id()));
    match std::fs::File::create(&probe) {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
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
        s.move_tab(1, 0, None, None);
        assert_eq!(s.groups.len(), 3);
        assert_eq!(s.groups[2].tabs[0].path, PathBuf::from("/c"));
        let mut s = session_with(&[&["/a"], &["/b"]]);
        s.move_tab(0, 0, Some(1), Some(0));
        assert_eq!(s.groups.len(), 1);
        assert_eq!(s.groups[0].tabs[0].path, PathBuf::from("/a"));
        assert_eq!(s.groups[0].tabs[1].path, PathBuf::from("/b"));
    }

    #[test]
    fn reorder_within_group_by_drop() {
        let mut s = session_with(&[&["/a", "/b", "/c"]]);
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
    fn windows_switch_and_keep_groups() {
        let mut s = session_with(&[&["/a"], &["/b", "/c"]]);
        assert_eq!(s.windows.len(), 1);
        let w = s.new_window(Some("Trabalho".into()), None, PathBuf::from("/w"));
        assert_eq!(w, 1);
        assert_eq!(s.active_window, 1);
        assert_eq!(s.groups.len(), 1);
        assert_eq!(s.groups[0].tabs[0].path, PathBuf::from("/w"));
        assert_eq!(s.window_tab_count(0), 3);
        assert_eq!(s.window_tab_count(1), 1);
        s.switch_window(0);
        assert_eq!(s.groups.len(), 2);
        assert_eq!(s.groups[1].tabs[1].path, PathBuf::from("/c"));
        assert!(s.windows[0].groups.is_empty());
        assert_eq!(s.windows[1].groups.len(), 1);
        s.cycle_window(1);
        assert_eq!(s.active_window, 1);
        s.cycle_window(1);
        assert_eq!(s.active_window, 0);
    }

    #[test]
    fn move_group_and_tab_to_other_window() {
        let mut s = session_with(&[&["/a"], &["/b", "/c"]]);
        s.new_window(None, None, PathBuf::from("/w"));
        s.switch_window(0);
        s.move_group_to_window(1, 1);
        assert_eq!(s.groups.len(), 1);
        assert_eq!(s.windows[1].groups.len(), 2);
        assert_eq!(s.windows[1].tab_count(), 3);
        s.move_tab_to_window(0, 0, 1);
        // a janela ativa nunca fica vazia
        assert_eq!(s.groups.len(), 1);
        assert_eq!(s.groups[0].tabs.len(), 1);
        assert_eq!(s.windows[1].tab_count(), 4);
    }

    #[test]
    fn close_window_moves_focus() {
        let mut s = session_with(&[&["/a"]]);
        s.new_window(None, None, PathBuf::from("/w1"));
        s.new_window(None, None, PathBuf::from("/w2"));
        assert_eq!(s.active_window, 2);
        let tabs = s.close_window(2);
        assert_eq!(tabs.len(), 1);
        assert_eq!(s.windows.len(), 2);
        assert_eq!(s.active_window, 1);
        s.close_window(0);
        assert_eq!(s.windows.len(), 1);
        assert_eq!(s.active_window, 0);
        assert_eq!(s.groups[0].tabs[0].path, PathBuf::from("/w1"));
        // última janela: reinicia em vez de sumir
        let old = s.close_window(0);
        assert_eq!(old.len(), 1);
        assert_eq!(s.windows.len(), 1);
        assert_eq!(s.groups.len(), 1);
    }

    #[test]
    fn session_roundtrip_json_and_legacy() {
        let mut s = session_with(&[&["/a"], &["/b", "/c"]]);
        s.new_window(Some("Outra".into()), Some(GroupColor::Red), PathBuf::from("/w"));
        s.stash_active();
        let json = serde_json::to_string(&SessionFileRef {
            version: 2,
            windows: &s.windows,
            active_window: s.active_window,
            pinned: &s.pinned,
            show_hidden: false,
            dark_theme: false,
            sidebar_visible: true,
            next_id: s.next_id,
            groups_created: s.groups_created,
            windows_created: s.windows_created,
            use_native_menu: true,
            auto_update: true,
            last_update_check: 0,
        })
        .unwrap();
        s.unstash_active();
        let back = Session::from_file(serde_json::from_str(&json).unwrap());
        assert_eq!(back.windows.len(), 2);
        assert_eq!(back.active_window, 1);
        assert_eq!(back.groups.len(), 1);
        assert_eq!(back.windows[0].groups.len(), 2);
        assert_eq!(back.windows[1].name, "Outra");

        let legacy = r#"{"groups":[{"id":1,"name":"G","color":"Green","active":0,"collapsed":false,
            "tabs":[{"id":2,"path":"/x","history":["/x"],"history_idx":0,"sort_key":"Name","sort_asc":true,"group_by":"None"}]}],
            "active_group":0,"pinned":["/p"],"show_hidden":false,"dark_theme":true,"sidebar_visible":true,"next_id":3,"groups_created":1}"#;
        let old = Session::from_file(serde_json::from_str(legacy).unwrap());
        assert_eq!(old.windows.len(), 1);
        assert_eq!(old.groups.len(), 1);
        assert_eq!(old.groups[0].name, "G");
        assert!(old.dark_theme);
        assert_eq!(old.pinned, vec![PathBuf::from("/p")]);
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
