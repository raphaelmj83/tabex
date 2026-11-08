use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

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
}

/// Operação de arquivo em segundo plano (copiar, mover, excluir).
pub struct OpJob {
    pub progress: Arc<Mutex<OpProgress>>,
    /// Pastas que devem ser recarregadas ao terminar.
    pub refresh: Vec<PathBuf>,
}

impl OpJob {
    pub fn is_finished(&self) -> bool {
        self.progress.lock().map(|p| p.finished).unwrap_or(true)
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
    set_current(progress, src);
    let md = fs::metadata(src)?;
    if md.is_dir() {
        fs::create_dir_all(dst)?;
        tick(progress);
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            copy_recursive(&entry.path(), &dst.join(entry.file_name()), progress)?;
        }
    } else {
        fs::copy(src, dst)?;
        tick(progress);
    }
    Ok(())
}

fn move_path(src: &Path, dst: &Path, progress: &Arc<Mutex<OpProgress>>) -> io::Result<()> {
    set_current(progress, src);
    match fs::rename(src, dst) {
        Ok(()) => {
            tick(progress);
            Ok(())
        }
        Err(_) => {
            // Provavelmente outro volume: copia e remove.
            copy_recursive(src, dst, progress)?;
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

/// Cola os itens da área de transferência em `dest`.
pub fn start_paste(clip: &Clipboard, dest: &Path) -> OpJob {
    let paths = clip.paths.clone();
    let cut = clip.cut;
    let dest = dest.to_path_buf();
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
                if cut {
                    if same_dir {
                        continue;
                    }
                    let dst = unique_path(&dest, &name);
                    move_path(src, &dst, &prog)?;
                } else {
                    let dst = if same_dir {
                        copy_name(&dest, &name)
                    } else {
                        unique_path(&dest, &name)
                    };
                    copy_recursive(src, &dst, &prog)?;
                }
            }
            Ok(())
        })();
        if let Ok(mut p) = prog.lock() {
            p.finished = true;
            if let Err(e) = result {
                p.error = Some(e.to_string());
            }
        }
    });
    OpJob { progress, refresh }
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
        if let Ok(mut p) = prog.lock() {
            p.finished = true;
            if let Err(e) = result {
                p.error = Some(e);
            }
        }
    });
    OpJob { progress, refresh }
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

        // excluir permanentemente
        let job = start_delete(vec![again.clone()], true);
        wait(&job);
        assert!(!again.exists());
        fs::remove_dir_all(&dir).unwrap();
    }
}
