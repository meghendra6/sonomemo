# Agenda Timeline UX Spec (Implementation-Focused)

Purpose
- Add a timeline-style agenda view that is usable without manual date typing.
- Keep all scheduling metadata inside plain Markdown lines for Obsidian compatibility.
- Be explicit enough to implement directly (data model, parsing rules, UI layout, actions).

Scope (v1)
- Agenda: list view + timeline view for a selected day.
- Task metadata: scheduled, due, start, time, duration (optional).
- Fast date/time input via a picker + relative shortcuts.
- Jump from agenda to timeline entry (existing behavior).

Non-goals (v1)
- Full week grid with multi-day blocks.
- Recurrence rules (daily/weekly) beyond a simple placeholder.
- Time tracking / time blocks tied to actual effort.

1) Metadata Format (Markdown/Obsidian-Friendly)
Canonical tokens (ASCII, inline, order-free):
- @sched(YYYY-MM-DD)
- @due(YYYY-MM-DD)
- @start(YYYY-MM-DD)
- @time(HH:MM)
- @dur(30m|1h|90m)

Rules
- Tokens can appear anywhere in the task line, but UI inserts them at the end.
- Tokens are removed from display text but retained in the file.
- If a token appears multiple times, the last one wins.
- Unknown tokens are ignored (leave in text).

Obsidian compatibility (aliases)
- Also accept Dataview-style aliases:
  - scheduled:: YYYY-MM-DD
  - due:: YYYY-MM-DD
  - start:: YYYY-MM-DD
  - time:: HH:MM
  - duration:: 30m
- Optional (future): accept Obsidian Tasks emoji markers as aliases.
  - Due (calendar) U+1F4C5
  - Scheduled (hourglass) U+23F3
  - Time (alarm clock) U+23F0

Examples
- [ ] Write spec @sched(2025-01-15) @time(09:30) @dur(90m)
- [ ] Submit report @due(2025-01-18)
- [ ] Draft proposal @start(2025-01-10)
- [x] Done task @due(2025-01-05)

2) Parsing and Data Model

2.1 TaskSchedule (new)
struct TaskSchedule {
  scheduled: Option<NaiveDate>
  due: Option<NaiveDate>
  start: Option<NaiveDate>
  time: Option<NaiveTime>
  duration_minutes: Option<u32>
}

2.2 ParsedTask (extend or wrap)
struct TaskItem {
  ...
  schedule: TaskSchedule
  raw_text: String        // line text without list prefix
  display_text: String    // line text with tokens stripped
}

2.3 AgendaEntry (new, merged source)
enum AgendaEntryKind { Task, Log }
struct AgendaEntry {
  kind: AgendaEntryKind
  date: NaiveDate
  time: Option<NaiveTime>
  duration_minutes: Option<u32>
  text: String
  is_done: bool
  priority: Option<Priority>
  source_file: String
  source_line: usize
}

2.4 Derived Date Rules
date_for_agenda (used for grouping/day view):
1) If scheduled exists -> scheduled
2) Else if due exists -> due
3) Else if start exists -> start
4) Else -> file date (current log file date)

Overdue
- A task is overdue if: due exists AND due < selected_day AND not done.
- Overdue tasks appear in a separate "Overdue" lane for the selected day.

3) Agenda Views (UI)

3.1 List View (existing, enhanced)
- Group by date (same as today).
- Show badges inline: [S] for scheduled, [D] for due, [T] for time.
- Overdue tasks appear at the top of each day with a red "OVERDUE" label.
- Sorting inside a day:
  1) Overdue first
  2) Time ascending
  3) Priority (High -> Low -> None)
  4) Line order

3.2 Timeline View (new, selected day)
Layout (popup 80x70 by default)
Header:
- "Agenda: Timeline" + date label + hints
Body:
- Row 1: "All-day" lane for tasks without time or outside range
- Rows below: time-ordered list with a fixed time column (default range 06:00 - 22:00)
Left column (width 6): time labels (e.g., "06:00")
Main column: items listed at their time; no block spanning in v1

Rendering rules
- Task with time: appears at its time row.
- Task with duration: render as a block spanning multiple rows (v1 can show "(90m)" text without block).
- Task with only due/scheduled: shown in All-day lane with badges.
- Log entry (from timeline): show as a dot + text at its timestamp.
- Overdue tasks: show in an "Overdue" lane above All-day.
- Done tasks: dim style; optionally hidden by filter.

