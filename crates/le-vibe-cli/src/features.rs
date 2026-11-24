use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub(crate) const MAX_IMAGE_BYTES: usize = 3_750_000;

#[derive(Debug, Clone)]
pub(crate) struct ScheduledLoop {
    pub id: u64,
    pub interval: Duration,
    pub prompt: String,
    pub next_run: Instant,
    pub runs: u64,
}

#[derive(Default)]
pub(crate) struct LoopScheduler {
    next_id: u64,
    items: Vec<ScheduledLoop>,
}

impl LoopScheduler {
    pub(crate) fn add(&mut self, interval: Duration, prompt: String) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        self.items.push(ScheduledLoop {
            id,
            interval,
            prompt,
            next_run: Instant::now() + interval,
            runs: 0,
        });
        id
    }

    pub(crate) fn stop(&mut self, target: &str) -> usize {
        if target.eq_ignore_ascii_case("all") || target.is_empty() {
            let n = self.items.len();
            self.items.clear();
            return n;
        }
        if let Ok(id) = target.parse::<u64>() {
            let before = self.items.len();
            self.items.retain(|l| l.id != id);
            return before - self.items.len();
        }
        0
    }

    pub(crate) fn len(&self) -> usize {
        self.items.len()
    }

    pub(crate) fn list_summary(&self) -> String {
        if self.items.is_empty() {
            return "No active loops. Start one with: /loop 2m <prompt> or /loop every 30s <prompt>"
                .to_string();
        }
        self.items
            .iter()
            .map(|l| {
                let rem = l
                    .next_run
                    .saturating_duration_since(Instant::now())
                    .as_secs();
                format!(
                    "#{} every {}s (runs: {}, next in {}s): {}",
                    l.id,
                    l.interval.as_secs(),
                    l.runs,
                    rem,
                    l.prompt
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub(crate) fn pop_due(&mut self) -> Option<String> {
        let now = Instant::now();
        for item in &mut self.items {
            if now >= item.next_run {
                item.next_run = now + item.interval;
                item.runs += 1;
                return Some(item.prompt.clone());
            }
        }
        None
    }
}

/// Parses `"2m cargo test"`, `"every 30s git status"`, `"every 5 minutes check build"`.
pub(crate) fn parse_loop_spec(raw: &str) -> Option<(Duration, String)> {
    let tokens: Vec<&str> = raw.split_whitespace().collect();
    if tokens.is_empty() {
        return None;
    }
    let start = if tokens[0].eq_ignore_ascii_case("every") {
        1
    } else {
        0
    };
    let tok = *tokens.get(start)?;
    let (secs, consumed) = if let Some(n) = tok.strip_suffix('s').and_then(|v| v.parse::<u64>().ok())
    {
        (n, start + 1)
    } else if let Some(n) = tok.strip_suffix('m').and_then(|v| v.parse::<u64>().ok()) {
        (n * 60, start + 1)
    } else if let Some(n) = tok.strip_suffix('h').and_then(|v| v.parse::<u64>().ok()) {
        (n * 3600, start + 1)
    } else if let Ok(n) = tok.parse::<u64>() {
        let unit = tokens.get(start + 1).copied().unwrap_or("");
        let mult = match unit.to_lowercase().as_str() {
            "s" | "sec" | "secs" | "second" | "seconds" => 1,
            "m" | "min" | "mins" | "minute" | "minutes" => 60,
            "h" | "hr" | "hrs" | "hour" | "hours" => 3600,
            _ => return None,
        };
        (n * mult, start + 2)
    } else {
        return None;
    };

    let prompt = tokens.get(consumed..)?.join(" ");
    if prompt.trim().is_empty() || secs == 0 {
        return None;
    }
    Some((Duration::from_secs(secs.clamp(5, 86_400)), prompt))
}

pub(crate) fn sniff_image_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some("image/png");
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("image/jpeg");
    }
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("image/gif");
    }
    None
}

