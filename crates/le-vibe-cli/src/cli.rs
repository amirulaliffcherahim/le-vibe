use std::fs;
use std::io::{self, IsTerminal, Read};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CliOptions {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub thinking: Option<String>,
    pub agent_mode: Option<String>,
    pub mode: Option<String>,
    pub print_mode: bool,
    pub yolo: bool,
    pub continue_session: bool,
    pub resume_id: Option<String>,
    pub session_path: Option<String>,
    pub session_dir: Option<String>,
    pub fork_id: Option<String>,
    pub no_session: bool,
    pub session_name: Option<String>,
    pub allowed_tools: Option<Vec<String>>,
    pub prompt: Option<String>,
    pub at_files: Vec<String>,
    pub extra_extensions: Vec<String>,
    pub no_extensions: bool,
    pub system_prompt_extra: Option<String>,
    pub offline: bool,
    pub trust_override: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CliAction {
    Help,
    Version,
    Subcommand(String, Vec<String>),
    ListModels(Option<String>),
    ExportOffline { input: String, output: Option<String> },
    Run(Box<CliOptions>),
}

pub(crate) fn help_text() -> &'static str {
    "le-vibe — Modular Rust Pi harness + Python provider/policy/extension engine

USAGE:
  le-vibe [OPTIONS] [--] [@files...] [prompt...]
  le-vibe <login|auth|mcp|config|install|remove|list|update> [OPTIONS]

SUBCOMMANDS:
  login [--provider ...]       Authenticate provider (OIDC / OAuth / API key)
  auth                         Print JSON auth status for all providers
  mcp list                     List configured MCP servers and status
  config [path|set <k> <v>]    Inspect or update layered settings
  install <file.py|git-url>    Install a Python extension into ~/.le-vibe/extensions/
  remove <name>                Remove an installed extension
  list                         List installed extensions
  update                       Pull latest main & reinstall le-vibe binary
  doctor                       Run local environment, auth, MCP & config health check

OPTIONS:
  -h, --help                   Show this help and exit
  -V, --version                Show version and exit
  -p, --print                  Run one-shot prompt and exit
      --mode <text|json|rpc>   Select output/protocol interface
      --provider <name>        Provider (vibe | deepseek | agy)
      --model <spec>           Model (id or provider/id[:thinking])
      --thinking <level>       off | minimal | low | medium | high | xhigh | max
      --agent-mode <mode>      normal | plan | ask | rea | cyber
      --tools <csv>            Comma-separated tool allowlist (e.g. read,grep,ls)
      --yolo, --auto-approve   Auto-approve non-blocked tools
  -c, --continue               Continue latest session
  -r, --resume [id]            Resume a saved session
      --session <path|id>      Open specific session file or id
      --session-dir <dir>      Override session storage directory
      --fork <id>              Fork an existing session into a new session
      --no-session             Run ephemeral in-memory session
  -n, --name <name>            Set session display name
      --list-models [query]    List available live models and exit
      --export <jsonl> [out]   Export a session .jsonl to .md or .html and exit
  -e, --extension <path>       Load an extra Python extension file for this run
      --no-extensions          Start without loading any extensions/plugins
      --system-prompt <text>   Append custom system prompt instructions for this run
      --offline                Skip startup live catalog/usage network requests
      --approve                Trust project .le-vibe resources for this run
      --no-approve             Skip project .le-vibe resources for this run"
}

