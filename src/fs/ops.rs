use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// O que fazer quando o destino já existe.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ConflictPolicy {
    /// Renomeia o novo item ("Nome (2)").
    #[default]
    KeepBoth,
    /// Substitui o existente (pastas são mescladas, arquivos sobrescritos).
    Replace,
    /// Não copia/move o item em conflito.
    Skip,
}

/// Item em conflito, para o diálogo.
#[derive(Clone, Debug)]
pub struct Conflict {
    pub name: String,
    pub src: PathBuf,
    pub dst: PathBuf,
}

/// Lista os itens de `paths` cujo nome já existe em `dest`.
pub fn find_conflicts(paths: &[PathBuf], dest: &Path, cut: bool) -> Vec<Conflict> {
    let mut out = Vec::new();
    for src in paths {
        let Some(name) = src.file_name() else { continue };
        let same_dir = src.parent().map(|p| p == dest).unwrap_or(false);
        if same_dir {
            // Copiar na mesma pasta vira "- Cópia"; mover na mesma pasta não faz nada.
            let _ = cut;
            continue;
        }
        let dst = dest.join(name);
        if dst.exists() {
            out.push(Conflict {
                name: name.to_string_lossy().into_owned(),
                src: src.clone(),
                dst,
            });
        }
    }
    out
}

/// Área de transferência interna (recortar/copiar).
#[derive(Clone, Debug)]
pub struct Clipboard {
    pub paths: Vec<PathBuf>,
    pub cut: bool,
}

#[derive(Debug, Default)]
pub struct OpProgress {
    pub label: String,
    pub done: usize,
    pub total: usize,
    pub current: String,
    pub finished: bool,
    pub error: Option<String>,
    /// Pares (origem, destino) dos itens de nível superior copiados/movidos.
    pub pairs: Vec<(PathBuf, PathBuf)>,
    /// A operação foi cancelada pelo usuário (não é erro).
    pub cancelled: bool,
}

/// Uma operação que pode ser desfeita (ou refeita).
#[derive(Clone, Debug)]
pub enum UndoKind {
    Rename { from: PathBuf, to: PathBuf },
    Move { pairs: Vec<(PathBuf, PathBuf)> },
    Copy { pairs: Vec<(PathBuf, PathBuf)> },
    Create { path: PathBuf, is_dir: bool },
    Trash { paths: Vec<PathBuf> },
}

#[derive(Clone, Debug)]
pub struct UndoEntry {
    pub label: String,
    pub kind: UndoKind,
}

impl UndoEntry {
    pub fn new(label: impl Into<String>, kind: UndoKind) -> UndoEntry {
        UndoEntry {
            label: label.into(),
            kind,
        }
    }
}

/// Em qual pilha a entrada deve entrar quando a operação terminar com sucesso.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stack {
    Undo,
    Redo,
}

/// Como montar a entrada de desfazer ao fim de uma operação.
#[derive(Clone, Debug)]
pub enum UndoTemplate {
    None,
    /// Mover: pares preenchidos durante a operação.
    Move,
    /// Copiar: pares preenchidos durante a operação.
    Copy,
    Trash(Vec<PathBuf>),
    Fixed(UndoEntry),
}

/// Operação de arquivo em segundo plano (copiar, mover, excluir).
pub struct OpJob {
    pub progress: Arc<Mutex<OpProgress>>,
    /// Pastas que devem ser recarregadas ao terminar.
    pub refresh: Vec<PathBuf>,
    pub undo: UndoTemplate,
    pub push_to: Stack,
    pub cancel: Arc<AtomicBool>,
}

/// Operação ainda não iniciada (fila).
#[derive(Clone, Debug)]
pub enum QueuedOp {
    Paste {
        clip: Clipboard,
        dest: PathBuf,
        policy: ConflictPolicy,
    },
    Delete {
        paths: Vec<PathBuf>,
        permanent: bool,
    },
    UndoRedo {
        entry: UndoEntry,
        undo: bool,
    },
}

impl QueuedOp {
    pub fn label(&self) -> String {
        match self {
            QueuedOp::Paste { clip, .. } => {
                format!(
                    "{} {} {}",
                    if clip.cut { "Mover" } else { "Copiar" },
                    clip.paths.len(),
                    plural(clip.paths.len())
                )
            }
            QueuedOp::Delete { paths, .. } => format!("Excluir {} {}", paths.len(), plural(paths.len())),
            QueuedOp::UndoRedo { entry, undo } => {
                format!("{} {}", if *undo { "Desfazer" } else { "Refazer" }, entry.label)
            }
        }
    }