pub(crate) fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((triple >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(triple & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

pub(crate) enum PastedItem {
    Image {
        name: String,
        mime: String,
        base64: String,
    },
    ImageTooLarge(String),
    FilePath(String),
    PlainText(String),
}

pub(crate) fn try_load_image_file(path: &Path) -> Option<PastedItem> {
    let bytes = fs::read(path).ok()?;
    let mime = sniff_image_mime(&bytes)?;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("image.png")
        .to_string();
    if bytes.len() > MAX_IMAGE_BYTES {
        return Some(PastedItem::ImageTooLarge(format!(
            "{name} is {:.2} MB (max 3.75 MB)",
            bytes.len() as f64 / 1_000_000.0
        )));
    }
    Some(PastedItem::Image {
        name,
        mime: mime.to_string(),
        base64: base64_encode(&bytes),
    })
}

pub(crate) fn classify_pasted_text(raw: &str) -> PastedItem {
    let trimmed = raw.trim();
    let unquoted = trimmed
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .or_else(|| trimmed.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
        .unwrap_or(trimmed);
    if !unquoted.is_empty() && !unquoted.contains('\n') {
        let p = Path::new(unquoted);
        if p.exists() && p.is_file() {
            if let Some(img) = try_load_image_file(p) {
                return img;
            }
            return PastedItem::FilePath(unquoted.to_string());
        }
    }
    PastedItem::PlainText(raw.to_string())
}

#[cfg(target_os = "windows")]
fn read_win32_clipboard_text_fast() -> Option<String> {
    const CF_BITMAP: u32 = 2;
    const CF_UNICODETEXT: u32 = 13;
    const CF_HDROP: u32 = 15;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn IsClipboardFormatAvailable(format: u32) -> i32;
        fn OpenClipboard(hwnd: *mut core::ffi::c_void) -> i32;
        fn CloseClipboard() -> i32;
        fn GetClipboardData(format: u32) -> *mut core::ffi::c_void;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GlobalLock(hmem: *mut core::ffi::c_void) -> *const u16;
        fn GlobalUnlock(hmem: *mut core::ffi::c_void) -> i32;
    }

    // SAFETY: Standard Win32 clipboard APIs; null checks guard every handle/pointer,
    // and CloseClipboard/GlobalUnlock are always paired on success.
    unsafe {
        if IsClipboardFormatAvailable(CF_BITMAP) != 0 || IsClipboardFormatAvailable(CF_HDROP) != 0 {
            return None;
        }
        if IsClipboardFormatAvailable(CF_UNICODETEXT) == 0 {
            return None;
        }
        if OpenClipboard(core::ptr::null_mut()) == 0 {
            return None;
        }
        let handle = GetClipboardData(CF_UNICODETEXT);
        let result = if !handle.is_null() {
            let ptr = GlobalLock(handle);
            if !ptr.is_null() {
                let mut len = 0usize;
                while len < 1_000_000 && *ptr.add(len) != 0 {
                    len += 1;
                }
                let slice = core::slice::from_raw_parts(ptr, len);
                let text = String::from_utf16_lossy(slice);
                GlobalUnlock(handle);
                Some(text)
            } else {
                None
            }
        } else {
            None
        };
        CloseClipboard();
        result
    }
}

pub(crate) fn grab_clipboard_item() -> Option<PastedItem> {
    #[cfg(target_os = "windows")]
    if let Some(plain) = read_win32_clipboard_text_fast() {
        return Some(classify_pasted_text(&plain));
    }

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let tmp_png = std::env::temp_dir().join(format!("le-vibe-clip-{ts}.png"));

    #[cfg(target_os = "windows")]
    {
        let script = format!(
            "Add-Type -AssemblyName System.Windows.Forms; \
             if ([Windows.Forms.Clipboard]::ContainsImage()) {{ \
               $img = [Windows.Forms.Clipboard]::GetImage(); \
               $img.Save('{}', [System.Drawing.Imaging.ImageFormat]::Png); \
               Write-Output 'IMG'; \
             }} elseif ([Windows.Forms.Clipboard]::ContainsFileDropList()) {{ \
               $f = [Windows.Forms.Clipboard]::GetFileDropList()[0]; \
               Write-Output ('FILE:' + $f); \
             }} elseif ([Windows.Forms.Clipboard]::ContainsText()) {{ \
               Write-Output ('TEXT:' + [Windows.Forms.Clipboard]::GetText()); \
             }}",
            tmp_png.display().to_string().replace('\'', "''")
        );
        if let Ok(out) = Command::new("powershell")
            .args(["-NoProfile", "-STA", "-Command", &script])
            .output()
            && out.status.success()
        {
            let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if text == "IMG" && tmp_png.exists() {
                let item = try_load_image_file(&tmp_png);
                let _ = fs::remove_file(&tmp_png);
                return item;
            }
            if let Some(file_path) = text.strip_prefix("FILE:") {
                return Some(classify_pasted_text(file_path));
            }
            if let Some(plain) = text.strip_prefix("TEXT:") {
                return Some(classify_pasted_text(plain));
            }
        }
        None
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = tmp_png;
        let out = if cfg!(target_os = "macos") {
            Command::new("pbpaste").output().ok()
        } else {
            Command::new("wl-paste")
                .output()
                .ok()
                .or_else(|| Command::new("xclip").args(["-o", "-selection", "clipboard"]).output().ok())
        };
        out.and_then(|o| {
            if o.status.success() {
                let s = String::from_utf8_lossy(&o.stdout).to_string();
                if !s.is_empty() {
                    return Some(classify_pasted_text(&s));
                }
            }
            None
        })
    }
}

fn escape_html_str(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub(crate) fn render_bug_report_html(repo: &str, model: &str, body_md: &str) -> String {
    let mut sections_html = String::new();
    let mut in_code = false;
    for line in body_md.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            in_code = !in_code;
            if in_code {
                sections_html.push_str("<pre><code>");
            } else {
                sections_html.push_str("</code></pre>\n");
            }
            continue;
        }
        if in_code {
            sections_html.push_str(&escape_html_str(line));
            sections_html.push('\n');
            continue;
        }
        if let Some(h) = trimmed.strip_prefix("### ") {
            sections_html.push_str(&format!("<h3>{}</h3>\n", escape_html_str(h)));
        } else if let Some(h) = trimmed.strip_prefix("## ") {
            sections_html.push_str(&format!("<h2>{}</h2>\n", escape_html_str(h)));
        } else if let Some(h) = trimmed.strip_prefix("# ") {
            sections_html.push_str(&format!("<h1>{}</h1>\n", escape_html_str(h)));
        } else if let Some(item) = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")) {
            sections_html.push_str(&format!("<p class=\"li\">• {}</p>\n", escape_html_str(item)));
        } else if trimmed.is_empty() {
            sections_html.push_str("<div class=\"sp\"></div>\n");
        } else {
            sections_html.push_str(&format!("<p>{}</p>\n", escape_html_str(line)));
        }
    }
    if in_code {
        sections_html.push_str("</code></pre>\n");
    }

    format!(
        "<!doctype html>\n\
         <html lang=\"en\">\n\
         <head>\n\
         <meta charset=\"utf-8\">\n\
         <title>Bug Report — {}</title>\n\
         <style>\n\
         @page {{ size: A4; margin: 18mm; }}\n\
         body {{ font-family: 'Segoe UI', system-ui, sans-serif; color: #111827; line-height: 1.55; font-size: 11pt; }}\n\
         header {{ border-bottom: 2px solid #0f172a; padding-bottom: 10px; margin-bottom: 18px; }}\n\
         .meta {{ color: #4b5563; font-size: 9.5pt; }}\n\
         h1 {{ font-size: 18pt; margin: 0 0 6px; color: #0f172a; }}\n\
         h2 {{ font-size: 13pt; margin: 18px 0 6px; color: #1e3a8a; border-bottom: 1px solid #e5e7eb; padding-bottom: 3px; }}\n\
         h3 {{ font-size: 11.5pt; margin: 12px 0 4px; color: #374151; }}\n\
         p {{ margin: 4px 0; }}\n\
         p.li {{ padding-left: 14px; }}\n\
         .sp {{ height: 6px; }}\n\
         pre {{ background: #f3f4f6; border: 1px solid #d1d5db; border-radius: 4px; padding: 10px; font-size: 9pt; overflow-x: auto; white-space: pre-wrap; }}\n\
         </style>\n\
         </head>\n\
         <body>\n\
         <header>\n\
         <h1>Software Defect &amp; Fix Report</h1>\n\
         <div class=\"meta\">Repository: {} · Generated by le-vibe ({})</div>\n\
         </header>\n\
         {sections_html}\n\
         </body>\n\
         </html>\n",
        escape_html_str(repo),
        escape_html_str(repo),
        escape_html_str(model),
    )
}

pub(crate) fn export_bug_report_pdf(
    repo: &str,
    model: &str,
    body_md: &str,
) -> Result<(PathBuf, bool), String> {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let html_path = std::env::temp_dir().join(format!("le-vibe-report-{ts}.html"));
    let pdf_path = PathBuf::from(format!("bug-report-{ts}.pdf"));
    let html = render_bug_report_html(repo, model, body_md);
    fs::write(&html_path, &html).map_err(|e| e.to_string())?;

    let browsers: &[&str] = if cfg!(target_os = "windows") {
        &[
            r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
            r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
            r"C:\Program Files\Google\Chrome\Application\chrome.exe",
            "msedge",
            "chrome",
        ]
    } else if cfg!(target_os = "macos") {
        &[
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
        ]
    } else {
        &["chromium", "chromium-browser", "google-chrome", "microsoft-edge"]
    };

    let pdf_abs = std::env::current_dir()
        .map(|d| d.join(&pdf_path))
        .unwrap_or_else(|_| pdf_path.clone());
    let file_url = format!(
        "file:///{}",
        html_path.to_string_lossy().replace('\\', "/")
    );

    for bin in browsers {
        let status = Command::new(bin)
            .args([
                "--headless",
                "--disable-gpu",
                "--no-sandbox",
                &format!("--print-to-pdf={}", pdf_abs.display()),
                &file_url,
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if status.is_ok_and(|s| s.success()) && pdf_abs.exists() {
            let _ = fs::remove_file(&html_path);
            return Ok((pdf_path, true));
        }
    }

    let fallback = PathBuf::from(format!("bug-report-{ts}.html"));
    fs::write(&fallback, html).map_err(|e| e.to_string())?;
    let _ = fs::remove_file(&html_path);
    Ok((fallback, false))
}

pub(crate) fn build_optimization_digest(
    sessions_root: &Path,
    logs_dir: &Path,
    self_repo: &str,
) -> String {
    let mut session_files: Vec<(u64, PathBuf)> = Vec::new();
    let mut collect_jsonl = |dir: &Path| {
        if let Ok(entries) = fs::read_dir(dir) {
            for e in entries.flatten() {
                let p = e.path();
                if p.extension().and_then(|s| s.to_str()) == Some("jsonl") {
                    let mtime = e
                        .metadata()
                        .ok()
                        .and_then(|m| m.modified().ok())
                        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    session_files.push((mtime, p));
                }
            }
        }
    };
    collect_jsonl(sessions_root);
    if let Ok(dirs) = fs::read_dir(sessions_root) {
        for d in dirs.flatten() {
            if d.path().is_dir() {
                collect_jsonl(&d.path());
            }
        }
    }
    session_files.sort_by_key(|(t, _)| std::cmp::Reverse(*t));
    session_files.truncate(12);

    let mut total_user_turns = 0usize;
    let mut interrupted_turns = 0usize;
    let mut max_turn_hits = 0usize;
    let mut tool_errors: BTreeMap<String, usize> = BTreeMap::new();
    let mut recent_prompts: Vec<String> = Vec::new();

    for (_, path) in &session_files {
        let Ok(raw) = fs::read_to_string(path) else {
            continue;
        };
        for line in raw.lines() {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            let kind = v.get("kind").and_then(|x| x.as_str()).unwrap_or("message");
            let role = v.get("role").and_then(|x| x.as_str()).unwrap_or("");
            let content = v.get("content").and_then(|x| x.as_str()).unwrap_or("");
            if kind == "message" && role == "user" {
                total_user_turns += 1;
                if recent_prompts.len() < 15 && !content.trim().is_empty() {
                    let snippet: String = content.chars().take(90).collect();
                    recent_prompts.push(snippet.replace('\n', " "));
                }
            }
            if content.contains("[Turn interrupted:") || content.contains("[Interrupted by user]") {
                interrupted_turns += 1;
            }
            if content.contains("max turns exceeded") {
                max_turn_hits += 1;
            }
            if kind == "tool_result"
                && (content.starts_with("Tool error:") || content.starts_with("Policy denied:"))
            {
                let tname = v
                    .get("tool_name")
                    .and_then(|x| x.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                *tool_errors.entry(tname).or_insert(0) += 1;
            }
        }
    }

    let mut slow_frames = 0usize;
    let mut logged_errors: Vec<String> = Vec::new();
    if let Ok(entries) = fs::read_dir(logs_dir) {
        let mut logs: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("jsonl"))
            .collect();
        logs.sort();
        for log_path in logs.iter().rev().take(5) {
            if let Ok(raw) = fs::read_to_string(log_path) {
                for line in raw.lines() {
                    let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
                        continue;
                    };
                    if v.get("event").and_then(|x| x.as_str()) == Some("slow_frame") {
                        slow_frames += 1;
                    }
                    if v.get("category").and_then(|x| x.as_str()) == Some("error")
                        && logged_errors.len() < 8
                        && let Some(d) = v.get("detail").and_then(|x| x.as_str())
                    {
                        logged_errors.push(d.chars().take(100).collect());
                    }
                }
            }
        }
    }

    let err_summary = if tool_errors.is_empty() {
        "none".to_string()
    } else {
        tool_errors
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(", ")
    };

    let rag = le_vibe_tools::memory::MemoryRagStore::open_default();
    let learn_report = crate::learn::mine_sessions(sessions_root, &rag, 20);
    let learn_summary = crate::learn::format_learn_report(&learn_report);
    let rag_stats = rag.stats_summary();

    format!(
        "Analyze my recent le-vibe sessions, Headroom compression stats, `/learn` friction report, and event logs below, then propose 3-6 concrete, high-impact optimizations to le-vibe itself.\n\
         Split every suggestion cleanly into Harness2-style scopes so project quirks never pollute global defaults:\n\
         1. **Invariant (Global) — applies everywhere**:\n\
            - Model profile adjustments (`memory_write` with `key=\"model:<provider>/<model>\"`, <=400 bytes, include `evidence: ...`).\n\
            - Global harness config/rules (`~/.le-vibe/SYSTEM.md`, `~/.le-vibe/settings.json`, global `MEMORY.md` with `key=\"learn:...\"`).\n\
            - Rust harness updates (`{self_repo}/crates/*`): exact file paths and changes.\n\
         2. **Domain (Repo-Local) — applies only to this workspace**:\n\
            - `<cwd>/.le-vibe/APPEND_SYSTEM.md`, project `MEMORY.md`, `<cwd>/.le-vibe/policy.json`, or project skills.\n\
         3. **Refine / Retire Stale Rules**:\n\
            - If `<learn_report>` lists any `[stale]` rule (still recurring >=3x despite existing in `MEMORY.md`), propose tightening or deleting it via `memory_delete`.\n\
         Number each suggestion (#1, #2, ...) and tag `[invariant]` or `[domain]` so I can reply `apply #1` directly. Skip non-stale `[known]` items.\n\n\
         <learn_report>\n\
         {learn_summary}\n\
         </learn_report>\n\n\
         <rag_memory>\n\
         {rag_stats}\n\
         </rag_memory>\n\n\
         <session_telemetry>\n\
         sessions_analyzed: {}\n\
         total_user_turns: {total_user_turns}\n\
         interrupted_turns: {interrupted_turns}\n\
         max_turn_hits: {max_turn_hits}\n\
         tool_errors: {err_summary}\n\
         slow_frames_gt_16ms: {slow_frames}\n\
         recent_logged_errors: {}\n\
         recent_user_prompts:\n- {}\n\
         </session_telemetry>",
        session_files.len(),
        if logged_errors.is_empty() {
            "none".to_string()
        } else {
            logged_errors.join(" | ")
        },
        if recent_prompts.is_empty() {
            "(none yet)".to_string()
        } else {
            recent_prompts.join("\n- ")
        }
    )
}

/// Normalize a directory path into a stable cross-platform trust key
/// (strips Windows verbatim `\\?\` prefix and lowercases on Windows).
pub(crate) fn canonical_folder_key(dir: &Path) -> String {
    let raw = fs::canonicalize(dir)
        .unwrap_or_else(|_| dir.to_path_buf())
        .to_string_lossy()
        .to_string();
    let stripped = raw.strip_prefix(r"\\?\").unwrap_or(&raw).to_string();
    if cfg!(windows) {
        stripped.replace('\\', "/").to_lowercase()
    } else {
        stripped
    }
}

/// Detect protected project resources under `.le-vibe/` that require project trust.
pub(crate) fn detect_project_resources(cwd: &Path) -> Vec<String> {
    let candidates = [
        ".le-vibe/settings.json",
        ".le-vibe/mcp.json",
        ".le-vibe/policy.json",
        ".le-vibe/SYSTEM.md",
        ".le-vibe/APPEND_SYSTEM.md",
        ".le-vibe/extensions",
        ".le-vibe/prompts",
    ];
    let mut found = Vec::new();
    for rel in candidates {
        let p = cwd.join(rel);
        if p.is_file() {
            found.push(rel.to_string());
        } else if p.is_dir()
            && fs::read_dir(&p)
                .map(|mut rd| rd.next().is_some())
                .unwrap_or(false)
        {
            found.push(format!("{rel}/"));
        }
    }
    found
}

fn trust_file_path() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".le-vibe").join("trust.json")
}

/// Look up saved trust decision in `~/.le-vibe/trust.json` for `cwd` or an ancestor.
pub(crate) fn lookup_project_trust(cwd: &Path) -> Option<bool> {
    let content = fs::read_to_string(trust_file_path()).ok()?;
    let map: BTreeMap<String, bool> = serde_json::from_str(&content).ok()?;
    let mut cur = fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf());
    loop {
        let key = canonical_folder_key(&cur);
        if let Some(&decision) = map.get(&key) {
            return Some(decision);
        }
        if !cur.pop() {
            break;
        }
    }
    None
}

