use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Categoria de arquivo, usada para ícone, rótulo de tipo e filtros.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum FileKind {
    Folder,
    Document,
    Spreadsheet,
    Presentation,
    Pdf,
    Image,
    Video,
    Audio,
    Archive,
    Code,
    Executable,
    Font,
    Other,
}

impl FileKind {
    /// Tipos exibidos no menu de filtro (na ordem).
    pub const FILTERABLE: [FileKind; 12] = [
        FileKind::Folder,
        FileKind::Document,
        FileKind::Spreadsheet,
        FileKind::Presentation,
        FileKind::Pdf,
        FileKind::Image,
        FileKind::Video,
        FileKind::Audio,
        FileKind::Archive,
        FileKind::Code,
        FileKind::Executable,
        FileKind::Other,
    ];

    pub fn label(self) -> &'static str {
        match self {
            FileKind::Folder => "Pastas",
            FileKind::Document => "Documentos",
            FileKind::Spreadsheet => "Planilhas",
            FileKind::Presentation => "Apresentações",
            FileKind::Pdf => "PDF",
            FileKind::Image => "Imagens",
            FileKind::Video => "Vídeos",
            FileKind::Audio => "Áudio",
            FileKind::Archive => "Compactados",
            FileKind::Code => "Código",
            FileKind::Executable => "Programas",
            FileKind::Font => "Fontes",
            FileKind::Other => "Outros",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            FileKind::Folder => "📁",
            FileKind::Document => "📝",
            FileKind::Spreadsheet => "📊",
            FileKind::Presentation => "📽",
            FileKind::Pdf => "📕",
            FileKind::Image => "🖼",
            FileKind::Video => "🎬",
            FileKind::Audio => "🎵",
            FileKind::Archive => "📦",
            FileKind::Code => "📜",
            FileKind::Executable => "⚙",
            FileKind::Font => "🔤",
            FileKind::Other => "📄",
        }
    }
}

#[derive(Clone, Debug)]
pub struct FileEntry {
    pub path: PathBuf,
    pub name: String,
    pub name_lower: String,
    pub is_dir: bool,
    pub is_hidden: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub ext: String,
    pub kind: FileKind,
    pub type_label: String,
}

impl FileEntry {
    pub fn from_path(path: &Path) -> Option<FileEntry> {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        let md = fs::metadata(path).or_else(|_| fs::symlink_metadata(path)).ok()?;
        Some(Self::from_metadata(path.to_path_buf(), name, &md))
    }

    pub fn from_metadata(path: PathBuf, name: String, md: &fs::Metadata) -> FileEntry {
        let is_dir = md.is_dir();
        let ext = if is_dir {
            String::new()
        } else {
            path.extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                .unwrap_or_default()
        };
        let kind = if is_dir { FileKind::Folder } else { kind_for_ext(&ext) };
        let is_hidden = is_hidden(&name, md);
        FileEntry {
            name_lower: name.to_lowercase(),
            type_label: type_label(&ext, is_dir),
            is_dir,
            is_hidden,
            size: if is_dir { 0 } else { md.len() },
            modified: md.modified().ok(),
            ext,
            kind,
            path,
            name,
        }
    }

    pub fn parent_display(&self) -> String {
        self.path
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}

#[cfg(windows)]
fn is_hidden(_name: &str, md: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const HIDDEN: u32 = 0x2;
    const SYSTEM: u32 = 0x4;
    md.file_attributes() & (HIDDEN | SYSTEM) != 0
}

#[cfg(not(windows))]
fn is_hidden(name: &str, _md: &fs::Metadata) -> bool {
    name.starts_with('.')
}

/// Lista o conteúdo de um diretório.
pub fn read_dir(path: &Path, show_hidden: bool) -> io::Result<Vec<FileEntry>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(path)? {
        let Ok(entry) = entry else { continue };
        let name = entry.file_name().to_string_lossy().into_owned();
        let p = entry.path();
        // Segue links simbólicos para pastas; se falhar, usa os metadados do próprio link.
        let md = match fs::metadata(&p) {
            Ok(md) => md,
            Err(_) => match entry.metadata() {
                Ok(md) => md,
                Err(_) => continue,
            },
        };
        let fe = FileEntry::from_metadata(p, name, &md);
        if !show_hidden && fe.is_hidden {
            continue;
        }
        out.push(fe);
    }
    Ok(out)
}

