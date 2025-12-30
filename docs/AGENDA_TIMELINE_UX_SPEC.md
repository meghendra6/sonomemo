# Agenda Timeline UX Spec (Implementation-Focused)

Purpose
- Add a timeline-style agenda view that is usable without manual date typing.
- Replace the separate agenda popup with an always-visible agenda panel.
- Keep all scheduling metadata inside plain Markdown lines for Obsidian compatibility.
- Be explicit enough to implement directly (data model, parsing rules, UI layout, actions).

Scope (v2)
- Agenda panel: timeline view for a selected day (no separate popup list).
- Right panel is split: Agenda (top) + Tasks (bottom).
- Task metadata: scheduled, due, start, time, duration (optional).
- Notes (non-task lines) with dates also appear in agenda.
- Fast date/time input via a picker + relative shortcuts.
- Enter on agenda item opens a memo preview popup.

Non-goals (v2)
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

2.3 AgendaEntry (merged source)
enum AgendaEntryKind { Task, Note }
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

3) Agenda Panel (UI)

3.1 Layout (Right Panel Split)
- Screen layout: left Timeline, right panel split horizontally.
- Right panel top: Agenda (timeline view for selected day).
- Right panel bottom: Tasks (existing tasks list).
- Default split ratio: Agenda 60%, Tasks 40% (adjustable later if needed).
- Agenda is always visible; no separate agenda popup in v2.

3.2 Timeline View (Agenda Panel, selected day)
Header:
- "Agenda" + date label + filter state + hints (eg: "Open | Day").
Body:
- Section 1: Overdue (if any)
- Section 2: All-day (date-only items)
- Section 3: Timed (time-ordered list with a fixed time column)
- Section 4: Unscheduled (optional, collapsed by default)

Rendering rules
- Task with time: appears in Timed at its time.
- Task with duration: show "(90m)" after text (no block spanning in v2).
- Task with only date (scheduled/due/start): appears in All-day with badges.
- Task with no date/time: appears in Unscheduled section (collapsed by default).
- Note (non-task line) with date/time: appears with "•" prefix (no checkbox).
- Log entries are not shown in Agenda v2 (Timeline already provides them).
- Done tasks: hidden by default, shown when filter includes Done/All.

3.3 Memo Preview Popup
- Enter on any agenda item opens a read-only memo preview popup.
- Popup shows the full entry containing the item.
- Keys: Esc close, E edit entry, J/K scroll.

3.4 Day Navigation
- H/L or Left/Right: move selected day by 1 day.
- PgUp/PgDn: move by 1 week.
- G: jump to today.

3.5 Focus Model
- Focus targets: Timeline (left), Agenda (right-top), Tasks (right-bottom), Composer.
- Tab / Shift+Tab cycles focus across these panes.
- Global focus bindings:
  - Ctrl+H/J/K/L: move focus (state-aware)
  - A: focus Agenda
  - I: focus Composer

4) Interaction and Keybindings

Agenda panel (focused)
- Up/Down or J/K: move selection
- Enter: open memo preview popup
- Space: toggle task checkbox (tasks only)
- F: cycle filter (Open -> Done -> All)
- U: toggle Unscheduled section
- H/L or Left/Right: change day
- PgUp/PgDn: change week
- G: jump to today

Timeline panel (focused)
- Space toggles task checkbox (no Enter toggle).
- Enter opens edit for the entry (keeps toggle on Space only).

Tasks panel (focused)
- Space toggles task checkbox (no Enter toggle).
- Enter opens source entry (or edit).

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
  - Merge tasks + notes
  - Notes: any non-task line with valid date tokens
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
- Agenda panel renders with Timeline + Tasks split.
- Day navigation and filters in agenda panel.
- Overdue rendering.
- Memo preview popup opens from agenda.

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