/// Persist a trust decision for `cwd` in `~/.le-vibe/trust.json`.
pub(crate) fn save_project_trust(cwd: &Path, trusted: bool) {
    let path = trust_file_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let mut map: BTreeMap<String, bool> = fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    map.insert(canonical_folder_key(cwd), trusted);
    if let Ok(json) = serde_json::to_string_pretty(&map) {
        let _ = fs::write(path, json);
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PromptTemplate {
    pub command: String,
    pub description: String,
    pub body: String,
}

/// Load Markdown prompt templates from `~/.le-vibe/prompts/*.md` and (when trusted) `.le-vibe/prompts/*.md`.
pub(crate) fn load_prompt_templates(cwd: &Path, project_trusted: bool) -> Vec<PromptTemplate> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    let mut dirs = vec![PathBuf::from(home).join(".le-vibe").join("prompts")];
    if project_trusted {
        dirs.push(cwd.join(".le-vibe").join("prompts"));
    }
    let mut by_cmd: BTreeMap<String, PromptTemplate> = BTreeMap::new();
    for dir in dirs {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let Ok(body) = fs::read_to_string(&path) else {
                continue;
            };
            let trimmed = body.trim();
            if trimmed.is_empty() {
                continue;
            }
            let first_line = trimmed
                .lines()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("Prompt template")
                .trim_start_matches('#')
                .trim();
            let desc: String = first_line.chars().take(60).collect();
            let cmd = format!("/{}", stem.trim());
            by_cmd.insert(
                cmd.clone(),
                PromptTemplate {
                    command: cmd,
                    description: format!("[prompt] {desc}"),
                    body: trimmed.to_string(),
                },
            );
        }
    }
    by_cmd.into_values().collect()
}

