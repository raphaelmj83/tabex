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
    pub fn start(root: PathBuf, text: String, show_hidden: bool, content_mode: bool) -> SearchJob {
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
                        let hit = if content_mode && !query.terms.is_empty() {
                            // Modo conteúdo: os termos livres valem para o nome OU para o texto do arquivo.
                            let name_only = Query {
                                terms: Vec::new(),
                                content: Vec::new(),
                                ..query.clone()
                            };
                            name_only.matches(&fe, &now)
                                && (query.terms.iter().all(|t| fe.name_lower.contains(t.as_str()))
                                    || file_contains(&fe.path, &query.terms))
                                && (query.content.is_empty() || file_contains(&fe.path, &query.content))
                        } else {
                            query.matches(&fe, &now)
                                && (query.content.is_empty() || file_contains(&fe.path, &query.content))
                        };
                        if hit {
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

/// O arquivo de texto contém todos os `terms` (sem diferenciar maiúsculas)? Ignora binários e
/// arquivos acima de 20 MB.
pub fn file_contains(path: &std::path::Path, terms: &[String]) -> bool {
    use std::io::Read;
    const MAX: u64 = 20 * 1024 * 1024;
    let Ok(md) = std::fs::metadata(path) else { return false };
    if md.is_dir() || md.len() > MAX || md.len() == 0 {
        return false;
    }
    let Ok(mut f) = std::fs::File::open(path) else {
        return false;
    };
    let mut head = [0u8; 4096];
    let n = f.read(&mut head).unwrap_or(0);
    if head[..n].contains(&0) {
        return false; // binário
    }
    let mut bytes = head[..n].to_vec();
    if f.read_to_end(&mut bytes).is_err() {
        return false;
    }
    let text = String::from_utf8_lossy(&bytes).to_lowercase();
    terms.iter().all(|t| text.contains(t.as_str()))
}

impl Drop for SearchJob {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn content_search_reads_text_and_skips_binary() {
        let dir = std::env::temp_dir().join(format!("tabex_content_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), "Contrato com Cláusula Penal").unwrap();
        std::fs::write(dir.join("b.bin"), [0u8, 1, 2, 3]).unwrap();
        assert!(super::file_contains(
            &dir.join("a.txt"),
            &["cláusula".to_owned(), "contrato".to_owned()]
        ));
        assert!(!super::file_contains(&dir.join("a.txt"), &["multa".to_owned()]));
        assert!(!super::file_contains(&dir.join("b.bin"), &["\u{1}".to_owned()]));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
