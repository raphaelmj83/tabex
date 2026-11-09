use crate::fs::cloud::{self, CloudFolder};
use crate::fs::entry::{list_drives, FileEntry};
use crate::fs::ops::{self, Clipboard, ConflictPolicy, OpJob, QueuedOp, Stack, UndoEntry};
use crate::fs::sysclip;
use crate::fs::thumbs::ThumbService;
use crate::model::color::GroupColor;
use crate::model::session::Session;
use crate::model::tab::{GroupBy, SortKey, Tab};
use crate::platform::{self, MenuExtra};
use crate::shortcuts;
use crate::ui;
use crate::ui::dialogs::Dialog;
use crate::ui::dnd::FileDrag;
use crate::update::{UpdateState, Updater};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Tudo o que a interface pode pedir ao aplicativo. As telas enfileiram ações e o
/// `TabexApp` as aplica no fim do quadro, evitando conflitos de empréstimo.
#[derive(Debug, Clone)]
pub enum Action {
    Navigate(PathBuf),
    OpenInNewTab(PathBuf),
    OpenInNewGroup(PathBuf),
    OpenInNewWindow(PathBuf),
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

    NewWindow,
    EditWindow(usize),
    EditActiveWindow,
    SetWindowColor(usize, GroupColor),
    SwitchWindow(usize),
    CycleWindow(i32),
    CloseWindow(usize),
    CloseActiveWindow,
    MoveWindow(usize, isize),
    MoveGroupToWindow {
        g: usize,
        w: usize,
    },
    MoveTabToWindow {
        g: usize,
        t: usize,
        w: usize,
    },
    MoveTabToNewWindow(usize, usize),
    /// Abre a janela `w` como janela separada do sistema.
    DetachWindow(usize),
    /// Traz a janela (por id) de volta para a janela principal.
    AttachWindow(u64),

    /// Menu de contexto nativo do sistema (Windows) para `paths`, ou da pasta se vazio.
    NativeMenu {
        paths: Vec<PathBuf>,
        folder: PathBuf,
    },
    ToggleNativeMenu,
    CheckUpdates {
        manual: bool,
    },
    ShowUpdateDialog,
    InstallUpdate,
    RestartApp,
    ToggleAutoUpdate,

