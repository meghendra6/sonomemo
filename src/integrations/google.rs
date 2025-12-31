use crate::config::{Config, GoogleConfig, google_sync_state_path, google_token_path};
use crate::models::{AgendaItem, AgendaItemKind, TaskSchedule};
use crate::storage::{self, NoteLineUpdate, TaskLineUpdate};
use chrono::{DateTime, Duration, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use reqwest::blocking::Client;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{self, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration as StdDuration;

const OAUTH_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const OAUTH_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const CALENDAR_API: &str = "https://www.googleapis.com/calendar/v3";
const TASKS_API: &str = "https://tasks.googleapis.com/tasks/v1";
const DEFAULT_EVENT_DURATION_MINUTES: i64 = 60;

#[derive(Debug)]
pub enum SyncError {
    AuthRequired(AuthSession),
    Config(String),
    Request(String),
    Io(String),
}

impl SyncError {
    pub fn message(&self) -> String {
        match self {
            SyncError::AuthRequired(_) => {
                "Google auth required. Open the auth popup to continue.".to_string()
            }
            SyncError::Config(msg) => msg.clone(),
            SyncError::Request(msg) => msg.clone(),
            SyncError::Io(msg) => msg.clone(),
        }
    }
}

impl From<io::Error> for SyncError {
    fn from(err: io::Error) -> Self {
        SyncError::Io(err.to_string())
    }
}

#[derive(Clone, Debug)]
pub struct AuthDisplay {
    pub auth_url: String,
    pub listen_addr: String,
    pub expires_at: DateTime<Local>,
}

#[derive(Debug)]
pub struct AuthSession {
    pub display: AuthDisplay,
    listener: TcpListener,
    state: String,
    redirect_uri: String,
    expires_at: DateTime<Local>,
}

#[derive(Clone, Debug)]
pub enum AuthPollResult {
    Success,
    Error(String),
}

#[derive(Default, Debug)]
pub struct SyncReport {
    pub tasks_created: usize,
    pub tasks_updated: usize,
    pub tasks_imported: usize,
    pub events_created: usize,
    pub events_updated: usize,
    pub events_imported: usize,
    pub conflicts: usize,
}

impl SyncReport {
    pub fn summary(&self) -> String {
        format!(
            "Tasks +{} ~{} <-{} | Events +{} ~{} <-{} | Conflicts {}",
            self.tasks_created,
            self.tasks_updated,
            self.tasks_imported,
            self.events_created,
            self.events_updated,
            self.events_imported,
            self.conflicts
        )
    }
}

#[derive(Serialize, Deserialize)]
struct StoredToken {
    access_token: String,
    refresh_token: String,
    expires_at: i64,
}

#[derive(Serialize, Deserialize, Default)]
struct SyncState {
    tasks: HashMap<String, SyncItem>,
    events: HashMap<String, SyncItem>,
}

#[derive(Serialize, Deserialize, Clone)]
struct SyncItem {
    google_id: String,
    hash: String,
    remote_updated: Option<String>,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
    refresh_token: Option<String>,
}

#[derive(Deserialize)]
struct TokenErrorResponse {
    error: String,
    error_description: Option<String>,
}

#[derive(Deserialize)]
struct TasksListResponse {
    items: Option<Vec<RemoteTask>>,
}

#[derive(Deserialize, Clone)]
struct RemoteTask {
    id: String,
    title: Option<String>,
    status: Option<String>,
    updated: Option<String>,
    due: Option<String>,
}

#[derive(Deserialize)]
struct EventsListResponse {
    items: Option<Vec<RemoteEvent>>,
}

#[derive(Deserialize, Clone)]
struct RemoteEvent {
    id: String,
    status: Option<String>,
    summary: Option<String>,
    updated: Option<String>,
    start: Option<EventDateTime>,
    end: Option<EventDateTime>,
}

#[derive(Deserialize, Serialize, Clone)]
struct EventDateTime {
    #[serde(rename = "dateTime", skip_serializing_if = "Option::is_none")]
    date_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    date: Option<String>,
}

#[derive(Serialize)]
struct TaskUpdateRequest {
    title: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    due: Option<String>,
}

#[derive(Serialize)]
struct EventUpdateRequest {
    summary: String,
    start: EventDateTime,
    end: EventDateTime,
}

pub fn sync(config: &Config) -> Result<SyncReport, SyncError> {
    let access_token = ensure_access_token(config)?;
    let client = Client::new();
    let mut state = load_sync_state(&google_sync_state_path(config))?;
    let policy = conflict_policy(&config.google.conflict_policy);

    let (start_date, end_date) = sync_range(config);
    let local_tasks = storage::read_tasks_for_date_range(&config.data.log_path, start_date, end_date)?;
    let mut local_events = storage::read_agenda_entries(&config.data.log_path, start_date, end_date)?;
    if !config.google.sync_tasks_to_calendar {
        local_events.retain(|item| item.kind == AgendaItemKind::Note);
    }

    let remote_tasks = list_google_tasks(&client, &access_token, &config.google.tasks_list_id)?;
    let remote_events = list_google_events(
        &client,
        &access_token,
        &config.google.calendar_id,
        start_date,
        end_date,
    )?;

    let mut report = SyncReport::default();
    sync_tasks(
        config,
        &client,
        &access_token,
        &mut state,
        &local_tasks,
        &remote_tasks,
        policy,
        &mut report,
    )?;
    sync_events(
        config,
        &client,
        &access_token,
        &mut state,
        &local_events,
        &remote_events,
        policy,
        &mut report,
    )?;

    save_sync_state(&google_sync_state_path(config), &state)?;
    Ok(report)
}

pub fn start_local_oauth_flow(config: &GoogleConfig) -> Result<AuthSession, SyncError> {
    if config.client_id.trim().is_empty() || config.client_secret.trim().is_empty() {
        return Err(SyncError::Config(
            "Google client_id/client_secret required in config.toml".to_string(),
        ));
    }

    let listener =
        TcpListener::bind("127.0.0.1:0").map_err(|e| SyncError::Request(e.to_string()))?;
    let addr = listener
        .local_addr()
        .map_err(|e| SyncError::Request(e.to_string()))?;
    let redirect_uri = format!("http://{}", addr);
    let expires_at = Local::now() + Duration::minutes(10);
    let state = generate_state();

    let scope = "https://www.googleapis.com/auth/calendar https://www.googleapis.com/auth/tasks";
    let auth_url = Url::parse_with_params(
        OAUTH_AUTH_URL,
        [
            ("client_id", config.client_id.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("response_type", "code"),
            ("scope", scope),
            ("access_type", "offline"),
            ("prompt", "consent"),
            ("include_granted_scopes", "true"),
            ("state", state.as_str()),
        ],
    )
    .map_err(|e| SyncError::Request(e.to_string()))?
    .to_string();

    Ok(AuthSession {
        display: AuthDisplay {
            auth_url,
            listen_addr: addr.to_string(),
            expires_at,
        },
        listener,
        state,
        redirect_uri,
        expires_at,
    })
}

pub fn spawn_auth_flow_poll(
    config: GoogleConfig,
    session: AuthSession,
    token_path: PathBuf,
) -> Receiver<AuthPollResult> {
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let client = Client::new();
        loop {
            if Local::now() >= session.expires_at {
                let _ = tx.send(AuthPollResult::Error(
                    "Google auth expired. Please retry.".to_string(),
                ));
                return;
            }

            if let Err(err) = session.listener.set_nonblocking(true) {
                let _ = tx.send(AuthPollResult::Error(err.to_string()));
                return;
            }

            match session.listener.accept() {
                Ok((mut stream, _addr)) => {
                    if let Err(err) =
                        handle_auth_redirect(&client, &config, &session, &mut stream, &token_path)
                    {
                        let _ = tx.send(AuthPollResult::Error(err));
                    } else {
                        let _ = tx.send(AuthPollResult::Success);
                    }
                    return;
                }
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(StdDuration::from_millis(200));
                }
                Err(err) => {
                    let _ = tx.send(AuthPollResult::Error(err.to_string()));
                    return;
                }
            }
        }
    });

    rx
}

