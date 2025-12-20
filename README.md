# MemoLog

MemoLog is a terminal-based daily memo + task logger that writes to plain Markdown files.

This project was forked from https://github.com/sonohoshi/sonomemo.

## What it does

- **Timeline**: browse and edit timestamped log entries (multi-line supported)
- **Tasks**: detect Markdown checkboxes (`- [ ]`, `- [x]`) and toggle them
- **Pomodoro per task**: start a timer for a selected task; when it completes, MemoLog appends `🍅` to that task line
- **Search / Tags**: find entries across days
- **Markdown rendering**: lists (multi-level), checkboxes, headings, inline code, code fences, links, tags
- **Vim-first TUI**: focus switching + navigation optimized for tmux splits

## Install

### From crates.io

`cargo install memolog`

### From source

```bash
git clone https://github.com/meghendra6/memolog.git
cd memolog
cargo install --path .
```

## Run

`memolog`

## Data model

- Logs are stored as `YYYY-MM-DD.md` files under `data.log_path`.
- Each log entry is a timestamped block:
  - First line: `[HH:MM:SS] <your first line>`
  - Following lines: stored as-is (no auto prefix insertion)
- App state is stored at `<log_path>/.memolog/state.toml` (carryover bookkeeping, etc.)

## Configuration

MemoLog loads `config.toml` from the OS config directory by default.

### Environment variables

- `MEMOLOG_CONFIG`: override config file path
- `MEMOLOG_DATA_DIR`: override default data directory
- `MEMOLOG_LOG_DIR`: override default log directory (used as default `data.log_path`)

### Example

See `examples/full_config_sample.toml`.

The repository root also includes a small `config.toml` you can copy and edit.

## Keybindings (defaults)

All keybindings are configurable in `config.toml`.

- Global: `?` help, `h/l` focus, `i` compose, `/` search, `t` tags, `p` pomodoro, `g` activity, `o` log dir, `Ctrl+Q` quit
- Timeline: `j/k` move, `e` edit entry, `Enter/Space` toggle checkbox
- Tasks: `j/k` move, `Enter/Space` toggle checkbox, `p` start/stop pomodoro, `e` edit source entry
- Composer: `Enter` newline, `Shift+Enter` save, `Tab/Shift+Tab` indent/outdent, `Esc` back

## License

MIT. See `LICENSE`.
