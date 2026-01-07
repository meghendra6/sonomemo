# MemoLog

MemoLog is a terminal-based daily memo and task logger that stores everything as plain Markdown.
It is designed for fast capture, lightweight planning, and working directly in your editor or Obsidian.

This project was forked from https://github.com/sonohoshi/sonomemo.

## Highlights

- Timeline-first daily log with multi-line entries
- Agenda timeline view built from schedule metadata
- Tasks, priorities, tags, and a pomodoro timer
- Folding with persistent state stored in the Markdown file
- Vim-style composer (Normal/Insert/Visual) with configurable keybindings

## Quick start

1) Install

```bash
cargo install memolog
```

2) Run

```bash
memolog
```

3) Capture a note

- Press `i` to open the composer
- Write your note (multi-line is OK)
- Press `Shift+Enter` to save

## Interface overview

- Left: Timeline (your daily log entries)
- Right top: Agenda timeline (scheduled tasks and notes)
- Right bottom: Tasks (filtered task list)

Use `Ctrl+H/J/K/L` to move focus between Timeline, Agenda, and Tasks.
Press `?` anytime to see the full help overlay.

## Data model

- Logs are stored as `YYYY-MM-DD.md` under `data.log_path`.
- Each log entry is a block:
  - Heading line: `## [HH:MM:SS]`
  - Body lines: stored as-is
- Tasks are Markdown checkboxes: `- [ ]` and `- [x]`.
- Tags are words starting with `#` (example: `#work`).

## Timeline

- Navigate with `j/k`, edit entry with `e`.
- Toggle task checkbox with `Space`.
- Fold/unfold with `Tab`. Cycle fold mode with `Shift+Tab`.
- Fold state is persisted per entry using hidden HTML comments.
  Obsidian ignores these, so the content stays clean.

## Agenda timeline

Agenda is built from schedule metadata embedded in tasks and notes:

- Tasks with schedule metadata appear in the timeline.
- Non-task lines with schedule metadata also appear (as notes).
- Unscheduled tasks can be shown in a separate section.

Agenda controls (when focused):

- `j/k` move selection
- `Enter` open memo preview
- `Space` toggle task checkbox (tasks only)
- `h/l` day navigation, `PgUp/PgDn` week navigation
- `f` filter (Open -> Done -> All)
- `u` toggle unscheduled section

## Tasks panel

- `j/k` move
- `Space` toggle checkbox
- `Shift+P` cycle priority
- `p` start/stop pomodoro
- `e` open source entry

## Composer (editing)

MemoLog uses a Vim-style composer by default.

- `i` enter composer
- `Shift+Enter` save and exit
- `Esc` exit (shows confirm popup)
  - `y` or `Enter` save and exit
  - `d` discard
  - `n` or `Esc` cancel

Insert mode shortcuts:

- `Ctrl+T` toggle task checkbox
- `Ctrl+P` cycle priority
- `Ctrl+;` open date/time picker
- `Tab`/`Shift+Tab` indent/outdent

Normal/Visual mode:

- Arrow keys move the cursor (in addition to `h/j/k/l`)
- `ZZ` save and exit (Vim)
- `ZQ` discard and exit (Vim)
- `ciw`/`diw`/`yiw` change/delete/yank inner word

## Task priorities

Use markers right after the checkbox:

```
- [ ] [#A] Important task
- [ ] [#B] Normal task
- [ ] [#C] Low priority
```

Priority order: A -> B -> C -> none.

## Scheduling metadata (agenda/timeline)

Use inline tokens (Obsidian-friendly):

- `@sched(YYYY-MM-DD)`
- `@due(YYYY-MM-DD)`
- `@start(YYYY-MM-DD)`
- `@time(HH:MM)`
- `@dur(30m|1h|90m)`

Example:

```
- [ ] [#A] Plan sprint @sched(2025-01-10) @time(10:00) @dur(90m)
```

Date picker (`Ctrl+;`) supports relative input:

- `today`, `tomorrow`, `next mon`
- `+3d`, `+2w`
- `14:30`

## Pomodoro

Start a pomodoro from the Tasks panel with `p`.
When it completes, MemoLog appends a tomato (🍅) to the task line.

## Search and tags

- `/` opens search
- `t` shows tag list (tags are any `#word` in your logs)
- AI search: prefix your query with `?`, `ai:`, or `ask:` to run a Gemini-assisted search

## Gemini AI search (experimental)

MemoLog can analyze a question with Gemini, extract keywords, search local logs, and answer
from your own notes (no external browsing). It makes two API calls per question.
The default setup uses `gemma-3-27b` for keyword extraction and `gemini-3-flash` for answers.

1) Enable Gemini in `config.toml`

