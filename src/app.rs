use crate::fs::entry::{list_drives, FileEntry};
use crate::fs::ops::{self, Clipboard, OpJob};
use crate::model::color::GroupColor;
use crate::model::session::Session;
use crate::model::tab::{GroupBy, SortKey, Tab};
use crate::shortcuts;
use crate::ui;
use crate::ui::dialogs::Dialog;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Tudo o que a interface pode pedir ao aplicativo. As telas enfileiram ações e o
/// `TabexApp` as aplica no fim do quadro, evitando conflitos de empréstimo.
#[derive(Debug, Clone)]
pub enum Action {
    Navigate(PathBuf),
    OpenInNewTab(PathBuf),
    OpenInNewGroup(PathBuf),
    Back,
    Forward,
    Up,
    Refresh,

    NewTab,
    CloseTab(usize, usize),
    CloseActiveTab,
    CloseOtherTabs(usize, usize),
    CloseTabsRight(usize, usize),
    DuplicateTab(usize, usize),
    ReopenClosedTab,
    ActivateTab(usize, usize),
    ActivateTabIndex(usize),
    CycleTab(i32),
    CycleGroup(i32),
    MoveTab(isize),
    /// Move a aba (g, t) para o grupo `target` (None = novo grupo) na posição `at`.
    MoveTabTo {
        g: usize,
        t: usize,
        target: Option<usize>,
        at: Option<usize>,
    },

    NewGroup,
    EditGroup(usize),
    EditActiveGroup,
    SetGroupColor(usize, GroupColor),
    ToggleCollapse(usize),
    MoveGroup(usize, isize),
    CloseGroup(usize),
    ActivateGroup(usize),

    Open(Vec<PathBuf>),
    OpenSelected,
    Copy,
    Cut,
    Paste,
    PasteInto(PathBuf),
    Delete {
        permanent: bool,
    },
    Rename,
    NewFolder,
    NewFile,
    SelectAll,
    InvertSelection,
    ClearSelection,
    CopyPath,
    Properties,
    PropertiesOfFolder,
    TogglePin(PathBuf),
    TogglePinCurrent,

    MoveCursor(isize, bool),
    CursorHome(bool),
    CursorEnd(bool),
    TypeAhead(String),

    SetSort(SortKey),
    SetSortAsc(bool),
    SetGroupBy(GroupBy),
    ToggleHidden,
    ToggleTheme,
    ToggleSidebar,
    FocusAddress,
    FocusSearch,
    ClearSearch,
    ClearFilters,
    Escape,

    ShowShortcuts,
    Toast(String),
}

pub struct TabexApp {
    pub session: Session,
    pub clipboard: Option<Clipboard>,
    pub dialog: Dialog,
    pub ops: Vec<OpJob>,
    pub address_edit: Option<String>,
    pub focus_address: bool,
    pub focus_search: bool,
    pub pending: Vec<Action>,
    pub closed_tabs: Vec<Tab>,
    pub toast: Option<(String, Instant)>,
    pub drives: Vec<(String, PathBuf)>,
    pub pending_copy_text: Option<String>,
    drives_at: Instant,
    dirty: bool,
    last_save: Instant,
    theme_applied: Option<bool>,
    last_title: String,
}