fn ensure_access_token(config: &Config) -> Result<String, SyncError> {
    if !config.google.enabled {
        return Err(SyncError::Config(
            "Enable [google] in config.toml to sync.".to_string(),
        ));
    }
    if config.google.client_id.trim().is_empty() || config.google.client_secret.trim().is_empty() {
        return Err(SyncError::Config(
            "Google client_id/client_secret required in config.toml".to_string(),
        ));
    }

    let token_path = google_token_path(config);
    if !token_path.exists() {
        let session = start_local_oauth_flow(&config.google)?;
        return Err(SyncError::AuthRequired(session));
    }

    let stored = load_token(&token_path)?;
    let now = Utc::now().timestamp();
    if stored.expires_at > now + 60 {
        return Ok(stored.access_token);
    }

    match refresh_access_token(config, &stored.refresh_token) {
        Ok(updated) => {
            save_token(&token_path, &updated)?;
            Ok(updated.access_token)
        }
        Err(_) => {
            let session = start_local_oauth_flow(&config.google)?;
            Err(SyncError::AuthRequired(session))
        }
    }
}

fn refresh_access_token(
    config: &Config,
    refresh_token: &str,
) -> Result<StoredToken, SyncError> {
    let client = Client::new();
    let resp = client
        .post(OAUTH_TOKEN_URL)
        .form(&[
            ("client_id", config.google.client_id.as_str()),
            ("client_secret", config.google.client_secret.as_str()),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .map_err(|e| SyncError::Request(e.to_string()))?;

    if !resp.status().is_success() {
        return Err(SyncError::Request(format!(
            "Token refresh failed: HTTP {}",
            resp.status()
        )));
    }

    let token: TokenResponse = resp
        .json()
        .map_err(|e| SyncError::Request(e.to_string()))?;
    Ok(StoredToken {
        access_token: token.access_token,
        refresh_token: token.refresh_token.unwrap_or_else(|| refresh_token.to_string()),
        expires_at: (Utc::now() + Duration::seconds(token.expires_in as i64)).timestamp(),
    })
}

fn load_token(path: &Path) -> Result<StoredToken, SyncError> {
    let content = fs::read_to_string(path)?;
    let token: StoredToken = serde_json::from_str(&content)
        .map_err(|e| SyncError::Request(e.to_string()))?;
    Ok(token)
}

fn handle_auth_redirect(
    client: &Client,
    config: &GoogleConfig,
    session: &AuthSession,
    stream: &mut std::net::TcpStream,
    token_path: &Path,
) -> Result<(), String> {
    stream
        .set_read_timeout(Some(StdDuration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).map_err(|e| e.to_string())?;
    let request = String::from_utf8_lossy(&buf);
    let request_line = request.lines().next().unwrap_or("");
    let path = request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/");
    let query = path.split_once('?').map(|(_, q)| q).unwrap_or("");
    let params = parse_query(query);

    if let Some(error) = params.get("error") {
        let desc = params
            .get("error_description")
            .map(|s| format!(" ({})", s))
            .unwrap_or_default();
        let _ = respond_with_message(stream, &format!(
            "Authorization failed: {error}{desc}"
        ));
        return Err(format!("Google auth failed: {error}{desc}"));
    }

    let Some(code) = params.get("code") else {
        let _ = respond_with_message(stream, "Missing authorization code.");
        return Err("Missing authorization code from Google.".to_string());
    };

    if params.get("state").map(String::as_str) != Some(session.state.as_str()) {
        let _ = respond_with_message(stream, "Invalid state.");
        return Err("Invalid OAuth state. Please retry.".to_string());
    }

    let resp = client
        .post(OAUTH_TOKEN_URL)
        .form(&[
            ("client_id", config.client_id.as_str()),
            ("client_secret", config.client_secret.as_str()),
            ("code", code.as_str()),
            ("redirect_uri", session.redirect_uri.as_str()),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        let detail = format_oauth_error(status, &body);
        let _ = respond_with_message(stream, &format!("Authorization failed: {}", detail));
        return Err(detail);
    }

    let token: TokenResponse = resp.json().map_err(|e| e.to_string())?;
    let Some(refresh) = token.refresh_token else {
        let _ = respond_with_message(
            stream,
            "Missing refresh token. Please retry and grant offline access.",
        );
        return Err("Missing refresh token from Google.".to_string());
    };

    let stored = StoredToken {
        access_token: token.access_token,
        refresh_token: refresh,
        expires_at: (Utc::now() + Duration::seconds(token.expires_in as i64)).timestamp(),
    };
    save_token(token_path, &stored).map_err(|e| e.message())?;
    let _ = respond_with_message(stream, "Authorization complete. You can close this window.");
    Ok(())
}

fn respond_with_message(stream: &mut std::net::TcpStream, message: &str) -> io::Result<()> {
    let body = format!("{message}\n");
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(response.as_bytes())
}

fn parse_query(query: &str) -> HashMap<String, String> {
    let mut params = HashMap::new();
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (key, value) = pair
            .split_once('=')
            .map(|(k, v)| (k, v))
            .unwrap_or((pair, ""));
        params.insert(decode_component(key), decode_component(value));
    }
    params
}

fn decode_component(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::new();
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                if let Some(hex) = std::str::from_utf8(&bytes[i + 1..i + 3])
                    .ok()
                    .and_then(|s| u8::from_str_radix(s, 16).ok())
                {
                    out.push(hex as char);
                    i += 3;
                } else {
                    out.push('%');
                    i += 1;
                }
            }
            _ => {
                out.push(bytes[i] as char);
                i += 1;
            }
        }
    }
    out
}