    pub fn start(self) -> OpJob {
        match self {
            QueuedOp::Paste { clip, dest, policy } => start_paste_with(&clip, &dest, policy),
            QueuedOp::Delete { paths, permanent } => start_delete(paths, permanent),
            QueuedOp::UndoRedo { entry, undo } => start_undo_redo(entry, undo),
        }
    }
}

fn cancelled(cancel: &AtomicBool) -> io::Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(io::Error::other(CANCEL_MSG))
    } else {
        Ok(())
    }
}

const CANCEL_MSG: &str = "__cancelado__";

impl OpJob {
    pub fn is_finished(&self) -> bool {
        self.progress.lock().map(|p| p.finished).unwrap_or(true)
    }

    pub fn request_cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// Entrada de desfazer/refazer resultante (só faz sentido após terminar sem erro).
    pub fn undo_entry(&self) -> Option<UndoEntry> {
        let pairs = self.progress.lock().map(|p| p.pairs.clone()).unwrap_or_default();
        match &self.undo {
            UndoTemplate::None => None,
            UndoTemplate::Move if pairs.is_empty() => None,
            UndoTemplate::Move => Some(UndoEntry::new(
                format!("mover {} {}", pairs.len(), plural(pairs.len())),
                UndoKind::Move { pairs },
            )),
            UndoTemplate::Copy if pairs.is_empty() => None,
            UndoTemplate::Copy => Some(UndoEntry::new(
                format!("copiar {} {}", pairs.len(), plural(pairs.len())),
                UndoKind::Copy { pairs },
            )),
            UndoTemplate::Trash(paths) => Some(UndoEntry::new(
                format!("excluir {} {}", paths.len(), plural(paths.len())),
                UndoKind::Trash { paths: paths.clone() },
            )),
            UndoTemplate::Fixed(e) => Some(e.clone()),
        }
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        "item"
    } else {
        "itens"
    }
}

fn record_pair(progress: &Arc<Mutex<OpProgress>>, src: &Path, dst: &Path) {
    if let Ok(mut p) = progress.lock() {
        p.pairs.push((src.to_path_buf(), dst.to_path_buf()));
    }
}

fn finish(progress: &Arc<Mutex<OpProgress>>, result: Result<(), String>) {
    if let Ok(mut p) = progress.lock() {
        p.finished = true;
        match result {
            Err(e) if e.contains(CANCEL_MSG) => p.cancelled = true,
            Err(e) => p.error = Some(e),
            Ok(()) => {}
        }
    }
}

fn count_items(paths: &[PathBuf]) -> usize {
    let mut n = 0;
    for p in paths {
        if p.is_dir() {
            n += walkdir::WalkDir::new(p).into_iter().filter_map(Result::ok).count();
        } else {
            n += 1;
        }
    }
    n.max(1)
}

fn set_current(progress: &Arc<Mutex<OpProgress>>, path: &Path) {
    if let Ok(mut p) = progress.lock() {
        p.current = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
    }
}

fn tick(progress: &Arc<Mutex<OpProgress>>) {
    if let Ok(mut p) = progress.lock() {
        p.done += 1;
    }
}

fn copy_recursive(src: &Path, dst: &Path, progress: &Arc<Mutex<OpProgress>>) -> io::Result<()> {
    copy_recursive_c(src, dst, progress, &AtomicBool::new(false))
}

fn copy_recursive_c(src: &Path, dst: &Path, progress: &Arc<Mutex<OpProgress>>, cancel: &AtomicBool) -> io::Result<()> {
    cancelled(cancel)?;
    set_current(progress, src);
    let md = fs::metadata(src)?;
    if md.is_dir() {
        fs::create_dir_all(dst)?;
        tick(progress);
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            copy_recursive_c(&entry.path(), &dst.join(entry.file_name()), progress, cancel)?;
        }
    } else {
        if dst.is_dir() {
            fs::remove_dir_all(dst)?;
        }
        fs::copy(src, dst)?;
        tick(progress);
    }
    Ok(())
}

