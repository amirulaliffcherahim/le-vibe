//! Pure-Rust Hybrid Vector + BM25 RAG Memory Engine (v2) for le-vibe.
//!
//! - Markdown is the sole source of truth (`MEMORY.md` + `daily/YYYY-MM-DD.md`).
//! - Project memory is stored centrally under `~/.le-vibe/projects/<git-root-hash>/memory/`
//!   (zero git noise); global memory lives under `~/.le-vibe/memory/`.
//! - 512-dim signed feature-hashed vectors quantized to `i8` with per-vector `f32` scale
//!   (516 bytes/chunk), cached in `rag-cache.bin` and validated by `(path, mtime, len)`.
//! - Hybrid Lucene-IDF BM25 + Cosine Vector + Reciprocal Rank Fusion (RRF),
//!   with pre-RRF hard abstention, daily recency decay, adaptive inlining, and upsert/delete.

use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const VEC_DIM: usize = 512;
const CACHE_MAGIC: &[u8; 4] = b"LVRM";
const CACHE_VERSION: u16 = 2;
const TOKENIZER_VERSION: u16 = 2;
const HARD_ABSTAIN_COS: f32 = 0.14;
const ADAPTIVE_INLINE_MAX_CHARS: usize = 350;
const SEARCH_MAX_CHARS: usize = 2500;

#[derive(Debug, Clone)]
pub struct MemoryEntry {
    pub id: String,
    pub key: Option<String>,
    pub title: String,
    pub keywords: Vec<String>,
    pub body: String,
    pub source: String,
    pub is_daily: bool,
    pub timestamp: u64,
}

#[derive(Debug, Clone)]
pub struct QuantizedVec {
    pub scale: f32,
    pub data: Vec<i8>,
}

#[derive(Debug, Clone)]
pub struct RagHit {
    pub id: String,
    pub source: String,
    pub title: String,
    pub score: f32,
    pub cosine: f32,
    pub bm25: f32,
    pub body: String,
}

fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn hex_hash_16(text: &str) -> String {
    let h1 = fnv1a_64(text.as_bytes());
    let h2 = fnv1a_64(format!("{text}#v2").as_bytes());
    format!("{h1:016x}{h2:016x}")[..16].to_string()
}

/// Dual-emit tokenizer: preserves full identifiers (`max_batch_limit`, `getUserToken`, `#tag`)
/// AND emits split sub-tokens for `snake_case`, `kebab-case`, and `camelCase`/`PascalCase`.
pub fn tokenize_dual(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw in text.split(|c: char| !c.is_alphanumeric() && c != '_' && c != '#' && c != '-') {
        if raw.len() < 2 {
            continue;
        }
        let lower_full = raw.to_lowercase();
        out.push(lower_full.clone());

        let stripped = raw.trim_start_matches('#');
        if stripped.contains('_') || stripped.contains('-') {
            for part in stripped.split(['_', '-']) {
                if part.len() >= 2 {
                    let p_low = part.to_lowercase();
                    if p_low != lower_full {
                        out.push(p_low);
                    }
                    emit_camel_parts(part, &mut out);
                }
            }
        } else {
            emit_camel_parts(stripped, &mut out);
        }
    }
    out
}

fn emit_camel_parts(word: &str, out: &mut Vec<String>) {
    let chars: Vec<char> = word.chars().collect();
    if chars.len() < 3 {
        return;
    }
    let mut start = 0usize;
    let mut split_found = false;
    for i in 1..chars.len() {
        let prev = chars[i - 1];
        let cur = chars[i];
        let next_is_lower = chars.get(i + 1).is_some_and(|c| c.is_lowercase());
        let boundary = (prev.is_lowercase() && cur.is_uppercase())
            || (prev.is_uppercase() && cur.is_uppercase() && next_is_lower);
        if boundary {
            let seg: String = chars[start..i].iter().collect();
            if seg.len() >= 2 {
                out.push(seg.to_lowercase());
                split_found = true;
            }
            start = i;
        }
    }
    if split_found && start < chars.len() {
        let seg: String = chars[start..].iter().collect();
        if seg.len() >= 2 {
            out.push(seg.to_lowercase());
        }
    }
}

/// 512-dim signed feature-hashed dense vector (word unigrams + char 3-grams) with 1.8x boost on title/keywords.
pub fn embed_weighted(title: &str, keywords: &[String], body: &str) -> Vec<f32> {
    let mut vec = vec![0.0f32; VEC_DIM];
    let mut accumulate = |text: &str, field_boost: f32| {
        for tok in tokenize_dual(text) {
            let h = fnv1a_64(tok.as_bytes());
            let idx = (h as usize) % VEC_DIM;
            let sign = if (h >> 63) == 0 { 1.0 } else { -1.0 };
            vec[idx] += 2.0 * field_boost * sign;

            let chars: Vec<char> = tok.chars().collect();
            if chars.len() >= 3 {
                for win in chars.windows(3) {
                    let tri: String = win.iter().collect();
                    let th = fnv1a_64(tri.as_bytes());
                    let tidx = (th as usize) % VEC_DIM;
                    let tsign = if (th >> 63) == 0 { 1.0 } else { -1.0 };
                    vec[tidx] += 0.65 * field_boost * tsign;
                }
            }
        }
    };

    if !title.is_empty() {
        accumulate(title, 1.8);
    }
    if !keywords.is_empty() {
        accumulate(&keywords.join(" "), 1.8);
    }
    accumulate(body, 1.0);

    let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for v in &mut vec {
            *v /= norm;
        }
    }
    vec
}

