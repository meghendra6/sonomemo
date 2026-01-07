use crate::config::GeminiConfig;
use crate::models::LogEntry;
use crate::storage;
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

#[derive(Clone)]
pub struct AiSearchResult {
    pub question: String,
    pub keywords: Vec<String>,
    pub entries: Vec<LogEntry>,
    pub answer: String,
}

pub enum AiSearchOutcome {
    Success(AiSearchResult),
    Error(String),
}

pub fn spawn_ai_search(
    config: GeminiConfig,
    log_path: PathBuf,
    question: String,
) -> Receiver<AiSearchOutcome> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let result = run_ai_search(&config, &log_path, &question);
        let _ = sender.send(result);
    });
    receiver
}

fn run_ai_search(
    config: &GeminiConfig,
    log_path: &PathBuf,
    question: &str,
) -> AiSearchOutcome {
    if !config.enabled {
        return AiSearchOutcome::Error("Gemini is disabled in config.".to_string());
    }

    let api_key = resolve_api_key(config);
    if api_key.is_empty() {
        return AiSearchOutcome::Error("Missing Gemini API key.".to_string());
    }

    let client = match Client::builder()
        .timeout(Duration::from_secs(config.timeout_seconds.max(5)))
        .build()
    {
        Ok(client) => client,
        Err(err) => {
            return AiSearchOutcome::Error(format!("Failed to create HTTP client: {err}"));
        }
    };

    let keywords = match extract_keywords(&client, config, &api_key, question) {
        Ok(list) if !list.is_empty() => list,
        Ok(_) => {
            return AiSearchOutcome::Error("No keywords extracted from query.".to_string());
        }
        Err(err) => return AiSearchOutcome::Error(err),
    };

    let mut entries =
        match storage::search_entries_by_keywords(log_path, &keywords) {
            Ok(results) => results,
            Err(err) => {
                return AiSearchOutcome::Error(format!("Search failed: {err}"));
            }
        };

    if config.max_results > 0 && entries.len() > config.max_results {
        entries.truncate(config.max_results);
    }

    if entries.is_empty() {
        return AiSearchOutcome::Success(AiSearchResult {
            question: question.to_string(),
            keywords,
            entries,
            answer: "관련 문서를 찾지 못했습니다. 질문을 더 구체적으로 적어주세요.".to_string(),
        });
    }

    let answer = match generate_answer(&client, config, &api_key, question, &entries) {
        Ok(text) => text,
        Err(err) => return AiSearchOutcome::Error(err),
    };

    AiSearchOutcome::Success(AiSearchResult {
        question: question.to_string(),
        keywords,
        entries,
        answer,
    })
}

fn resolve_api_key(config: &GeminiConfig) -> String {
    if !config.api_key.trim().is_empty() {
        return config.api_key.trim().to_string();
    }
    std::env::var("GEMINI_API_KEY").unwrap_or_default()
}

fn extract_keywords(
    client: &Client,
    config: &GeminiConfig,
    api_key: &str,
    question: &str,
) -> Result<Vec<String>, String> {
    let model = resolve_extraction_model(config);
    let max_keywords = config.max_keywords.max(1);
    let attempts = config.extraction_attempts.max(1).min(6);
    let prompts = keyword_prompts(question.trim(), max_keywords);
    let mut candidates: HashMap<String, (String, usize)> = HashMap::new();
    let mut last_error: Option<String> = None;

    for attempt in 0..attempts {
        let prompt = prompts
            .get(attempt)
            .unwrap_or_else(|| prompts.first().expect("prompt"));
        let temperature = extraction_temperature_for_attempt(config, attempt);
        let response =
            match generate_text(client, api_key, &model, prompt, 128, temperature) {
            Ok(text) => text,
            Err(err) => {
                last_error = Some(err);
                continue;
            }
        };
        match parse_keywords_response(&response) {
            Ok(list) => {
                for keyword in list {
                    let lower = keyword.to_ascii_lowercase();
                    let entry = candidates.entry(lower).or_insert((keyword, 0));
                    entry.1 = entry.1.saturating_add(1);
                }
            }
            Err(err) => {
                last_error = Some(err);
            }
        }
    }

    if candidates.is_empty() {
        return Err(last_error.unwrap_or_else(|| "Keyword extraction failed.".to_string()));
    }

    let mut ranked = candidates
        .into_iter()
        .map(|(_, (keyword, count))| (keyword, count))
        .collect::<Vec<(String, usize)>>();
    ranked.sort_by(|(left, left_count), (right, right_count)| {
        right_count
            .cmp(left_count)
            .then_with(|| left.len().cmp(&right.len()))
            .then_with(|| left.cmp(right))
    });

    let mut keywords: Vec<String> = ranked.iter().map(|(keyword, _)| keyword.clone()).collect();
    if keywords.len() > max_keywords {
            let refined = refine_keywords(
                client,
                api_key,
                question,
                &ranked,
                max_keywords,
                &model,
            );
        if let Ok(refined) = refined {
            keywords = refined;
        } else {
            keywords.truncate(max_keywords);
        }
    }

    let mut seen = HashSet::new();
    keywords.retain(|keyword| seen.insert(keyword.to_ascii_lowercase()));
    if keywords.is_empty() {
        return Err("Keyword extraction returned empty list.".to_string());
    }
    Ok(keywords)
}