pub fn kind_for_ext(ext: &str) -> FileKind {
    match ext {
        "txt" | "md" | "rtf" | "doc" | "docx" | "odt" | "pages" | "tex" | "log" | "wps" => FileKind::Document,
        "xls" | "xlsx" | "xlsm" | "ods" | "csv" | "tsv" | "numbers" => FileKind::Spreadsheet,
        "ppt" | "pptx" | "odp" | "key" => FileKind::Presentation,
        "pdf" => FileKind::Pdf,
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "svg" | "ico" | "tif" | "tiff" | "heic" | "raw" | "psd"
        | "avif" => FileKind::Image,
        "mp4" | "mkv" | "avi" | "mov" | "wmv" | "webm" | "flv" | "m4v" | "mpg" | "mpeg" | "3gp" => FileKind::Video,
        "mp3" | "wav" | "flac" | "aac" | "ogg" | "m4a" | "wma" | "opus" | "aiff" => FileKind::Audio,
        "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" | "zst" | "iso" | "cab" | "tgz" => FileKind::Archive,
        "rs" | "py" | "js" | "ts" | "tsx" | "jsx" | "java" | "c" | "cpp" | "h" | "hpp" | "cs" | "go" | "rb" | "php"
        | "html" | "htm" | "css" | "scss" | "json" | "xml" | "yaml" | "yml" | "toml" | "sh" | "ps1" | "bat" | "cmd"
        | "sql" | "kt" | "swift" | "lua" | "ipynb" | "vue" | "dart" => FileKind::Code,
        "exe" | "msi" | "app" | "dmg" | "deb" | "rpm" | "appimage" | "apk" | "com" | "dll" | "so" => {
            FileKind::Executable
        }
        "ttf" | "otf" | "woff" | "woff2" => FileKind::Font,
        _ => FileKind::Other,
    }
}