fn move_path(src: &Path, dst: &Path, progress: &Arc<Mutex<OpProgress>>) -> io::Result<()> {
    move_path_c(src, dst, progress, &AtomicBool::new(false))
}

fn move_path_c(src: &Path, dst: &Path, progress: &Arc<Mutex<OpProgress>>, cancel: &AtomicBool) -> io::Result<()> {
    cancelled(cancel)?;
    set_current(progress, src);
    if dst.exists() && !(src.is_dir() && dst.is_dir()) {
        // Substituir arquivo: remove o destino antes do rename.
        remove_any(dst)?;
    }
    if src.is_dir() && dst.is_dir() {
        // Mesclar pasta em pasta existente.
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            move_path_c(&entry.path(), &dst.join(entry.file_name()), progress, cancel)?;
        }
        return fs::remove_dir(src);
    }
    match fs::rename(src, dst) {
        Ok(()) => {
            tick(progress);
            Ok(())
        }
        Err(_) => {
            // Provavelmente outro volume: copia e remove.
            copy_recursive_c(src, dst, progress, cancel)?;
            if src.is_dir() {
                fs::remove_dir_all(src)
            } else {
                fs::remove_file(src)
            }
        }
    }
}

/// Gera um nome que não existe em `dir`, no estilo "Nome (2)".
pub fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let candidate = dir.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let (stem, ext) = split_name(name);
    for i in 2.. {
        let n = if ext.is_empty() {
            format!("{stem} ({i})")
        } else {
            format!("{stem} ({i}).{ext}")
        };
        let c = dir.join(n);
        if !c.exists() {
            return c;
        }
    }
    unreachable!()
}

/// Nome para cópia na mesma pasta: "Nome - Cópia", "Nome - Cópia (2)".
pub fn copy_name(dir: &Path, name: &str) -> PathBuf {
    let (stem, ext) = split_name(name);
    let base = if ext.is_empty() {
        format!("{stem} - Cópia")
    } else {
        format!("{stem} - Cópia.{ext}")
    };
    unique_path(dir, &base)
}

fn split_name(name: &str) -> (String, String) {
    let p = Path::new(name);
    match (p.file_stem(), p.extension()) {
        (Some(s), Some(e)) if !s.is_empty() => (s.to_string_lossy().into_owned(), e.to_string_lossy().into_owned()),
        _ => (name.to_owned(), String::new()),
    }
}

/// Cola os itens da área de transferência em `dest` (conflitos: "manter ambos").
#[cfg_attr(not(test), allow(dead_code))]
pub fn start_paste(clip: &Clipboard, dest: &Path) -> OpJob {
    start_paste_with(clip, dest, ConflictPolicy::KeepBoth)
}

/// Cola os itens da área de transferência em `dest` com a política de conflito dada.
pub fn start_paste_with(clip: &Clipboard, dest: &Path, policy: ConflictPolicy) -> OpJob {
    let paths = clip.paths.clone();
    let cut = clip.cut;
    let dest = dest.to_path_buf();
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_t = cancel.clone();
    let progress = Arc::new(Mutex::new(OpProgress {
        label: if cut {
            "Movendo".to_owned()
        } else {
            "Copiando".to_owned()
        },
        ..Default::default()
    }));
    let mut refresh: Vec<PathBuf> = vec![dest.clone()];
    for p in &paths {
        if let Some(parent) = p.parent() {
            refresh.push(parent.to_path_buf());
        }
    }
    let prog = progress.clone();
    std::thread::spawn(move || {
        let result = (|| -> io::Result<()> {
            if let Ok(mut p) = prog.lock() {
                p.total = count_items(&paths);
            }
            for src in &paths {
                let Some(name) = src.file_name() else { continue };
                let name = name.to_string_lossy().into_owned();
                if dest.starts_with(src) {
                    return Err(io::Error::other(format!(
                        "Não é possível copiar \"{name}\" para dentro dela mesma."
                    )));
                }
                let same_dir = src.parent().map(|p| p == dest).unwrap_or(false);
                let plain = dest.join(&name);
                let conflict = !same_dir && plain.exists();
                if conflict && policy == ConflictPolicy::Skip {
                    continue;
                }
                if cut {
                    if same_dir {
                        continue;
                    }
                    let dst = if conflict && policy == ConflictPolicy::Replace {
                        plain
                    } else {
                        unique_path(&dest, &name)
                    };
                    move_path_c(src, &dst, &prog, &cancel_t)?;
                    record_pair(&prog, src, &dst);
                } else {
                    let dst = if same_dir {
                        copy_name(&dest, &name)
                    } else if conflict && policy == ConflictPolicy::Replace {
                        plain
                    } else {
                        unique_path(&dest, &name)
                    };
                    copy_recursive_c(src, &dst, &prog, &cancel_t)?;
                    record_pair(&prog, src, &dst);
                }
            }
            Ok(())
        })();
        finish(&prog, result.map_err(|e| e.to_string()));
    });
    // Substituir destrói os originais do destino: não há como desfazer com segurança.
    let undo = if policy == ConflictPolicy::Replace {
        UndoTemplate::None
    } else if cut {
        UndoTemplate::Move
    } else {
        UndoTemplate::Copy
    };
    OpJob {
        progress,
        refresh,
        undo,
        push_to: Stack::Undo,
        cancel,
    }
}