fn generate_answer(
    client: &Client,
    config: &GeminiConfig,
    api_key: &str,
    question: &str,
    entries: &[LogEntry],
) -> Result<String, String> {
    let mut context = String::new();
    for (idx, entry) in entries.iter().enumerate() {
        let index = idx + 1;
        let file = std::path::Path::new(&entry.file_path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(entry.file_path.as_str())
            .to_string();
        let snippet = truncate_chars(&entry.content, config.max_entry_chars.max(200));
        context.push_str(&format!(
            "[{index}] file: {file} line: {line}\n{snippet}\n\n",
            index = index,
            file = file,
            line = entry.line_number,
            snippet = snippet
        ));
    }

    let prompt = format!(
        "You answer questions using ONLY the provided memo entries.\n\
If the answer is not present, say you cannot find it.\n\
Answer in Korean and cite sources like [1], [2].\n\n\
Question: {question}\n\n\
Entries:\n{context}",
        question = question.trim(),
        context = context.trim_end()
    );

    let model = resolve_answer_model(config);
    generate_text(client, api_key, &model, &prompt, 512, 0.2)
}

fn generate_text(
    client: &Client,
    api_key: &str,
    model: &str,
    prompt: &str,
    max_tokens: u32,
    temperature: f32,
) -> Result<String, String> {
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        model, api_key
    );
    let body = json!({
        "contents": [
            {
                "role": "user",
                "parts": [{"text": prompt}]
            }
        ],
        "generationConfig": {
            "temperature": temperature,
            "maxOutputTokens": max_tokens,
            "topP": 0.9
        }
    });

    let response = client
        .post(url)
        .json(&body)
        .send()
        .map_err(|e| format!("Gemini request failed: {e}"))?;
    let status = response.status();
    let body_text = response
        .text()
        .map_err(|e| format!("Gemini response read failed: {e}"))?;
    if !status.is_success() {
        return Err(format!("Gemini error ({status}): {body_text}"));
    }

    let parsed: GeminiResponse =
        serde_json::from_str(&body_text).map_err(|e| format!("Gemini parse failed: {e}"))?;
    let text = parsed
        .candidates
        .iter()
        .filter_map(|candidate| candidate.content.as_ref())
        .flat_map(|content| content.parts.iter())
        .filter_map(|part| part.text.as_ref())
        .find(|text| !text.trim().is_empty())
        .cloned()
        .ok_or_else(|| "Gemini returned empty response.".to_string())?;
    Ok(text.trim().to_string())
}

fn parse_keywords_response(response: &str) -> Result<Vec<String>, String> {
    let json_text = extract_json_object(response)
        .ok_or_else(|| "Keyword extraction returned invalid JSON.".to_string())?;
    let parsed: serde_json::Value =
        serde_json::from_str(json_text).map_err(|e| format!("Invalid JSON: {e}"))?;
    let mut keywords = Vec::new();
    if let Some(list) = parsed.get("keywords").and_then(|v| v.as_array()) {
        for item in list {
            if let Some(text) = item.as_str() {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    keywords.push(trimmed.to_string());
                }
            }
        }
    }
    Ok(keywords)
}