3.3 Day Navigation
- Left/Right or H/L: move selected day by 1 day.
- PgUp/PgDn: move by 1 week.
- T: toggle List <-> Timeline.
- Enter: jump to source entry in Timeline panel (existing).

4) Interaction and Keybindings

Agenda popup (new bindings in popup handler)
- Up/Down: move selection
- Enter: jump to selected entry
- Esc: close
- T: toggle view (list/timeline)
- H/L or Left/Right: change day
- F: cycle filter (Open -> Done -> All)

Composer (date/time insert)
- New keybinding: open date/time picker (default: Ctrl+;)
- Optional (future) quick keys:
  - Ctrl+D: set due
  - Ctrl+S: set scheduled
  - Ctrl+T: set time

All new bindings must be configurable under keybindings.composer / keybindings.popup.

5) Date/Time Input UX

5.1 Date/Time Picker Popup (new)
Goals
- Never require manual typing of full YYYY-MM-DD unless user wants it.
- Fast keyboard-only selection.

Popup layout
- Field selector: Scheduled / Due / Start / Time / Duration
- Calendar grid for date fields
- Time row for time field (increment in 15-min steps)
- Footer: "Enter apply, Esc cancel, +/- day, [/] week, T today, R relative"

Relative input (single-line prompt; triggered by R)
Accepted inputs
- today, tomorrow, yesterday
- +3d, +2w, +1m (days/weeks/months)
- next mon, mon, tue, wed, thu, fri, sat, sun
- 2025-01-15 (explicit)
- 14:30, 930 (time)

Insertion rules
- If token already exists, replace it.
- If not, append token to end of line.
- If line has no checkbox, do not add one automatically (just insert token).

6) Rendering Details

Badge format (ASCII)
- [S] scheduled
- [D] due
- [T] time
- [O] overdue

Color guidance
- Due/Overdue: red accent
- Scheduled: blue accent
- Time: highlight
- Done: muted/gray

7) Storage + Parsing Functions

New functions (storage)
- parse_task_metadata(text: &str) -> (TaskSchedule, String)
  - Returns schedule + display_text (tokens removed)
- format_task_metadata(schedule: &TaskSchedule) -> String
- read_agenda_entries(start, end) -> Vec<AgendaEntry>
  - Merge tasks + log entries
  - Apply date_for_agenda and time parsing

Parsing strategy (tokens)
- Scan for @token(value) patterns and dataview aliases.
- Strip tokens in reverse order to preserve spacing.
- Parse date as YYYY-MM-DD, time as HH:MM (24h).
- Duration: accept "30m", "1h", "1h30m", "90m".

8) States and Filters

Agenda state (App)
- agenda_view: List | Timeline
- agenda_selected_day: NaiveDate
- agenda_filter: Open | Done | All

Behavior
- Open default: today
- If filter excludes selected item, keep selection index stable if possible.

9) Testing Plan

Unit tests
- parse_task_metadata: multiple tokens, replacement, spacing.
- date parser: today/tomorrow/+3d/next mon.
- time parser: 930 -> 09:30, 14:30 valid, 25:00 invalid.
- agenda grouping rules (scheduled vs due vs file date).

UI tests (manual)
- Toggle list/timeline, day navigation.
- Overdue rendering.
- Jump to timeline entry.

10) PR Design (Breakdown)

PR1: Spec + scaffolding
- Add this spec doc.
- Add empty types/enums (TaskSchedule, AgendaEntry) behind feature flags if needed.

PR2: Parsing + models
- storage: parse_task_metadata, format_task_metadata.
- models: TaskSchedule, AgendaEntry.
- tests for parsing/date/time.

PR3: Agenda data pipeline
- storage: read_agenda_entries(start, end).
- app: agenda state (view + selected day + filter).
- agenda list view uses display_text + badges.

PR4: Timeline view UI
- ui/popups: render_agenda_timeline_view.
- input/popups: timeline navigation keys.
- selection + jump logic.

PR5: Date/time picker
- new popup for picker (ui + input).
- composer integration for inserting tokens.
- config defaults + README updates.

PR6: Polish
- colors/styles, empty states, error messages.
- additional tests + docs.
