use serde::Deserialize;
use serde::Serialize;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

const DEFAULT_REFUSE_MESSAGE: &str = r#"NOTE TO LLM:
You just ran `apply_patch` as a shell command, not as a model-native editing tool.
This environment is configured to refuse shell-based patching, so nothing was changed.

Use your native editing tool (your built-in patch/editor tool) to apply this change now, and do not run `apply_patch` in the shell for future changes."#;

const DEFAULT_WARN_MESSAGE: &str = r#"NOTE TO LLM:
The patch was applied by a shell `apply_patch` wrapper.
For future changes, use your native editing tool instead of running `apply_patch` in the shell."#;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Mode {
    Apply,
    Refuse,
    Warn,
}

impl Default for Mode {
    fn default() -> Self {
        Self::Apply
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Config {
    #[serde(default)]
    mode: Mode,
    #[serde(default)]
    refuse_message: Option<String>,
    #[serde(default)]
    warn_message: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mode: Mode::Apply,
            refuse_message: None,
            warn_message: None,
        }
    }
}

fn config_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("APPLY_PATCH_CONFIG") {
        return Some(PathBuf::from(path));
    }
    let base = if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        PathBuf::from(xdg)
    } else {
        PathBuf::from(std::env::var_os("HOME")?)
    };
    Some(base.join(".apply_patch").join("config.json"))
}

fn load_config(path: &Path) -> Config {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(_) => return Config::default(),
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

fn save_config(path: &Path, cfg: &Config) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    let data = serde_json::to_vec_pretty(cfg).unwrap_or_else(|_| b"{}".to_vec());
    std::fs::write(&tmp, data)?;
    if path.exists() {
        let _ = std::fs::remove_file(path);
    }
    std::fs::rename(tmp, path)?;
    Ok(())
}

fn parse_mode(s: &str) -> Option<Mode> {
    match s {
        "apply" => Some(Mode::Apply),
        "refuse" => Some(Mode::Refuse),
        "warn" => Some(Mode::Warn),
        _ => None,
    }
}

fn print_help(mut out: impl Write) {
    let _ = writeln!(
        out,
        r#"apply_patch

Applies Codex-style *** Begin Patch patches from stdin (or a single PATCH argument).

Config flags (persist in your home directory):
  --show-config
  --mode <apply|refuse|warn>   (aliases: --apply, --refuse, --warn)
  --set-refuse-message <text>
  --clear-refuse-message
  --set-warn-message <text>
  --clear-warn-message

Notes:
  - Config is stored at $XDG_CONFIG_HOME/.apply_patch/config.json (or ~/.apply_patch/config.json).
  - You can override the config path with $APPLY_PATCH_CONFIG."#
    );
}

