use super::entry::{FileEntry, FileKind};
use chrono::{DateTime, Datelike, Local, NaiveDate};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::time::SystemTime;

/// Filtro por data de modificação (mesmas faixas do Explorador).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DateFilter {
    #[default]
    Any,
    Today,
    Yesterday,
    ThisWeek,
    LastWeek,
    ThisMonth,
    LastMonth,
    ThisYear,
    LongAgo,
}

impl DateFilter {
    pub const ALL: [DateFilter; 9] = [
        DateFilter::Any,
        DateFilter::Today,
        DateFilter::Yesterday,
        DateFilter::ThisWeek,
        DateFilter::LastWeek,
        DateFilter::ThisMonth,
        DateFilter::LastMonth,
        DateFilter::ThisYear,
        DateFilter::LongAgo,
    ];

    pub fn label(self) -> &'static str {
        match self {
            DateFilter::Any => "Qualquer data",
            DateFilter::Today => "Hoje",
            DateFilter::Yesterday => "Ontem",
            DateFilter::ThisWeek => "Esta semana",
            DateFilter::LastWeek => "Semana passada",
            DateFilter::ThisMonth => "Este mês",
            DateFilter::LastMonth => "Mês passado",
            DateFilter::ThisYear => "Este ano",
            DateFilter::LongAgo => "Há muito tempo",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.to_lowercase().as_str() {
            "hoje" | "today" => DateFilter::Today,
            "ontem" | "yesterday" => DateFilter::Yesterday,
            "semana" | "estasemana" | "week" | "thisweek" => DateFilter::ThisWeek,
            "semanapassada" | "lastweek" => DateFilter::LastWeek,
            "mes" | "mês" | "estemes" | "month" | "thismonth" => DateFilter::ThisMonth,
            "mespassado" | "mêspassado" | "lastmonth" => DateFilter::LastMonth,
            "ano" | "esteano" | "year" | "thisyear" => DateFilter::ThisYear,
            _ => return None,
        })
    }
}

/// Faixa de datas relativa a "hoje"; usada tanto pelo filtro quanto pelo agrupamento.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DateBucket {
    Today,
    Yesterday,
    ThisWeek,
    LastWeek,
    ThisMonth,
    LastMonth,
    ThisYear,
    LongAgo,
    Unknown,
}

impl DateBucket {
    pub fn label(self) -> &'static str {
        match self {
            DateBucket::Today => "Hoje",
            DateBucket::Yesterday => "Ontem",
            DateBucket::ThisWeek => "No início desta semana",
            DateBucket::LastWeek => "Semana passada",
            DateBucket::ThisMonth => "No início deste mês",
            DateBucket::LastMonth => "Mês passado",
            DateBucket::ThisYear => "No início deste ano",
            DateBucket::LongAgo => "Há muito tempo",
            DateBucket::Unknown => "Data desconhecida",
        }
    }

    pub fn of(modified: Option<SystemTime>, now: &DateTime<Local>) -> DateBucket {
        let Some(t) = modified else { return DateBucket::Unknown };
        let dt: DateTime<Local> = t.into();
        let d: NaiveDate = dt.date_naive();
        let today = now.date_naive();
        if d >= today {
            return DateBucket::Today;
        }
        if d == today.pred_opt().unwrap_or(today) {
            return DateBucket::Yesterday;
        }
        // Semana começa no domingo (como no Explorador em pt-BR).
        let days_since_sunday = today.weekday().num_days_from_sunday() as i64;
        let week_start = today - chrono::Duration::days(days_since_sunday);
        if d >= week_start {
            return DateBucket::ThisWeek;
        }
        if d >= week_start - chrono::Duration::days(7) {
            return DateBucket::LastWeek;
        }
        if d.year() == today.year() && d.month() == today.month() {
            return DateBucket::ThisMonth;
        }
        let (ly, lm) = if today.month() == 1 {
            (today.year() - 1, 12)
        } else {
            (today.year(), today.month() - 1)
        };
        if d.year() == ly && d.month() == lm {
            return DateBucket::LastMonth;
        }
        if d.year() == today.year() {
            return DateBucket::ThisYear;
        }
        DateBucket::LongAgo
    }

    pub fn matches(self, f: DateFilter) -> bool {
        match f {
            DateFilter::Any => true,
            DateFilter::Today => self == DateBucket::Today,
            DateFilter::Yesterday => self == DateBucket::Yesterday,
            DateFilter::ThisWeek => self <= DateBucket::ThisWeek,
            DateFilter::LastWeek => self == DateBucket::LastWeek,
            DateFilter::ThisMonth => self <= DateBucket::ThisMonth,
            DateFilter::LastMonth => self == DateBucket::LastMonth,
            DateFilter::ThisYear => self <= DateBucket::ThisYear,
            DateFilter::LongAgo => self == DateBucket::LongAgo,
        }
    }
}