/// Expand `$1`..`$9`, `$@`, and `$ARGUMENTS` placeholders in a prompt template.
pub(crate) fn expand_prompt_template(body: &str, raw_args: &str) -> String {
    let args: Vec<&str> = raw_args.split_whitespace().collect();
    let mut out = body
        .replace("$ARGUMENTS", raw_args.trim())
        .replace("$@", raw_args.trim());
    for idx in (1..=9).rev() {
        let val = args.get(idx - 1).copied().unwrap_or("");
        out = out.replace(&format!("${idx}"), val);
    }
    if !raw_args.trim().is_empty()
        && !body.contains("$1")
        && !body.contains("$@")
        && !body.contains("$ARGUMENTS")
    {
        out.push_str("\n\n");
        out.push_str(raw_args.trim());
    }
    out
}

/// Open `initial` text in `$VISUAL` / `$EDITOR` (Windows-safe `.cmd`/arg splitting, fallback `notepad.exe`/`vi`).
pub(crate) fn run_external_editor(initial: &str) -> Result<String, String> {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let tmp = std::env::temp_dir().join(format!("le-vibe-edit-{ts}.md"));
    fs::write(&tmp, initial).map_err(|e| format!("Failed writing temp file: {e}"))?;

    let editor_env = std::env::var("VISUAL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| std::env::var("EDITOR").ok().filter(|s| !s.trim().is_empty()))
        .unwrap_or_else(|| {
            if cfg!(windows) {
                "notepad.exe".to_string()
            } else {
                "vi".to_string()
            }
        });

    let parts: Vec<&str> = editor_env.split_whitespace().collect();
    let (prog, extra_args) = parts
        .split_first()
        .map(|(p, rest)| (*p, rest))
        .unwrap_or(("notepad.exe", &[]));

    let status = if cfg!(windows) {
        let mut full_args: Vec<String> = vec!["/C".to_string(), prog.to_string()];
        full_args.extend(extra_args.iter().map(|s| s.to_string()));
        full_args.push(tmp.to_string_lossy().to_string());
        Command::new("cmd")
            .args(&full_args)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
    } else {
        Command::new(prog)
            .args(extra_args)
            .arg(&tmp)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
    };

    let result = match status {
        Ok(s) if s.success() => fs::read_to_string(&tmp)
            .map(|s| s.replace("\r\n", "\n"))
            .map_err(|e| format!("Failed reading edited file: {e}")),
        Ok(s) => Err(format!("Editor exited with status {s}")),
        Err(e) => Err(format!("Failed launching editor `{editor_env}`: {e}")),
    };
    let _ = fs::remove_file(&tmp);
    result
}

/// Pure-Rust BM25 search across saved session JSONL files (zero extra deps).
pub(crate) fn search_past_sessions(session_dirs: &[PathBuf], query: &str, limit: usize) -> Vec<String> {
    let q_terms: Vec<String> = query
        .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
        .map(|s| s.to_lowercase())
        .filter(|s| s.len() >= 2)
        .collect();
    if q_terms.is_empty() {
        return vec!["Enter at least one search term (min 2 chars).".to_string()];
    }

    struct Hit {
        file: String,
        role: String,
        snippet: String,
        score: f64,
    }
    let mut hits: Vec<Hit> = Vec::new();
    for dir in session_dirs {
        let Ok(rd) = fs::read_dir(dir) else {
            continue;
        };
        for entry in rd.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let Ok(raw) = fs::read_to_string(&path) else {
                continue;
            };
            let fname = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("session.jsonl")
                .to_string();
            for line in raw.lines() {
                let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
                    continue;
                };
                let role = v.get("role").and_then(|r| r.as_str()).unwrap_or("");
                if !matches!(role, "user" | "assistant") {
                    continue;
                }
                let content = v.get("content").and_then(|c| c.as_str()).unwrap_or("");
                if content.trim().is_empty() {
                    continue;
                }
                let lower = content.to_lowercase();
                let dl = lower.split_whitespace().count().max(1) as f64;
                let mut score = 0.0f64;
                for term in &q_terms {
                    let tf = lower.matches(term.as_str()).count() as f64;
                    if tf > 0.0 {
                        let k1 = 1.2;
                        let b = 0.75;
                        let avgdl = 80.0;
                        score += (tf * (k1 + 1.0)) / (tf + k1 * (1.0 - b + b * (dl / avgdl)));
                    }
                }
                if score > 0.0 {
                    let snippet: String = content
                        .replace('\n', " ")
                        .chars()
                        .take(120)
                        .collect();
                    hits.push(Hit {
                        file: fname.clone(),
                        role: role.to_string(),
                        snippet,
                        score,
                    });
                }
            }
        }
    }
    hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    if hits.is_empty() {
        return vec![format!("No matches found for \"{query}\" across saved sessions.")];
    }
    let mut lines = vec![format!("Session Search: \"{query}\" (top {} hit(s))", hits.len().min(limit)), String::new()];
    for h in hits.into_iter().take(limit) {
        lines.push(format!("• [{:.2}] {} ({})", h.score, h.file, h.role));
        lines.push(format!("    {}", h.snippet));
    }
    lines
}