fn generate_state() -> String {
    use rand::{distributions::Alphanumeric, Rng};
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .map(char::from)
        .collect()
}

fn format_oauth_error(status: reqwest::StatusCode, body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return format!("HTTP {}", status);
    }

    let summary = if let Ok(err) = serde_json::from_str::<TokenErrorResponse>(trimmed) {
        if let Some(desc) = err.error_description {
            format!("{} ({})", desc, err.error)
        } else {
            err.error
        }
    } else {
        truncate_error(trimmed)
    };
    format!("HTTP {}: {}", status, summary)
}

fn truncate_error(message: &str) -> String {
    let mut out = message.replace(['\n', '\r'], " ");
    if out.len() > 240 {
        out.truncate(240);
        out.push_str("...");
    }
    out
}
fn save_token(path: &Path, token: &StoredToken) -> Result<(), SyncError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(token)
        .map_err(|e| SyncError::Request(e.to_string()))?;
    fs::write(path, content)?;
    Ok(())
}

fn load_sync_state(path: &Path) -> Result<SyncState, SyncError> {
    if !path.exists() {
        return Ok(SyncState::default());
    }
    let content = fs::read_to_string(path)?;
    let state: SyncState = serde_json::from_str(&content)
        .map_err(|e| SyncError::Request(e.to_string()))?;
    Ok(state)
}