/// Filtro por tamanho (faixas do Explorador).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SizeFilter {
    #[default]
    Any,
    Empty,
    Tiny,
    Small,
    Medium,
    Large,
    Huge,
    Gigantic,
}

impl SizeFilter {
    pub const ALL: [SizeFilter; 8] = [
        SizeFilter::Any,
        SizeFilter::Empty,
        SizeFilter::Tiny,
        SizeFilter::Small,
        SizeFilter::Medium,
        SizeFilter::Large,
        SizeFilter::Huge,
        SizeFilter::Gigantic,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SizeFilter::Any => "Qualquer tamanho",
            SizeFilter::Empty => "Vazio (0 KB)",
            SizeFilter::Tiny => "Minúsculo (0 - 16 KB)",
            SizeFilter::Small => "Pequeno (16 KB - 1 MB)",
            SizeFilter::Medium => "Médio (1 - 128 MB)",
            SizeFilter::Large => "Grande (128 MB - 1 GB)",
            SizeFilter::Huge => "Enorme (1 - 4 GB)",
            SizeFilter::Gigantic => "Gigantesco (> 4 GB)",
        }
    }

    pub fn matches(self, size: u64, is_dir: bool) -> bool {
        const KB: u64 = 1024;
        const MB: u64 = 1024 * KB;
        const GB: u64 = 1024 * MB;
        if self == SizeFilter::Any {
            return true;
        }
        if is_dir {
            return false;
        }
        match self {
            SizeFilter::Any => true,
            SizeFilter::Empty => size == 0,
            SizeFilter::Tiny => size > 0 && size <= 16 * KB,
            SizeFilter::Small => size > 16 * KB && size <= MB,
            SizeFilter::Medium => size > MB && size <= 128 * MB,
            SizeFilter::Large => size > 128 * MB && size <= GB,
            SizeFilter::Huge => size > GB && size <= 4 * GB,
            SizeFilter::Gigantic => size > 4 * GB,
        }
    }
}

/// Filtros rápidos do menu "Filtrar".
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct QuickFilter {
    pub kinds: BTreeSet<FileKind>,
    pub date: DateFilter,
    pub size: SizeFilter,
}

impl QuickFilter {
    pub fn is_active(&self) -> bool {
        !self.kinds.is_empty() || self.date != DateFilter::Any || self.size != SizeFilter::Any
    }

    pub fn clear(&mut self) {
        *self = QuickFilter::default();
    }