fn run_config_command(args: &[String]) -> Option<i32> {
    let mut show = false;
    let mut mode: Option<Mode> = None;
    let mut refuse_message: Option<Option<String>> = None;
    let mut warn_message: Option<Option<String>> = None;
    let mut positional: Vec<String> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--show-config" => {
                show = true;
                i += 1;
            }
            "--mode" => {
                let Some(val) = args.get(i + 1) else {
                    eprintln!("Error: --mode requires a value.");
                    return Some(2);
                };
                let Some(parsed) = parse_mode(val) else {
                    eprintln!("Error: invalid --mode value: {val}");
                    return Some(2);
                };
                mode = Some(parsed);
                i += 2;
            }
            "--apply" => {
                mode = Some(Mode::Apply);
                i += 1;
            }
            "--refuse" => {
                mode = Some(Mode::Refuse);
                i += 1;
            }
            "--warn" => {
                mode = Some(Mode::Warn);
                i += 1;
            }
            "--set-refuse-message" => {
                let Some(val) = args.get(i + 1) else {
                    eprintln!("Error: --set-refuse-message requires a value.");
                    return Some(2);
                };
                refuse_message = Some(Some(val.to_string()));
                i += 2;
            }
            "--clear-refuse-message" => {
                refuse_message = Some(None);
                i += 1;
            }
            "--set-warn-message" => {
                let Some(val) = args.get(i + 1) else {
                    eprintln!("Error: --set-warn-message requires a value.");
                    return Some(2);
                };
                warn_message = Some(Some(val.to_string()));
                i += 2;
            }
            "--clear-warn-message" => {
                warn_message = Some(None);
                i += 1;
            }
            "-h" | "--help" => {
                print_help(std::io::stdout());
                return Some(0);
            }
            arg if arg.starts_with('-') => {
                eprintln!("Error: unknown option: {arg}");
                return Some(2);
            }
            other => {
                positional.push(other.to_string());
                i += 1;
            }
        }
    }

    let has_config_flags = show || mode.is_some() || refuse_message.is_some() || warn_message.is_some();

    if !has_config_flags {
        return None;
    }

    if !positional.is_empty() {
        eprintln!("Error: configuration flags cannot be combined with a PATCH argument.");
        return Some(2);
    }

    let Some(path) = config_path() else {
        eprintln!("Error: could not determine config path (HOME/XDG_CONFIG_HOME not set).");
        return Some(1);
    };
    let mut cfg = load_config(&path);
    let mode_changed = mode.is_some();
    let refuse_message_changed = refuse_message.is_some();
    let warn_message_changed = warn_message.is_some();
    if let Some(m) = mode {
        cfg.mode = m;
    }
    if let Some(val) = refuse_message {
        cfg.refuse_message = val;
    }
    if let Some(val) = warn_message {
        cfg.warn_message = val;
    }

    if mode_changed || refuse_message_changed || warn_message_changed {
        if let Err(err) = save_config(&path, &cfg) {
            eprintln!("Error: failed to write config: {err}");
            return Some(1);
        }
    }

    if show {
        let mode_str = match cfg.mode {
            Mode::Apply => "apply",
            Mode::Refuse => "refuse",
            Mode::Warn => "warn",
        };
        let _ = writeln!(std::io::stdout(), "Config file: {}", path.display());
        let _ = writeln!(std::io::stdout(), "mode: {mode_str}");
        let _ = writeln!(
            std::io::stdout(),
            "refuse_message: {}",
            if cfg.refuse_message.is_some() {
                "custom"
            } else {
                "default"
            }
        );
        let _ = writeln!(
            std::io::stdout(),
            "warn_message: {}",
            if cfg.warn_message.is_some() {
                "custom"
            } else {
                "default"
            }
        );
    } else {
        let _ = writeln!(std::io::stdout(), "Updated config: {}", path.display());
    }

    Some(0)
}

fn read_patch_from_stdin() -> Result<String, i32> {
    let mut buf = String::new();
    match std::io::stdin().read_to_string(&mut buf) {
        Ok(_) => {
            if buf.is_empty() {
                eprintln!("Usage: apply_patch 'PATCH'\n       echo 'PATCH' | apply-patch");
                return Err(2);
            }
            Ok(buf)
        }
        Err(err) => {
            eprintln!("Error: Failed to read PATCH from stdin.\n{err}");
            Err(1)
        }
    }
}

fn run_main() -> i32 {
    let mut args_os = std::env::args_os();
    let _argv0 = args_os.next();

    let mut args: Vec<String> = Vec::new();
    for arg in args_os {
        match arg.into_string() {
            Ok(s) => args.push(s),
            Err(_) => {
                eprintln!("Error: apply_patch requires a UTF-8 PATCH argument.");
                return 1;
            }
        }
    }

    if let Some(code) = run_config_command(&args) {
        return code;
    }

    let cfg = config_path()
        .as_deref()
        .map(load_config)
        .unwrap_or_default();

    let patch_arg = match args.as_slice() {
        [] => match read_patch_from_stdin() {
            Ok(s) => s,
            Err(code) => return code,
        },
        [body] => body.to_string(),
        _ => {
            eprintln!("Error: apply_patch accepts exactly one argument.");
            return 2;
        }
    };

    match cfg.mode {
        Mode::Refuse => {
            let msg = cfg
                .refuse_message
                .as_deref()
                .unwrap_or(DEFAULT_REFUSE_MESSAGE);
            println!("{msg}");
            0
        }
        Mode::Apply | Mode::Warn => {
            let mut stdout = std::io::stdout();
            let mut stderr = std::io::stderr();
            match codex_apply_patch::apply_patch(&patch_arg, &mut stdout, &mut stderr) {
                Ok(()) => {
                    let _ = stdout.flush();
                    if cfg.mode == Mode::Warn {
                        let msg = cfg.warn_message.as_deref().unwrap_or(DEFAULT_WARN_MESSAGE);
                        println!("{msg}");
                    }
                    0
                }
                Err(_) => 1,
            }
        }
    }
}

pub fn main() -> ! {
    let code = run_main();
    std::process::exit(code);
}