fn save_sync_state(path: &Path, state: &SyncState) -> Result<(), SyncError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(state)
        .map_err(|e| SyncError::Request(e.to_string()))?;
    fs::write(path, content)?;
    Ok(())
}

fn sync_range(config: &Config) -> (NaiveDate, NaiveDate) {
    let today = Local::now().date_naive();
    let start = today - Duration::days(config.google.sync_past_days.max(0));
    let end = today + Duration::days(config.google.sync_future_days.max(0));
    (start, end)
}

fn conflict_policy(value: &str) -> ConflictPolicy {
    match value.trim().to_lowercase().as_str() {
        "prefer_remote" => ConflictPolicy::PreferRemote,
        _ => ConflictPolicy::PreferLocal,
    }
}

#[derive(Clone, Copy)]
enum ConflictPolicy {
    PreferLocal,
    PreferRemote,
}

fn sync_tasks(
    config: &Config,
    client: &Client,
    access_token: &str,
    state: &mut SyncState,
    local_items: &[AgendaItem],
    remote_items: &[RemoteTask],
    policy: ConflictPolicy,
    report: &mut SyncReport,
) -> Result<(), SyncError> {
    let mut remote_by_id = HashMap::new();
    for item in remote_items {
        remote_by_id.insert(item.id.clone(), item.clone());
    }

    for item in local_items {
        if item.kind != AgendaItemKind::Task {
            continue;
        }
        let key = local_task_key(item);
        let hash = task_hash(item);
        let stored = state.tasks.get(&key).cloned();
        let remote = stored
            .as_ref()
            .and_then(|entry| remote_by_id.get(&entry.google_id));

        match (stored, remote) {
            (Some(entry), Some(remote)) => {
                let local_changed = entry.hash != hash;
                let remote_updated = remote.updated.clone();
                let remote_changed = entry.remote_updated != remote_updated;

                match (local_changed, remote_changed) {
                    (true, true) => {
                        report.conflicts += 1;
                        match policy {
                            ConflictPolicy::PreferLocal => {
                                let updated = update_remote_task(
                                    client,
                                    access_token,
                                    &config.google.tasks_list_id,
                                    &entry.google_id,
                                    item,
                                )?;
                                report.tasks_updated += 1;
                                state.tasks.insert(
                                    key,
                                    SyncItem {
                                        google_id: entry.google_id,
                                        hash,
                                        remote_updated: updated.updated.clone().or(remote_updated),
                                    },
                                );
                            }
                            ConflictPolicy::PreferRemote => {
                                let applied = apply_remote_task_update(item, remote)?;
                                report.tasks_imported += 1;
                                state.tasks.insert(
                                    key,
                                    SyncItem {
                                        google_id: entry.google_id,
                                        hash: task_hash_from_update(&applied),
                                        remote_updated: remote_updated.clone(),
                                    },
                                );
                            }
                        }
                    }
                    (true, false) => {
                        let updated = update_remote_task(
                            client,
                            access_token,
                            &config.google.tasks_list_id,
                            &entry.google_id,
                            item,
                        )?;
                        report.tasks_updated += 1;
                        state.tasks.insert(
                            key,
                            SyncItem {
                                google_id: entry.google_id,
                                hash,
                                remote_updated: updated.updated.clone().or(remote_updated),
                            },
                        );
                    }
                    (false, true) => {
                        let applied = apply_remote_task_update(item, remote)?;
                        report.tasks_imported += 1;
                        state.tasks.insert(
                            key,
                            SyncItem {
                                google_id: entry.google_id,
                                hash: task_hash_from_update(&applied),
                                remote_updated: remote_updated.clone(),
                            },
                        );
                    }
                    (false, false) => {}
                }
            }
            (Some(_entry), None) => {
                let created = create_remote_task(
                    client,
                    access_token,
                    &config.google.tasks_list_id,
                    item,
                )?;
                report.tasks_created += 1;
                state.tasks.insert(
                    key,
                    SyncItem {
                        google_id: created.id,
                        hash,
                        remote_updated: created.updated,
                    },
                );
            }
            (None, _) => {
                let created = create_remote_task(
                    client,
                    access_token,
                    &config.google.tasks_list_id,
                    item,
                )?;
                report.tasks_created += 1;
                state.tasks.insert(
                    key,
                    SyncItem {
                        google_id: created.id,
                        hash,
                        remote_updated: created.updated,
                    },
                );
            }
        }
    }

    for remote in remote_items {
        let exists = state
            .tasks
            .values()
            .any(|entry| entry.google_id == remote.id);
        if !exists {
            let update = TaskLineUpdate {
                text: remote.title.clone().unwrap_or_else(|| "Untitled task".to_string()),
                is_done: remote.status.as_deref() == Some("completed"),
                priority: None,
                schedule: schedule_from_remote_task(remote),
            };
            let line = storage::compose_task_line(&update);
            let date = schedule_anchor_date(&update.schedule)
                .unwrap_or_else(|| Local::now().date_naive());
            storage::append_entry_to_date(&config.data.log_path, date, &line)?;
            report.tasks_imported += 1;
            let log_file = log_path_for_date(&config.data.log_path, date);
            let key = find_last_line_number(&log_file, &line)
                .map(|line_number| local_task_key_for_path(&log_file, line_number))
                .unwrap_or_else(|| local_key_for_import(&config.data.log_path, date, &line));
            state.tasks.insert(
                key,
                SyncItem {
                    google_id: remote.id.clone(),
                    hash: task_hash_from_update(&update),
                    remote_updated: remote.updated.clone(),
                },
            );
        }
    }

    Ok(())
}