    Open(Vec<PathBuf>),
    OpenSelected,
    Copy,
    Cut,
    Paste,
    Undo,
    Redo,
    PushUndo(UndoEntry),
    RollbackUpdate,
    /// Enfileira uma operação (executa quando a atual terminar).
    Enqueue(QueuedOp),
    /// Cola com política de conflito já decidida.
    PasteWith {
        clip: Clipboard,
        dest: PathBuf,
        policy: ConflictPolicy,
    },
    CancelCurrentOp,
    ClearQueue,
    /// Arquivos soltos de fora (Explorer) na pasta `dest`; `cut` move em vez de copiar.
    DropFiles {
        paths: Vec<PathBuf>,
        dest: PathBuf,
        cut: bool,
    },
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
    SetViewIcons(bool),
    TogglePreview,
    ToggleSearchContent,
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
    pub clouds: Vec<CloudFolder>,
    pub updater: Updater,
    pub hwnd: Option<isize>,
    pub pending_copy_text: Option<String>,
    pub undo_stack: Vec<UndoEntry>,
    pub redo_stack: Vec<UndoEntry>,
    pub op_queue: std::collections::VecDeque<QueuedOp>,
    pub thumbs: Option<ThumbService>,
    pub preview_cache: crate::ui::preview::PreviewCache,
    /// Pasta sob o ponteiro durante um arrasto vindo de fora (para o aviso visual).
    pub hovering_external: bool,
    /// Renderizando uma janela separada (viewport) neste momento.
    pub in_detached: bool,
    /// Viewport a focar no próximo quadro (clique numa janela separada).
    focus_viewport: Option<egui::ViewportId>,
    /// Salvar sessão em disco (desligado em testes).
    persist: bool,
    watcher: Option<notify::RecommendedWatcher>,
    watch_rx: Option<std::sync::mpsc::Receiver<PathBuf>>,
    watched: Option<PathBuf>,
    /// Há arquivos na área de transferência do sistema (atualizado uma vez por quadro).
    pub sys_clip_has_files: bool,
    /// Sequência da área de transferência quando o Tabex a escreveu pela última vez.
    clip_seq: Option<u32>,
    drives_at: Instant,
    clouds_at: Instant,
    restart_requested: bool,
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
            clouds: cloud::detect(),
            updater: Updater::new(),
            hwnd: None,
            pending_copy_text: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            op_queue: std::collections::VecDeque::new(),
            thumbs: Some(ThumbService::new()),
            preview_cache: Default::default(),
            hovering_external: false,
            in_detached: false,
            focus_viewport: None,
            persist: true,
            watcher: None,
            watch_rx: None,
            watched: None,
            sys_clip_has_files: false,
            clip_seq: None,
            drives_at: Instant::now(),
            clouds_at: Instant::now(),
            restart_requested: false,
            dirty: false,
            last_save: Instant::now(),
            last_title: String::new(),
        }
    }

    /// Instância para testes: sessão nova apontando para `root`, sem rede nem gravação em disco.
    pub fn for_test(root: PathBuf) -> Self {
        let mut session = Session::fresh();
        session.auto_update = false;
        session.use_native_menu = false;
        session.active_tab_mut().navigate(root);
        TabexApp {
            theme_applied: None,
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
            drives: Vec::new(),
            clouds: Vec::new(),
            updater: Updater::new(),
            hwnd: None,
            pending_copy_text: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            sys_clip_has_files: false,
            clip_seq: None,
            op_queue: std::collections::VecDeque::new(),
            thumbs: None,
            preview_cache: Default::default(),
            hovering_external: false,
            in_detached: false,
            focus_viewport: None,
            persist: false,
            watcher: None,
            watch_rx: None,
            watched: None,
            drives_at: Instant::now(),
            clouds_at: Instant::now(),
            restart_requested: false,
            dirty: false,
            last_save: Instant::now(),
            last_title: String::new(),
        }
    }

    /// Um quadro completo sem `eframe::Frame` (usado pelos testes de interface).
    pub fn frame_for_test(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        // Sem fontes do sistema nos testes: glifos ausentes viram "tofu" em vez de pânico.
        ctx.set_missing_glyph_policy(egui::epaint::text::MissingGlyphPolicy::Tofu);
        self.logic_ctx(&ctx);
        self.render_root(ui);
        self.frame_end(&ctx);
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
                let (err, cancelled) = op
                    .progress
                    .lock()
                    .map(|p| (p.error.clone(), p.cancelled))
                    .unwrap_or((None, false));
                let entry = if err.is_none() && !cancelled {
                    op.undo_entry()
                } else {
                    None
                };
                finished.push((op.refresh.clone(), err, entry, op.push_to, cancelled));
                false
            } else {
                true
            }
        });
        for (dirs, err, entry, stack, cancelled) in finished {
            self.refresh_dirs(&dirs);
            if cancelled {
                self.toast("Operação cancelada");
            }
            if let Some(e) = err {
                self.error(e);
            } else if let Some(entry) = entry {
                match stack {
                    Stack::Undo => {
                        self.toast(format!("Concluído: {} (Ctrl+Z desfaz)", entry.label));
                        self.undo_stack.push(entry);
                        self.undo_stack.truncate(50);
                        self.redo_stack.clear();
                    }
                    Stack::Redo => {
                        self.toast(format!("Desfeito: {} (Ctrl+Y refaz)", entry.label));
                        self.redo_stack.push(entry);
                    }
                }
            }
        }
        self.sys_clip_has_files = sysclip::has_files();
        // Fila: uma operação por vez.
        if self.ops.is_empty() {
            if let Some(next) = self.op_queue.pop_front() {
                self.ops.push(next.start());
            }
        }
        if !self.ops.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }

        // Monitoramento da pasta ativa (notify) com polling de segurança.
        self.update_watcher(ctx);
        let mut changed_dirs: Vec<PathBuf> = Vec::new();
        if let Some(rx) = &self.watch_rx {
            while let Ok(p) = rx.try_recv() {
                changed_dirs.push(p);
            }
        }
        if !changed_dirs.is_empty() {
            changed_dirs.sort();
            changed_dirs.dedup();
            self.refresh_dirs(&changed_dirs);
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
                if t.poll_loading() {
                    busy = true;
                }
                if visible {
                    t.check_fs_changed();
                    if t.needs_reload || t.loaded_hidden != show_hidden {
                        // Agrupa rajadas de eventos: no máximo uma releitura a cada 250 ms.
                        let recent = t
                            .last_reload
                            .map(|r| r.elapsed() < Duration::from_millis(250))
                            .unwrap_or(false);
                        if recent && t.loaded_hidden == show_hidden {
                            busy = true;
                        } else {
                            t.reload(show_hidden);
                            busy = true;
                        }
                    }
                    if t.is_loading() {
                        busy = true;
                    }
                }
            }
        }
        if busy {
            ctx.request_repaint_after(Duration::from_millis(60));
        } else if self.session.active_tab().search.is_none() {
            // Verificação periódica da pasta ativa (fallback do notify).
            ctx.request_repaint_after(Duration::from_millis(5000));
        }

        if self.drives_at.elapsed() > Duration::from_secs(10) {
            self.drives = list_drives();
            self.drives_at = Instant::now();
        }
        if self.clouds_at.elapsed() > Duration::from_secs(30) {
            self.clouds = cloud::detect();
            self.clouds_at = Instant::now();
        }
        if let Some(t) = &self.thumbs {
            if t.poll(ctx) {
                ctx.request_repaint();
            }
        }

        // Arquivos soltos de fora (Explorer etc.) vão para a pasta ativa.
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        self.hovering_external = ctx.input(|i| !i.raw.hovered_files.is_empty());
        if !dropped.is_empty() {
            let dest = self.session.active_tab().path.clone();
            let cut = ctx.input(|i| i.modifiers.shift);
            self.pending.push(Action::DropFiles {
                paths: dropped,
                dest,
                cut,
            });
        }

        // Arrasto interno que saiu da janela: entrega ao sistema (Windows).
        if platform::drag_out_supported() {
            if let Some(p) = egui::DragAndDrop::payload::<FileDrag>(ctx) {
                let outside = ctx.input(|i| i.pointer.primary_down() && i.pointer.hover_pos().is_none());
                if outside {
                    egui::DragAndDrop::clear_payload(ctx);
                    match platform::drag_out(&p.paths) {
                        Ok(moved) => {
                            if moved {
                                let dirs: Vec<PathBuf> = p
                                    .paths
                                    .iter()
                                    .filter_map(|x| x.parent().map(|d| d.to_path_buf()))
                                    .collect();
                                self.refresh_dirs(&dirs);
                            }
                        }
                        Err(e) => self.toast(format!("Arrastar para fora: {e}")),
                    }
                }
            }
        }

        // Atualizações: verificação automática (uma vez por dia) e aviso quando houver versão nova.
        if self.session.auto_update {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            if now.saturating_sub(self.session.last_update_check) > 24 * 3600 {
                self.session.last_update_check = now;
                self.dirty = true;
                self.updater.check();
            }
        }
        if self.updater.is_busy() {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        if !self.updater.notified {
            if let UpdateState::Available(info) = self.updater.state() {
                self.updater.notified = true;
                self.toast(format!(
                    "Nova versão {} disponível — veja em ⋯ › Atualizações",
                    info.tag
                ));
            }
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

            OpenInNewWindow(p) => {
                self.session.new_window(None, None, p);
                self.dirty = true;
            }
            NewWindow => {
                let n = self.session.windows_created + 1;
                self.dialog = Dialog::Window {
                    index: None,
                    name: format!("Janela {n}"),
                    color: GroupColor::nth(n.saturating_sub(1)),
                    first: true,
                };
            }
            EditWindow(w) => {
                if let Some(win) = self.session.windows.get(w) {
                    self.dialog = Dialog::Window {
                        index: Some(w),
                        name: win.name.clone(),
                        color: win.color,
                        first: true,
                    };
                }
            }
            EditActiveWindow => {
                let w = self.session.active_window;
                self.apply(EditWindow(w));
            }
            SetWindowColor(w, c) => {
                if let Some(win) = self.session.windows.get_mut(w) {
                    win.color = c;
                    self.dirty = true;
                }
            }
            SwitchWindow(w) => {
                if self.session.windows.get(w).map(|x| x.detached).unwrap_or(false) && !self.in_detached {
                    let id = self.session.windows[w].id;
                    self.focus_viewport = Some(egui::ViewportId::from_hash_of(("tabex-window", id)));
                } else {
                    self.session.switch_window(w);
                    self.dirty = true;
                }
            }
            DetachWindow(w) => {
                if w >= self.session.windows.len() {
                    return;
                }
                if self.session.windows.len() < 2 {
                    self.toast("Crie outra janela antes de separar esta (Ctrl+Alt+T)");
                    return;
                }
                if w == self.session.active_window {
                    let other = if w + 1 < self.session.windows.len() {
                        w + 1
                    } else {
                        w - 1
                    };
                    let other = (0..self.session.windows.len())
                        .find(|&i| i != w && !self.session.windows[i].detached)
                        .unwrap_or(other);
                    if other == w {
                        self.toast("Traga outra janela de volta antes de separar esta");
                        return;
                    }
                    self.session.switch_window(other);
                }
                self.session.windows[w].detached = true;
                self.dirty = true;
            }
            AttachWindow(id) => {
                if let Some(w) = self.session.windows.iter_mut().find(|x| x.id == id) {
                    w.detached = false;
                    self.dirty = true;
                }
            }
            CycleWindow(d) => {
                self.session.cycle_window(d);
                self.dirty = true;
            }
            CloseWindow(w) => {
                let tabs = self.session.close_window(w);
                self.closed_tabs.extend(tabs);
                self.closed_tabs.truncate(20);
                self.dirty = true;
            }
            CloseActiveWindow => {
                let w = self.session.active_window;
                self.apply(CloseWindow(w));
            }
            MoveWindow(w, d) => {
                self.session.move_window(w, d);
                self.dirty = true;
            }
            MoveGroupToWindow { g, w } => {
                self.session.move_group_to_window(g, w);
                self.dirty = true;
            }
            MoveTabToWindow { g, t, w } => {
                self.session.move_tab_to_window(g, t, w);
                self.dirty = true;
            }
            MoveTabToNewWindow(g, t) => {
                let path = match self.session.groups.get(g).and_then(|grp| grp.tabs.get(t)) {
                    Some(tab) => tab.path.clone(),
                    None => return,
                };
                // Fecha a aba na janela atual e abre uma janela nova nessa pasta.
                self.session.close_tab(g, t);
                self.session.new_window(None, None, path);
                self.dirty = true;
            }

            NativeMenu { paths, folder } => self.native_menu(paths, folder),
            ToggleNativeMenu => {
                self.session.use_native_menu = !self.session.use_native_menu;
                self.dirty = true;
            }
            CheckUpdates { manual } => {
                self.updater.check();
                if manual {
                    self.dialog = Dialog::Update;
                }
            }
            ShowUpdateDialog => self.dialog = Dialog::Update,
            InstallUpdate => {
                if let UpdateState::Available(info) = self.updater.state() {
                    self.updater.install(info);
                }
                self.dialog = Dialog::Update;
            }
            RestartApp => match crate::update::spawn_new_instance() {
                Ok(()) => self.restart_requested = true,
                Err(e) => self.error(format!("Não foi possível reiniciar: {e}")),
            },
            ToggleAutoUpdate => {
                self.session.auto_update = !self.session.auto_update;
                self.dirty = true;
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
                    // Área de transferência do sistema (CF_HDROP no Windows) + cópia interna.
                    match sysclip::write(&paths, cut) {
                        Ok(()) => self.clip_seq = sysclip::sequence(),
                        Err(_) => {
                            let text = paths
                                .iter()
                                .map(|p| p.to_string_lossy().into_owned())
                                .collect::<Vec<_>>()
                                .join("\n");
                            self.pending_copy_text = Some(text);
                        }
                    }
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
                // Se a área de transferência do sistema mudou desde que o Tabex a escreveu
                // (ex.: copiado no Explorador), ela tem prioridade sobre a cópia interna.
                let system_newer = self.clipboard.is_none() || sysclip::sequence() != self.clip_seq;
                if system_newer {
                    if let Some(sys) = sysclip::read() {
                        self.clipboard = Some(Clipboard {
                            paths: sys.paths,
                            cut: sys.cut,
                        });
                        self.clip_seq = sysclip::sequence();
                    }
                }
                if let Some(clip) = self.clipboard.clone() {
                    if clip.cut {
                        self.clipboard = None;
                        self.clip_seq = None;
                    }
                    self.paste_clip(clip, dest);
                }
            }
            PasteWith { clip, dest, policy } => {
                self.enqueue(QueuedOp::Paste { clip, dest, policy });
            }
            Enqueue(op) => self.enqueue(op),
            CancelCurrentOp => {
                for op in &self.ops {
                    op.request_cancel();
                }
            }
            ClearQueue => {
                let n = self.op_queue.len();
                self.op_queue.clear();
                if n > 0 {
                    self.toast(format!("{n} operação(ões) removida(s) da fila"));
                }
            }
            DropFiles { paths, dest, cut } => {
                if !paths.is_empty() && dest.is_dir() {
                    // Soltar na própria pasta de origem não faz nada.
                    let all_same = paths.iter().all(|p| p.parent() == Some(dest.as_path()));
                    if !all_same {
                        self.paste_clip(Clipboard { paths, cut }, dest);
                    }
                }
            }
            Undo => {
                if let Some(entry) = self.undo_stack.pop() {
                    self.enqueue(QueuedOp::UndoRedo { entry, undo: true });
                } else {
                    self.toast("Nada para desfazer");
                }
            }
            Redo => {
                if let Some(entry) = self.redo_stack.pop() {
                    self.enqueue(QueuedOp::UndoRedo { entry, undo: false });
                } else {
                    self.toast("Nada para refazer");
                }
            }
            PushUndo(entry) => {
                self.undo_stack.push(entry);
                self.undo_stack.truncate(50);
                self.redo_stack.clear();
            }
            RollbackUpdate => {
                self.updater.rollback();
                self.dialog = Dialog::Update;
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
            SetViewIcons(v) => {
                self.session.view_icons = v;
                self.dirty = true;
            }
            TogglePreview => {
                self.session.preview_visible = !self.session.preview_visible;
                self.dirty = true;
            }
            ToggleSearchContent => {
                self.session.search_content = !self.session.search_content;
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

    /// Mantém o `notify` observando a pasta da aba ativa.
    fn update_watcher(&mut self, ctx: &egui::Context) {
        use notify::Watcher;
        let want = self.session.active_tab().path.clone();
        if self.watched.as_ref() == Some(&want) {
            return;
        }
        if self.watcher.is_none() {
            let (tx, rx) = std::sync::mpsc::channel::<PathBuf>();
            let ctx2 = ctx.clone();
            let watcher = notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
                if let Ok(ev) = res {
                    for p in ev.paths {
                        // O evento traz o arquivo; a pasta a recarregar é a pai (e a própria, se for pasta).
                        if let Some(parent) = p.parent() {
                            let _ = tx.send(parent.to_path_buf());
                        }
                        let _ = tx.send(p);
                    }
                    ctx2.request_repaint();
                }
            });
            match watcher {
                Ok(w) => {
                    self.watcher = Some(w);
                    self.watch_rx = Some(rx);
                }
                Err(e) => {
                    eprintln!("notify indisponível, usando polling: {e}");
                    self.watched = Some(want);
                    return;
                }
            }
        }
        if let Some(w) = self.watcher.as_mut() {
            if let Some(old) = self.watched.take() {
                let _ = w.unwatch(&old);
            }
            if let Err(e) = w.watch(&want, notify::RecursiveMode::NonRecursive) {
                eprintln!("não foi possível observar {}: {e}", want.display());
            }
        }
        self.watched = Some(want);
    }

    /// Inicia (ou enfileira) uma colagem, perguntando sobre conflitos quando houver.
    fn paste_clip(&mut self, clip: Clipboard, dest: PathBuf) {
        let conflicts = ops::find_conflicts(&clip.paths, &dest, clip.cut);
        if conflicts.is_empty() {
            self.enqueue(QueuedOp::Paste {
                clip,
                dest,
                policy: ConflictPolicy::KeepBoth,
            });
        } else {
            self.dialog = Dialog::Conflict { clip, dest, conflicts };
        }
    }

    fn enqueue(&mut self, op: QueuedOp) {
        if self.ops.is_empty() && self.op_queue.is_empty() {
            self.ops.push(op.start());
        } else {
            self.toast(format!("Na fila: {}", op.label()));
            self.op_queue.push_back(op);
        }
    }

    /// Usa o menu de contexto do Windows? (preferência + plataforma)
    pub fn native_menu_enabled(&self) -> bool {
        self.session.use_native_menu && platform::native_menu_supported()
    }

    fn native_menu(&mut self, paths: Vec<PathBuf>, folder: PathBuf) {
        const OPEN_TAB: u32 = 1;
        const OPEN_GROUP: u32 = 2;
        const OPEN_WINDOW: u32 = 3;
        const PIN: u32 = 4;
        const BG_REFRESH: u32 = 11;
        const BG_PIN: u32 = 12;
        const BG_NEW_TAB: u32 = 13;
        let single_dir = paths.len() == 1 && paths[0].is_dir();
        let mut extras = Vec::new();
        if paths.is_empty() {
            extras.push(MenuExtra {
                id: BG_NEW_TAB,
                label: "Abrir esta pasta em nova aba".into(),
            });
            let pinned = self.session.is_pinned(&folder);
            extras.push(MenuExtra {
                id: BG_PIN,
                label: if pinned {
                    "Desafixar do Acesso rápido".into()
                } else {
                    "Fixar no Acesso rápido".into()
                },
            });
            extras.push(MenuExtra {
                id: BG_REFRESH,
                label: "Atualizar\tF5".into(),
            });
        } else if single_dir {
            extras.push(MenuExtra {
                id: OPEN_TAB,
                label: "Abrir em nova aba".into(),
            });
            extras.push(MenuExtra {
                id: OPEN_GROUP,
                label: "Abrir em novo grupo".into(),
            });
            extras.push(MenuExtra {
                id: OPEN_WINDOW,
                label: "Abrir em nova janela".into(),
            });
            let pinned = self.session.is_pinned(&paths[0]);
            extras.push(MenuExtra {
                id: PIN,
                label: if pinned {
                    "Desafixar do Acesso rápido".into()
                } else {
                    "Fixar no Acesso rápido".into()
                },
            });
        }
        let hwnd = if self.in_detached { None } else { self.hwnd };
        match platform::show_native_menu(hwnd, &paths, &folder, &extras) {
            Ok(r) => {
                match r.extra {
                    Some(OPEN_TAB) => self.apply(Action::OpenInNewTab(paths[0].clone())),
                    Some(OPEN_GROUP) => self.apply(Action::OpenInNewGroup(paths[0].clone())),
                    Some(OPEN_WINDOW) => self.apply(Action::OpenInNewWindow(paths[0].clone())),
                    Some(PIN) => self.apply(Action::TogglePin(paths[0].clone())),
                    Some(BG_REFRESH) => self.apply(Action::Refresh),
                    Some(BG_PIN) => self.apply(Action::TogglePin(folder.clone())),
                    Some(BG_NEW_TAB) => self.apply(Action::OpenInNewTab(folder.clone())),
                    _ => {}
                }
                // O comando do shell pode ter alterado a pasta.
                self.session.active_tab_mut().needs_reload = true;
            }
            Err(e) => self.toast(format!("Menu nativo indisponível: {e}")),
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
        let title = format!(
            "{} · {} - Tabex",
            self.session.active_tab().title(),
            self.session.active_workspace().name
        );
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

impl TabexApp {
    /// Lógica de um quadro: tema, trabalhos em segundo plano e atalhos da janela principal.
    pub fn logic_ctx(&mut self, ctx: &egui::Context) {
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

    /// Conteúdo da janela principal: barra de janelas + espaço de trabalho ativo + diálogos.
    pub fn render_root(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.render_workspace(ui, true);
        ui::dnd::ghost(&ctx);
        ui::dialogs::show(&ctx, self);
        self.drain_actions(&ctx);
        if let Some(v) = self.focus_viewport.take() {
            ctx.send_viewport_cmd_to(v, egui::ViewportCommand::Focus);
        }
    }

    /// Painéis de um espaço de trabalho (o ativo em `session`). `main` mostra a barra de janelas;
    /// caso contrário mostra o cabeçalho de janela separada.
    pub fn render_workspace(&mut self, ui: &mut egui::Ui, main: bool) {
        let ctx = ui.ctx().clone();
        let visuals = ctx.global_style().visuals.clone();
        let vid = ctx.viewport_id();

        if main {
            egui::Panel::top(("windowbar", vid))
                .exact_size(ui::windowbar::HEIGHT)
                .frame(egui::Frame::new().fill(visuals.panel_fill).inner_margin(egui::Margin {
                    left: 8,
                    right: 4,
                    top: 3,
                    bottom: 0,
                }))
                .show(ui, |ui| ui::windowbar::show(ui, self));
        } else {
            egui::Panel::top(("detached_header", vid))
                .exact_size(ui::windowbar::HEIGHT)
                .frame(egui::Frame::new().fill(visuals.panel_fill).inner_margin(egui::Margin {
                    left: 8,
                    right: 4,
                    top: 3,
                    bottom: 0,
                }))
                .show(ui, |ui| ui::windowbar::show_detached_header(ui, self));
        }

        egui::Panel::top(("tabstrip", vid))
            .exact_size(36.0)
            .frame(egui::Frame::new().fill(visuals.panel_fill).inner_margin(egui::Margin {
                left: 4,
                right: 4,
                top: 4,
                bottom: 0,
            }))
            .show(ui, |ui| ui::tabstrip::show(ui, self));

        egui::Panel::top(("toolbar", vid))
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(visuals.window_fill)
                    .inner_margin(egui::Margin::symmetric(8, 6)),
            )
            .show(ui, |ui| ui::toolbar::show(ui, self));

        egui::Panel::bottom(("status", vid))
            .exact_size(26.0)
            .frame(
                egui::Frame::new()
                    .fill(visuals.panel_fill)
                    .inner_margin(egui::Margin::symmetric(10, 4)),
            )
            .show(ui, |ui| ui::statusbar::show(ui, self));

        if self.session.sidebar_visible {
            egui::Panel::left(("sidebar", vid))
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

        if self.session.preview_visible {
            egui::Panel::right(("preview", vid))
                .resizable(true)
                .default_size(300.0)
                .size_range(200.0..=700.0)
                .frame(
                    egui::Frame::new()
                        .fill(visuals.panel_fill)
                        .inner_margin(egui::Margin::symmetric(8, 6)),
                )
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| ui::preview::show(ui, self));
                });
        }

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(visuals.extreme_bg_color)
                    .inner_margin(egui::Margin::same(0)),
            )
            .show(ui, |ui| {
                ui::filelist::show(ui, self);
                if self.hovering_external {
                    let r = ui.max_rect();
                    ui.painter().rect(
                        r.shrink(4.0),
                        6,
                        egui::Color32::from_rgba_unmultiplied(0x1A, 0x73, 0xE8, 18),
                        egui::Stroke::new(2.0, visuals.hyperlink_color),
                        egui::StrokeKind::Inside,
                    );
                    ui.painter().text(
                        r.center(),
                        egui::Align2::CENTER_CENTER,
                        format!(
                            "Solte para copiar em \"{}\" (Shift: mover)",
                            self.session.active_tab().title()
                        ),
                        egui::FontId::proportional(16.0),
                        visuals.hyperlink_color,
                    );
                }
            });
    }

    /// Aplica as ações enfileiradas (ações podem gerar novas ações).
    pub fn drain_actions(&mut self, ctx: &egui::Context) {
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
    }

    /// Janelas separadas (viewports imediatos): cada uma renderiza seu espaço de trabalho.
    fn render_detached(&mut self, ctx: &egui::Context) {
        let ids: Vec<u64> = self
            .session
            .windows
            .iter()
            .filter(|w| w.detached)
            .map(|w| w.id)
            .collect();
        if ids.is_empty() {
            return;
        }
        let main_id = self.session.active_workspace().id;
        for id in ids {
            let Some(idx) = self.session.windows.iter().position(|w| w.id == id && w.detached) else {
                continue;
            };
            if idx == self.session.active_window {
                continue;
            }
            let name = self.session.windows[idx].name.clone();
            self.session.switch_window(idx);
            self.in_detached = true;
            let vid = egui::ViewportId::from_hash_of(("tabex-window", id));
            let builder = egui::ViewportBuilder::default()
                .with_title(format!("{name} - Tabex"))
                .with_inner_size([1100.0, 720.0])
                .with_min_inner_size([640.0, 400.0]);
            let mut reattach = false;
            ctx.show_viewport_immediate(vid, builder, |ui, class| {
                if class == egui::ViewportClass::EmbeddedWindow {
                    ui.centered_and_justified(|ui| {
                        ui.label("Janelas separadas não são suportadas neste sistema.");
                    });
                    reattach = true;
                    return;
                }
                let vctx = ui.ctx().clone();
                if matches!(self.dialog, Dialog::None) {
                    let acts = shortcuts::collect(&vctx, self.address_edit.is_some());
                    self.pending.extend(acts);
                }
                self.render_workspace(ui, false);
                ui::dnd::ghost(&vctx);
                self.drain_actions(&vctx);
                if vctx.input(|i| i.viewport().close_requested()) {
                    reattach = true;
                }
            });
            self.in_detached = false;
            if reattach {
                if let Some(w) = self.session.windows.iter_mut().find(|w| w.id == id) {
                    w.detached = false;
                }
                self.dirty = true;
            }
            // Volta para a janela principal (pelo id, pois índices podem ter mudado).
            if let Some(back) = self.session.windows.iter().position(|w| w.id == main_id) {
                self.session.switch_window(back);
            } else if let Some(back) = self.session.windows.iter().position(|w| !w.detached) {
                self.session.switch_window(back);
            }
        }
    }

    /// Fim do quadro: título, reinício, salvamento e expiração do aviso.
    pub fn frame_end(&mut self, ctx: &egui::Context) {
        self.update_title(ctx);

        if self.restart_requested {
            let _ = self.session.save();
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        if self.persist && self.dirty && self.last_save.elapsed() > Duration::from_secs(2) {
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
}

impl eframe::App for TabexApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.logic_ctx(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if self.hwnd.is_none() {
            if let Ok(h) = frame.window_handle() {
                if let RawWindowHandle::Win32(w) = h.as_raw() {
                    self.hwnd = Some(w.hwnd.get());
                }
            }
        }
        self.render_root(ui);
        self.render_detached(&ctx);
        self.frame_end(&ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.persist {
            if let Err(e) = self.session.save() {
                eprintln!("falha ao salvar sessão: {e}");
            }
        }
    }
}
