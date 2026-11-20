use crate::state::{TranscriptBlock, TuiState, format_tokens};
use crate::transcript::format_duration_ms;

pub fn export_session_markdown(state: &TuiState) -> String {
    let branch = state.git_branch.as_deref().unwrap_or("-");
    let mut out = format!(
        "# le-vibe Session Export\n\n\
         - **Repository:** `{}` (branch `{branch}`)\n\
         - **CWD:** `{}`\n\
         - **Provider / Model:** `{}/{}` (thinking: `{}`)\n\
         - **Mode:** `{}` | **Shell:** `{}`\n\
         - **Tokens:** in={} out={} cache={}\n\
         - **Cost:** `${:.4}` | **Active:** {}\n\n---\n\n",
        state.repo_name,
        state.cwd,
        state.provider,
        state.model,
        state.thinking_level,
        state.mode.name(),
        state.shell,
        format_tokens(state.input_tokens),
        format_tokens(state.output_tokens),
        format_tokens(state.cache_read_tokens),
        state.cost_usd,
        format_duration_ms(state.active_ms),
    );

    for block in &state.blocks {
        match block {
            TranscriptBlock::User(text) => {
                out.push_str("## User\n\n");
                out.push_str(text);
                out.push_str("\n\n");
            }
            TranscriptBlock::Thinking(text) => {
                out.push_str("<details>\n<summary>Thinking</summary>\n\n");
                out.push_str(text);
                out.push_str("\n\n</details>\n\n");
            }
            TranscriptBlock::Assistant(text) => {
                out.push_str("## Assistant\n\n");
                out.push_str(text);
                out.push_str("\n\n");
            }
            TranscriptBlock::Tool {
                name,
                status,
                output,
            } => {
                out.push_str(&format!("### Tool: `{name}` ({status})\n\n"));
                if !output.trim().is_empty() {
                    out.push_str("```\n");
                    out.push_str(output);
                    out.push_str("\n```\n\n");
                }
            }
            TranscriptBlock::Script {
                command,
                output,
                in_context,
            } => {
                let scope = if *in_context { "in-context" } else { "local" };
                out.push_str(&format!("### Shell ({scope}): `{command}`\n\n"));
                if !output.trim().is_empty() {
                    out.push_str("```\n");
                    out.push_str(output);
                    out.push_str("\n```\n\n");
                }
            }
            TranscriptBlock::System(text) => {
                out.push_str(&format!("> **System:** {text}\n\n"));
            }
            TranscriptBlock::Report(text) => {
                out.push_str("```\n");
                out.push_str(text);
                out.push_str("\n```\n\n");
            }
        }
    }
    out
}

fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