fn sync_events(
    config: &Config,
    client: &Client,
    access_token: &str,
    state: &mut SyncState,
    local_items: &[AgendaItem],
    remote_items: &[RemoteEvent],
    policy: ConflictPolicy,
    report: &mut SyncReport,
) -> Result<(), SyncError> {
    let mut remote_by_id = HashMap::new();
    for item in remote_items {
        remote_by_id.insert(item.id.clone(), item.clone());
    }

    for item in local_items {
        let key = local_event_key(item);
        let hash = event_hash(item);
        let stored = state.events.get(&key).cloned();
        let remote = stored
            .as_ref()
            .and_then(|entry| remote_by_id.get(&entry.google_id));

        match (stored, remote) {
            (Some(entry), Some(remote)) => {
                let local_changed = entry.hash != hash;
                let remote_updated = remote.updated.clone();
                let remote_changed = entry.remote_updated != remote_updated;

                match (local_changed, remote_changed) {
                    (true, true) => {
                        report.conflicts += 1;
                        match policy {
                            ConflictPolicy::PreferLocal => {
                                let updated = update_remote_event(
                                    client,
                                    access_token,
                                    &config.google.calendar_id,
                                    &entry.google_id,
                                    item,
                                )?;
                                report.events_updated += 1;
                                state.events.insert(
                                    key,
                                    SyncItem {
                                        google_id: entry.google_id,
                                        hash,
                                        remote_updated: updated.updated.clone().or(remote_updated),
                                    },
                                );
                            }
                            ConflictPolicy::PreferRemote => {
                                let applied = apply_remote_event_update(item, remote)?;
                                report.events_imported += 1;
                                state.events.insert(
                                    key,
                                    SyncItem {
                                        google_id: entry.google_id,
                                        hash: event_hash_from_update(&applied),
                                        remote_updated: remote_updated.clone(),
                                    },
                                );
                            }
                        }
                    }
                    (true, false) => {
                        let updated = update_remote_event(
                            client,
                            access_token,
                            &config.google.calendar_id,
                            &entry.google_id,
                            item,
                        )?;
                        report.events_updated += 1;
                        state.events.insert(
                            key,
                            SyncItem {
                                google_id: entry.google_id,
                                hash,
                                remote_updated: updated.updated.clone().or(remote_updated),
                            },
                        );
                    }
                    (false, true) => {
                        let applied = apply_remote_event_update(item, remote)?;
                        report.events_imported += 1;
                        state.events.insert(
                            key,
                            SyncItem {
                                google_id: entry.google_id,
                                hash: event_hash_from_update(&applied),
                                remote_updated: remote_updated.clone(),
                            },
                        );
                    }
                    (false, false) => {}
                }
            }
            (Some(_entry), None) => {
                let created = create_remote_event(
                    client,
                    access_token,
                    &config.google.calendar_id,
                    item,
                )?;
                report.events_created += 1;
                state.events.insert(
                    key,
                    SyncItem {
                        google_id: created.id,
                        hash,
                        remote_updated: created.updated,
                    },
                );
            }
            (None, _) => {
                let created = create_remote_event(
                    client,
                    access_token,
                    &config.google.calendar_id,
                    item,
                )?;
                report.events_created += 1;
                state.events.insert(
                    key,
                    SyncItem {
                        google_id: created.id,
                        hash,
                        remote_updated: created.updated,
                    },
                );
            }
        }
    }

    for remote in remote_items {
        let exists = state
            .events
            .values()
            .any(|entry| entry.google_id == remote.id);
        if !exists {
            let update = NoteLineUpdate {
                text: remote
                    .summary
                    .clone()
                    .unwrap_or_else(|| "Untitled event".to_string()),
                schedule: schedule_from_remote_event(remote),
            };
            let line = storage::compose_note_line(&update);
            let date = schedule_anchor_date(&update.schedule)
                .unwrap_or_else(|| Local::now().date_naive());
            storage::append_entry_to_date(&config.data.log_path, date, &line)?;
            report.events_imported += 1;
            let log_file = log_path_for_date(&config.data.log_path, date);
            let key = find_last_line_number(&log_file, &line)
                .map(|line_number| local_event_key_for_path(&log_file, line_number))
                .unwrap_or_else(|| local_key_for_import(&config.data.log_path, date, &line));
            state.events.insert(
                key,
                SyncItem {
                    google_id: remote.id.clone(),
                    hash: event_hash_from_update(&update),
                    remote_updated: remote.updated.clone(),
                },
            );
        }
    }

    Ok(())
}