/// Rótulo de tipo no estilo do Explorador ("Arquivo Fonte Python", "Pasta de arquivos"...).
pub fn type_label(ext: &str, is_dir: bool) -> String {
    if is_dir {
        return "Pasta de arquivos".to_owned();
    }
    let known = match ext {
        "" => Some("Arquivo"),
        "txt" => Some("Documento de Texto"),
        "md" => Some("Arquivo MD"),
        "pdf" => Some("Documento PDF"),
        "doc" | "docx" => Some("Documento do Microsoft Word"),
        "xls" | "xlsx" | "xlsm" => Some("Planilha do Microsoft Excel"),
        "ppt" | "pptx" => Some("Apresentação do Microsoft PowerPoint"),
        "csv" => Some("Arquivo de Valores Separados por Vírgulas"),
        "py" => Some("Arquivo Fonte Python"),
        "rs" => Some("Arquivo Fonte Rust"),
        "js" => Some("Arquivo JavaScript"),
        "ts" => Some("Arquivo TypeScript"),
        "ps1" => Some("Script do Windows PowerShell"),
        "bat" | "cmd" => Some("Arquivo em Lotes do Windows"),
        "sh" => Some("Script Shell"),
        "json" => Some("Arquivo JSON"),
        "xml" => Some("Documento XML"),
        "html" | "htm" => Some("Documento HTML"),
        "css" => Some("Folha de Estilos em Cascata"),
        "zip" => Some("Pasta compactada"),
        "rar" => Some("Arquivo WinRAR"),
        "7z" => Some("Arquivo 7-Zip"),
        "exe" => Some("Aplicativo"),
        "msi" => Some("Pacote do Windows Installer"),
        "dll" => Some("Extensão de aplicativo"),
        "lnk" => Some("Atalho"),
        "png" => Some("Imagem PNG"),
        "jpg" | "jpeg" => Some("Imagem JPEG"),
        "gif" => Some("Imagem GIF"),
        "svg" => Some("Imagem SVG"),
        "webp" => Some("Imagem WebP"),
        "bmp" => Some("Imagem de bitmap"),
        "mp3" => Some("Áudio MP3"),
        "wav" => Some("Áudio WAV"),
        "flac" => Some("Áudio FLAC"),
        "mp4" => Some("Vídeo MP4"),
        "mkv" => Some("Vídeo Matroska"),
        "avi" => Some("Vídeo AVI"),
        "mov" => Some("Filme QuickTime"),
        "iso" => Some("Arquivo de Imagem de Disco"),
        "ttf" => Some("Arquivo de fonte TrueType"),
        "otf" => Some("Arquivo de fonte OpenType"),
        "toml" => Some("Arquivo TOML"),
        "yaml" | "yml" => Some("Arquivo YAML"),
        "sql" => Some("Arquivo SQL"),
        "log" => Some("Documento de Texto (log)"),
        "ipynb" => Some("Jupyter Notebook"),
        _ => None,
    };
    match known {
        Some(k) => k.to_owned(),
        None => format!("Arquivo {}", ext.to_uppercase()),
    }
}

/// Tamanho no estilo do Explorador: sempre em KB, arredondado para cima ("29 KB").
pub fn format_size_kb(size: u64) -> String {
    let kb = size.div_ceil(1024);
    format!("{} KB", thousands(kb))
}

/// Tamanho legível (B, KB, MB, GB...).
pub fn format_size_human(size: u64) -> String {
    use humansize::{format_size, DECIMAL};
    format_size(size, DECIMAL)
}

pub fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push('.');
        }
        out.push(ch);
    }
    out
}

pub fn format_date(t: Option<SystemTime>) -> String {
    match t {
        Some(t) => {
            let dt: chrono::DateTime<chrono::Local> = t.into();
            dt.format("%d/%m/%Y %H:%M").to_string()
        }
        None => String::new(),
    }
}

/// Comparação "natural": "arquivo2" < "arquivo10".
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut ai = a.chars().peekable();
    let mut bi = b.chars().peekable();
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(ca), Some(cb)) => {
                if ca.is_ascii_digit() && cb.is_ascii_digit() {
                    let mut na = String::new();
                    while let Some(c) = ai.peek().copied().filter(|c| c.is_ascii_digit()) {
                        na.push(c);
                        ai.next();
                    }
                    let mut nb = String::new();
                    while let Some(c) = bi.peek().copied().filter(|c| c.is_ascii_digit()) {
                        nb.push(c);
                        bi.next();
                    }
                    let ta = na.trim_start_matches('0');
                    let tb = nb.trim_start_matches('0');
                    let ord = ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb));
                    if ord != Ordering::Equal {
                        return ord;
                    }
                } else {
                    let ord = ca.cmp(&cb);
                    if ord != Ordering::Equal {
                        return ord;
                    }
                    ai.next();
                    bi.next();
                }
            }
        }
    }
}