pub fn export_session_html(state: &TuiState) -> String {
    let branch = escape_html(state.git_branch.as_deref().unwrap_or("-"));
    let mut body = String::new();
    for block in &state.blocks {
        match block {
            TranscriptBlock::User(text) => {
                body.push_str(&format!(
                    "<section class=\"msg user\"><div class=\"role\">User</div><pre class=\"prose\">{}</pre></section>\n",
                    escape_html(text)
                ));
            }
            TranscriptBlock::Thinking(text) => {
                body.push_str(&format!(
                    "<details class=\"card thinking\"><summary>thinking</summary><pre>{}</pre></details>\n",
                    escape_html(text)
                ));
            }
            TranscriptBlock::Assistant(text) => {
                body.push_str(&format!(
                    "<section class=\"msg assistant\"><div class=\"role\">Assistant</div><pre class=\"prose\">{}</pre></section>\n",
                    escape_html(text)
                ));
            }
            TranscriptBlock::Tool {
                name,
                status,
                output,
            } => {
                let cls = if status == "failed" { "tool err" } else { "tool" };
                let body_html = if output.trim().is_empty() {
                    String::new()
                } else {
                    format!("<pre>{}</pre>", escape_html(output))
                };
                body.push_str(&format!(
                    "<details class=\"card {cls}\" open><summary>{} · {}</summary>{body_html}</details>\n",
                    escape_html(name),
                    escape_html(status),
                ));
            }
            TranscriptBlock::Script {
                command,
                output,
                in_context,
            } => {
                let scope = if *in_context { "in-context" } else { "local" };
                let body_html = if output.trim().is_empty() {
                    String::new()
                } else {
                    format!("<pre>{}</pre>", escape_html(output))
                };
                body.push_str(&format!(
                    "<details class=\"card script\" open><summary>shell ({scope}): {}</summary>{body_html}</details>\n",
                    escape_html(command),
                ));
            }
            TranscriptBlock::System(text) => {
                body.push_str(&format!(
                    "<div class=\"system\">{}</div>\n",
                    escape_html(text)
                ));
            }
            TranscriptBlock::Report(text) => {
                body.push_str(&format!(
                    "<pre class=\"report\">{}</pre>\n",
                    escape_html(text)
                ));
            }
        }
    }

    format!(
        "<!doctype html>\n\
         <html lang=\"en\">\n\
         <head>\n\
         <meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>le-vibe Session Export — {}</title>\n\
         <style>\n\
         :root{{color-scheme:dark;--bg:#0d1117;--panel:#161b22;--border:#30363d;--fg:#e6edf3;--muted:#8b949e;--accent:#56d4dd;--ok:#98c379;--err:#e06c75;}}\n\
         body{{margin:0;padding:24px;background:var(--bg);color:var(--fg);font:14px/1.55 ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;}}\n\
         .wrap{{max-width:960px;margin:0 auto;}}\n\
         header{{border-bottom:1px solid var(--border);padding-bottom:14px;margin-bottom:20px;}}\n\
         h1{{margin:0 0 8px;font-size:18px;color:var(--accent);}}\n\
         .meta{{color:var(--muted);font-size:12px;display:flex;flex-wrap:wrap;gap:12px;}}\n\
         .msg{{margin:14px 0;padding:12px 14px;border-radius:8px;background:var(--panel);border:1px solid var(--border);}}\n\
         .msg.user{{border-left:3px solid var(--accent);}}\n\
         .msg.assistant{{border-left:3px solid var(--ok);}}\n\
         .role{{font-size:11px;text-transform:uppercase;letter-spacing:.06em;color:var(--muted);margin-bottom:6px;}}\n\
         .prose{{margin:0;white-space:pre-wrap;word-break:break-word;font-family:inherit;}}\n\
         .card{{margin:10px 0;padding:8px 12px;border-radius:6px;background:#131720;border-left:3px solid var(--muted);}}\n\
         .card.tool{{border-left-color:var(--ok);}}\n\
         .card.tool.err{{border-left-color:var(--err);}}\n\
         .card.thinking{{border-left-color:#c678dd;}}\n\
         summary{{cursor:pointer;color:var(--muted);font-size:12px;}}\n\
         pre{{margin:8px 0 0;white-space:pre-wrap;word-break:break-word;overflow-x:auto;}}\n\
         .system{{margin:8px 0;color:var(--muted);font-size:12px;}}\n\
         .report{{padding:12px;background:var(--panel);border:1px solid var(--border);border-radius:6px;}}\n\
         </style>\n\
         </head>\n\
         <body>\n\
         <div class=\"wrap\">\n\
         <header>\n\
         <h1>le-vibe Session Export</h1>\n\
         <div class=\"meta\">\n\
         <span>repo: {} ({branch})</span>\n\
         <span>model: {}/{} ({})</span>\n\
         <span>mode: {}</span>\n\
         <span>tokens: ↑{} ↓{} ⚡{}</span>\n\
         <span>cost: ${:.4}</span>\n\
         <span>active: {}</span>\n\
         </div>\n\
         </header>\n\
         {body}\n\
         </div>\n\
         </body>\n\
         </html>\n",
        escape_html(&state.repo_name),
        escape_html(&state.repo_name),
        escape_html(&state.provider),
        escape_html(&state.model),
        escape_html(&state.thinking_level),
        escape_html(state.mode.name()),
        format_tokens(state.input_tokens),
        format_tokens(state.output_tokens),
        format_tokens(state.cache_read_tokens),
        state.cost_usd,
        format_duration_ms(state.active_ms),
    )
}