fn list_google_tasks(
    client: &Client,
    access_token: &str,
    task_list_id: &str,
) -> Result<Vec<RemoteTask>, SyncError> {
    let url = format!("{TASKS_API}/lists/{task_list_id}/tasks");
    let resp = client
        .get(url)
        .bearer_auth(access_token)
        .query(&[
            ("showCompleted", "true"),
            ("showHidden", "true"),
            ("maxResults", "100"),
        ])
        .send()
        .map_err(|e| SyncError::Request(e.to_string()))?;

    if !resp.status().is_success() {
        return Err(SyncError::Request(format!(
            "Tasks list failed: HTTP {}",
            resp.status()
        )));
    }

    let body: TasksListResponse = resp
        .json()
        .map_err(|e| SyncError::Request(e.to_string()))?;
    Ok(body.items.unwrap_or_default())
}

fn list_google_events(
    client: &Client,
    access_token: &str,
    calendar_id: &str,
    start_date: NaiveDate,
    end_date: NaiveDate,
) -> Result<Vec<RemoteEvent>, SyncError> {
    let time_min = to_rfc3339(start_date, NaiveTime::from_hms_opt(0, 0, 0).unwrap());
    let time_max = to_rfc3339(end_date, NaiveTime::from_hms_opt(23, 59, 59).unwrap());
    let url = format!("{CALENDAR_API}/calendars/{calendar_id}/events");
    let resp = client
        .get(url)
        .bearer_auth(access_token)
        .query(&[
            ("timeMin", time_min.as_str()),
            ("timeMax", time_max.as_str()),
            ("singleEvents", "true"),
            ("showDeleted", "false"),
            ("maxResults", "2500"),
        ])
        .send()
        .map_err(|e| SyncError::Request(e.to_string()))?;

    if !resp.status().is_success() {
        return Err(SyncError::Request(format!(
            "Calendar list failed: HTTP {}",
            resp.status()
        )));
    }

    let body: EventsListResponse = resp
        .json()
        .map_err(|e| SyncError::Request(e.to_string()))?;
    let mut items = body.items.unwrap_or_default();
    items.retain(|item| item.status.as_deref() != Some("cancelled"));
    Ok(items)
}