```toml
[gemini]
enabled = true
api_key = "YOUR_GEMINI_API_KEY"
model = "gemini-3-flash"
extraction_model = "gemma-3-27b"
answer_model = "gemini-3-flash"
max_keywords = 6
max_results = 8
max_entry_chars = 1200
timeout_seconds = 20
```

2) Ask a question

- Press `/` and type `?your question` (or `ai:` / `ask:` prefix)
- MemoLog will show the AI answer and the matched entries

## Configuration

MemoLog loads `config.toml` from the OS config directory by default.
You can always override the path with `MEMOLOG_CONFIG`.

Environment variables:

- `MEMOLOG_CONFIG`: override config file path
- `MEMOLOG_DATA_DIR`: override default data directory
- `MEMOLOG_LOG_DIR`: override default log directory (used as default `data.log_path`)
- `GEMINI_API_KEY`: Gemini API key (used if `gemini.api_key` is empty)

The repository root includes a small `config.toml` you can copy and edit.
You can also open the active config directly inside MemoLog:

- Press `,` to open `config.toml` in the composer.
- Edit and save with `Shift+Enter`.
- Restart MemoLog to apply changes.

### Google sync (Calendar + Tasks, experimental)

MemoLog can sync tasks and scheduled notes with Google Tasks + Calendar (two-way). Experimental.
This uses a local browser redirect (temporary localhost listener) and stores tokens locally.

1) Prepare Google Cloud
- Create a Google Cloud project
- Enable APIs: "Google Calendar API" and "Google Tasks API"
- Create OAuth credentials for a Desktop app
- Copy the client ID and client secret

2) Configure `config.toml`

```toml
[google]
enabled = true
client_id = "YOUR_CLIENT_ID.apps.googleusercontent.com"
client_secret = "YOUR_CLIENT_SECRET"
calendar_id = "primary"
tasks_list_id = "@default"
sync_tasks_to_calendar = true
sync_past_days = 30
sync_future_days = 365
conflict_policy = "prefer_local"
# token_path = "/path/to/google_token.json"
# sync_state_path = "/path/to/google_sync_state.json"
```

Notes:
- `calendar_id`: use `"primary"` for your main calendar, or a specific calendar ID.
- `tasks_list_id`: `"@default"` is your default list. To use another list, fetch its ID from the Tasks API.
- `token_path` and `sync_state_path` are optional overrides. By default they live in your OS config directory.

3) Authorize and sync
- Launch MemoLog
- Press `Ctrl+G`
- Open the URL shown in the popup and approve access
- The browser will redirect to localhost and MemoLog will finish the login

What syncs
- Tasks: Markdown checkboxes are synced to Google Tasks.
  - `@due`/`@sched`/`@start` are mapped to task due date/time.
  - Completion state is synced.
- Calendar events:
  - Notes with schedule metadata (`@sched`, `@start`, `@due`, `@time`, `@dur`) sync as events.
  - If `sync_tasks_to_calendar = true`, tasks also sync as events.
    Tasks without schedule metadata become all-day events on the log date.

Conflict policy
- `prefer_local`: local edits win when both sides changed since last sync.
- `prefer_remote`: Google edits win in the same case.

Sync range
- `sync_past_days` and `sync_future_days` limit the calendar sync window.
- Tasks are pulled from the entire Google Tasks list and merged with local items in the date range.

Limitations
- Deletions are not synced. Removing items in Google may cause them to be re-created from local data.

### Theme

You can customize UI colors by adding a `[theme]` section to `config.toml`.
Colors accept built-in names (case-insensitive) or RGB values in `R,G,B` form.

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

Theme presets can be selected via config or the Theme Switcher popup:

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

Global
- `?` help
- `Ctrl+H/J/K/L` focus move
- `a` agenda focus
- `i` compose
- `/` search
- `t` tags
- `g` activity
- `T` theme presets
- `p` pomodoro
- `o` log dir
- `Ctrl+G` google sync (experimental)
- `Ctrl+Q` quit

Timeline
- `j/k` move
- `Tab` fold entry
- `Shift+Tab` cycle fold mode
- `e` edit entry
- `Space` complete tasks

Agenda
- `j/k` move
- `Enter` memo preview
- `Space` toggle task
- `h/l` day navigation
- `PgUp/PgDn` week navigation
- `f` filter
- `u` unscheduled toggle

Tasks
- `j/k` move
- `Space` toggle checkbox
- `Enter` memo preview
- `Shift+P` cycle priority
- `p` pomodoro
- `e` edit source

Composer
- `Enter` newline
- `Shift+Enter` save
- `Ctrl+T` toggle task
- `Ctrl+P` cycle priority
- `Ctrl+;` date picker
- `Tab/Shift+Tab` indent/outdent
- `Esc` back

## License

MIT. See `LICENSE`.
