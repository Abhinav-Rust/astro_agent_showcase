use serde::{Deserialize, Serialize};
use reqwest::Client;
use std::env;
use chrono::{Datelike, NaiveDate};
use console::style;
use std::time::Duration;
use tokio::time::sleep;
use rand::Rng;
use reqwest::{Response, StatusCode};

const MAX_RETRIES: u32 = 10;
const BACKOFF_BASE_MS: u64 = 30_000;
const BACKOFF_MAX_MS: u64 = 120_000;

#[derive(Serialize)]
pub struct GeminiRequest {
    pub contents: Vec<GeminiContent>,
    #[serde(rename = "generationConfig")]
    pub generation_config: GenerationConfig,
    #[serde(rename = "safetySettings")]
    pub safety_settings: Vec<SafetySetting>,
}

#[derive(Serialize)]
pub struct SafetySetting {
    pub category: String,
    pub threshold: String,
}

#[derive(Serialize)]
pub struct GeminiContent {
    pub role: String,
    pub parts: Vec<GeminiPart>,
}

#[derive(Serialize)]
pub struct GeminiPart {
    pub text: String,
}

#[derive(Serialize)]
pub struct GenerationConfig {
    #[serde(rename = "maxOutputTokens")]
    pub max_output_tokens: u32,
    pub temperature: f32,
}

#[derive(Deserialize)]
pub struct GeminiResponse {
    pub candidates: Option<Vec<GeminiCandidate>>,
    #[serde(rename = "usageMetadata")]
    pub usage_metadata: Option<GeminiUsageMetadata>,
}

#[derive(Deserialize)]
pub struct GeminiCandidate {
    pub content: GeminiCandidateContent,
}

#[derive(Deserialize)]
pub struct GeminiCandidateContent {
    pub parts: Vec<GeminiCandidatePart>,
}

#[derive(Deserialize)]
pub struct GeminiCandidatePart {
    pub text: String,
}

#[derive(Deserialize)]
pub struct GeminiUsageMetadata {
    #[serde(rename = "promptTokenCount")]
    pub prompt_token_count: Option<u64>,
    #[serde(rename = "candidatesTokenCount")]
    pub candidates_token_count: Option<u64>,
    #[serde(rename = "totalTokenCount")]
    pub total_token_count: Option<u64>,
}

#[derive(Deserialize, Debug)]
pub struct GeminiErrorEnvelope {
    pub error: GeminiErrorBody,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
pub struct GeminiErrorBody {
    #[allow(dead_code)]
    pub code: u16,
    pub message: String,
    pub status: String,
}

#[derive(Debug)]
pub enum GeminiError {
    RateLimited { retry_after_secs: Option<u64>, message: Option<String> },
    ServerError(String),
    RequestFailed(reqwest::Error),
    ParseError(String),
}

impl std::fmt::Display for GeminiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RateLimited { retry_after_secs, message } =>
                write!(f, "Rate limited (retry_after={:?}s, message={:?})", retry_after_secs, message),
            Self::ServerError(msg) => write!(f, "Server error: {}", msg),
            Self::RequestFailed(e) => write!(f, "Request failed: {}", e),
            Self::ParseError(msg) => write!(f, "Parse error: {}", msg),
        }
    }
}
impl std::error::Error for GeminiError {}

fn parse_retry_after(response: &Response) -> Option<u64> {
    response
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
}

async fn call_gemini_once(
    client: &Client,
    api_key: &str,
    model: &str,
    request_body: &GeminiRequest,
) -> Result<GeminiResponse, GeminiError> {
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        model, api_key
    );

    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(request_body)
        .send()
        .await
        .map_err(GeminiError::RequestFailed)?;

    match response.status() {
        StatusCode::OK => {
            let body: GeminiResponse = response
                .json()
                .await
                .map_err(|e| GeminiError::ParseError(e.to_string()))?;
            Ok(body)
        }
        StatusCode::TOO_MANY_REQUESTS => {
            let retry_after = parse_retry_after(&response);
            let mut error_message: Option<String> = None;
            if let Ok(err_body) = response.json::<GeminiErrorEnvelope>().await {
                eprintln!(
                    "[RATE_LIMIT] Gemini 429: status={} message={}",
                    err_body.error.status, err_body.error.message
                );
                error_message = Some(err_body.error.message);
            }
            Err(GeminiError::RateLimited { retry_after_secs: retry_after, message: error_message })
        }
        status if status.is_server_error() => {
            let msg = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown server error".to_string());
            Err(GeminiError::ServerError(msg))
        }
        status => {
            let msg = response.text().await.unwrap_or_default();
            Err(GeminiError::ServerError(format!("Unexpected status {}: {}", status, msg)))
        }
    }
}

/// Parses "retry in Xs" or "retry in X.Ys" from a Gemini error message body.
fn parse_retry_seconds_from_message(message: &str) -> Option<f64> {
    // Look for pattern: "retry in <number>s" (case-insensitive)
    let lower = message.to_lowercase();
    if let Some(idx) = lower.find("retry in ") {
        let after = &lower[idx + 9..]; // skip "retry in "
        let num_str: String = after.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
        if !num_str.is_empty() {
            return num_str.parse::<f64>().ok();
        }
    }
    // Also try "retry after <number>s"
    if let Some(idx) = lower.find("retry after ") {
        let after = &lower[idx + 12..];
        let num_str: String = after.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
        if !num_str.is_empty() {
            return num_str.parse::<f64>().ok();
        }
    }
    None
}

