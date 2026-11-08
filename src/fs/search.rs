use super::entry::FileEntry;
use super::filter::Query;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant};
use walkdir::WalkDir;

/// Pesquisa recursiva em segundo plano.
pub struct SearchJob {
    #[allow(dead_code)]
    pub root: PathBuf,
    #[allow(dead_code)]
    pub text: String,
    pub hits: Vec<FileEntry>,
    pub done: bool,
    scanned: Arc<AtomicUsize>,
    cancel: Arc<AtomicBool>,
    rx: Receiver<Vec<FileEntry>>,
}

impl SearchJob {
    pub fn start(root: PathBuf, text: String, show_hidden: bool) -> SearchJob {
        let query = Query::parse(&text);
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let scanned = Arc::new(AtomicUsize::new(0));
        {
            let root = root.clone();
            let cancel = cancel.clone();
            let scanned = scanned.clone();
            std::thread::Builder::new()
                .name("tabex-search".into())
                .spawn(move || {
                    let now = chrono::Local::now();
                    let mut batch = Vec::new();
                    let mut last_flush = Instant::now();
                    let walker = WalkDir::new(&root)
                        .follow_links(false)
                        .min_depth(1)
                        .into_iter()
                        .filter_entry(|e| show_hidden || !e.file_name().to_string_lossy().starts_with('.'));
                    for entry in walker {
                        if cancel.load(Ordering::Relaxed) {
                            return;
                        }
                        let Ok(entry) = entry else { continue };
                        scanned.fetch_add(1, Ordering::Relaxed);
                        let Ok(md) = entry.metadata() else { continue };
                        let name = entry.file_name().to_string_lossy().into_owned();
                        let fe = FileEntry::from_metadata(entry.path().to_path_buf(), name, &md);
                        if !show_hidden && fe.is_hidden {
                            continue;
                        }
                        if query.matches(&fe, &now) {
                            batch.push(fe);
                        }
                        if batch.len() >= 200 || last_flush.elapsed() > Duration::from_millis(150) {
                            if !batch.is_empty() && tx.send(std::mem::take(&mut batch)).is_err() {
                                return;
                            }
                            last_flush = Instant::now();
                        }
                    }
                    if !batch.is_empty() {
                        let _ = tx.send(batch);
                    }
                })
                .expect("falha ao iniciar thread de pesquisa");
        }
        SearchJob {
            root,
            text,
            hits: Vec::new(),
            done: false,
            scanned,
            cancel,
            rx,
        }
    }

    /// Recebe novos resultados. Retorna `true` se algo mudou.
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        loop {
            match self.rx.try_recv() {
                Ok(batch) => {
                    self.hits.extend(batch);
                    changed = true;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    if !self.done {
                        self.done = true;
                        changed = true;
                    }
                    break;
                }
            }
        }
        changed
    }

    pub fn scanned(&self) -> usize {
        self.scanned.load(Ordering::Relaxed)
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Drop for SearchJob {
    fn drop(&mut self) {
        self.cancel();
    }
}