/// Exclui itens (lixeira ou permanente).
pub fn start_delete(paths: Vec<PathBuf>, permanent: bool) -> OpJob {
    let progress = Arc::new(Mutex::new(OpProgress {
        label: if permanent {
            "Excluindo".to_owned()
        } else {
            "Movendo para a Lixeira".to_owned()
        },
        total: paths.len().max(1),
        ..Default::default()
    }));
    let refresh: Vec<PathBuf> = paths.iter().filter_map(|p| p.parent().map(Path::to_path_buf)).collect();
    let paths_for_undo = paths.clone();
    let prog = progress.clone();
    std::thread::spawn(move || {
        let result = (|| -> Result<(), String> {
            if permanent {
                for p in &paths {
                    set_current(&prog, p);
                    let r = if p.is_dir() {
                        fs::remove_dir_all(p)
                    } else {
                        fs::remove_file(p)
                    };
                    r.map_err(|e| format!("{}: {e}", p.display()))?;
                    tick(&prog);
                }
            } else {
                trash::delete_all(&paths).map_err(|e| e.to_string())?;
                if let Ok(mut p) = prog.lock() {
                    p.done = p.total;
                }
            }
            Ok(())
        })();
        finish(&prog, result);
    });
    let undo = if permanent {
        UndoTemplate::None
    } else {
        UndoTemplate::Trash(paths_for_undo)
    };
    OpJob {
        progress,
        refresh,
        undo,
        push_to: Stack::Undo,
        cancel: Arc::new(AtomicBool::new(false)),
    }
}

pub fn rename(path: &Path, new_name: &str) -> io::Result<PathBuf> {
    let new_name = new_name.trim();
    if new_name.is_empty() {
        return Err(io::Error::other("O nome não pode ficar em branco."));
    }
    if new_name.contains(['/', '\\']) {
        return Err(io::Error::other("O nome não pode conter / ou \\."));
    }
    let parent = path.parent().ok_or_else(|| io::Error::other("Caminho inválido."))?;
    let dst = parent.join(new_name);
    if dst == path {
        return Ok(dst);
    }
    if dst.exists() && !same_file_case_insensitive(path, &dst) {
        return Err(io::Error::other(format!(
            "Já existe um item chamado \"{new_name}\" neste local."
        )));
    }
    fs::rename(path, &dst)?;
    Ok(dst)
}

fn same_file_case_insensitive(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
}

pub fn create_folder(dir: &Path, name: &str) -> io::Result<PathBuf> {
    let name = if name.trim().is_empty() {
        "Nova pasta"
    } else {
        name.trim()
    };
    let p = unique_path(dir, name);
    fs::create_dir(&p)?;
    Ok(p)
}

pub fn create_file(dir: &Path, name: &str) -> io::Result<PathBuf> {
    let name = if name.trim().is_empty() {
        "Novo Documento de Texto.txt"
    } else {
        name.trim()
    };
    let p = unique_path(dir, name);
    fs::File::create(&p)?;
    Ok(p)
}

fn remove_any(p: &Path) -> io::Result<()> {
    if p.is_dir() {
        fs::remove_dir_all(p)
    } else {
        fs::remove_file(p)
    }
}