pub fn quantize_i8(v: &[f32]) -> QuantizedVec {
    let scale = v
        .iter()
        .map(|x| x.abs())
        .fold(0.0_f32, f32::max)
        .max(1e-8);
    let inv = 127.0 / scale;
    let data: Vec<i8> = v
        .iter()
        .map(|x| (x * inv).round().clamp(-127.0, 127.0) as i8)
        .collect();
    QuantizedVec { scale, data }
}

pub fn cosine_f32_i8(query: &[f32], qv: &QuantizedVec) -> f32 {
    if query.len() != qv.data.len() {
        return 0.0;
    }
    let dot: f32 = query
        .iter()
        .zip(qv.data.iter())
        .map(|(q, &d)| q * (d as f32))
        .sum();
    dot * (qv.scale / 127.0)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = path.with_extension(format!("tmp.{ts}"));
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    if let Err(err) = fs::rename(&tmp, path) {
        // Windows rename fails if destination is transiently locked; fallback copy+remove.
        let _ = fs::write(path, bytes);
        let _ = fs::remove_file(&tmp);
        if !path.exists() {
            return Err(err);
        }
    }
    Ok(())
}

pub fn resolve_git_root(start: &Path) -> PathBuf {
    let canon = fs::canonicalize(start).unwrap_or_else(|_| start.to_path_buf());
    let mut cur = canon.as_path();
    loop {
        if cur.join(".git").exists() {
            return cur.to_path_buf();
        }
        let Some(parent) = cur.parent() else {
            return canon;
        };
        cur = parent;
    }
}

pub fn default_global_memory_dir() -> PathBuf {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".le-vibe").join("memory")
}