pub(crate) fn parse_cli_args(args: &[String]) -> CliAction {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        return CliAction::Help;
    }
    if args.iter().any(|a| a == "--version" || a == "-V") {
        return CliAction::Version;
    }
    if let Some(first) = args.first().map(String::as_str)
        && (matches!(
            first,
            "login" | "auth" | "mcp" | "config" | "install" | "remove" | "list" | "update" | "doctor"
        ) || (cfg!(feature = "dev-debug") && first == "tui-probe"))
    {
        return CliAction::Subcommand(first.to_string(), args[1..].to_vec());
    }

    let mut opts = CliOptions {
        provider: None,
        model: None,
        thinking: None,
        agent_mode: None,
        mode: None,
        print_mode: false,
        yolo: false,
        continue_session: false,
        resume_id: None,
        session_path: None,
        session_dir: None,
        fork_id: None,
        no_session: false,
        session_name: None,
        allowed_tools: None,
        prompt: None,
        at_files: Vec::new(),
        extra_extensions: Vec::new(),
        no_extensions: false,
        system_prompt_extra: None,
        offline: false,
        trust_override: None,
    };
    let mut positional: Vec<String> = Vec::new();
    let mut i = 0;
    let mut stop_flags = false;

    while i < args.len() {
        let a = &args[i];
        if !stop_flags && a == "--" {
            stop_flags = true;
            i += 1;
            continue;
        }
        if !stop_flags {
            match a.as_str() {
                "--list-models" => {
                    let query = args.get(i + 1).filter(|s| !s.starts_with('-')).cloned();
                    return CliAction::ListModels(query);
                }
                "--export" => {
                    let input = args.get(i + 1).cloned().unwrap_or_default();
                    let output = args.get(i + 2).filter(|s| !s.starts_with('-')).cloned();
                    return CliAction::ExportOffline { input, output };
                }
                "-p" | "--print" => {
                    opts.print_mode = true;
                    i += 1;
                    continue;
                }
                "--yolo" | "--auto-approve" => {
                    opts.yolo = true;
                    i += 1;
                    continue;
                }
                "-c" | "--continue" => {
                    opts.continue_session = true;
                    i += 1;
                    continue;
                }
                "--no-session" => {
                    opts.no_session = true;
                    i += 1;
                    continue;
                }
                "--no-extensions" => {
                    opts.no_extensions = true;
                    i += 1;
                    continue;
                }
                "--offline" => {
                    opts.offline = true;
                    i += 1;
                    continue;
                }
                "--approve" => {
                    opts.trust_override = Some(true);
                    i += 1;
                    continue;
                }
                "--no-approve" => {
                    opts.trust_override = Some(false);
                    i += 1;
                    continue;
                }
                "-e" | "--extension" if i + 1 < args.len() => {
                    opts.extra_extensions.push(args[i + 1].clone());
                    i += 2;
                    continue;
                }
                "--system-prompt" if i + 1 < args.len() => {
                    opts.system_prompt_extra = Some(args[i + 1].clone());
                    i += 2;
                    continue;
                }
                "-r" | "--resume" => {
                    if let Some(next) = args.get(i + 1)
                        && !next.starts_with('-')
                    {
                        opts.resume_id = Some(next.clone());
                        i += 2;
                    } else {
                        opts.resume_id = Some(String::new());
                        i += 1;
                    }
                    continue;
                }
                "--provider" if i + 1 < args.len() => {
                    opts.provider = Some(args[i + 1].clone());
                    i += 2;
                    continue;
                }
                "--model" if i + 1 < args.len() => {
                    let spec = &args[i + 1];
                    let mut rest = spec.as_str();
                    if let Some((base, th)) = rest.rsplit_once(':')
                        && matches!(th, "off" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max")
                    {
                        opts.thinking.get_or_insert_with(|| th.to_string());
                        rest = base;
                    }
                    if let Some((prov, id)) = rest.split_once('/') {
                        opts.provider.get_or_insert_with(|| prov.to_string());
                        opts.model = Some(id.to_string());
                    } else {
                        opts.model = Some(rest.to_string());
                    }
                    i += 2;
                    continue;
                }
                "--thinking" if i + 1 < args.len() => {
                    opts.thinking = Some(args[i + 1].clone());
                    i += 2;
                    continue;
                }
                "--agent-mode" if i + 1 < args.len() => {
                    opts.agent_mode = Some(args[i + 1].clone());
                    i += 2;
                    continue;
                }
                "--mode" if i + 1 < args.len() => {
                    opts.mode = Some(args[i + 1].clone());
                    i += 2;
                    continue;
                }
                "--session" if i + 1 < args.len() => {
                    opts.session_path = Some(args[i + 1].clone());
                    i += 2;
                    continue;
                }
                "--session-dir" if i + 1 < args.len() => {
                    opts.session_dir = Some(args[i + 1].clone());
                    i += 2;
                    continue;
                }
                "--fork" if i + 1 < args.len() => {
                    opts.fork_id = Some(args[i + 1].clone());
                    i += 2;
                    continue;
                }
                "-n" | "--name" if i + 1 < args.len() => {
                    opts.session_name = Some(args[i + 1].clone());
                    i += 2;
                    continue;
                }
                "--tools" if i + 1 < args.len() => {
                    let tools: Vec<String> = args[i + 1]
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                    opts.allowed_tools = Some(tools);
                    i += 2;
                    continue;
                }
                _ => {}
            }
        }
        if let Some(file_ref) = a.strip_prefix('@') {
            if !file_ref.is_empty() {
                opts.at_files.push(file_ref.to_string());
            }
        } else {
            positional.push(a.clone());
        }
        i += 1;
    }

    if !positional.is_empty() {
        opts.prompt = Some(positional.join(" "));
    }
    CliAction::Run(Box::new(opts))
}