/// Restaura da Lixeira os itens cujo caminho original está em `paths`.
fn restore_from_trash(paths: &[PathBuf]) -> Result<(), String> {
    #[cfg(any(windows, all(unix, not(target_os = "macos"))))]
    {
        let items = trash::os_limited::list().map_err(|e| format!("Lixeira: {e}"))?;
        let mut chosen: Vec<trash::TrashItem> = Vec::new();
        for p in paths {
            let best = items
                .iter()
                .filter(|it| it.original_parent.join(&it.name) == *p)
                .max_by_key(|it| it.time_deleted)
                .cloned();
            match best {
                Some(it) => chosen.push(it),
                None => return Err(format!("\"{}\" não foi encontrado na Lixeira.", p.display())),
            }
        }
        trash::os_limited::restore_all(chosen).map_err(|e| format!("Não foi possível restaurar da Lixeira: {e}"))
    }
    #[cfg(not(any(windows, all(unix, not(target_os = "macos")))))]
    {
        let _ = paths;
        Err("Restaurar da Lixeira não é suportado neste sistema.".to_owned())
    }
}

fn touched_dirs(kind: &UndoKind) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = Vec::new();
    let mut add = |p: &Path| {
        if let Some(par) = p.parent() {
            v.push(par.to_path_buf());
        }
    };
    match kind {
        UndoKind::Rename { from, to } => {
            add(from);
            add(to);
        }
        UndoKind::Move { pairs } | UndoKind::Copy { pairs } => {
            for (a, b) in pairs {
                add(a);
                add(b);
            }
        }
        UndoKind::Create { path, .. } => add(path),
        UndoKind::Trash { paths } => paths.iter().for_each(|p| add(p)),
    }
    v
}

/// Executa o inverso de `entry` (desfazer) ou a própria operação de novo (refazer), em segundo plano.
/// Ao terminar com sucesso, a mesma entrada vai para a pilha oposta.
pub fn start_undo_redo(entry: UndoEntry, undo: bool) -> OpJob {
    let label = format!("{} {}", if undo { "Desfazendo" } else { "Refazendo" }, entry.label);
    let progress = Arc::new(Mutex::new(OpProgress {
        label,
        total: 1,
        ..Default::default()
    }));
    let refresh = touched_dirs(&entry.kind);
    let prog = progress.clone();
    let kind = entry.kind.clone();
    std::thread::spawn(move || {
        let result = (|| -> Result<(), String> {
            match (&kind, undo) {
                (UndoKind::Rename { from, to }, true) => {
                    fs::rename(to, from).map_err(|e| format!("Renomear de volta: {e}"))
                }
                (UndoKind::Rename { from, to }, false) => fs::rename(from, to).map_err(|e| format!("Renomear: {e}")),
                (UndoKind::Move { pairs }, _) => {
                    if let Ok(mut p) = prog.lock() {
                        p.total = pairs.len().max(1);
                    }
                    for (src, dst) in pairs {
                        let (a, b) = if undo { (dst, src) } else { (src, dst) };
                        if b.exists() {
                            return Err(format!(
                                "\"{}\" já existe; não foi possível mover de volta.",
                                b.display()
                            ));
                        }
                        move_path(a, b, &prog).map_err(|e| format!("{}: {e}", a.display()))?;
                    }
                    Ok(())
                }
                (UndoKind::Copy { pairs }, true) => {
                    for (_, dst) in pairs {
                        set_current(&prog, dst);
                        if dst.exists() {
                            remove_any(dst).map_err(|e| format!("{}: {e}", dst.display()))?;
                        }
                        tick(&prog);
                    }
                    Ok(())
                }
                (UndoKind::Copy { pairs }, false) => {
                    if let Ok(mut p) = prog.lock() {
                        p.total = count_items(&pairs.iter().map(|(s, _)| s.clone()).collect::<Vec<_>>());
                    }
                    for (src, dst) in pairs {
                        copy_recursive(src, dst, &prog).map_err(|e| format!("{}: {e}", src.display()))?;
                    }
                    Ok(())
                }
                (UndoKind::Create { path, is_dir }, true) => {
                    set_current(&prog, path);
                    if !path.exists() {
                        return Ok(());
                    }
                    let untouched = if *is_dir {
                        fs::read_dir(path).map(|mut r| r.next().is_none()).unwrap_or(false)
                    } else {
                        fs::metadata(path).map(|m| m.len() == 0).unwrap_or(false)
                    };
                    if untouched {
                        remove_any(path).map_err(|e| e.to_string())
                    } else {
                        // Já tem conteúdo: vai para a Lixeira em vez de apagar de vez.
                        trash::delete(path).map_err(|e| e.to_string())
                    }
                }
                (UndoKind::Create { path, is_dir }, false) => {
                    if *is_dir {
                        fs::create_dir(path).map_err(|e| e.to_string())
                    } else {
                        fs::File::create(path).map(|_| ()).map_err(|e| e.to_string())
                    }
                }
                (UndoKind::Trash { paths }, true) => restore_from_trash(paths),
                (UndoKind::Trash { paths }, false) => trash::delete_all(paths).map_err(|e| e.to_string()),
            }
        })();
        if let Ok(mut p) = prog.lock() {
            p.done = p.total;
        }
        finish(&prog, result);
    });
    OpJob {
        progress,
        refresh,
        undo: UndoTemplate::Fixed(entry),
        push_to: if undo { Stack::Redo } else { Stack::Undo },
        cancel: Arc::new(AtomicBool::new(false)),
    }
}

