# Patch-22

Patch-22 provides an `apply_patch` command you can put on your `PATH` as a safety net when a model tries to run `apply_patch` as a shell command.

It can also optionally print an LLM-facing warning (or outright refuse to patch) to nudge the model back toward its native editing tool (see Configuration below).

It ensures that running a Codex-style patch block in a shell:

```bash
apply_patch <<'PATCH'
*** Begin Patch
*** Update File: some/file.txt
@@
 hello
-world
+there
*** End Patch
PATCH
```

…actually applies the patch, instead of failing with “command not found”.

## Why “Patch-22”?

Because it’s a patch-shaped Catch‑22: the model keeps trying to run `apply_patch` as a shell command, and the whole point is to convince it to stop. So this repo provides a safety net (apply/refuse/warn) that hopefully gets used once, prints a stern note, and then never gets invoked again.

## Install

### Option B (recommended): build a standalone Rust binary (Codex reuse)

This repo vendors the `codex-rs/apply-patch` crate from the Codex repository (see `vendor/CODEX_COMMIT`) and builds a tiny `apply_patch` binary that reads from stdin.

```bash
cargo install --path . --locked
```

Ensure `~/.cargo/bin` is on your `PATH`.

### Option A: use the Bash/Python fallback script

From this directory:

```bash
chmod +x ./apply_patch
mkdir -p ~/.local/bin
ln -sf "$PWD/apply_patch" ~/.local/bin/apply_patch
export PATH="$HOME/.local/bin:$PATH"
```

## Notes

- Reads the patch from stdin.
- Supports `*** Add File:`, `*** Update File:` (with optional `*** Move to:`), and `*** Delete File:`.
- Option A (script) is a Python implementation intended to match the vendored Codex behavior/output as closely as possible; Option B is still preferred.

## Configuration (LLM Guardrails)

The goal is for this tool to be used as little as possible.

It can be configured to either:
- **Refuse patching** and print an instruction message for the model (`refuse` mode), or
- **Apply the patch** and also print an instruction message (`warn` mode).

In other words: ideally you run this once, it prints a stern note to the LLM about using the model-native patching tool instead, and then it never gets invoked again.

### Defaults

- Default mode: `apply` (applies the patch; prints no LLM banner).
- `--apply`, `--refuse`, `--warn`: aliases that set `mode` to `apply` / `refuse` / `warn` (persisted).
- Default refuse banner: built-in `DEFAULT_REFUSE_MESSAGE` (used when `mode=refuse` and `refuse_message` is unset).
- Default warn banner: built-in `DEFAULT_WARN_MESSAGE` (used when `mode=warn` and `warn_message` is unset).
- `--set-refuse-message <text>` / `--clear-refuse-message`: set custom refuse banner / revert to default.
- `--set-warn-message <text>` / `--clear-warn-message`: set custom warn banner / revert to default.

### Config Location

- `$APPLY_PATCH_CONFIG` if set, otherwise `$XDG_CONFIG_HOME/.apply_patch/config.json`, otherwise `~/.apply_patch/config.json`.
- If neither `HOME` nor `XDG_CONFIG_HOME` is set and you run a config command (e.g. `--show-config`), it exits `1` with:
  `Error: could not determine config path (HOME/XDG_CONFIG_HOME not set).`

Examples:

```bash
apply_patch --show-config
apply_patch --refuse   # persistently refuse patching
apply_patch --warn     # persistently apply + print message
apply_patch --apply    # back to normal behavior (default)

apply_patch --set-refuse-message "..."   # customize message
apply_patch --clear-refuse-message
apply_patch --set-warn-message "..."
apply_patch --clear-warn-message
```

## License & Attribution

- This project is licensed under Apache-2.0 (see `LICENSE`).
- It vendors `codex-rs/apply-patch` from OpenAI Codex at `vendor/CODEX_COMMIT` (see `vendor/CODEX_LICENSE` and `vendor/CODEX_NOTICE`).