pub(crate) fn build_initial_prompt(opts: &CliOptions) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    if !io::stdin().is_terminal() && opts.mode.as_deref() != Some("rpc") {
        let mut piped = String::new();
        if io::stdin().read_to_string(&mut piped).is_ok() && !piped.trim().is_empty() {
            parts.push(piped.trim_end().to_string());
        }
    }
    for rel in &opts.at_files {
        let path = Path::new(rel);
        if let Ok(content) = fs::read_to_string(path) {
            parts.push(format!("<file path=\"{rel}\">\n{content}\n</file>"));
        }
    }
    if let Some(p) = &opts.prompt
        && !p.trim().is_empty()
    {
        parts.push(p.clone());
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vec_s(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn parses_help_and_version() {
        assert_eq!(parse_cli_args(&vec_s(&["--help"])), CliAction::Help);
        assert_eq!(parse_cli_args(&vec_s(&["-V"])), CliAction::Version);
    }

    #[test]
    fn parses_model_tools_no_session_and_at_files() {
        let action = parse_cli_args(&vec_s(&[
            "-p",
            "--no-session",
            "--model",
            "agy/gemini-4-neon:high",
            "--tools",
            "read,grep",
            "@README.md",
            "explain",
            "this",
        ]));
        let CliAction::Run(opts) = action else {
            panic!("expected Run");
        };
        assert!(opts.print_mode);
        assert!(opts.no_session);
        assert_eq!(opts.provider.as_deref(), Some("agy"));
        assert_eq!(opts.model.as_deref(), Some("gemini-4-neon"));
        assert_eq!(opts.thinking.as_deref(), Some("high"));
        assert_eq!(opts.allowed_tools, Some(vec!["read".into(), "grep".into()]));
        assert_eq!(opts.at_files, vec!["README.md".to_string()]);
        assert_eq!(opts.prompt.as_deref(), Some("explain this"));
    }

    #[test]
    fn parses_new_subcommands_and_extension_offline_flags() {
        assert!(matches!(
            parse_cli_args(&vec_s(&["config"])),
            CliAction::Subcommand(s, _) if s == "config"
        ));
        assert!(matches!(
            parse_cli_args(&vec_s(&["install", "my_ext.py"])),
            CliAction::Subcommand(s, _) if s == "install"
        ));
        let CliAction::Run(opts) = parse_cli_args(&vec_s(&[
            "-e",
            "a.py",
            "--no-extensions",
            "--offline",
            "--system-prompt",
            "be terse",
        ])) else {
            panic!("expected Run");
        };
        assert_eq!(opts.extra_extensions, vec!["a.py"]);
        assert!(opts.no_extensions);
        assert!(opts.offline);
        assert_eq!(opts.system_prompt_extra.as_deref(), Some("be terse"));
    }
}
