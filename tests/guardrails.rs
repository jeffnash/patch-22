use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("tmp_tests");
        std::fs::create_dir_all(&base).expect("create temp base dir");
        let path = base.join(format!(
            "apply_patch_{}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            n
        ));
        std::fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn bin_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("debug")
        .join("apply_patch")
}

fn script_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("apply_patch")
}

fn run(mut cmd: Command) -> (i32, String, String) {
    let output = cmd.output().expect("failed to run command");
    let code = output.status.code().unwrap_or(1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (code, stdout, stderr)
}

fn run_with_stdin(mut cmd: Command, stdin: &str) -> (i32, String, String) {
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn command");
    {
        use std::io::Write;
        let mut handle = child.stdin.take().expect("missing stdin handle");
        handle
            .write_all(stdin.as_bytes())
            .expect("failed to write stdin");
    }
    let output = child.wait_with_output().expect("failed to read output");
    let code = output.status.code().unwrap_or(1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (code, stdout, stderr)
}

fn add_file_patch(path: &str, lines: &[&str]) -> String {
    let mut s = String::new();
    s.push_str("*** Begin Patch\n");
    s.push_str(&format!("*** Add File: {path}\n"));
    for ln in lines {
        s.push('+');
        s.push_str(ln);
        s.push('\n');
    }
    s.push_str("*** End Patch\n");
    s
}

fn update_file_patch(path: &str, old: &str, new: &str) -> String {
    format!(
        "*** Begin Patch\n*** Update File: {path}\n@@\n-{old}\n+{new}\n*** End Patch\n"
    )
}

fn apply_mode_config(program: &Path, cfg_path: &Path) {
    let (code, _stdout, stderr) = run({
        let mut cmd = Command::new(program);
        cmd.arg("--apply").env("APPLY_PATCH_CONFIG", cfg_path);
        cmd
    });
    assert_eq!(code, 0, "stderr:\n{stderr}");
    assert!(stderr.is_empty(), "stderr:\n{stderr}");
}

fn assert_patch_argument_mode(program: &Path, cfg_path: &Path) {
    let work = TempDir::new();
    apply_mode_config(program, cfg_path);

    let patch = add_file_patch("arg.txt", &["from-arg"]);
    let (code, stdout, stderr) = run({
        let mut cmd = Command::new(program);
        cmd.current_dir(work.path())
            .env("APPLY_PATCH_CONFIG", cfg_path)
            .arg(patch);
        cmd
    });
    assert_eq!(code, 0, "stdout:\n{stdout}\nstderr:\n{stderr}");
    assert!(stderr.is_empty(), "stderr:\n{stderr}");
    assert!(stdout.contains("A arg.txt"), "stdout:\n{stdout}");
    assert_eq!(
        std::fs::read_to_string(work.path().join("arg.txt")).unwrap(),
        "from-arg\n"
    );
}

fn assert_non_patch_argument_errors(program: &Path, cfg_path: &Path) {
    let (code, _stdout, _stderr) = run({
        let mut cmd = Command::new(program);
        cmd.env("APPLY_PATCH_CONFIG", cfg_path)
            .arg("this is not a patch");
        cmd
    });
    assert_ne!(code, 0);
}

fn assert_config_flag_error_paths(program: &Path, cfg_path: &Path) {
    let (code, _stdout, stderr) = run({
        let mut cmd = Command::new(program);
        cmd.arg("--mode").env("APPLY_PATCH_CONFIG", cfg_path);
        cmd
    });
    assert_eq!(code, 2);
    assert_eq!(stderr, "Error: --mode requires a value.\n");

    let (code, _stdout, stderr) = run({
        let mut cmd = Command::new(program);
        cmd.arg("--mode")
            .arg("nope")
            .env("APPLY_PATCH_CONFIG", cfg_path);
        cmd
    });
    assert_eq!(code, 2);
    assert_eq!(stderr, "Error: invalid --mode value: nope\n");

    let (code, _stdout, stderr) = run({
        let mut cmd = Command::new(program);
        cmd.arg("--set-refuse-message")
            .env("APPLY_PATCH_CONFIG", cfg_path);
        cmd
    });
    assert_eq!(code, 2);
    assert_eq!(stderr, "Error: --set-refuse-message requires a value.\n");

    let (code, _stdout, stderr) = run({
        let mut cmd = Command::new(program);
        cmd.arg("--set-warn-message").env("APPLY_PATCH_CONFIG", cfg_path);
        cmd
    });
    assert_eq!(code, 2);
    assert_eq!(stderr, "Error: --set-warn-message requires a value.\n");

    let (code, _stdout, stderr) = run({
        let mut cmd = Command::new(program);
        cmd.arg("--definitely-not-a-flag")
            .env("APPLY_PATCH_CONFIG", cfg_path);
        cmd
    });
    assert_eq!(code, 2);
    assert_eq!(stderr, "Error: unknown option: --definitely-not-a-flag\n");
}

fn assert_empty_stdin_usage(program: &Path) {
    let (code, _stdout, stderr) = run_with_stdin(Command::new(program), "");
    assert_eq!(code, 2);
    assert!(stderr.contains("Usage:"), "stderr:\n{stderr}");
}

fn assert_two_patch_args_usage(program: &Path, cfg_path: &Path) {
    let (code, _stdout, stderr) = run({
        let mut cmd = Command::new(program);
        cmd.env("APPLY_PATCH_CONFIG", cfg_path)
            .arg("patch1")
            .arg("patch2");
        cmd
    });
    assert_eq!(code, 2);
    assert_eq!(stderr, "Error: apply_patch accepts exactly one argument.\n");
}

fn assert_help_exits_zero(program: &Path) {
    let (code, _stdout, stderr) = run({
        let mut cmd = Command::new(program);
        cmd.arg("--help");
        cmd
    });
    assert_eq!(code, 0, "stderr:\n{stderr}");
}

fn assert_config_path_error_when_env_missing(program: &Path) {
    let (code, _stdout, stderr) = run({
        let mut cmd = Command::new(program);
        cmd.arg("--show-config")
            .env_remove("APPLY_PATCH_CONFIG")
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("HOME");
        cmd
    });
    assert_eq!(code, 1);
    assert_eq!(
        stderr,
        "Error: could not determine config path (HOME/XDG_CONFIG_HOME not set).\n"
    );
}

fn assert_show_config_uses_dot_apply_patch(program: &Path) {
    let xdg = TempDir::new();
    let (code, stdout, stderr) = run({
        let mut cmd = Command::new(program);
        cmd.arg("--show-config")
            .env("XDG_CONFIG_HOME", xdg.path())
            .env_remove("APPLY_PATCH_CONFIG");
        cmd
    });
    assert_eq!(code, 0, "stderr:\n{stderr}");
    assert!(
        stderr.is_empty(),
        "expected empty stderr, got:\n{stderr}"
    );
    assert!(
        stdout.contains(".apply_patch"),
        "expected config path to contain .apply_patch, got:\n{stdout}"
    );
    assert!(stdout.contains("config.json"), "stdout:\n{stdout}");
    assert!(stdout.contains("mode: apply"), "stdout:\n{stdout}");
}

fn assert_refuse_and_warn_modes(program: &Path, cfg_path: &Path) {
    let work = TempDir::new();

    // Refuse mode.
    let (code, stdout, stderr) = run({
        let mut cmd = Command::new(program);
        cmd.arg("--refuse")
            .env("APPLY_PATCH_CONFIG", cfg_path)
            .env_remove("XDG_CONFIG_HOME");
        cmd
    });
    assert_eq!(code, 0, "stderr:\n{stderr}");
    assert!(stderr.is_empty(), "stderr:\n{stderr}");
    assert!(stdout.contains("Updated config:"), "stdout:\n{stdout}");

    let patch = add_file_patch("hello.txt", &["hello"]);
    let (code, stdout, stderr) = run_with_stdin({
        let mut cmd = Command::new(program);
        cmd.current_dir(work.path())
            .env("APPLY_PATCH_CONFIG", cfg_path);
        cmd
    }, &patch);
    assert_eq!(code, 0, "stderr:\n{stderr}");
    assert!(stderr.is_empty(), "stderr:\n{stderr}");
    assert!(
        stdout.contains("nothing was changed"),
        "stdout:\n{stdout}"
    );
    assert!(!work.path().join("hello.txt").exists());

    // Warn mode.
    let (code, stdout, stderr) = run({
        let mut cmd = Command::new(program);
        cmd.arg("--warn").env("APPLY_PATCH_CONFIG", cfg_path);
        cmd
    });
    assert_eq!(code, 0, "stderr:\n{stderr}");
    assert!(stderr.is_empty(), "stderr:\n{stderr}");
    assert!(stdout.contains("Updated config:"), "stdout:\n{stdout}");

    let (code, stdout, stderr) = run_with_stdin({
        let mut cmd = Command::new(program);
        cmd.current_dir(work.path())
            .env("APPLY_PATCH_CONFIG", cfg_path);
        cmd
    }, &patch);
    assert_eq!(code, 0, "stderr:\n{stderr}");
    assert!(stderr.is_empty(), "stderr:\n{stderr}");
    assert!(
        stdout.contains("Success. Updated the following files:"),
        "stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("NOTE TO LLM:"),
        "expected warn message, got:\n{stdout}"
    );
    assert_eq!(
        std::fs::read_to_string(work.path().join("hello.txt")).unwrap(),
        "hello\n"
    );
}

fn assert_custom_messages(program: &Path, cfg_path: &Path) {
    let work = TempDir::new();

    // Set refuse + custom message.
    let (code, _stdout, stderr) = run({
        let mut cmd = Command::new(program);
        cmd.arg("--refuse")
            .arg("--set-refuse-message")
            .arg("REFUSED_CUSTOM")
            .env("APPLY_PATCH_CONFIG", cfg_path);
        cmd
    });
    assert_eq!(code, 0, "stderr:\n{stderr}");

    let patch = add_file_patch("nope.txt", &["nope"]);
    let (code, stdout, stderr) = run_with_stdin({
        let mut cmd = Command::new(program);
        cmd.current_dir(work.path())
            .env("APPLY_PATCH_CONFIG", cfg_path);
        cmd
    }, &patch);
    assert_eq!(code, 0, "stderr:\n{stderr}");
    assert!(stderr.is_empty(), "stderr:\n{stderr}");
    assert_eq!(stdout.trim_end(), "REFUSED_CUSTOM");
    assert!(!work.path().join("nope.txt").exists());

    // Clear refuse message -> default message should appear.
    let (code, _stdout, stderr) = run({
        let mut cmd = Command::new(program);
        cmd.arg("--clear-refuse-message")
            .env("APPLY_PATCH_CONFIG", cfg_path);
        cmd
    });
    assert_eq!(code, 0, "stderr:\n{stderr}");

    let (code, stdout, stderr) = run_with_stdin({
        let mut cmd = Command::new(program);
        cmd.current_dir(work.path())
            .env("APPLY_PATCH_CONFIG", cfg_path);
        cmd
    }, &patch);
    assert_eq!(code, 0, "stderr:\n{stderr}");
    assert!(stderr.is_empty(), "stderr:\n{stderr}");
    assert!(stdout.contains("NOTE TO LLM:"), "stdout:\n{stdout}");

    // Warn + custom message should apply patch and include custom warn text.
    let (code, _stdout, stderr) = run({
        let mut cmd = Command::new(program);
        cmd.arg("--warn")
            .arg("--set-warn-message")
            .arg("WARN_CUSTOM")
            .env("APPLY_PATCH_CONFIG", cfg_path);
        cmd
    });
    assert_eq!(code, 0, "stderr:\n{stderr}");

    let patch2 = add_file_patch("yep.txt", &["yep"]);
    let (code, stdout, stderr) = run_with_stdin({
        let mut cmd = Command::new(program);
        cmd.current_dir(work.path())
            .env("APPLY_PATCH_CONFIG", cfg_path);
        cmd
    }, &patch2);
    assert_eq!(code, 0, "stderr:\n{stderr}");
    assert!(stderr.is_empty(), "stderr:\n{stderr}");
    assert!(stdout.contains("A yep.txt"), "stdout:\n{stdout}");
    assert!(stdout.contains("WARN_CUSTOM"), "stdout:\n{stdout}");
    assert_eq!(
        std::fs::read_to_string(work.path().join("yep.txt")).unwrap(),
        "yep\n"
    );

    // Clear warn message -> default warn banner should appear again.
    let (code, _stdout, stderr) = run({
        let mut cmd = Command::new(program);
        cmd.arg("--clear-warn-message")
            .env("APPLY_PATCH_CONFIG", cfg_path);
        cmd
    });
    assert_eq!(code, 0, "stderr:\n{stderr}");

    let patch2b = add_file_patch("yep2.txt", &["yep2"]);
    let (code, stdout, stderr) = run_with_stdin({
        let mut cmd = Command::new(program);
        cmd.current_dir(work.path())
            .env("APPLY_PATCH_CONFIG", cfg_path);
        cmd
    }, &patch2b);
    assert_eq!(code, 0, "stderr:\n{stderr}");
    assert!(stderr.is_empty(), "stderr:\n{stderr}");
    assert!(stdout.contains("A yep2.txt"), "stdout:\n{stdout}");
    assert!(stdout.contains("NOTE TO LLM:"), "stdout:\n{stdout}");
    assert!(
        !stdout.contains("WARN_CUSTOM"),
        "expected custom warn text to be cleared, got:\n{stdout}"
    );
    assert_eq!(
        std::fs::read_to_string(work.path().join("yep2.txt")).unwrap(),
        "yep2\n"
    );

    // Reset to apply mode should stop printing warn message.
    let (code, _stdout, stderr) = run({
        let mut cmd = Command::new(program);
        cmd.arg("--apply").env("APPLY_PATCH_CONFIG", cfg_path);
        cmd
    });
    assert_eq!(code, 0, "stderr:\n{stderr}");

    let patch3 = update_file_patch("yep.txt", "yep", "yup");
    let (code, stdout, stderr) = run_with_stdin({
        let mut cmd = Command::new(program);
        cmd.current_dir(work.path())
            .env("APPLY_PATCH_CONFIG", cfg_path);
        cmd
    }, &patch3);
    assert_eq!(code, 0, "stderr:\n{stderr}");
    assert!(stderr.is_empty(), "stderr:\n{stderr}");
    assert!(stdout.contains("M yep.txt"), "stdout:\n{stdout}");
    assert!(
        !stdout.contains("NOTE TO LLM:") && !stdout.contains("WARN_CUSTOM"),
        "expected no warning in apply mode, got:\n{stdout}"
    );
    assert_eq!(
        std::fs::read_to_string(work.path().join("yep.txt")).unwrap(),
        "yup\n"
    );
}

fn assert_config_flags_cannot_mix_with_patch_arg(program: &Path, cfg_path: &Path) {
    let (code, _stdout, stderr) = run({
        let mut cmd = Command::new(program);
        cmd.arg("--refuse")
            .arg("not-a-patch")
            .env("APPLY_PATCH_CONFIG", cfg_path);
        cmd
    });
    assert_eq!(code, 2);
    assert!(
        stderr.contains("configuration flags cannot be combined"),
        "stderr:\n{stderr}"
    );

    let (code, _stdout, stderr) = run({
        let mut cmd = Command::new(program);
        cmd.arg("--show-config")
            .arg("not-a-patch")
            .env("APPLY_PATCH_CONFIG", cfg_path);
        cmd
    });
    assert_eq!(code, 2);
    assert!(
        stderr.contains("configuration flags cannot be combined"),
        "stderr:\n{stderr}"
    );
}

#[test]
fn rust_binary_config_path_and_modes() {
    assert_show_config_uses_dot_apply_patch(&bin_path());

    let cfgdir = TempDir::new();
    let cfg_path = cfgdir.path().join("config.json");

    let program = bin_path();
    assert_refuse_and_warn_modes(&program, &cfg_path);
    assert_custom_messages(&program, &cfg_path);
    assert_patch_argument_mode(&program, &cfg_path);
    assert_non_patch_argument_errors(&program, &cfg_path);
    assert_config_flag_error_paths(&program, &cfg_path);
    assert_empty_stdin_usage(&program);
    assert_two_patch_args_usage(&program, &cfg_path);
    assert_help_exits_zero(&program);
    assert_config_path_error_when_env_missing(&program);
    assert_config_flags_cannot_mix_with_patch_arg(&program, &cfg_path);
}

#[test]
#[cfg(unix)]
fn script_config_path_and_modes() {
    let script = script_path();
    assert_show_config_uses_dot_apply_patch(&script);

    let cfgdir = TempDir::new();
    let cfg_path = cfgdir.path().join("config.json");

    assert_refuse_and_warn_modes(&script, &cfg_path);
    assert_custom_messages(&script, &cfg_path);
    assert_patch_argument_mode(&script, &cfg_path);
    assert_non_patch_argument_errors(&script, &cfg_path);
    assert_config_flag_error_paths(&script, &cfg_path);
    assert_empty_stdin_usage(&script);
    assert_two_patch_args_usage(&script, &cfg_path);
    assert_help_exits_zero(&script);
    assert_config_flags_cannot_mix_with_patch_arg(&script, &cfg_path);
}