impl TabexApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let session = Session::load();
        ui::theme::apply(&cc.egui_ctx, session.dark_theme);
        TabexApp {
            theme_applied: Some(session.dark_theme),
            session,
            clipboard: None,
            dialog: Dialog::None,
            ops: Vec::new(),
            address_edit: None,
            focus_address: false,
            focus_search: false,
            pending: Vec::new(),
            closed_tabs: Vec::new(),
            toast: None,
            drives: list_drives(),
            pending_copy_text: None,
            drives_at: Instant::now(),
            dirty: false,
            last_save: Instant::now(),
            last_title: String::new(),
        }
    }

    pub fn toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
    }

    pub fn error(&mut self, msg: impl Into<String>) {
        self.dialog = Dialog::Error {
            title: "Tabex".to_owned(),
            message: msg.into(),
        };
    }

    /// Marca para recarregar todas as abas que mostram alguma das pastas.
    fn refresh_dirs(&mut self, dirs: &[PathBuf]) {
        for g in &mut self.session.groups {
            for t in &mut g.tabs {
                if dirs.contains(&t.path) {
                    t.needs_reload = true;
                }
            }
        }
    }

    fn poll_jobs(&mut self, ctx: &egui::Context) {
        // Operações de arquivo.
        let mut finished = Vec::new();
        self.ops.retain(|op| {
            if op.is_finished() {
                finished.push((
                    op.refresh.clone(),
                    op.progress.lock().ok().and_then(|p| p.error.clone()),
                ));
                false
            } else {
                true
            }
        });
        for (dirs, err) in finished {
            self.refresh_dirs(&dirs);
            if let Some(e) = err {
                self.error(e);
            }
        }
        if !self.ops.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }

        // Pesquisa, recarregamento e detecção de mudanças no disco.
        let show_hidden = self.session.show_hidden;
        let (ag, at) = self.session.active_pos();
        let mut busy = false;
        for (gi, g) in self.session.groups.iter_mut().enumerate() {
            for (ti, t) in g.tabs.iter_mut().enumerate() {
                let visible = gi == ag && ti == at;
                if t.poll_search() {
                    busy = true;
                }
                if visible {
                    t.check_fs_changed();
                    if t.needs_reload || t.loaded_hidden != show_hidden {
                        t.reload(show_hidden);
                    }
                }
            }
        }
        if busy {
            ctx.request_repaint_after(Duration::from_millis(120));
        } else if self.session.active_tab().search.is_none() {
            // Verificação periódica da pasta ativa.
            ctx.request_repaint_after(Duration::from_millis(1500));
        }

        if self.drives_at.elapsed() > Duration::from_secs(10) {
            self.drives = list_drives();
            self.drives_at = Instant::now();
        }
    }

    fn open_paths(&mut self, paths: Vec<PathBuf>) {
        let dirs: Vec<&PathBuf> = paths.iter().filter(|p| p.is_dir()).collect();
        if paths.len() == 1 && dirs.len() == 1 {
            self.session.active_tab_mut().navigate(paths[0].clone());
            self.dirty = true;
            return;
        }
        let (g, _) = self.session.active_pos();
        for p in paths.iter().take(10) {
            if p.is_dir() {
                self.session.open_tab(g, p.clone());
                self.dirty = true;
            } else if let Err(e) = open::that_detached(p) {
                self.error(format!("Não foi possível abrir \"{}\": {e}", p.display()));
            }
        }
    }

    fn apply(&mut self, a: Action) {
        use Action::*;
        match a {
            Navigate(p) => {
                self.address_edit = None;
                let p = expand_path(&p);
                if p.is_dir() {
                    self.session.active_tab_mut().navigate(p);
                    self.dirty = true;
                } else if p.is_file() {
                    if let Err(e) = open::that_detached(&p) {
                        self.error(e.to_string());
                    }
                } else {
                    self.error(format!(
                        "O Tabex não encontrou \"{}\". Verifique a ortografia e tente novamente.",
                        p.display()
                    ));
                }
            }
            OpenInNewTab(p) => {
                let (g, _) = self.session.active_pos();
                self.session.open_tab(g, p);
                self.dirty = true;
            }
            OpenInNewGroup(p) => {
                let g = self.session.new_group(None);
                self.session.open_tab(g, p);
                self.dirty = true;
            }
            Back => {
                self.session.active_tab_mut().back();
                self.dirty = true;
            }
            Forward => {
                self.session.active_tab_mut().forward();
                self.dirty = true;
            }
            Up => {
                self.session.active_tab_mut().up();
                self.dirty = true;
            }
            Refresh => {
                let t = self.session.active_tab_mut();
                t.needs_reload = true;
                if t.search.is_some() {
                    let hidden = self.session.show_hidden;
                    self.session.active_tab_mut().start_search(hidden);
                }
            }

            NewTab => {
                let (g, _) = self.session.active_pos();
                let p = self.session.active_tab().path.clone();
                self.session.open_tab(g, p);
                self.dirty = true;
            }
            CloseTab(g, t) => {
                if let Some(tab) = self.session.close_tab(g, t) {
                    self.closed_tabs.push(tab);
                    self.closed_tabs.truncate(20);
                }
                self.dirty = true;
            }
            CloseActiveTab => {
                let (g, t) = self.session.active_pos();
                self.apply(CloseTab(g, t));
            }
            CloseOtherTabs(g, t) => {
                let n = self.session.groups[g].tabs.len();
                for i in (0..n).rev() {
                    if i != t {
                        if let Some(tab) = self.session.close_tab(g, i) {
                            self.closed_tabs.push(tab);
                        }
                    }
                }
                self.session.activate(g, 0);
                self.dirty = true;
            }
            CloseTabsRight(g, t) => {
                let n = self.session.groups[g].tabs.len();
                for i in (t + 1..n).rev() {
                    if let Some(tab) = self.session.close_tab(g, i) {
                        self.closed_tabs.push(tab);
                    }
                }
                self.session.activate(g, t);
                self.dirty = true;
            }
            DuplicateTab(g, t) => {
                let id = self.session.alloc_id();
                let dup = self.session.groups[g].tabs[t].duplicate(id);
                self.session.groups[g].tabs.insert(t + 1, dup);
                self.session.activate(g, t + 1);
                self.dirty = true;
            }
            ReopenClosedTab => {
                if let Some(mut tab) = self.closed_tabs.pop() {
                    tab.needs_reload = true;
                    let (g, t) = self.session.active_pos();
                    let pos = t + 1;
                    self.session.groups[g].tabs.insert(pos, tab);
                    self.session.activate(g, pos);
                    self.dirty = true;
                }
            }
            ActivateTab(g, t) => {
                self.session.activate(g, t);
                self.dirty = true;
            }
            ActivateTabIndex(i) => {
                let g = self.session.active_group;
                let n = self.session.groups[g].tabs.len();
                let t = if i == usize::MAX { n - 1 } else { i.min(n - 1) };
                self.session.activate(g, t);
            }
            CycleTab(d) => {
                self.session.cycle_tab(d);
                self.dirty = true;
            }
            CycleGroup(d) => {
                self.session.cycle_group(d);
                self.dirty = true;
            }
            MoveTab(d) => {
                self.session.move_tab_within_group(d);
                self.dirty = true;
            }
            MoveTabTo { g, t, target, at } => {
                self.session.move_tab(g, t, target, at);
                self.dirty = true;
            }

            NewGroup => {
                let n = self.session.groups_created + 1;
                self.dialog = Dialog::Group {
                    index: None,
                    name: format!("Grupo {n}"),
                    color: GroupColor::nth(n.saturating_sub(1)),
                    first: true,
                };
            }
            EditGroup(g) => {
                if let Some(group) = self.session.groups.get(g) {
                    self.dialog = Dialog::Group {
                        index: Some(g),
                        name: group.name.clone(),
                        color: group.color,
                        first: true,
                    };
                }
            }
            EditActiveGroup => {
                let g = self.session.active_group;
                self.apply(EditGroup(g));
            }
            SetGroupColor(g, c) => {
                if let Some(group) = self.session.groups.get_mut(g) {
                    group.color = c;
                    self.dirty = true;
                }
            }
            ToggleCollapse(g) => {
                if let Some(group) = self.session.groups.get_mut(g) {
                    group.collapsed = !group.collapsed;
                    self.dirty = true;
                }
            }
            MoveGroup(g, d) => {
                self.session.move_group(g, d);
                self.dirty = true;
            }
            CloseGroup(g) => {
                if g < self.session.groups.len() {
                    let n = self.session.groups[g].tabs.len();
                    for i in (0..n).rev() {
                        if let Some(tab) = self.session.close_tab(g, i) {
                            self.closed_tabs.push(tab);
                        }
                        // close_tab remove o grupo quando esvazia; se ele sumiu, paramos.
                        if g >= self.session.groups.len() || self.session.groups[g].tabs.len() < i.max(1) {
                            break;
                        }
                    }
                    self.closed_tabs.truncate(20);
                    self.dirty = true;
                }
            }
            ActivateGroup(g) => {
                if g < self.session.groups.len() {
                    self.session.active_group = g;
                    self.session.groups[g].collapsed = false;
                    self.dirty = true;
                }
            }

            Open(paths) => self.open_paths(paths),
            OpenSelected => {
                let paths = self.session.active_tab().selected_paths();
                if !paths.is_empty() {
                    self.open_paths(paths);
                }
            }
            Copy | Cut => {
                let paths = self.session.active_tab().selected_paths();
                if !paths.is_empty() {
                    let n = paths.len();
                    let cut = matches!(a, Cut);
                    self.clipboard = Some(Clipboard { paths, cut });
                    self.toast(format!(
                        "{n} {} {}",
                        plural(n, "item", "itens"),
                        if cut { "recortado(s)" } else { "copiado(s)" }
                    ));
                }
            }
            Paste => {
                let dest = self.session.active_tab().path.clone();
                self.apply(PasteInto(dest));
            }
            PasteInto(dest) => {
                if let Some(clip) = self.clipboard.clone() {
                    let job = ops::start_paste(&clip, &dest);
                    self.ops.push(job);
                    if clip.cut {
                        self.clipboard = None;
                    }
                }
            }
            Delete { permanent } => {
                let paths = self.session.active_tab().selected_paths();
                if !paths.is_empty() {
                    self.dialog = Dialog::ConfirmDelete { paths, permanent };
                }
            }
            Rename => {
                let sel = self.session.active_tab().selected_entries();
                if let Some(e) = sel.first() {
                    self.dialog = Dialog::Rename {
                        path: e.path.clone(),
                        name: e.name.clone(),
                        is_dir: e.is_dir,
                        first: true,
                    };
                }
            }
            NewFolder => {
                let dir = self.session.active_tab().path.clone();
                self.dialog = Dialog::NewFolder {
                    dir,
                    name: "Nova pasta".to_owned(),
                    first: true,
                };
            }
            NewFile => {
                let dir = self.session.active_tab().path.clone();
                self.dialog = Dialog::NewFile {
                    dir,
                    name: "Novo Documento de Texto.txt".to_owned(),
                    first: true,
                };
            }
            SelectAll => self.session.active_tab_mut().select_all(),
            InvertSelection => self.session.active_tab_mut().invert_selection(),
            ClearSelection => self.session.active_tab_mut().clear_selection(),
            CopyPath => {
                let mut paths = self.session.active_tab().selected_paths();
                if paths.is_empty() {
                    paths.push(self.session.active_tab().path.clone());
                }
                let text = paths
                    .iter()
                    .map(|p| p.to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("\n");
                self.pending_copy_text = Some(text);
                self.toast("Caminho copiado");
            }
            Properties => {
                let sel = self.session.active_tab().selected_entries();
                if sel.is_empty() {
                    self.apply(PropertiesOfFolder);
                } else {
                    self.show_properties(sel);
                }
            }
            PropertiesOfFolder => {
                let p = self.session.active_tab().path.clone();
                if let Some(e) = FileEntry::from_path(&p) {
                    self.show_properties(vec![e]);
                }
            }
            TogglePinCurrent => {
                let p = self.session.active_tab().path.clone();
                self.apply(TogglePin(p));
            }
            TogglePin(p) => {
                let pinned = self.session.is_pinned(&p);
                self.session.toggle_pin(p);
                self.toast(if pinned {
                    "Pasta desafixada do Acesso rápido"
                } else {
                    "Pasta fixada no Acesso rápido"
                });
                self.dirty = true;
            }

            MoveCursor(d, ext) => self.session.active_tab_mut().move_cursor(d, ext),
            CursorHome(ext) => self.session.active_tab_mut().move_cursor_home(ext),
            CursorEnd(ext) => self.session.active_tab_mut().move_cursor_end(ext),
            TypeAhead(s) => self.session.active_tab_mut().type_ahead(&s),

            SetSort(k) => {
                self.session.active_tab_mut().set_sort(k);
                self.dirty = true;
            }
            SetSortAsc(asc) => {
                let t = self.session.active_tab_mut();
                t.sort_asc = asc;
                t.rebuild_rows();
                self.dirty = true;
            }
            SetGroupBy(g) => {
                let t = self.session.active_tab_mut();
                t.group_by = g;
                t.rebuild_rows();
                self.dirty = true;
            }
            ToggleHidden => {
                self.session.show_hidden = !self.session.show_hidden;
                self.dirty = true;
            }
            ToggleTheme => {
                self.session.dark_theme = !self.session.dark_theme;
                self.dirty = true;
            }
            ToggleSidebar => {
                self.session.sidebar_visible = !self.session.sidebar_visible;
                self.dirty = true;
            }
            FocusAddress => {
                let p = self.session.active_tab().path.to_string_lossy().into_owned();
                self.address_edit = Some(p);
                self.focus_address = true;
            }
            FocusSearch => self.focus_search = true,
            ClearSearch => self.session.active_tab_mut().clear_search(),
            ClearFilters => {
                let t = self.session.active_tab_mut();
                t.quick_filter.clear();
                t.rebuild_rows();
            }
            Escape => {
                let t = self.session.active_tab_mut();
                if t.search.is_some() || !t.search_text.is_empty() {
                    t.clear_search();
                } else if t.quick_filter.is_active() {
                    t.quick_filter.clear();
                    t.rebuild_rows();
                } else {
                    t.clear_selection();
                }
            }

            ShowShortcuts => self.dialog = Dialog::Shortcuts,
            Toast(msg) => self.toast(msg),
        }
    }

    fn show_properties(&mut self, entries: Vec<FileEntry>) {
        let paths: Vec<PathBuf> = entries.iter().map(|e| e.path.clone()).collect();
        let info = ops::compute_properties(paths.clone());
        let entry = if entries.len() == 1 {
            entries.into_iter().next()
        } else {
            None
        };
        self.dialog = Dialog::Properties { paths, entry, info };
    }

    fn update_title(&mut self, ctx: &egui::Context) {
        let title = format!("{} - Tabex", self.session.active_tab().title());
        if title != self.last_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }
    }
}