fn update_remote_task(
    client: &Client,
    access_token: &str,
    task_list_id: &str,
    task_id: &str,
    item: &AgendaItem,
) -> Result<RemoteTask, SyncError> {
    let url = format!("{TASKS_API}/lists/{task_list_id}/tasks/{task_id}");
    let due = schedule_to_task_due(&item.schedule);
    let update = TaskUpdateRequest {
        title: item.text.clone(),
        status: if item.is_done {
            "completed".to_string()
        } else {
            "needsAction".to_string()
        },
        due,
    };
    let resp = client
        .patch(url)
        .bearer_auth(access_token)
        .json(&update)
        .send()
        .map_err(|e| SyncError::Request(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(SyncError::Request(format!(
            "Task update failed: HTTP {}",
            resp.status()
        )));
    }
    let updated: RemoteTask = resp
        .json()
        .map_err(|e| SyncError::Request(e.to_string()))?;
    Ok(updated)
}

fn create_remote_task(
    client: &Client,
    access_token: &str,
    task_list_id: &str,
    item: &AgendaItem,
) -> Result<RemoteTask, SyncError> {
    let url = format!("{TASKS_API}/lists/{task_list_id}/tasks");
    let due = schedule_to_task_due(&item.schedule);
    let update = TaskUpdateRequest {
        title: item.text.clone(),
        status: if item.is_done {
            "completed".to_string()
        } else {
            "needsAction".to_string()
        },
        due,
    };
    let resp = client
        .post(url)
        .bearer_auth(access_token)
        .json(&update)
        .send()
        .map_err(|e| SyncError::Request(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(SyncError::Request(format!(
            "Task create failed: HTTP {}",
            resp.status()
        )));
    }
    let created: RemoteTask = resp
        .json()
        .map_err(|e| SyncError::Request(e.to_string()))?;
    Ok(created)
}

fn apply_remote_task_update(
    local: &AgendaItem,
    remote: &RemoteTask,
) -> Result<TaskLineUpdate, SyncError> {
    let schedule = schedule_from_remote_task(remote);
    let update = TaskLineUpdate {
        text: remote
            .title
            .clone()
            .unwrap_or_else(|| local.text.clone()),
        is_done: remote.status.as_deref() == Some("completed"),
        priority: local.priority,
        schedule: schedule.clone(),
    };
    storage::update_task_line(&local.file_path, local.line_number, update)?;
    Ok(TaskLineUpdate {
        text: remote
            .title
            .clone()
            .unwrap_or_else(|| local.text.clone()),
        is_done: remote.status.as_deref() == Some("completed"),
        priority: local.priority,
        schedule,
    })
}

fn update_remote_event(
    client: &Client,
    access_token: &str,
    calendar_id: &str,
    event_id: &str,
    item: &AgendaItem,
) -> Result<RemoteEvent, SyncError> {
    let url = format!("{CALENDAR_API}/calendars/{calendar_id}/events/{event_id}");
    let (start, end) = schedule_to_event_times(&item.schedule, item.date);
    let update = EventUpdateRequest {
        summary: item.text.clone(),
        start,
        end,
    };
    let resp = client
        .patch(url)
        .bearer_auth(access_token)
        .json(&update)
        .send()
        .map_err(|e| SyncError::Request(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(SyncError::Request(format!(
            "Event update failed: HTTP {}",
            resp.status()
        )));
    }
    let updated: RemoteEvent = resp
        .json()
        .map_err(|e| SyncError::Request(e.to_string()))?;
    Ok(updated)
}

fn create_remote_event(
    client: &Client,
    access_token: &str,
    calendar_id: &str,
    item: &AgendaItem,
) -> Result<RemoteEvent, SyncError> {
    let url = format!("{CALENDAR_API}/calendars/{calendar_id}/events");
    let (start, end) = schedule_to_event_times(&item.schedule, item.date);
    let update = EventUpdateRequest {
        summary: item.text.clone(),
        start,
        end,
    };
    let resp = client
        .post(url)
        .bearer_auth(access_token)
        .json(&update)
        .send()
        .map_err(|e| SyncError::Request(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(SyncError::Request(format!(
            "Event create failed: HTTP {}",
            resp.status()
        )));
    }
    let created: RemoteEvent = resp
        .json()
        .map_err(|e| SyncError::Request(e.to_string()))?;
    Ok(created)
}

fn apply_remote_event_update(
    local: &AgendaItem,
    remote: &RemoteEvent,
) -> Result<NoteLineUpdate, SyncError> {
    let schedule = schedule_from_remote_event(remote);
    let update = NoteLineUpdate {
        text: remote
            .summary
            .clone()
            .unwrap_or_else(|| local.text.clone()),
        schedule: schedule.clone(),
    };
    storage::update_note_line(&local.file_path, local.line_number, update)?;
    Ok(NoteLineUpdate {
        text: remote
            .summary
            .clone()
            .unwrap_or_else(|| local.text.clone()),
        schedule,
    })
}

fn local_task_key(item: &AgendaItem) -> String {
    local_task_key_for_path(Path::new(&item.file_path), item.line_number)
}

fn local_event_key(item: &AgendaItem) -> String {
    local_event_key_for_path(Path::new(&item.file_path), item.line_number)
}

fn local_task_key_for_path(path: &Path, line_number: usize) -> String {
    format!("task:{}:{}", path.to_string_lossy(), line_number)
}

fn local_event_key_for_path(path: &Path, line_number: usize) -> String {
    format!("event:{}:{}", path.to_string_lossy(), line_number)
}

fn local_key_for_import(log_path: &Path, date: NaiveDate, line: &str) -> String {
    let path = log_path.join(format!("{}.md", date.format("%Y-%m-%d")));
    format!("import:{}:{}", path.to_string_lossy(), stable_hash(line))
}

fn task_hash(item: &AgendaItem) -> String {
    let schedule = schedule_signature(&item.schedule);
    let priority = item.priority.map(|p| p.as_char()).unwrap_or('-');
    stable_hash(&format!(
        "{}|{}|{}|{}",
        item.text, item.is_done, priority, schedule
    ))
}

fn event_hash(item: &AgendaItem) -> String {
    let schedule = schedule_signature(&item.schedule);
    stable_hash(&format!("{}|{}", item.text, schedule))
}

fn task_hash_from_update(update: &TaskLineUpdate) -> String {
    let schedule = schedule_signature(&update.schedule);
    let priority = update.priority.map(|p| p.as_char()).unwrap_or('-');
    stable_hash(&format!(
        "{}|{}|{}|{}",
        update.text, update.is_done, priority, schedule
    ))
}

fn event_hash_from_update(update: &NoteLineUpdate) -> String {
    let schedule = schedule_signature(&update.schedule);
    stable_hash(&format!("{}|{}", update.text, schedule))
}

fn schedule_signature(schedule: &TaskSchedule) -> String {
    format!(
        "{}|{}|{}|{}|{}",
        schedule
            .scheduled
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_default(),
        schedule
            .due
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_default(),
        schedule
            .start
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_default(),
        schedule
            .time
            .map(|t| t.format("%H:%M").to_string())
            .unwrap_or_default(),
        schedule
            .duration_minutes
            .map(|m| m.to_string())
            .unwrap_or_default()
    )
}

fn schedule_to_task_due(schedule: &TaskSchedule) -> Option<String> {
    let date = schedule
        .due
        .or(schedule.scheduled)
        .or(schedule.start)?;
    let time = schedule.time.unwrap_or_else(|| NaiveTime::from_hms_opt(23, 59, 0).unwrap());
    Some(to_rfc3339(date, time))
}

fn schedule_to_event_times(
    schedule: &TaskSchedule,
    fallback_date: NaiveDate,
) -> (EventDateTime, EventDateTime) {
    let date = schedule
        .scheduled
        .or(schedule.start)
        .or(schedule.due)
        .unwrap_or(fallback_date);
    if let Some(time) = schedule.time {
        let start = to_rfc3339(date, time);
        let duration = schedule
            .duration_minutes
            .map(|m| m as i64)
            .unwrap_or(DEFAULT_EVENT_DURATION_MINUTES);
        let start_dt = Local
            .from_local_datetime(&NaiveDateTime::new(date, time))
            .unwrap();
        let end_dt = start_dt + Duration::minutes(duration);
        let end = end_dt.to_rfc3339();
        (
            EventDateTime {
                date_time: Some(start),
                date: None,
            },
            EventDateTime {
                date_time: Some(end),
                date: None,
            },
        )
    } else {
        let start = EventDateTime {
            date_time: None,
            date: Some(date.format("%Y-%m-%d").to_string()),
        };
        let end = EventDateTime {
            date_time: None,
            date: Some((date + Duration::days(1)).format("%Y-%m-%d").to_string()),
        };
        (start, end)
    }
}

fn schedule_from_remote_task(remote: &RemoteTask) -> TaskSchedule {
    let mut schedule = TaskSchedule::default();
    if let Some(due) = remote.due.as_deref() {
        if let Ok(dt) = DateTime::parse_from_rfc3339(due) {
            schedule.due = Some(dt.date_naive());
            schedule.time = Some(dt.time());
        }
    }
    schedule
}

fn schedule_from_remote_event(remote: &RemoteEvent) -> TaskSchedule {
    let mut schedule = TaskSchedule::default();
    if let Some(start) = &remote.start {
        if let Some(date_time) = start.date_time.as_deref() {
            if let Ok(dt) = DateTime::parse_from_rfc3339(date_time) {
                schedule.scheduled = Some(dt.date_naive());
                schedule.time = Some(dt.time());
            }
        } else if let Some(date) = start.date.as_deref() {
            if let Ok(date) = NaiveDate::parse_from_str(date, "%Y-%m-%d") {
                schedule.scheduled = Some(date);
            }
        }
    }
    if let (Some(start), Some(end)) = (&remote.start, &remote.end) {
        if let (Some(start_dt), Some(end_dt)) =
            (start.date_time.as_deref(), end.date_time.as_deref())
        {
            if let (Ok(start), Ok(end)) = (
                DateTime::parse_from_rfc3339(start_dt),
                DateTime::parse_from_rfc3339(end_dt),
            ) {
                let duration = end - start;
                if duration.num_minutes() > 0 {
                    schedule.duration_minutes = Some(duration.num_minutes() as u32);
                }
            }
        }
    }
    schedule
}

fn schedule_anchor_date(schedule: &TaskSchedule) -> Option<NaiveDate> {
    schedule.scheduled.or(schedule.due).or(schedule.start)
}

fn log_path_for_date(log_path: &Path, date: NaiveDate) -> PathBuf {
    log_path.join(format!("{}.md", date.format("%Y-%m-%d")))
}

fn find_last_line_number(path: &Path, line: &str) -> Option<usize> {
    let content = fs::read_to_string(path).ok()?;
    let mut last_index = None;
    for (idx, existing) in content.lines().enumerate() {
        if existing == line {
            last_index = Some(idx);
        }
    }
    last_index
}

fn to_rfc3339(date: NaiveDate, time: NaiveTime) -> String {
    Local
        .from_local_datetime(&NaiveDateTime::new(date, time))
        .unwrap()
        .to_rfc3339()
}

fn stable_hash(input: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in input.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{:x}", hash)
}
