use crate::fs::entry::{natural_cmp, read_dir, FileEntry};
use crate::fs::filter::{DateBucket, Query, QuickFilter};
use crate::fs::search::SearchJob;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SortKey {
    #[default]
    Name,
    Modified,
    Kind,
    Size,
}

impl SortKey {
    pub fn label(self) -> &'static str {
        match self {
            SortKey::Name => "Nome",
            SortKey::Modified => "Data de modificação",
            SortKey::Kind => "Tipo",
            SortKey::Size => "Tamanho",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum GroupBy {
    #[default]
    None,
    Date,
    Kind,
}

impl GroupBy {
    pub fn label(self) -> &'static str {
        match self {
            GroupBy::None => "Nenhum",
            GroupBy::Date => "Data de modificação",
            GroupBy::Kind => "Tipo",
        }
    }
}

/// Linha visível na lista: cabeçalho de grupo ou um item (índice em `Tab::source()`).
#[derive(Clone, Debug)]
pub enum Row {
    Header { label: String, count: usize },
    Entry(usize),
}

#[derive(Serialize, Deserialize)]
pub struct Tab {
    pub id: u64,
    pub path: PathBuf,
    pub history: Vec<PathBuf>,
    pub history_idx: usize,
    pub sort_key: SortKey,
    pub sort_asc: bool,
    pub group_by: GroupBy,

    // ---- estado de execução (não persistido) ----
    #[serde(skip)]
    pub search_text: String,
    #[serde(skip)]
    pub quick_filter: QuickFilter,
    #[serde(skip)]
    pub listing: Vec<FileEntry>,
    #[serde(skip)]
    pub search: Option<SearchJob>,
    #[serde(skip)]
    pub rows: Vec<Row>,
    #[serde(skip)]
    pub selected: HashSet<PathBuf>,
    #[serde(skip)]
    pub cursor: Option<PathBuf>,
    #[serde(skip)]
    pub anchor: Option<PathBuf>,
    #[serde(skip, default = "default_true")]
    pub needs_reload: bool,
    #[serde(skip)]
    pub error: Option<String>,
    #[serde(skip)]
    pub scroll_to_cursor: bool,
    #[serde(skip)]
    pub dir_mtime: Option<SystemTime>,
    #[serde(skip)]
    pub last_check: Option<Instant>,
    #[serde(skip)]
    pub type_ahead: String,
    #[serde(skip)]
    pub type_ahead_at: Option<Instant>,
    #[serde(skip)]
    pub loaded_hidden: bool,
}

fn default_true() -> bool {
    true
}

impl Tab {
    pub fn new(id: u64, path: PathBuf) -> Tab {
        Tab {
            id,
            history: vec![path.clone()],
            history_idx: 0,
            path,
            sort_key: SortKey::Name,
            sort_asc: true,
            group_by: GroupBy::None,
            search_text: String::new(),
            quick_filter: QuickFilter::default(),
            listing: Vec::new(),
            search: None,
            rows: Vec::new(),
            selected: HashSet::new(),
            cursor: None,
            anchor: None,
            needs_reload: true,
            error: None,
            scroll_to_cursor: false,
            dir_mtime: None,
            last_check: None,
            type_ahead: String::new(),
            type_ahead_at: None,
            loaded_hidden: false,
        }
    }

    /// Cópia navegável (mesmo caminho, histórico e ordenação) com novo id.
    pub fn duplicate(&self, id: u64) -> Tab {
        let mut t = Tab::new(id, self.path.clone());
        t.history = self.history.clone();
        t.history_idx = self.history_idx;
        t.sort_key = self.sort_key;
        t.sort_asc = self.sort_asc;
        t.group_by = self.group_by;
        t
    }

    pub fn title(&self) -> String {
        let name = self
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.to_string_lossy().into_owned());
        if self.search.is_some() {
            format!("🔍 {name}")
        } else {
            name
        }
    }

    pub fn is_searching(&self) -> bool {
        self.search.is_some()
    }

    /// Itens de origem (resultado da pesquisa recursiva ou listagem da pasta).
    pub fn source(&self) -> &[FileEntry] {
        match &self.search {
            Some(s) => &s.hits,
            None => &self.listing,
        }
    }

    pub fn entry(&self, idx: usize) -> Option<&FileEntry> {
        self.source().get(idx)
    }

    // ------------------------------------------------------------------ navegação