/// Expande `~`, `%VAR%` (Windows) e `$VAR` no início de um caminho digitado.
pub fn expand_path(p: &std::path::Path) -> PathBuf {
    let s = p.to_string_lossy().trim().to_owned();
    let s = s.trim_matches('"').to_owned();
    if s.is_empty() {
        return p.to_path_buf();
    }
    if let Some(rest) = s.strip_prefix('~') {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest.trim_start_matches(['/', '\\']));
        }
    }
    if let Some(rest) = s.strip_prefix('%') {
        if let Some(end) = rest.find('%') {
            if let Ok(val) = std::env::var(&rest[..end]) {
                return PathBuf::from(format!("{}{}", val, &rest[end + 1..]));
            }
        }
    }
    if let Some(rest) = s.strip_prefix('$') {
        let end = rest.find(['/', '\\']).unwrap_or(rest.len());
        if let Ok(val) = std::env::var(&rest[..end]) {
            return PathBuf::from(format!("{}{}", val, &rest[end..]));
        }
    }
    PathBuf::from(s)
}

pub fn plural<'a>(n: usize, one: &'a str, many: &'a str) -> &'a str {
    if n == 1 {
        one
    } else {
        many
    }
}

impl eframe::App for TabexApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.theme_applied != Some(self.session.dark_theme) {
            ui::theme::apply(ctx, self.session.dark_theme);
            self.theme_applied = Some(self.session.dark_theme);
        }

        self.poll_jobs(ctx);

        if matches!(self.dialog, Dialog::None) {
            let acts = shortcuts::collect(ctx, self.address_edit.is_some());
            self.pending.extend(acts);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let visuals = ctx.global_style().visuals.clone();

        egui::Panel::top("tabstrip")
            .exact_size(36.0)
            .frame(egui::Frame::new().fill(visuals.panel_fill).inner_margin(egui::Margin {
                left: 4,
                right: 4,
                top: 4,
                bottom: 0,
            }))
            .show(ui, |ui| ui::tabstrip::show(ui, self));

        egui::Panel::top("toolbar")
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(visuals.window_fill)
                    .inner_margin(egui::Margin::symmetric(8, 6)),
            )
            .show(ui, |ui| ui::toolbar::show(ui, self));

        egui::Panel::bottom("status")
            .exact_size(26.0)
            .frame(
                egui::Frame::new()
                    .fill(visuals.panel_fill)
                    .inner_margin(egui::Margin::symmetric(10, 4)),
            )
            .show(ui, |ui| ui::statusbar::show(ui, self));

        if self.session.sidebar_visible {
            egui::Panel::left("sidebar")
                .resizable(true)
                .default_size(230.0)
                .size_range(160.0..=420.0)
                .frame(
                    egui::Frame::new()
                        .fill(visuals.panel_fill)
                        .inner_margin(egui::Margin::symmetric(6, 6)),
                )
                .show(ui, |ui| ui::sidebar::show(ui, self));
        }

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(visuals.extreme_bg_color)
                    .inner_margin(egui::Margin::same(0)),
            )
            .show(ui, |ui| ui::filelist::show(ui, self));

        ui::dialogs::show(&ctx, self);

        // Aplica ações enfileiradas (ações podem gerar novas ações).
        let mut guard = 0;
        while !self.pending.is_empty() && guard < 50 {
            let batch = std::mem::take(&mut self.pending);
            for a in batch {
                self.apply(a);
            }
            guard += 1;
        }
        if let Some(text) = self.pending_copy_text.take() {
            ctx.copy_text(text);
        }

        self.update_title(&ctx);

        if self.dirty && self.last_save.elapsed() > Duration::from_secs(2) {
            if let Err(e) = self.session.save() {
                eprintln!("falha ao salvar sessão: {e}");
            }
            self.dirty = false;
            self.last_save = Instant::now();
        }
        if let Some((_, at)) = self.toast {
            if at.elapsed() > Duration::from_secs(4) {
                self.toast = None;
            } else {
                ctx.request_repaint_after(Duration::from_millis(500));
            }
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Err(e) = self.session.save() {
            eprintln!("falha ao salvar sessão: {e}");
        }
    }
}