fn backoff_duration(attempt: u32, hint_secs: Option<u64>, message: Option<&str>) -> Duration {
    // Priority 1: Parse dynamic wait time from the error message body
    if let Some(msg) = message {
        if let Some(wait_secs) = parse_retry_seconds_from_message(msg) {
            let with_buffer = (wait_secs.ceil() as u64) + 2;
            eprintln!("[BACKOFF] Parsed wait from error message: {:.1}s + 2s buffer = {}s", wait_secs, with_buffer);
            return Duration::from_secs(with_buffer);
        }
    }

    // Priority 2: Respect the Retry-After HTTP header
    if let Some(hint) = hint_secs {
        let with_buffer = hint + 2;
        eprintln!("[BACKOFF] Respecting Retry-After header: {}s", with_buffer);
        return Duration::from_secs(with_buffer);
    }

    // Priority 3: Default patience floor of 30s with exponential jitter
    let exponential = BACKOFF_BASE_MS
        .saturating_mul(1u64.checked_shl(attempt).unwrap_or(1u64 << 31))
        .min(BACKOFF_MAX_MS);

    let jitter_ms = rand::thread_rng().gen_range(0..=exponential);
    let wait = Duration::from_millis(jitter_ms);

    eprintln!(
        "[BACKOFF] Attempt {}: sleeping {}ms (floor={}ms, cap={}ms)",
        attempt, jitter_ms, BACKOFF_BASE_MS, exponential
    );

    wait
}

pub async fn call_gemini_with_retry(
    client: &Client,
    system_prompt: String,
    user_prompt: String,
    model: &str,
    max_output_tokens: u32,
) -> Result<String, GeminiError> {
    let api_key = env::var("GEMINI_API_KEY").unwrap_or_default().trim().trim_matches('"').to_string();
    let combined_prompt = format!("{}\n\n{}", system_prompt, user_prompt);

    let request_body = GeminiRequest {
        contents: vec![GeminiContent {
            role: "user".to_string(),
            parts: vec![GeminiPart { text: combined_prompt }],
        }],
        generation_config: GenerationConfig {
            max_output_tokens,
            temperature: 0.0,
        },
        safety_settings: vec![
            SafetySetting { category: "HARM_CATEGORY_HARASSMENT".to_string(), threshold: "BLOCK_NONE".to_string() },
            SafetySetting { category: "HARM_CATEGORY_HATE_SPEECH".to_string(), threshold: "BLOCK_NONE".to_string() },
            SafetySetting { category: "HARM_CATEGORY_SEXUALLY_EXPLICIT".to_string(), threshold: "BLOCK_NONE".to_string() },
            SafetySetting { category: "HARM_CATEGORY_DANGEROUS_CONTENT".to_string(), threshold: "BLOCK_NONE".to_string() },
        ],
    };

    for attempt in 0..MAX_RETRIES {
        match call_gemini_once(client, &api_key, model, &request_body).await {
            Ok(response) => {
                if let Some(usage) = &response.usage_metadata {
                    let input = usage.prompt_token_count.unwrap_or(0);
                    let output = usage.candidates_token_count.unwrap_or(0);
                    let total = usage.total_token_count.unwrap_or(input + output);
                    println!("\n{}", style(format!("[+] Telemetry - Input Tokens: {}, Output Tokens: {}, Total: {}", input, output, total)).green().bold());
                }

                let text = response.candidates
                    .unwrap_or_default()
                    .into_iter()
                    .next()
                    .and_then(|c| c.content.parts.into_iter().next())
                    .map(|p| p.text)
                    .ok_or_else(|| GeminiError::ParseError("Empty candidates".to_string()))?;
                return Ok(text);
            }
            Err(GeminiError::RateLimited { retry_after_secs, message }) => {
                if attempt == MAX_RETRIES - 1 {
                    eprintln!("[FATAL] Max retries ({}) exhausted on rate limit.", MAX_RETRIES);
                    return Err(GeminiError::RateLimited { retry_after_secs, message });
                }
                let wait = backoff_duration(attempt, retry_after_secs, message.as_deref());
                eprintln!("[RETRY] Rate limited. Waiting {:?} before attempt {}...", wait, attempt + 1);
                sleep(wait).await;
            }
            Err(e) => return Err(e),
        }
    }
    unreachable!()
}

pub async fn extract_target_date(client: &Client, question: &str, current_date: &str) -> String {
    // [PROPRIETARY AGENT 1 PROMPT REDACTED FOR PUBLIC REPOSITORY]
    let system_prompt = "[PROPRIETARY AGENT 1 PROMPT REDACTED FOR PUBLIC REPOSITORY] Extract a target date in YYYY-MM-DD format.";
    let user_prompt = format!("Current Date: {}\nQuestion: {}", current_date, question);

    match call_gemini_with_retry(client, system_prompt.to_string(), user_prompt, "gemini-3.1-flash-lite", 50).await {
        Ok(text) => {
            let text = text.trim();
            if text.len() >= 10 && text.contains("-") {
                return text[..10].to_string();
            }
            fallback_date(current_date)
        }
        Err(_) => fallback_date(current_date),
    }
}

fn fallback_date(current_date: &str) -> String {
    if let Ok(date) = NaiveDate::parse_from_str(current_date, "%Y-%m-%d") {
        if let Some(next_year) = date.with_year(date.year() + 1) {
            return next_year.format("%Y-%m-%d").to_string();
        }
    }
    "2030-01-01".to_string()
}