/// Lista de unidades/raízes para a seção "Este Computador".
pub fn list_drives() -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    #[cfg(windows)]
    {
        for c in b'A'..=b'Z' {
            let letter = c as char;
            let root = format!("{letter}:\\");
            if Path::new(&root).exists() {
                let label = match letter {
                    'C' => format!("Disco Local ({letter}:)"),
                    _ => format!("Unidade ({letter}:)"),
                };
                out.push((label, PathBuf::from(root)));
            }
        }
    }
    #[cfg(not(windows))]
    {
        out.push(("Sistema de arquivos (/)".to_owned(), PathBuf::from("/")));
        let user = std::env::var("USER").unwrap_or_default();
        let candidates = [
            PathBuf::from("/media").join(&user),
            PathBuf::from("/run/media").join(&user),
            PathBuf::from("/mnt"),
            PathBuf::from("/Volumes"),
        ];
        for dir in candidates {
            if let Ok(rd) = fs::read_dir(&dir) {
                for e in rd.flatten() {
                    if e.path().is_dir() {
                        let name = e.file_name().to_string_lossy().into_owned();
                        out.push((name, e.path()));
                    }
                }
            }
        }
    }
    out
}

/// Pastas padrão do usuário para a seção "Acesso rápido".
pub fn standard_folders() -> Vec<(&'static str, PathBuf)> {
    let mut v = Vec::new();
    if let Some(p) = dirs::desktop_dir() {
        v.push(("Área de Trabalho", p));
    }
    if let Some(p) = dirs::download_dir() {
        v.push(("Downloads", p));
    }
    if let Some(p) = dirs::document_dir() {
        v.push(("Documentos", p));
    }
    if let Some(p) = dirs::picture_dir() {
        v.push(("Imagens", p));
    }
    if let Some(p) = dirs::audio_dir() {
        v.push(("Músicas", p));
    }
    if let Some(p) = dirs::video_dir() {
        v.push(("Vídeos", p));
    }
    if let Some(p) = dirs::home_dir() {
        v.push(("Pasta pessoal", p));
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_order_handles_numbers() {
        assert_eq!(natural_cmp("arquivo2", "arquivo10"), Ordering::Less);
        assert_eq!(natural_cmp("build_v9", "build_v11"), Ordering::Less);
        assert_eq!(natural_cmp("a", "a"), Ordering::Equal);
        assert_eq!(natural_cmp("b", "a"), Ordering::Greater);
        assert_eq!(natural_cmp("007", "7"), Ordering::Equal);
    }

    #[test]
    fn sizes_round_up_like_explorer() {
        assert_eq!(format_size_kb(0), "0 KB");
        assert_eq!(format_size_kb(1), "1 KB");
        assert_eq!(format_size_kb(1024), "1 KB");
        assert_eq!(format_size_kb(1025), "2 KB");
        assert_eq!(format_size_kb(29 * 1024), "29 KB");
        assert_eq!(format_size_kb(1_500_000), "1.465 KB");
    }

    #[test]
    fn thousands_separator_pt_br() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1000), "1.000");
        assert_eq!(thousands(1234567), "1.234.567");
    }

    #[test]
    fn kinds_and_labels() {
        assert_eq!(kind_for_ext("py"), FileKind::Code);
        assert_eq!(kind_for_ext("docx"), FileKind::Document);
        assert_eq!(kind_for_ext("xyz"), FileKind::Other);
        assert_eq!(type_label("py", false), "Arquivo Fonte Python");
        assert_eq!(type_label("", true), "Pasta de arquivos");
        assert_eq!(type_label("abc", false), "Arquivo ABC");
    }

    #[test]
    fn read_dir_lists_entries() {
        let dir = std::env::temp_dir().join(format!("tabex_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("sub")).unwrap();
        fs::write(dir.join("a.txt"), b"hello").unwrap();
        fs::write(dir.join(".hidden"), b"").unwrap();
        let visible = read_dir(&dir, false).unwrap();
        let all = read_dir(&dir, true).unwrap();
        assert_eq!(visible.len(), 2);
        assert!(all.len() >= 2);
        let a = visible.iter().find(|e| e.name == "a.txt").unwrap();
        assert_eq!(a.size, 5);
        assert!(!a.is_dir);
        assert!(visible.iter().any(|e| e.name == "sub" && e.is_dir));
        fs::remove_dir_all(&dir).unwrap();
    }
}