    pub fn navigate(&mut self, path: PathBuf) {
        if path == self.path && self.search.is_none() {
            self.needs_reload = true;
            return;
        }
        self.history.truncate(self.history_idx + 1);
        self.history.push(path.clone());
        self.history_idx = self.history.len() - 1;
        self.set_path(path);
    }

    fn set_path(&mut self, path: PathBuf) {
        self.path = path;
        self.search = None;
        self.search_text.clear();
        self.selected.clear();
        self.cursor = None;
        self.anchor = None;
        self.error = None;
        self.needs_reload = true;
        self.dir_mtime = None;
    }

    pub fn can_back(&self) -> bool {
        self.history_idx > 0
    }

    pub fn can_forward(&self) -> bool {
        self.history_idx + 1 < self.history.len()
    }

    pub fn back(&mut self) -> bool {
        if self.can_back() {
            self.history_idx -= 1;
            let p = self.history[self.history_idx].clone();
            self.set_path(p);
            true
        } else {
            false
        }
    }

    pub fn forward(&mut self) -> bool {
        if self.can_forward() {
            self.history_idx += 1;
            let p = self.history[self.history_idx].clone();
            self.set_path(p);
            true
        } else {
            false
        }
    }

    pub fn up(&mut self) -> bool {
        let child = self.path.clone();
        match child.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => {
                self.navigate(parent.to_path_buf());
                // Ao subir, seleciona a pasta de onde viemos (como o Explorador).
                self.selected.insert(child.clone());
                self.cursor = Some(child);
                self.scroll_to_cursor = true;
                true
            }
            _ => false,
        }
    }

    // ------------------------------------------------------------------ carregamento

    pub fn reload(&mut self, show_hidden: bool) {
        self.needs_reload = false;
        self.loaded_hidden = show_hidden;
        match read_dir(&self.path, show_hidden) {
            Ok(list) => {
                self.listing = list;
                self.error = None;
            }
            Err(e) => {
                self.listing.clear();
                self.error = Some(match e.kind() {
                    std::io::ErrorKind::NotFound => "Esta pasta não existe mais.".to_owned(),
                    std::io::ErrorKind::PermissionDenied => {
                        "Você não tem permissão para acessar esta pasta.".to_owned()
                    }
                    _ => format!("Não foi possível abrir a pasta: {e}"),
                });
            }
        }
        self.dir_mtime = std::fs::metadata(&self.path).and_then(|m| m.modified()).ok();
        self.last_check = Some(Instant::now());
        // Remove da seleção o que deixou de existir.
        let existing: HashSet<&Path> = self.listing.iter().map(|e| e.path.as_path()).collect();
        if self.search.is_none() {
            self.selected.retain(|p| existing.contains(p.as_path()));
            if let Some(c) = &self.cursor {
                if !existing.contains(c.as_path()) {
                    self.cursor = None;
                }
            }
        }
        self.rebuild_rows();
    }

    /// Verifica periodicamente se a pasta mudou no disco.
    pub fn check_fs_changed(&mut self) -> bool {
        if self.search.is_some() {
            return false;
        }
        let due = self
            .last_check
            .map(|t| t.elapsed() > Duration::from_millis(1500))
            .unwrap_or(true);
        if !due {
            return false;
        }
        self.last_check = Some(Instant::now());
        let now_mtime = std::fs::metadata(&self.path).and_then(|m| m.modified()).ok();
        if now_mtime != self.dir_mtime {
            self.needs_reload = true;
            return true;
        }
        false
    }

    pub fn start_search(&mut self, show_hidden: bool) {
        let text = self.search_text.trim().to_owned();
        if text.is_empty() {
            self.clear_search();
            return;
        }
        self.search = Some(SearchJob::start(self.path.clone(), text, show_hidden));
        self.selected.clear();
        self.cursor = None;
        self.rebuild_rows();
    }

    pub fn clear_search(&mut self) {
        self.search = None;
        self.search_text.clear();
        self.selected.clear();
        self.cursor = None;
        self.rebuild_rows();
    }

    /// Recebe resultados da pesquisa em andamento. Retorna `true` se há trabalho em curso.
    pub fn poll_search(&mut self) -> bool {
        let mut changed = false;
        let mut running = false;
        if let Some(s) = &mut self.search {
            changed = s.poll();
            running = !s.done;
        }
        if changed {
            self.rebuild_rows();
        }
        running
    }

    // ------------------------------------------------------------------ linhas visíveis

    pub fn rebuild_rows(&mut self) {
        let now = chrono::Local::now();
        let searching = self.search.is_some();
        let query = if searching {
            Query::default()
        } else {
            Query::parse(&self.search_text)
        };
        let source = self.source();
        let mut idxs: Vec<usize> = source
            .iter()
            .enumerate()
            .filter(|(_, e)| self.quick_filter.matches(e, &now))
            .filter(|(_, e)| query.is_empty() || query.matches(e, &now))
            .map(|(i, _)| i)
            .collect();

        let key = self.sort_key;
        let asc = self.sort_asc;
        let cmp = |a: &FileEntry, b: &FileEntry| -> Ordering {
            // Pastas sempre primeiro (como o Explorador).
            let dir_ord = b.is_dir.cmp(&a.is_dir);
            if dir_ord != Ordering::Equal {
                return dir_ord;
            }
            let primary = match key {
                SortKey::Name => natural_cmp(&a.name_lower, &b.name_lower),
                SortKey::Modified => a.modified.cmp(&b.modified),
                SortKey::Kind => a.type_label.cmp(&b.type_label),
                SortKey::Size => a.size.cmp(&b.size),
            };
            let primary = if asc { primary } else { primary.reverse() };
            primary.then_with(|| natural_cmp(&a.name_lower, &b.name_lower))
        };
        idxs.sort_by(|&a, &b| cmp(&source[a], &source[b]));

        let mut rows = Vec::with_capacity(idxs.len() + 8);
        match self.group_by {
            GroupBy::None => rows.extend(idxs.into_iter().map(Row::Entry)),
            GroupBy::Date => {
                let mut buckets: Vec<(DateBucket, Vec<usize>)> = Vec::new();
                for i in idxs {
                    let b = DateBucket::of(source[i].modified, &now);
                    match buckets.iter_mut().find(|(k, _)| *k == b) {
                        Some((_, v)) => v.push(i),
                        None => buckets.push((b, vec![i])),
                    }
                }
                buckets.sort_by_key(|(k, _)| *k);
                if key == SortKey::Modified && asc {
                    buckets.reverse();
                }
                for (b, v) in buckets {
                    rows.push(Row::Header {
                        label: b.label().to_owned(),
                        count: v.len(),
                    });
                    rows.extend(v.into_iter().map(Row::Entry));
                }
            }
            GroupBy::Kind => {
                let mut groups: Vec<(String, Vec<usize>)> = Vec::new();
                for i in idxs {
                    let label = source[i].type_label.clone();
                    match groups.iter_mut().find(|(k, _)| *k == label) {
                        Some((_, v)) => v.push(i),
                        None => groups.push((label, vec![i])),
                    }
                }
                // Pastas primeiro, depois por nome do tipo.
                groups.sort_by(|a, b| {
                    let fa = a.0 == "Pasta de arquivos";
                    let fb = b.0 == "Pasta de arquivos";
                    fb.cmp(&fa).then_with(|| a.0.cmp(&b.0))
                });
                for (label, v) in groups {
                    rows.push(Row::Header { label, count: v.len() });
                    rows.extend(v.into_iter().map(Row::Entry));
                }
            }
        }
        self.rows = rows;
    }

    pub fn visible_count(&self) -> usize {
        self.rows.iter().filter(|r| matches!(r, Row::Entry(_))).count()
    }

    pub fn set_sort(&mut self, key: SortKey) {
        if self.sort_key == key {
            self.sort_asc = !self.sort_asc;
        } else {
            self.sort_key = key;
            self.sort_asc = !matches!(key, SortKey::Modified | SortKey::Size);
        }
        self.rebuild_rows();
    }

    // ------------------------------------------------------------------ seleção

    pub fn selected_entries(&self) -> Vec<FileEntry> {
        self.source()
            .iter()
            .filter(|e| self.selected.contains(&e.path))
            .cloned()
            .collect()
    }

    pub fn selected_paths(&self) -> Vec<PathBuf> {
        self.selected_entries().into_iter().map(|e| e.path).collect()
    }

    pub fn selected_size(&self) -> u64 {
        self.selected_entries().iter().map(|e| e.size).sum()
    }

    pub fn clear_selection(&mut self) {
        self.selected.clear();
        self.anchor = None;
    }

    pub fn select_only(&mut self, idx: usize) {
        if let Some(e) = self.entry(idx) {
            let p = e.path.clone();
            self.selected.clear();
            self.selected.insert(p.clone());
            self.cursor = Some(p.clone());
            self.anchor = Some(p);
        }
    }

    pub fn toggle(&mut self, idx: usize) {
        if let Some(e) = self.entry(idx) {
            let p = e.path.clone();
            if !self.selected.remove(&p) {
                self.selected.insert(p.clone());
            }
            self.cursor = Some(p.clone());
            self.anchor = Some(p);
        }
    }

    /// Seleciona do item-âncora até `idx`, seguindo a ordem visível.
    pub fn select_range(&mut self, idx: usize) {
        let Some(target_row) = self.row_of_idx(idx) else { return };
        let anchor_row = self
            .anchor
            .as_ref()
            .and_then(|a| self.row_of_path(a))
            .unwrap_or(target_row);
        let (lo, hi) = if anchor_row <= target_row {
            (anchor_row, target_row)
        } else {
            (target_row, anchor_row)
        };
        self.selected.clear();
        for r in lo..=hi {
            if let Row::Entry(i) = self.rows[r] {
                if let Some(e) = self.source().get(i) {
                    self.selected.insert(e.path.clone());
                }
            }
        }
        if let Some(e) = self.entry(idx) {
            self.cursor = Some(e.path.clone());
        }
    }

    pub fn select_all(&mut self) {
        self.selected = self
            .rows
            .iter()
            .filter_map(|r| match r {
                Row::Entry(i) => self.source().get(*i).map(|e| e.path.clone()),
                _ => None,
            })
            .collect();
    }

    pub fn invert_selection(&mut self) {
        let all: Vec<PathBuf> = self
            .rows
            .iter()
            .filter_map(|r| match r {
                Row::Entry(i) => self.source().get(*i).map(|e| e.path.clone()),
                _ => None,
            })
            .collect();
        for p in all {
            if !self.selected.remove(&p) {
                self.selected.insert(p);
            }
        }
    }

    pub fn row_of_idx(&self, idx: usize) -> Option<usize> {
        self.rows.iter().position(|r| matches!(r, Row::Entry(i) if *i == idx))
    }

    pub fn row_of_path(&self, path: &Path) -> Option<usize> {
        self.rows.iter().position(|r| match r {
            Row::Entry(i) => self.source().get(*i).map(|e| e.path == path).unwrap_or(false),
            _ => false,
        })
    }

    pub fn cursor_row(&self) -> Option<usize> {
        self.cursor.as_ref().and_then(|c| self.row_of_path(c))
    }

    fn entry_rows(&self) -> Vec<usize> {
        self.rows
            .iter()
            .enumerate()
            .filter_map(|(r, row)| matches!(row, Row::Entry(_)).then_some(r))
            .collect()
    }

    /// Move o cursor `delta` itens (pulando cabeçalhos). `extend` mantém a seleção (Shift).
    pub fn move_cursor(&mut self, delta: isize, extend: bool) {
        let entry_rows = self.entry_rows();
        if entry_rows.is_empty() {
            return;
        }
        let cur = self.cursor_row();
        let pos = match cur {
            Some(r) => entry_rows.iter().position(|&x| x == r).map(|p| p as isize).unwrap_or(0),
            None => {
                if delta >= 0 {
                    -1
                } else {
                    entry_rows.len() as isize
                }
            }
        };
        let new_pos = (pos + delta).clamp(0, entry_rows.len() as isize - 1) as usize;
        self.focus_row(entry_rows[new_pos], extend);
    }

    pub fn move_cursor_home(&mut self, extend: bool) {
        if let Some(&r) = self.entry_rows().first() {
            self.focus_row(r, extend);
        }
    }

    pub fn move_cursor_end(&mut self, extend: bool) {
        if let Some(&r) = self.entry_rows().last() {
            self.focus_row(r, extend);
        }
    }

    fn focus_row(&mut self, row: usize, extend: bool) {
        if let Row::Entry(i) = self.rows[row] {
            if extend {
                self.select_range(i);
            } else {
                self.select_only(i);
            }
            self.scroll_to_cursor = true;
        }
    }

    /// Digitação rápida: pula para o primeiro item cujo nome começa com o texto digitado.
    pub fn type_ahead(&mut self, text: &str) {
        let now = Instant::now();
        let fresh = self
            .type_ahead_at
            .map(|t| now.duration_since(t) < Duration::from_millis(900))
            .unwrap_or(false);
        if !fresh {
            self.type_ahead.clear();
        }
        self.type_ahead.push_str(&text.to_lowercase());
        self.type_ahead_at = Some(now);
        let needle = self.type_ahead.clone();
        let entry_rows = self.entry_rows();
        // Começa a procurar depois do cursor quando a mesma letra é repetida.
        let start = self
            .cursor_row()
            .and_then(|c| entry_rows.iter().position(|&r| r == c))
            .map(|p| p + 1)
            .unwrap_or(0);
        let order = entry_rows
            .iter()
            .cycle()
            .skip(if needle.chars().count() == 1 { start } else { 0 })
            .take(entry_rows.len());
        for &r in order {
            if let Row::Entry(i) = self.rows[r] {
                if self.source()[i].name_lower.starts_with(&needle) {
                    self.focus_row(r, false);
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tabex_tab_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("pasta_b")).unwrap();
        std::fs::create_dir_all(dir.join("pasta_a")).unwrap();
        std::fs::write(dir.join("doc10.txt"), b"0123456789").unwrap();
        std::fs::write(dir.join("doc2.txt"), b"01").unwrap();
        std::fs::write(dir.join("foto.jpg"), b"").unwrap();
        dir
    }

    fn names(tab: &Tab) -> Vec<String> {
        tab.rows
            .iter()
            .filter_map(|r| match r {
                Row::Entry(i) => Some(tab.source()[*i].name.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn listing_sorts_folders_first_and_naturally() {
        let dir = tmp_dir("sort");
        let mut t = Tab::new(1, dir.clone());
        t.reload(false);
        assert_eq!(
            names(&t),
            vec!["pasta_a", "pasta_b", "doc2.txt", "doc10.txt", "foto.jpg"]
        );
        t.set_sort(SortKey::Name); // inverte
        assert!(!t.sort_asc);
        assert_eq!(
            names(&t),
            vec!["pasta_b", "pasta_a", "foto.jpg", "doc10.txt", "doc2.txt"]
        );
        t.set_sort(SortKey::Size);
        assert!(!t.sort_asc); // tamanho começa decrescente
        assert_eq!(names(&t)[2], "doc10.txt");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn instant_filter_and_selection() {
        let dir = tmp_dir("filter");
        let mut t = Tab::new(1, dir.clone());
        t.reload(false);
        t.search_text = "doc".into();
        t.rebuild_rows();
        assert_eq!(names(&t), vec!["doc2.txt", "doc10.txt"]);
        t.search_text = "ext:jpg".into();
        t.rebuild_rows();
        assert_eq!(names(&t), vec!["foto.jpg"]);
        t.search_text.clear();
        t.rebuild_rows();

        t.move_cursor(1, false);
        assert_eq!(t.cursor.as_ref().unwrap().file_name().unwrap(), "pasta_a");
        t.move_cursor(2, true);
        assert_eq!(t.selected.len(), 3);
        t.select_all();
        assert_eq!(t.selected.len(), 5);
        t.type_ahead("f");
        assert_eq!(t.cursor.as_ref().unwrap().file_name().unwrap(), "foto.jpg");
        assert_eq!(t.selected.len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn grouping_by_kind_adds_headers() {
        let dir = tmp_dir("group");
        let mut t = Tab::new(1, dir.clone());
        t.group_by = GroupBy::Kind;
        t.reload(false);
        let headers: Vec<String> = t
            .rows
            .iter()
            .filter_map(|r| match r {
                Row::Header { label, .. } => Some(label.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(headers[0], "Pasta de arquivos");
        assert!(headers.contains(&"Documento de Texto".to_owned()));
        assert_eq!(t.visible_count(), 5);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn history_navigation() {
        let dir = tmp_dir("hist");
        let mut t = Tab::new(1, dir.clone());
        t.navigate(dir.join("pasta_a"));
        assert!(t.can_back());
        assert!(t.up());
        assert_eq!(t.path, dir);
        assert!(t.selected.contains(&dir.join("pasta_a")));
        assert!(t.back());
        assert_eq!(t.path, dir.join("pasta_a"));
        assert!(t.forward());
        assert_eq!(t.path, dir);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
