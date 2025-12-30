# MemoLog

MemoLog is a terminal-based daily memo + task logger that writes to plain Markdown files.

This project was forked from https://github.com/sonohoshi/sonomemo.

## What it does

- **Timeline**: browse and edit timestamped log entries (multi-line supported)
- **Tasks**: detect Markdown checkboxes (`- [ ]`, `- [x]`) and toggle them
- **Agenda view**: see open tasks from recent days in one list
- **Outlining/folding**: collapse Timeline entries for quick overview
- **Task priorities**: add `[#A]`, `[#B]`, `[#C]` after the checkbox to mark priority
- **Pomodoro per task**: start a timer for a selected task; when it completes, MemoLog appends `🍅` to that task line
- **Search / Tags**: find entries across days
- **Markdown rendering**: lists (multi-level), checkboxes, headings, inline code, code fences, links, tags
- **Vim-first TUI**: focus switching + navigation optimized for tmux splits

## Install

### From crates.io

`cargo install memolog`

### From source

```bash
git clone https://github.com/meghendra6/sonomemo.git memolog
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

The repository root also includes a small `config.toml` you can copy and edit.

### Theme

You can customize the UI colors by adding a `[theme]` section to `config.toml`.
Colors accept the built-in names (case-insensitive) or RGB values in `R,G,B` form.
If `[theme]` is omitted, MemoLog uses a theme preset (see `[ui] theme_preset`).

```toml
[theme]
border_default = "Blue"
border_editing = "Cyan"
border_search = "LightBlue"
border_todo_header = "Cyan"
text_highlight = "0,0,100"
todo_done = "LightGreen"
todo_wip = "Magenta"
tag = "Cyan"
mood = "Blue"
timestamp = "LightCyan"
```

Theme presets can be selected via config or the Theme Switcher popup.

```toml
[ui]
theme_preset = "Dracula Dark"
```

Available presets:
- Dracula Dark
- Solarized Dark
- Solarized Light
- Nord Calm
- Mono Contrast

## Keybindings (defaults)

All keybindings are configurable in `config.toml`.

- Global: `?` help, `h/l` focus, `i` compose, `/` search, `t` tags, `g` activity, `A` agenda, `T` theme presets, `p` pomodoro, `o` log dir, `Ctrl+Q` quit
- Timeline: `j/k` move, `Tab` fold entry, `Shift+Tab` cycle overview/contents/show-all, `e` edit entry, `Enter/Space` toggle checkbox
- Tasks: `j/k` move, `Enter/Space` toggle checkbox, `p` start/stop pomodoro, `e` edit source entry
- Composer: `Enter` newline, `Shift+Enter` save, `Tab/Shift+Tab` indent/outdent, `Esc` back

## License

MIT. See `LICENSE`.