fn keyword_prompts(question: &str, max_keywords: usize) -> Vec<String> {
    vec![
        format!(
            "You extract search keywords from user questions for a personal memo log.\n\
Return ONLY JSON with a single object: {{\"keywords\": [\"...\", ...]}}.\n\
Rules:\n\
- Provide 1 to {max} short keywords or short phrases.\n\
- No punctuation, no duplicates, no extra text.\n\
Question: {question}",
            max = max_keywords,
            question = question
        ),
        format!(
            "Extract the core entities and proper nouns from the question.\n\
Return ONLY JSON: {{\"keywords\": [\"...\", ...]}}.\n\
Rules:\n\
- 1 to {max} entries, no extra text.\n\
Question: {question}",
            max = max_keywords,
            question = question
        ),
        format!(
            "Extract action/intent keywords and likely tag-like terms from the question.\n\
Return ONLY JSON: {{\"keywords\": [\"...\", ...]}}.\n\
Rules:\n\
- 1 to {max} entries, no extra text.\n\
Question: {question}",
            max = max_keywords,
            question = question
        ),
    ]
}

fn extraction_temperature_for_attempt(config: &GeminiConfig, attempt: usize) -> f32 {
    let base = config.extraction_temperature.clamp(0.0, 1.0);
    let bump = (attempt as f32) * 0.15;
    (base + bump).clamp(0.0, 0.9)
}

fn refine_keywords(
    client: &Client,
    api_key: &str,
    question: &str,
    candidates: &[(String, usize)],
    max_keywords: usize,
    model: &str,
) -> Result<Vec<String>, String> {
    let candidate_limit = (max_keywords.saturating_mul(6)).clamp(12, 80);
    let trimmed_candidates: Vec<String> = candidates
        .iter()
        .take(candidate_limit)
        .map(|(keyword, count)| format!("{keyword} ({count})"))
        .collect();
    let prompt = format!(
        "Select the best search keywords for a personal memo log.\n\
Return ONLY JSON: {{\"keywords\": [\"...\", ...]}}.\n\
Rules:\n\
- Choose up to {max} keywords from the candidates.\n\
- Favor precise nouns, names, projects, and topics in the question.\n\
Question: {question}\n\
Candidates: {candidates}",
        max = max_keywords,
        question = question.trim(),
        candidates = trimmed_candidates.join(", ")
    );

    let response = generate_text(client, api_key, model, &prompt, 128, 0.1)?;
    let keywords = parse_keywords_response(&response)?;
    if keywords.is_empty() {
        return Err("Refined keyword list is empty.".to_string());
    }

    let mut seen = HashSet::new();
    let mut unique = Vec::new();
    for keyword in keywords {
        if seen.insert(keyword.to_ascii_lowercase()) {
            unique.push(keyword);
        }
    }
    if unique.len() > max_keywords {
        unique.truncate(max_keywords);
    }
    Ok(unique)
}

fn resolve_extraction_model(config: &GeminiConfig) -> String {
    if !config.extraction_model.trim().is_empty() {
        return normalize_model_name(config.extraction_model.trim());
    }
    if !config.model.trim().is_empty() {
        return normalize_model_name(config.model.trim());
    }
    if !config.answer_model.trim().is_empty() {
        return normalize_model_name(config.answer_model.trim());
    }
    normalize_model_name("gemma-3-27b-it")
}

fn resolve_answer_model(config: &GeminiConfig) -> String {
    if !config.answer_model.trim().is_empty() {
        return normalize_model_name(config.answer_model.trim());
    }
    if !config.model.trim().is_empty() {
        return normalize_model_name(config.model.trim());
    }
    normalize_model_name("gemini-3-flash-preview")
}

fn normalize_model_name(model: &str) -> String {
    let trimmed = model.trim();
    let stripped = trimmed.strip_prefix("models/").unwrap_or(trimmed);
    match stripped {
        "gemini-3-flash" => "gemini-3-flash-preview".to_string(),
        "gemma-3-12b" => "gemma-3-12b-it".to_string(),
        "gemma-3-27b" => "gemma-3-27b-it".to_string(),
        other => other.to_string(),
    }
}

fn extract_json_object(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end <= start {
        return None;
    }
    Some(&text[start..=end])
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let mut out = String::new();
    for (idx, ch) in text.chars().enumerate() {
        if idx >= max_chars {
            out.push_str("...");
            return out;
        }
        out.push(ch);
    }
    out
}

#[derive(Deserialize)]
struct GeminiResponse {
    candidates: Vec<GeminiCandidate>,
}

#[derive(Deserialize)]
struct GeminiCandidate {
    content: Option<GeminiContent>,
}

#[derive(Deserialize)]
struct GeminiContent {
    parts: Vec<GeminiPart>,
}

#[derive(Deserialize)]
struct GeminiPart {
    text: Option<String>,
}