    pub fn matches(&self, e: &FileEntry, now: &DateTime<Local>) -> bool {
        if !self.kinds.is_empty() && !self.kinds.contains(&e.kind) {
            return false;
        }
        if self.date != DateFilter::Any && !DateBucket::of(e.modified, now).matches(self.date) {
            return false;
        }
        if !self.size.matches(e.size, e.is_dir) {
            return false;
        }
        true
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KindSel {
    Folder,
    File,
    Kind(FileKind),
}

/// Consulta digitada na caixa de pesquisa. Suporta termos livres e prefixos:
/// `ext:pdf`, `tipo:pasta`, `tipo:arquivo`, `tipo:imagem`, `tamanho:>1mb`, `tamanho:<500kb`, `data:hoje`.
#[derive(Clone, Debug, Default)]
pub struct Query {
    pub terms: Vec<String>,
    /// Termos que devem aparecer no conteúdo do arquivo (`conteudo:palavra`).
    pub content: Vec<String>,
    pub exts: Vec<String>,
    pub kind: Option<KindSel>,
    pub min_size: Option<u64>,
    pub max_size: Option<u64>,
    pub date: Option<DateFilter>,
}

impl Query {
    pub fn parse(text: &str) -> Query {
        let mut q = Query::default();
        for raw in text.split_whitespace() {
            let tok = raw.to_lowercase();
            if let Some((k, v)) = tok.split_once(':') {
                match k {
                    "ext" | "extensao" | "extensão" => {
                        q.exts
                            .extend(v.split(',').map(|s| s.trim_start_matches('.').to_owned()));
                        continue;
                    }
                    "tipo" | "type" | "kind" => {
                        q.kind = match v {
                            "pasta" | "pastas" | "folder" | "dir" => Some(KindSel::Folder),
                            "arquivo" | "arquivos" | "file" => Some(KindSel::File),
                            "imagem" | "imagens" | "image" => Some(KindSel::Kind(FileKind::Image)),
                            "video" | "vídeo" | "videos" => Some(KindSel::Kind(FileKind::Video)),
                            "audio" | "áudio" | "musica" | "música" => Some(KindSel::Kind(FileKind::Audio)),
                            "doc" | "documento" | "documentos" => Some(KindSel::Kind(FileKind::Document)),
                            "planilha" | "planilhas" => Some(KindSel::Kind(FileKind::Spreadsheet)),
                            "pdf" => Some(KindSel::Kind(FileKind::Pdf)),
                            "zip" | "compactado" | "compactados" => Some(KindSel::Kind(FileKind::Archive)),
                            "codigo" | "código" | "code" => Some(KindSel::Kind(FileKind::Code)),
                            "programa" | "exe" => Some(KindSel::Kind(FileKind::Executable)),
                            _ => None,
                        };
                        continue;
                    }
                    "tamanho" | "size" => {
                        if let Some(rest) = v.strip_prefix('>') {
                            q.min_size = parse_size(rest);
                        } else if let Some(rest) = v.strip_prefix('<') {
                            q.max_size = parse_size(rest);
                        } else if let Some(n) = parse_size(v) {
                            q.min_size = Some(n);
                        }
                        continue;
                    }
                    "data" | "date" | "modificado" => {
                        q.date = DateFilter::parse(v);
                        continue;
                    }
                    "conteudo" | "conteúdo" | "texto" | "content" => {
                        if !v.is_empty() {
                            q.content.push(v.to_owned());
                        }
                        continue;
                    }
                    _ => {}
                }
            }
            q.terms.push(tok);
        }
        q
    }

    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
            && self.content.is_empty()
            && self.exts.is_empty()
            && self.kind.is_none()
            && self.min_size.is_none()
            && self.max_size.is_none()
            && self.date.is_none()
    }

    pub fn matches(&self, e: &FileEntry, now: &DateTime<Local>) -> bool {
        for t in &self.terms {
            let ok = if t.contains(['*', '?']) {
                wildcard_match(t, &e.name_lower)
            } else {
                e.name_lower.contains(t.as_str())
            };
            if !ok {
                return false;
            }
        }
        if !self.exts.is_empty() && !self.exts.contains(&e.ext) {
            return false;
        }
        match self.kind {
            Some(KindSel::Folder) if !e.is_dir => return false,
            Some(KindSel::File) if e.is_dir => return false,
            Some(KindSel::Kind(k)) if e.kind != k => return false,
            _ => {}
        }
        if let Some(min) = self.min_size {
            if e.is_dir || e.size < min {
                return false;
            }
        }
        if let Some(max) = self.max_size {
            if e.is_dir || e.size > max {
                return false;
            }
        }
        if let Some(d) = self.date {
            if !DateBucket::of(e.modified, now).matches(d) {
                return false;
            }
        }
        true
    }
}

/// Curinga no estilo do Explorador: `*` = qualquer sequência, `?` = um caractere.
/// A comparação é feita sobre o nome inteiro (`*.pdf`, `relat*`, `foto_??.jpg`).
pub fn wildcard_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star_p, mut star_t): (Option<usize>, usize) = (None, 0);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star_p = Some(pi);
            star_t = ti;
            pi += 1;
        } else if let Some(sp) = star_p {
            pi = sp + 1;
            star_t += 1;
            ti = star_t;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

fn parse_size(s: &str) -> Option<u64> {
    let s = s.trim();
    let idx = s.find(|c: char| !c.is_ascii_digit() && c != '.' && c != ',')?;
    let (num, unit) = s.split_at(idx);
    let n: f64 = num.replace(',', ".").parse().ok()?;
    let mult = match unit.trim() {
        "" | "b" => 1.0,
        "k" | "kb" => 1024.0,
        "m" | "mb" => 1024.0 * 1024.0,
        "g" | "gb" => 1024.0 * 1024.0 * 1024.0,
        "t" | "tb" => 1024.0_f64.powi(4),
        _ => return None,
    };
    Some((n * mult) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::Duration;

    fn entry(name: &str, is_dir: bool, size: u64, age_days: u64) -> FileEntry {
        let ext = if is_dir {
            String::new()
        } else {
            name.rsplit('.').next().unwrap_or("").to_lowercase()
        };
        FileEntry {
            path: PathBuf::from(name),
            name: name.to_owned(),
            name_lower: name.to_lowercase(),
            is_dir,
            is_hidden: false,
            size,
            modified: Some(SystemTime::now() - Duration::from_secs(age_days * 86_400)),
            kind: if is_dir {
                FileKind::Folder
            } else {
                crate::fs::entry::kind_for_ext(&ext)
            },
            type_label: crate::fs::entry::type_label(&ext, is_dir),
            ext,
        }
    }

    #[test]
    fn query_parses_prefixes() {
        let q = Query::parse("relatorio ext:pdf,docx tipo:arquivo tamanho:>1mb data:hoje conteudo:cláusula");
        assert_eq!(q.terms, vec!["relatorio".to_owned()]);
        assert_eq!(q.content, vec!["cláusula".to_owned()]);
        assert_eq!(q.exts, vec!["pdf".to_owned(), "docx".to_owned()]);
        assert_eq!(q.kind, Some(KindSel::File));
        assert_eq!(q.min_size, Some(1024 * 1024));
        assert_eq!(q.date, Some(DateFilter::Today));
        assert!(Query::parse("").is_empty());
    }

    #[test]
    fn query_matches_entries() {
        let now = Local::now();
        let pdf = entry("Relatorio Final.pdf", false, 2 * 1024 * 1024, 0);
        let py = entry("build.py", false, 10, 40);
        let dir = entry("Pasta", true, 0, 1);

        assert!(Query::parse("relat").matches(&pdf, &now));
        assert!(!Query::parse("relat").matches(&py, &now));
        assert!(Query::parse("ext:pdf").matches(&pdf, &now));
        assert!(!Query::parse("ext:pdf").matches(&py, &now));
        assert!(Query::parse("tipo:pasta").matches(&dir, &now));
        assert!(!Query::parse("tipo:pasta").matches(&pdf, &now));
        assert!(Query::parse("tamanho:>1mb").matches(&pdf, &now));
        assert!(!Query::parse("tamanho:>1mb").matches(&py, &now));
        assert!(Query::parse("tamanho:<1kb").matches(&py, &now));
        assert!(Query::parse("data:hoje").matches(&pdf, &now));
        assert!(!Query::parse("data:hoje").matches(&py, &now));
        assert!(Query::parse("tipo:codigo").matches(&py, &now));
    }

    #[test]
    fn wildcards_like_explorer() {
        assert!(wildcard_match("*.pdf", "relatorio.pdf"));
        assert!(!wildcard_match("*.pdf", "relatorio.pdf.bak"));
        assert!(wildcard_match("*.exe", "setup.exe"));
        assert!(wildcard_match("relat*", "relatorio final.pdf"));
        assert!(wildcard_match("foto_??.jpg", "foto_01.jpg"));
        assert!(!wildcard_match("foto_??.jpg", "foto_001.jpg"));
        assert!(wildcard_match("*", "qualquer"));
        let now = Local::now();
        let exe = entry("Setup.EXE", false, 10, 0);
        let pdf = entry("a.pdf", false, 10, 0);
        assert!(Query::parse("*.exe").matches(&exe, &now));
        assert!(!Query::parse("*.exe").matches(&pdf, &now));
        assert!(Query::parse("*.pdf").matches(&pdf, &now));
    }

    #[test]
    fn date_buckets() {
        let now = Local::now();
        assert_eq!(DateBucket::of(Some(SystemTime::now()), &now), DateBucket::Today);
        let old = SystemTime::now() - Duration::from_secs(800 * 86_400);
        assert_eq!(DateBucket::of(Some(old), &now), DateBucket::LongAgo);
        assert_eq!(DateBucket::of(None, &now), DateBucket::Unknown);
        assert!(DateBucket::Today.matches(DateFilter::ThisWeek));
        assert!(DateBucket::Today.matches(DateFilter::ThisYear));
        assert!(!DateBucket::LongAgo.matches(DateFilter::ThisYear));
    }

    #[test]
    fn quick_filter_combines_criteria() {
        let now = Local::now();
        let mut f = QuickFilter::default();
        assert!(!f.is_active());
        f.kinds.insert(FileKind::Pdf);
        f.size = SizeFilter::Medium;
        let pdf = entry("a.pdf", false, 2 * 1024 * 1024, 0);
        let small_pdf = entry("b.pdf", false, 100, 0);
        let py = entry("c.py", false, 2 * 1024 * 1024, 0);
        assert!(f.matches(&pdf, &now));
        assert!(!f.matches(&small_pdf, &now));
        assert!(!f.matches(&py, &now));
        f.clear();
        assert!(!f.is_active());
    }

    #[test]
    fn size_filter_ranges() {
        assert!(SizeFilter::Empty.matches(0, false));
        assert!(SizeFilter::Tiny.matches(10 * 1024, false));
        assert!(SizeFilter::Small.matches(500 * 1024, false));
        assert!(SizeFilter::Gigantic.matches(5 * 1024 * 1024 * 1024, false));
        assert!(!SizeFilter::Tiny.matches(0, true));
        assert!(SizeFilter::Any.matches(0, true));
    }
}