/// Build the active `/hotkeys` reference lines, merging any overrides in `~/.le-vibe/keybindings.json`.
pub(crate) fn format_hotkeys_lines() -> Vec<String> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    let kb_path = PathBuf::from(&home).join(".le-vibe").join("keybindings.json");
    let custom: BTreeMap<String, String> = fs::read_to_string(&kb_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let resolve = |action: &str, default_key: &str| -> String {
        custom
            .get(action)
            .cloned()
            .unwrap_or_else(|| default_key.to_string())
    };
    let mut lines = vec![
        format!("Keybindings config: {}", kb_path.display()),
        String::new(),
        "Core Controls:".to_string(),
        format!("  • {:<18} Submit prompt (or inject steering while busy)", resolve("submit", "Enter")),
        format!("  • {:<18} Queue follow-up while busy", resolve("followup", "Alt+Enter / Ctrl+Enter")),
        format!("  • {:<18} Return queued/steering prompts to editor", resolve("recall_queued", "Alt+Q / Alt+Up")),
        format!("  • {:<18} Interrupt turn & return queued prompts (or double-Esc /rewind)", resolve("interrupt", "Esc")),
        format!("  • {:<18} Insert newline in editor", resolve("newline", "Shift+Enter / Ctrl+J")),
        format!("  • {:<18} Open input in external editor ($VISUAL/$EDITOR)", resolve("external_editor", "Ctrl+G")),
        format!("  • {:<18} Copy last assistant reply to clipboard", resolve("copy_last", "Ctrl+X")),
        format!("  • {:<18} Paste text / image / file attachment", resolve("paste", "Ctrl+V")),
        String::new(),
        "Navigation & Views:".to_string(),
        format!("  • {:<18} Cycle agent mode (normal/plan/ask/rea/cyber)", resolve("cycle_mode", "Shift+Tab")),
        format!("  • {:<18} Cycle scoped models", resolve("cycle_model", "Ctrl+P")),
        format!("  • {:<18} Toggle thinking blocks", resolve("toggle_thinking", "Ctrl+T")),
        format!("  • {:<18} Toggle expanded tool & raw XML blocks", resolve("toggle_tools", "Ctrl+O")),
        format!("  • {:<18} Cycle prompt history", resolve("history", "Up / Down")),
        format!("  • {:<18} Scroll transcript / modals", resolve("scroll", "Mouse Wheel / PgUp / PgDn")),
        format!("  • {:<18} Clear input (or exit on double press)", resolve("quit", "Ctrl+C / Ctrl+D")),
    ];
    if !custom.is_empty() {
        lines.push(String::new());
        lines.push(format!("Custom overrides loaded ({} key(s) from keybindings.json).", custom.len()));
    }
    let themes_dir = PathBuf::from(&home).join(".le-vibe").join("themes");
    if let Ok(rd) = fs::read_dir(&themes_dir) {
        let user_themes: Vec<String> = rd
            .flatten()
            .filter_map(|e| {
                let p = e.path();
                (p.extension().and_then(|x| x.to_str()) == Some("json"))
                    .then(|| p.file_stem().and_then(|s| s.to_str()).map(str::to_string))
                    .flatten()
            })
            .collect();
        if !user_themes.is_empty() {
            lines.push(format!("User theme files (~/.le-vibe/themes/): {}", user_themes.join(", ")));
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_loop_intervals_and_manages_scheduler() {
        let (d1, p1) = parse_loop_spec("2m cargo test").unwrap();
        assert_eq!(d1, Duration::from_secs(120));
        assert_eq!(p1, "cargo test");

        let (d2, p2) = parse_loop_spec("every 30 seconds git status").unwrap();
        assert_eq!(d2, Duration::from_secs(30));
        assert_eq!(p2, "git status");

        let mut sched = LoopScheduler::default();
        let id = sched.add(Duration::from_secs(60), "check".into());
        assert_eq!(sched.len(), 1);
        assert_eq!(sched.stop(&id.to_string()), 1);
        assert_eq!(sched.len(), 0);
    }

    #[test]
    fn sniffs_image_magic_bytes_and_encodes_base64() {
        assert_eq!(
            sniff_image_mime(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00]),
            Some("image/png")
        );
        assert_eq!(sniff_image_mime(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("image/jpeg"));
        assert_eq!(sniff_image_mime(b"hello"), None);
        assert_eq!(base64_encode(b"hi!"), "aGkh");
    }

    #[test]
    fn renders_bug_report_html_safely() {
        let html = render_bug_report_html("le-vibe", "vibe/m", "# Title\n- item <x>\n```rs\nfn a(){}\n```");
        assert!(html.contains("Software Defect &amp; Fix Report"));
        assert!(html.contains("item &lt;x&gt;"));
        assert!(html.contains("<pre><code>fn a(){}\n</code></pre>"));
    }

    #[test]
    fn expands_prompt_templates_and_normalizes_paths() {
        let expanded = expand_prompt_template("Fix $1 in $2: $@", "bug main.rs asap");
        assert_eq!(expanded, "Fix bug in main.rs: bug main.rs asap");
        let appended = expand_prompt_template("Review this code", "src/lib.rs");
        assert_eq!(appended, "Review this code\n\nsrc/lib.rs");
    }
}