/// Informações para a janela de propriedades.
#[derive(Debug, Default, Clone)]
pub struct PropertiesInfo {
    pub files: usize,
    pub folders: usize,
    pub bytes: u64,
    pub finished: bool,
}

pub fn compute_properties(paths: Vec<PathBuf>) -> Arc<Mutex<PropertiesInfo>> {
    let info = Arc::new(Mutex::new(PropertiesInfo::default()));
    let out = info.clone();
    std::thread::spawn(move || {
        let mut local = PropertiesInfo::default();
        let mut n = 0usize;
        for p in &paths {
            for e in walkdir::WalkDir::new(p).into_iter().filter_map(Result::ok) {
                let Ok(md) = e.metadata() else { continue };
                if md.is_dir() {
                    if e.depth() > 0 {
                        local.folders += 1;
                    }
                } else {
                    local.files += 1;
                    local.bytes += md.len();
                }
                n += 1;
                if n.is_multiple_of(500) {
                    if let Ok(mut o) = out.lock() {
                        *o = local.clone();
                    }
                }
            }
        }
        local.finished = true;
        if let Ok(mut o) = out.lock() {
            *o = local;
        }
    });
    info
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tabex_ops_{}_{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn wait(job: &OpJob) {
        for _ in 0..200 {
            if job.is_finished() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!("operação não terminou");
    }

    #[test]
    fn unique_and_copy_names() {
        let dir = tmp("names");
        fs::write(dir.join("a.txt"), b"x").unwrap();
        assert_eq!(unique_path(&dir, "a.txt"), dir.join("a (2).txt"));
        assert_eq!(unique_path(&dir, "b.txt"), dir.join("b.txt"));
        assert_eq!(copy_name(&dir, "a.txt"), dir.join("a - Cópia.txt"));
        fs::write(dir.join("a - Cópia.txt"), b"x").unwrap();
        assert_eq!(copy_name(&dir, "a.txt"), dir.join("a - Cópia (2).txt"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn create_rename_copy_move_delete() {
        let dir = tmp("flow");
        let folder = create_folder(&dir, "Nova pasta").unwrap();
        assert!(folder.is_dir());
        let again = create_folder(&dir, "Nova pasta").unwrap();
        assert_eq!(again.file_name().unwrap(), "Nova pasta (2)");
        let file = create_file(&folder, "nota.txt").unwrap();
        fs::write(&file, b"conteudo").unwrap();

        let renamed = rename(&file, "renomeado.txt").unwrap();
        assert!(renamed.exists() && !file.exists());
        assert!(rename(&renamed, "").is_err());
        assert!(rename(&renamed, "a/b").is_err());

        // copiar pasta para outra pasta
        let clip = Clipboard {
            paths: vec![folder.clone()],
            cut: false,
        };
        let job = start_paste(&clip, &again);
        wait(&job);
        assert!(again.join("Nova pasta").join("renomeado.txt").exists());

        // copiar na mesma pasta gera "- Cópia"
        let clip = Clipboard {
            paths: vec![renamed.clone()],
            cut: false,
        };
        let job = start_paste(&clip, &folder);
        wait(&job);
        assert!(folder.join("renomeado - Cópia.txt").exists());

        // mover
        let clip = Clipboard {
            paths: vec![folder.join("renomeado - Cópia.txt")],
            cut: true,
        };
        let job = start_paste(&clip, &dir);
        wait(&job);
        assert!(dir.join("renomeado - Cópia.txt").exists());
        assert!(!folder.join("renomeado - Cópia.txt").exists());

        // copiar pasta para dentro dela mesma falha
        let clip = Clipboard {
            paths: vec![folder.clone()],
            cut: false,
        };
        let job = start_paste(&clip, &folder);
        wait(&job);
        assert!(job.progress.lock().unwrap().error.is_some());

        // desfazer/refazer: mover
        let moved = dir.join("renomeado - Cópia.txt");
        let entry = UndoEntry::new(
            "mover",
            UndoKind::Move {
                pairs: vec![(folder.join("renomeado - Cópia.txt"), moved.clone())],
            },
        );
        let job = start_undo_redo(entry.clone(), true);
        wait(&job);
        assert!(job.progress.lock().unwrap().error.is_none());
        assert!(folder.join("renomeado - Cópia.txt").exists() && !moved.exists());
        assert_eq!(job.push_to, Stack::Redo);
        let job = start_undo_redo(entry, false);
        wait(&job);
        assert!(moved.exists());

        // desfazer renomear e criar
        let r = UndoEntry::new(
            "renomear",
            UndoKind::Rename {
                from: file.clone(),
                to: renamed.clone(),
            },
        );
        let job = start_undo_redo(r.clone(), true);
        wait(&job);
        assert!(file.exists() && !renamed.exists());
        let job = start_undo_redo(r, false);
        wait(&job);
        assert!(renamed.exists());
        let created = create_folder(&dir, "Temporaria").unwrap();
        let job = start_undo_redo(
            UndoEntry::new(
                "criar",
                UndoKind::Create {
                    path: created.clone(),
                    is_dir: true,
                },
            ),
            true,
        );
        wait(&job);
        assert!(!created.exists());

        // desfazer copiar remove as cópias
        let clip = Clipboard {
            paths: vec![renamed.clone()],
            cut: false,
        };
        let job = start_paste(&clip, &dir);
        wait(&job);
        let entry = job.undo_entry().expect("entrada de desfazer");
        assert!(matches!(entry.kind, UndoKind::Copy { .. }));
        assert!(dir.join("renomeado.txt").exists());
        let job = start_undo_redo(entry, true);
        wait(&job);
        assert!(!dir.join("renomeado.txt").exists());

        // conflitos: detectar, pular e substituir
        let c1 = dir.join("conf.txt");
        fs::write(&c1, b"novo").unwrap();
        fs::write(folder.join("conf.txt"), b"antigo").unwrap();
        let conflicts = find_conflicts(std::slice::from_ref(&c1), &folder, false);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].name, "conf.txt");
        let clip = Clipboard {
            paths: vec![c1.clone()],
            cut: false,
        };
        let job = start_paste_with(&clip, &folder, ConflictPolicy::Skip);
        wait(&job);
        assert_eq!(fs::read(folder.join("conf.txt")).unwrap(), b"antigo");
        assert!(job.undo_entry().is_none());
        let job = start_paste_with(&clip, &folder, ConflictPolicy::Replace);
        wait(&job);
        assert_eq!(fs::read(folder.join("conf.txt")).unwrap(), b"novo");
        assert!(job.undo_entry().is_none());
        let job = start_paste_with(&clip, &folder, ConflictPolicy::KeepBoth);
        wait(&job);
        assert!(folder.join("conf (2).txt").exists());

        // cancelar: a flag interrompe e marca como cancelado, sem erro
        let big = dir.join("big");
        fs::create_dir_all(&big).unwrap();
        for i in 0..300 {
            fs::write(big.join(format!("f{i}.txt")), b"x").unwrap();
        }
        let clip = Clipboard {
            paths: vec![big.clone()],
            cut: false,
        };
        let job = start_paste_with(&clip, &folder, ConflictPolicy::KeepBoth);
        job.request_cancel();
        wait(&job);
        let p = job.progress.lock().unwrap();
        assert!(p.cancelled || p.error.is_none());
        drop(p);

        // excluir permanentemente
        let job = start_delete(vec![again.clone()], true);
        wait(&job);
        assert!(!again.exists());
        fs::remove_dir_all(&dir).unwrap();
    }
}