pub fn default_project_memory_dir(cwd: &Path) -> PathBuf {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let git_root = resolve_git_root(cwd);
    let key = git_root.to_string_lossy().replace('\\', "/").to_lowercase();
    home.join(".le-vibe")
        .join("projects")
        .join(hex_hash_16(&key))
        .join("memory")
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn today_ymd() -> String {
    let secs = now_secs();
    let z = (secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    format!("{year:04}-{m:02}-{d:02}")
}

/// Parses sentinel-delimited entries (`<!-- id:... key:... ts:... keywords:... -->`).
/// Hand-written Markdown without sentinels is parsed as a single deterministic entry.
pub fn parse_markdown_entries(raw: &str, source: &str, is_daily: bool) -> Vec<MemoryEntry> {
    let mut entries = Vec::new();
    let mut cur_meta: Option<(String, Option<String>, u64, Vec<String>)> = None;
    let mut cur_lines: Vec<&str> = Vec::new();
    let mut preamble_lines: Vec<&str> = Vec::new();

    let flush = |meta: (String, Option<String>, u64, Vec<String>),
                 lines: &[&str],
                 out: &mut Vec<MemoryEntry>| {
        let joined = lines.join("\n").trim().to_string();
        if joined.is_empty() {
            return;
        }
        let (title, body) = if let Some(first) = joined.lines().next()
            && let Some(h) = first.trim().strip_prefix("### ")
        {
            let rest = joined
                .lines()
                .skip(1)
                .collect::<Vec<_>>()
                .join("\n")
                .trim()
                .to_string();
            (h.trim().to_string(), if rest.is_empty() { h.trim().to_string() } else { rest })
        } else {
            let first_line: String = joined.lines().next().unwrap_or("").chars().take(72).collect();
            (first_line, joined)
        };
        out.push(MemoryEntry {
            id: meta.0,
            key: meta.1,
            title,
            keywords: meta.3,
            body,
            source: source.to_string(),
            is_daily,
            timestamp: meta.2,
        });
    };

    for line in raw.lines() {
        let trimmed = line.trim();
        if let Some(inner) = trimmed
            .strip_prefix("<!--")
            .and_then(|s| s.strip_suffix("-->"))
            && (inner.contains("id:") || inner.contains("ts:"))
        {
            if let Some(prev) = cur_meta.take() {
                flush(prev, &cur_lines, &mut entries);
                cur_lines.clear();
            }
            let mut id = String::new();
            let mut key: Option<String> = None;
            let mut ts = 0u64;
            let mut keywords = Vec::new();
            for tok in inner.split_whitespace() {
                if let Some(v) = tok.strip_prefix("id:") {
                    id = v.to_string();
                } else if let Some(v) = tok.strip_prefix("key:") {
                    if !v.is_empty() {
                        key = Some(v.to_string());
                    }
                } else if let Some(v) = tok.strip_prefix("ts:") {
                    ts = v.parse::<u64>().unwrap_or(0);
                } else if let Some(v) = tok.strip_prefix("keywords:") {
                    keywords = v
                        .split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(ToString::to_string)
                        .collect();
                }
            }
            if id.is_empty() {
                id = format!("mem_{}", hex_hash_16(inner));
            }
            cur_meta = Some((id, key, ts, keywords));
            continue;
        }
        if cur_meta.is_some() {
            cur_lines.push(line);
        } else {
            preamble_lines.push(line);
        }
    }

    if let Some(prev) = cur_meta.take() {
        flush(prev, &cur_lines, &mut entries);
    }

    let preamble = preamble_lines.join("\n").trim().to_string();
    if !preamble.is_empty() && entries.is_empty() {
        let id = format!("mem_{}", hex_hash_16(&format!("{source}:{preamble}")));
        let title: String = preamble
            .lines()
            .next()
            .unwrap_or("")
            .trim_start_matches('#')
            .trim()
            .chars()
            .take(72)
            .collect();
        entries.push(MemoryEntry {
            id,
            key: None,
            title,
            keywords: Vec::new(),
            body: preamble,
            source: source.to_string(),
            is_daily,
            timestamp: now_secs(),
        });
    }
    entries
}

fn serialize_markdown_entries(entries: &[MemoryEntry]) -> String {
    let mut out = String::new();
    for e in entries {
        let mut header = format!("<!-- id:{}", e.id);
        if let Some(k) = &e.key {
            header.push_str(&format!(" key:{k}"));
        }
        header.push_str(&format!(" ts:{}", e.timestamp));
        if !e.keywords.is_empty() {
            header.push_str(&format!(" keywords:{}", e.keywords.join(",")));
        }
        header.push_str(" -->\n");
        out.push_str(&header);
        if !e.title.is_empty() {
            out.push_str(&format!("### {}\n", e.title));
        }
        out.push_str(e.body.trim());
        out.push_str("\n\n");
    }
    out
}

pub struct MemoryRagStore {
    global_dir: PathBuf,
    project_dir: PathBuf,
}

impl MemoryRagStore {
    pub fn open_default() -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self::open(default_global_memory_dir(), default_project_memory_dir(&cwd))
    }

    pub fn open(global_dir: impl AsRef<Path>, project_dir: impl AsRef<Path>) -> Self {
        let global_dir = global_dir.as_ref().to_path_buf();
        let project_dir = project_dir.as_ref().to_path_buf();
        let _ = fs::create_dir_all(global_dir.join("daily"));
        let _ = fs::create_dir_all(project_dir.join("daily"));
        Self {
            global_dir,
            project_dir,
        }
    }

    fn dir_for_scope(&self, scope: &str) -> &Path {
        if scope == "global" {
            &self.global_dir
        } else {
            &self.project_dir
        }
    }

    fn cache_path(&self) -> PathBuf {
        self.project_dir.join("rag-cache.bin")
    }

    fn repo_index_path(&self) -> PathBuf {
        self.project_dir.join("repo-docs.md")
    }

    /// Writes or upserts (when `key` is set) a memory entry atomically into Markdown.
    pub fn write_memory(
        &self,
        scope: &str,
        target: &str,
        title: Option<&str>,
        content: &str,
        keywords: Vec<String>,
        key: Option<String>,
    ) -> std::io::Result<(String, PathBuf, bool)> {
        let base = self.dir_for_scope(scope);
        let is_daily = target == "daily";
        let path = if is_daily {
            base.join("daily").join(format!("{}.md", today_ymd()))
        } else {
            base.join("MEMORY.md")
        };
        let source_label = if is_daily {
            format!("{scope}:daily/{}.md", today_ymd())
        } else {
            format!("{scope}:MEMORY.md")
        };

        let existing_raw = fs::read_to_string(&path).unwrap_or_default();
        let mut entries = parse_markdown_entries(&existing_raw, &source_label, is_daily);
        let ts = now_secs();
        let clean_title = title
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string)
            .unwrap_or_else(|| {
                content
                    .lines()
                    .next()
                    .unwrap_or("Note")
                    .chars()
                    .take(72)
                    .collect()
            });

        let mut updated = false;
        let id = if let Some(ref k) = key
            && let Some(pos) = entries
                .iter()
                .position(|e| e.key.as_deref() == Some(k.as_str()))
        {
            let existing_id = entries[pos].id.clone();
            entries[pos].title = clean_title;
            entries[pos].body = content.trim().to_string();
            entries[pos].keywords = keywords;
            entries[pos].timestamp = ts;
            updated = true;
            existing_id
        } else {
            let new_id = format!("mem_{ts}_{}", fnv1a_64(content.as_bytes()) % 100_000);
            entries.push(MemoryEntry {
                id: new_id.clone(),
                key,
                title: clean_title,
                keywords,
                body: content.trim().to_string(),
                source: source_label,
                is_daily,
                timestamp: ts,
            });
            new_id
        };

        let serialized = serialize_markdown_entries(&entries);
        atomic_write(&path, serialized.as_bytes())?;
        Ok((id, path, updated))
    }

    pub fn delete_memory(&self, id_or_key: &str, scope: Option<&str>) -> std::io::Result<bool> {
        let dirs: Vec<(&str, &Path)> = match scope {
            Some("global") => vec![("global", self.global_dir.as_path())],
            Some("project") => vec![("project", self.project_dir.as_path())],
            _ => vec![
                ("project", self.project_dir.as_path()),
                ("global", self.global_dir.as_path()),
            ],
        };
        for (sc, dir) in dirs {
            let mem_path = dir.join("MEMORY.md");
            if let Ok(raw) = fs::read_to_string(&mem_path) {
                let mut entries = parse_markdown_entries(&raw, &format!("{sc}:MEMORY.md"), false);
                let before = entries.len();
                entries.retain(|e| {
                    e.id != id_or_key && e.key.as_deref() != Some(id_or_key)
                });
                if entries.len() < before {
                    let serialized = serialize_markdown_entries(&entries);
                    atomic_write(&mem_path, serialized.as_bytes())?;
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    fn collect_memory_files(&self) -> Vec<(PathBuf, String, bool)> {
        let mut files = Vec::new();
        for (scope, dir) in [
            ("project", self.project_dir.as_path()),
            ("global", self.global_dir.as_path()),
        ] {
            let lt = dir.join("MEMORY.md");
            if lt.exists() {
                files.push((lt, format!("{scope}:MEMORY.md"), false));
            }
            if let Ok(entries) = fs::read_dir(dir.join("daily")) {
                let mut daily: Vec<PathBuf> = entries
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("md"))
                    .collect();
                daily.sort();
                for p in daily {
                    let name = p
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("daily.md")
                        .to_string();
                    files.push((p, format!("{scope}:daily/{name}"), true));
                }
            }
        }
        files
    }

    fn manifest_fingerprint(files: &[(PathBuf, String, bool)]) -> String {
        let mut s = String::new();
        for (p, _, _) in files {
            if let Ok(meta) = fs::metadata(p) {
                let mtime = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                s.push_str(&format!("{}|{}|{}\n", p.display(), mtime, meta.len()));
            }
        }
        hex_hash_16(&s)
    }

    fn load_entries_and_vectors(&self, scope: &str) -> (Vec<MemoryEntry>, Vec<QuantizedVec>) {
        let files = if scope == "repo" {
            let p = self.repo_index_path();
            if p.exists() {
                vec![(p, "repo:index".to_string(), false)]
            } else {
                Vec::new()
            }
        } else {
            self.collect_memory_files()
        };

        let mut entries = Vec::new();
        for (path, label, is_daily) in &files {
            if let Ok(raw) = fs::read_to_string(path) {
                entries.extend(parse_markdown_entries(&raw, label, *is_daily));
            }
        }
        if entries.is_empty() {
            return (entries, Vec::new());
        }

        // Try binary cache for `scope == "memory"`
        let fp = Self::manifest_fingerprint(&files);
        if scope != "repo"
            && let Some(cached) = self.try_read_cache(&fp, entries.len())
        {
            return (entries, cached);
        }

        let vecs: Vec<QuantizedVec> = entries
            .iter()
            .map(|e| quantize_i8(&embed_weighted(&e.title, &e.keywords, &e.body)))
            .collect();

        if scope != "repo" {
            let _ = self.write_cache(&fp, &vecs);
        }
        (entries, vecs)
    }

    fn try_read_cache(&self, expected_fp: &str, expected_count: usize) -> Option<Vec<QuantizedVec>> {
        let bytes = fs::read(self.cache_path()).ok()?;
        // Header: 4 magic + 2 ver + 2 tok_ver + 2 dim + 16 fp + 4 count = 30 bytes
        if bytes.len() < 30 || &bytes[0..4] != CACHE_MAGIC {
            return None;
        }
        let ver = u16::from_le_bytes([bytes[4], bytes[5]]);
        let tok_ver = u16::from_le_bytes([bytes[6], bytes[7]]);
        let dim = u16::from_le_bytes([bytes[8], bytes[9]]) as usize;
        if ver != CACHE_VERSION || tok_ver != TOKENIZER_VERSION || dim != VEC_DIM {
            return None;
        }
        let fp = std::str::from_utf8(&bytes[10..26]).ok()?;
        if fp != expected_fp {
            return None;
        }
        let count = u32::from_le_bytes([bytes[26], bytes[27], bytes[28], bytes[29]]) as usize;
        if count != expected_count {
            return None;
        }
        let stride = 4 + VEC_DIM;
        if bytes.len() != 30 + count * stride {
            return None;
        }
        let mut out = Vec::with_capacity(count);
        for i in 0..count {
            let off = 30 + i * stride;
            let scale = f32::from_le_bytes([
                bytes[off],
                bytes[off + 1],
                bytes[off + 2],
                bytes[off + 3],
            ]);
            let data: Vec<i8> = bytes[off + 4..off + stride]
                .iter()
                .map(|&b| b as i8)
                .collect();
            out.push(QuantizedVec { scale, data });
        }
        Some(out)
    }

    fn write_cache(&self, fp: &str, vecs: &[QuantizedVec]) -> std::io::Result<()> {
        let mut buf = Vec::with_capacity(30 + vecs.len() * (4 + VEC_DIM));
        buf.extend_from_slice(CACHE_MAGIC);
        buf.extend_from_slice(&CACHE_VERSION.to_le_bytes());
        buf.extend_from_slice(&TOKENIZER_VERSION.to_le_bytes());
        buf.extend_from_slice(&(VEC_DIM as u16).to_le_bytes());
        let fp_bytes = fp.as_bytes();
        buf.extend_from_slice(&fp_bytes[..16.min(fp_bytes.len())]);
        if fp_bytes.len() < 16 {
            buf.resize(26, b'0');
        }
        buf.extend_from_slice(&(vecs.len() as u32).to_le_bytes());
        for qv in vecs {
            buf.extend_from_slice(&qv.scale.to_le_bytes());
            for &v in &qv.data {
                buf.push(v as u8);
            }
        }
        atomic_write(&self.cache_path(), &buf)
    }

    /// Indexes Markdown docs, ADRs, and code signatures (fn/struct/enum/class/def) from `repo_root`.
    pub fn index_repo_docs(&self, repo_root: &Path, max_files: usize) -> std::io::Result<usize> {
        let mut files = Vec::new();
        fn collect(cur: &Path, depth: usize, max_files: usize, out: &mut Vec<PathBuf>) {
            if depth > 8 || out.len() >= max_files {
                return;
            }
            let Ok(entries) = fs::read_dir(cur) else {
                return;
            };
            for e in entries.flatten() {
                if e.file_type().map(|ft| ft.is_symlink()).unwrap_or(false) {
                    continue;
                }
                let name = e.file_name();
                let s = name.to_string_lossy();
                if matches!(
                    s.as_ref(),
                    ".git" | "target" | "node_modules" | ".venv" | "__pycache__" | "dist" | "build"
                ) {
                    continue;
                }
                let p = e.path();
                if p.is_dir() {
                    collect(&p, depth + 1, max_files, out);
                } else if let Some(ext) = p.extension().and_then(|x| x.to_str())
                    && matches!(ext, "md" | "rs" | "py" | "ts")
                {
                    out.push(p);
                }
            }
        }
        collect(repo_root, 0, max_files, &mut files);

        let ts = now_secs();
        let mut entries = Vec::new();
        for path in files {
            let Ok(raw) = fs::read_to_string(&path) else {
                continue;
            };
            let rel = path
                .strip_prefix(repo_root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext == "md" {
                let lines: Vec<&str> = raw.lines().collect();
                let mut start = 0usize;
                while start < lines.len() {
                    let end = (start + 35).min(lines.len());
                    let body = lines[start..end].join("\n").trim().to_string();
                    if body.len() >= 25 {
                        entries.push(MemoryEntry {
                            id: format!("repo_{}_{}", hex_hash_16(&rel), start + 1),
                            key: Some(format!("{rel}:L{}", start + 1)),
                            title: format!("{rel} (L{}-L{end})", start + 1),
                            keywords: vec![rel.clone()],
                            body,
                            source: format!("repo:{rel}:L{}-L{end}", start + 1),
                            is_daily: false,
                            timestamp: ts,
                        });
                    }
                    if end == lines.len() {
                        break;
                    }
                    start = end;
                }
            } else {
                // Extract function/struct/enum/class/def signatures only (not raw code dumps)
                let sigs: Vec<String> = raw
                    .lines()
                    .enumerate()
                    .filter(|(_, l)| {
                        let t = l.trim_start();
                        t.starts_with("pub fn ")
                            || t.starts_with("fn ")
                            || t.starts_with("pub struct ")
                            || t.starts_with("pub enum ")
                            || t.starts_with("pub trait ")
                            || t.starts_with("def ")
                            || t.starts_with("async def ")
                            || t.starts_with("class ")
                            || t.starts_with("export function ")
                            || t.starts_with("export class ")
                            || t.starts_with("export interface ")
                    })
                    .map(|(idx, l)| format!("L{}: {}", idx + 1, l.trim()))
                    .collect();
                if !sigs.is_empty() {
                    entries.push(MemoryEntry {
                        id: format!("repo_{}_sigs", hex_hash_16(&rel)),
                        key: Some(format!("{rel}:sigs")),
                        title: format!("{rel} signatures"),
                        keywords: vec![rel.clone()],
                        body: sigs.join("\n"),
                        source: format!("repo:{rel}:L1-L{}", raw.lines().count()),
                        is_daily: false,
                        timestamp: ts,
                    });
                }
            }
        }

        let serialized = serialize_markdown_entries(&entries);
        atomic_write(&self.repo_index_path(), serialized.as_bytes())?;
        Ok(entries.len())
    }

    /// Hybrid Lucene-BM25 + Scaled-i8 Cosine Vector + RRF with hard abstention & daily decay.
    pub fn search(&self, query: &str, limit: usize, scope: &str) -> Vec<RagHit> {
        if query.trim().is_empty() {
            return Vec::new();
        }
        let (entries, vecs) = self.load_entries_and_vectors(scope);
        if entries.is_empty() {
            return Vec::new();
        }

        let q_vec = embed_weighted("", &[], query);
        let q_tokens = tokenize_dual(query);
        let n_docs = entries.len() as f32;

        // Build per-document weighted token counts (title & keywords boosted 1.8x -> counted twice)
        let mut doc_tf: Vec<HashMap<String, f32>> = Vec::with_capacity(entries.len());
        let mut doc_len: Vec<f32> = Vec::with_capacity(entries.len());
        let mut df: HashMap<String, usize> = HashMap::new();

        for e in &entries {
            let mut tf: HashMap<String, f32> = HashMap::new();
            let mut len = 0.0f32;
            for tok in tokenize_dual(&e.title) {
                *tf.entry(tok).or_insert(0.0) += 1.8;
                len += 1.8;
            }
            for kw in &e.keywords {
                for tok in tokenize_dual(kw) {
                    *tf.entry(tok).or_insert(0.0) += 1.8;
                    len += 1.8;
                }
            }
            for tok in tokenize_dual(&e.body) {
                *tf.entry(tok).or_insert(0.0) += 1.0;
                len += 1.0;
            }
            for term in tf.keys() {
                *df.entry(term.clone()).or_insert(0) += 1;
            }
            doc_tf.push(tf);
            doc_len.push(len.max(1.0));
        }

        let avg_dl = (doc_len.iter().sum::<f32>() / n_docs).max(1.0);
        let now = now_secs();

        let mut vec_candidates: Vec<(usize, f32)> = Vec::new();
        let mut bm25_candidates: Vec<(usize, f32)> = Vec::new();
        let mut cos_map: HashMap<usize, f32> = HashMap::new();
        let mut bm25_map: HashMap<usize, f32> = HashMap::new();

        for (idx, qv) in vecs.iter().enumerate() {
            let cos = cosine_f32_i8(&q_vec, qv);
            let dl = doc_len[idx];
            let tf_map = &doc_tf[idx];
            let mut bm25 = 0.0f32;
            let k1 = 1.5f32;
            let b = 0.75f32;
            let seen_q: HashSet<&str> = q_tokens.iter().map(String::as_str).collect();
            for qt in seen_q {
                if let Some(&tf) = tf_map.get(qt) {
                    let df_t = *df.get(qt).unwrap_or(&0) as f32;
                    // Lucene non-negative IDF: strictly > 0 even when N = 1, df = 1
                    let idf = (1.0 + (n_docs - df_t + 0.5) / (df_t + 0.5)).ln();
                    bm25 += idf * (tf * (k1 + 1.0)) / (tf + k1 * (1.0 - b + b * (dl / avg_dl)));
                }
            }

            // Hard abstention filter BEFORE RRF ranking
            if bm25 <= 0.0 && cos < HARD_ABSTAIN_COS {
                continue;
            }
            cos_map.insert(idx, cos);
            bm25_map.insert(idx, bm25);
            if cos >= HARD_ABSTAIN_COS {
                vec_candidates.push((idx, cos));
            }
            if bm25 > 0.0 {
                bm25_candidates.push((idx, bm25));
            }
        }

        if vec_candidates.is_empty() && bm25_candidates.is_empty() {
            return Vec::new();
        }

        vec_candidates.sort_by(|a, b| b.1.total_cmp(&a.1));
        bm25_candidates.sort_by(|a, b| b.1.total_cmp(&a.1));

        let mut rrf: HashMap<usize, f32> = HashMap::new();
        for (rank, (idx, _)) in vec_candidates.iter().enumerate() {
            *rrf.entry(*idx).or_insert(0.0) += 1.0 / (60.0 + rank as f32);
        }
        for (rank, (idx, _)) in bm25_candidates.iter().enumerate() {
            *rrf.entry(*idx).or_insert(0.0) += 1.0 / (60.0 + rank as f32);
        }

        // Apply recency decay to `daily/*.md` entries ONLY
        for (idx, score) in &mut rrf {
            if entries[*idx].is_daily && entries[*idx].timestamp > 0 {
                let age_days = now.saturating_sub(entries[*idx].timestamp) as f32 / 86_400.0;
                *score *= 1.0 / (1.0 + age_days / 21.0);
            }
        }

        let mut ranked: Vec<(usize, f32)> = rrf.into_iter().collect();
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1));

        // Deduplicate overlapping repo chunks from the same file within 10 lines
        let mut seen_repo_lines: HashMap<String, Vec<usize>> = HashMap::new();
        let mut hits = Vec::new();
        for (idx, score) in ranked {
            let e = &entries[idx];
            if let Some(rest) = e.source.strip_prefix("repo:")
                && let Some((file, range)) = rest.rsplit_once(":L")
                && let Some(start_s) = range.split('-').next()
                && let Ok(start_line) = start_s.parse::<usize>()
            {
                let prev_lines = seen_repo_lines.entry(file.to_string()).or_default();
                if prev_lines.iter().any(|&p| p.abs_diff(start_line) <= 10) {
                    continue;
                }
                prev_lines.push(start_line);
            }
            hits.push(RagHit {
                id: e.id.clone(),
                source: e.source.clone(),
                title: e.title.clone(),
                score,
                cosine: *cos_map.get(&idx).unwrap_or(&0.0),
                bm25: *bm25_map.get(&idx).unwrap_or(&0.0),
                body: e.body.clone(),
            });
            if hits.len() >= limit.clamp(1, 20) {
                break;
            }
        }
        hits
    }

    pub fn read_by_ids(&self, ids: &[String]) -> String {
        let mut all = Vec::new();
        for scope in ["memory", "repo"] {
            let (entries, _) = self.load_entries_and_vectors(scope);
            all.extend(entries);
        }
        let mut out = Vec::new();
        for want in ids {
            if let Some(e) = all
                .iter()
                .find(|e| e.id == *want || e.key.as_deref() == Some(want.as_str()))
            {
                out.push(format!(
                    "[{}] ({}) ### {}\n{}",
                    e.id, e.source, e.title, e.body
                ));
            } else {
                out.push(format!("[{want}] not found."));
            }
        }
        out.join("\n\n---\n\n")
    }

    pub fn read_target(&self, target: &str) -> String {
        match target {
            "daily" => {
                let p = self.project_dir.join("daily").join(format!("{}.md", today_ymd()));
                fs::read_to_string(&p)
                    .unwrap_or_else(|_| format!("No daily memory log yet ({}).", p.display()))
            }
            "list" => {
                let files = self.collect_memory_files();
                if files.is_empty() {
                    "No memory files yet.".to_string()
                } else {
                    files
                        .into_iter()
                        .map(|(p, label, _)| format!("{label} ({})", p.display()))
                        .collect::<Vec<_>>()
                        .join("\n")
                }
            }
            _ => {
                let mut parts = Vec::new();
                for (label, dir) in [
                    ("project", &self.project_dir),
                    ("global", &self.global_dir),
                ] {
                    if let Ok(raw) = fs::read_to_string(dir.join("MEMORY.md"))
                        && !raw.trim().is_empty()
                    {
                        parts.push(format!("## {label} MEMORY.md\n{}", raw.trim()));
                    }
                }
                if parts.is_empty() {
                    "No long-term MEMORY.md entries yet.".to_string()
                } else {
                    parts.join("\n\n")
                }
            }
        }
    }

    /// Deterministic, sorted Table of Contents for `<memory_toc>` (built once at session start).
    pub fn build_toc(&self, max_entries: usize) -> String {
        let mut by_sort_key: BTreeMap<String, String> = BTreeMap::new();
        for (scope, dir) in [
            ("project", &self.project_dir),
            ("global", &self.global_dir),
        ] {
            if let Ok(raw) = fs::read_to_string(dir.join("MEMORY.md")) {
                for e in parse_markdown_entries(&raw, &format!("{scope}:MEMORY.md"), false) {
                    let sort_k = format!("{}:{}", scope, e.key.as_deref().unwrap_or(&e.id));
                    let key_part = e
                        .key
                        .as_deref()
                        .map(|k| format!(" [{k}]"))
                        .unwrap_or_default();
                    by_sort_key.insert(sort_k, format!("- ({scope}){key_part} {}", e.title));
                }
            }
        }
        by_sort_key
            .into_values()
            .take(max_entries)
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn stats_summary(&self) -> String {
        let (mem_entries, _) = self.load_entries_and_vectors("memory");
        let (repo_entries, _) = self.load_entries_and_vectors("repo");
        let cache_kb = fs::metadata(self.cache_path())
            .map(|m| m.len() as f64 / 1024.0)
            .unwrap_or(0.0);
        format!(
            "Rust Hybrid RAG Memory v2 (Lucene-BM25 + 512d scaled-i8 Cosine + RRF)\n  Project dir: {}\n  Global dir:  {}\n  Memory entries: {} · Repo doc/sig chunks: {} · Cache: {:.1} KB\n  Commands: /memory <query> · /memory repo <query> · /memory index",
            self.project_dir.display(),
            self.global_dir.display(),
            mem_entries.len(),
            repo_entries.len(),
            cache_kb
        )
    }
}

pub fn execute_memory_tool(name: &str, args: &Value) -> Result<String, String> {
    let store = MemoryRagStore::open_default();
    match name {
        "memory_write" => {
            let scope = args.get("scope").and_then(Value::as_str).unwrap_or("project");
            let target = args
                .get("target")
                .and_then(Value::as_str)
                .unwrap_or("long_term");
            let title = args.get("title").and_then(Value::as_str);
            let content = args
                .get("content")
                .and_then(Value::as_str)
                .ok_or_else(|| "memory_write requires string `content`".to_string())?;
            let keywords: Vec<String> = args
                .get("keywords")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(Value::as_str)
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(ToString::to_string)
                        .collect()
                })
                .unwrap_or_default();
            let key = args
                .get("key")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToString::to_string);
            let (id, path, updated) = store
                .write_memory(scope, target, title, content, keywords, key)
                .map_err(|e| e.to_string())?;
            let verb = if updated { "Updated" } else { "Saved" };
            Ok(format!("{verb} memory `{id}` in {}", path.display()))
        }
        "memory_delete" => {
            let id_or_key = args
                .get("id_or_key")
                .and_then(Value::as_str)
                .ok_or_else(|| "memory_delete requires `id_or_key`".to_string())?;
            let scope = args.get("scope").and_then(Value::as_str);
            let removed = store
                .delete_memory(id_or_key, scope)
                .map_err(|e| e.to_string())?;
            if removed {
                Ok(format!("Deleted memory `{id_or_key}`."))
            } else {
                Ok(format!("Memory `{id_or_key}` not found."))
            }
        }
        "memory_read" => {
            if let Some(arr) = args.get("ids").and_then(Value::as_array) {
                let ids: Vec<String> = arr
                    .iter()
                    .filter_map(Value::as_str)
                    .map(ToString::to_string)
                    .collect();
                if !ids.is_empty() {
                    return Ok(store.read_by_ids(&ids));
                }
            }
            if let Some(id) = args.get("id").and_then(Value::as_str) {
                return Ok(store.read_by_ids(&[id.to_string()]));
            }
            let target = args
                .get("target")
                .and_then(Value::as_str)
                .unwrap_or("long_term");
            Ok(store.read_target(target))
        }
        "memory_search" => {
            let query = args
                .get("query")
                .and_then(Value::as_str)
                .ok_or_else(|| "memory_search requires string `query`".to_string())?;
            let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(5) as usize;
            let scope = args.get("scope").and_then(Value::as_str).unwrap_or("memory");
            let hits = store.search(query, limit, scope);
            if hits.is_empty() {
                return Ok("No relevant memories found.".to_string());
            }
            let mut out = String::new();
            for (i, h) in hits.into_iter().enumerate() {
                let entry_str = if h.body.len() <= ADAPTIVE_INLINE_MAX_CHARS && !h.source.starts_with("repo:") {
                    format!(
                        "{}. [{}] [{}] (cos={:.2}, bm25={:.2}) [full] {}\n{}\n\n",
                        i + 1,
                        h.id,
                        h.source,
                        h.cosine,
                        h.bm25,
                        h.title,
                        h.body
                    )
                } else {
                    let snippet: String = h.body.chars().take(120).collect();
                    format!(
                        "{}. [{}] [{}] (cos={:.2}, bm25={:.2}) [stub] {} — {}...\n",
                        i + 1,
                        h.id,
                        h.source,
                        h.cosine,
                        h.bm25,
                        h.title,
                        snippet.replace('\n', " ")
                    )
                };
                if out.len() + entry_str.len() > SEARCH_MAX_CHARS && !out.is_empty() {
                    out.push_str("...[truncated to budget; call memory_read(ids=[...]) for full text]");
                    break;
                }
                out.push_str(&entry_str);
            }
            Ok(out.trim_end().to_string())
        }
        _ => Err(format!("unknown memory tool: {name}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store(name: &str) -> (PathBuf, MemoryRagStore) {
        let base = std::env::temp_dir().join(format!("le_vibe_rag_v2_{name}"));
        let _ = fs::remove_dir_all(&base);
        let store = MemoryRagStore::open(base.join("global"), base.join("project"));
        (base, store)
    }

    #[test]
    fn lucene_idf_works_on_single_entry_and_abstains_on_unrelated() {
        let (base, store) = temp_store("single");
        store
            .write_memory(
                "project",
                "long_term",
                Some("Power BI Batch Limit"),
                "Power BI streaming dataset push URL has a hard 15KB batch limit.",
                vec!["powerbi".into(), "413".into(), "payload".into(), "throttling".into()],
                Some("powerbi.push_limit".into()),
            )
            .unwrap();

        // N = 1 exact match must succeed (Lucene IDF > 0)
        let exact = store.search("Power BI batch limit", 5, "memory");
        assert_eq!(exact.len(), 1);
        assert!(exact[0].bm25 > 0.0);

        // Write-time synonym recall via keywords
        let syn = store.search("413 payload throttling", 5, "memory");
        assert_eq!(syn.len(), 1);
        assert!(syn[0].body.contains("15KB"));

        // Hard abstention on completely unrelated query
        let none = store.search("xylophone quantum banana", 5, "memory");
        assert!(none.is_empty());

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn dual_emit_splits_snake_and_camel_and_matches_morphological_stem() {
        let toks = tokenize_dual("max_batch_limit getUserToken #dataparc");
        assert!(toks.contains(&"max_batch_limit".to_string()));
        assert!(toks.contains(&"batch".to_string()));
        assert!(toks.contains(&"getusertoken".to_string()));
        assert!(toks.contains(&"user".to_string()));
        assert!(toks.contains(&"token".to_string()));

        // Pure stem/morphological test ("authenticating" vs "authentication")
        let (base, store) = temp_store("stem");
        store
            .write_memory(
                "project",
                "long_term",
                Some("OAuth Authentication"),
                "Provider authentication uses PKCE refresh tokens.",
                vec![],
                None,
            )
            .unwrap();
        let hits = store.search("authenticating", 3, "memory");
        assert!(!hits.is_empty(), "stem/typo query should pass calibrated cosine threshold");
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn upsert_by_key_delete_and_sentinel_parsing_with_subheadings() {
        let (base, store) = temp_store("upsert");
        let (_, _, upd1) = store
            .write_memory(
                "project",
                "long_term",
                Some("DB Engine"),
                "Using SQLite.\n### Gotchas\nWAL mode required.",
                vec!["database".into()],
                Some("db.engine".into()),
            )
            .unwrap();
        assert!(!upd1);

        // Upsert replaces old value
        let (_, _, upd2) = store
            .write_memory(
                "project",
                "long_term",
                Some("DB Engine"),
                "Migrated to Postgres.\n### Gotchas\nPool max 20.",
                vec!["database".into(), "postgres".into()],
                Some("db.engine".into()),
            )
            .unwrap();
        assert!(upd2);

        let hits = store.search("database", 5, "memory");
        assert_eq!(hits.len(), 1);
        assert!(hits[0].body.contains("Migrated to Postgres"));
        assert!(hits[0].body.contains("### Gotchas"));

        // Delete removes entry cleanly
        assert!(store.delete_memory("db.engine", Some("project")).unwrap());
        assert!(store.search("database", 5, "memory").is_empty());

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn scaled_i8_quantization_preserves_cosine_and_cache_reloads() {
        let v1 = embed_weighted("Auth Strategy", &["oauth".into()], "PKCE browser login flow");
        let v2 = embed_weighted("", &[], "oauth browser login");
        let exact_cos: f32 = v1.iter().zip(v2.iter()).map(|(a, b)| a * b).sum();
        let qv = quantize_i8(&v1);
        let quant_cos = cosine_f32_i8(&v2, &qv);
        assert!(
            (exact_cos - quant_cos).abs() < 0.015,
            "scaled i8 cosine error too large: exact={exact_cos}, quant={quant_cos}"
        );

        let (base, store) = temp_store("cache");
        store
            .write_memory("project", "long_term", Some("TUI"), "Dirty-driven redraw loop", vec![], None)
            .unwrap();
        let h1 = store.search("redraw loop", 3, "memory");
        assert_eq!(h1.len(), 1);
        assert!(store.cache_path().exists());
        // Second search hits binary cache
        let h2 = store.search("redraw loop", 3, "memory");
        assert_eq!(h2.len(), 1);
        assert_eq!(h1[0].id, h2[0].id);

        let _ = fs::remove_dir_all(&base);
    }
}
