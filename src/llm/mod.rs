mod classifier;

pub use classifier::{classify_input, resolve_location, InputCategory, should_npc_act};

use reqwest::Client;
use serde::{Deserialize, Serialize};

pub const OLLAMA_URL: &str = "http://localhost:11434/api/generate";

// Models are env-overridable so you can A/B a swap without a rebuild:
//   WE_PLAYER_MODEL=phi3:mini cargo run
//
// The defaults matter. A 3B model cannot hold this output format — it turns
// properties into edges and rambles past any stop instruction. Narrative
// quality here is dominated by model size, not by prompt wording.
fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

pub fn player_model() -> &'static str {
    static M: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    M.get_or_init(|| env_or("WE_PLAYER_MODEL", "llama3.1:8b-instruct-q8_0"))
}

pub fn npc_model() -> &'static str {
    static M: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    M.get_or_init(|| env_or("WE_NPC_MODEL", "llama3.1:8b-instruct-q8_0"))
}

pub fn classifier_model() -> &'static str {
    static M: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    M.get_or_init(|| env_or("WE_CLASSIFIER_MODEL", "llama3.2:3b"))
}

/// Token budgets. Without a cap the NPC agent will happily emit sixty lines of
/// increasingly unhinged pseudo-Edgescript.
pub const CLASSIFIER_TOKENS: u32 = 8;
pub const NPC_TOKENS: u32 = 250;
pub const PLAYER_TOKENS: u32 = 400;

#[derive(Serialize)]
pub struct OllamaOptions {
    pub num_predict: u32,
    pub temperature: f32,
    pub stop: Vec<String>,
}

#[derive(Serialize)]
pub struct OllamaRequest {
    pub model: String,
    pub prompt: String,
    pub stream: bool,
    pub options: OllamaOptions,
}

#[derive(Deserialize)]
pub struct OllamaResponse {
    pub response: String,
}

/// Ollama returns `{"error": "..."}` for things like an unpulled model.
#[derive(Deserialize)]
struct OllamaError {
    error: String,
}

pub const REQUEST_TIMEOUT_SECS: u64 = 180;

/// One client for the whole process. `Client::new()` per call built a fresh
/// connection pool every time.
fn client() -> &'static Client {
    static CLIENT: std::sync::OnceLock<Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        Client::builder()
            .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()
            .expect("failed to build http client")
    })
}

pub async fn call_ollama(model: &str, prompt: &str) -> Result<String, String> {
    call_ollama_capped(model, prompt, PLAYER_TOKENS).await
}

pub async fn call_ollama_capped(
    model: &str,
    prompt: &str,
    max_tokens: u32,
) -> Result<String, String> {
    let req = OllamaRequest {
        model: model.to_string(),
        prompt: prompt.to_string(),
        stream: false,
        options: OllamaOptions {
            num_predict: max_tokens,
            temperature: 0.7,
            // Models like to append commentary after the patch. Cut it off.
            stop: vec!["\n\nNote:".into(), "\n\nExplanation:".into(), "```".into()],
        },
    };

    let res = client()
        .post(OLLAMA_URL)
        .json(&req)
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                format!("ollama timed out after {}s (model: {})", REQUEST_TIMEOUT_SECS, model)
            } else {
                format!("ollama request failed: {} — is `ollama serve` running?", e)
            }
        })?;

    let body = res
        .text()
        .await
        .map_err(|e| format!("could not read ollama response: {}", e))?;

    // Parse the success shape first; fall back to reporting the actual error
    // rather than a misleading "failed to parse" message.
    if let Ok(parsed) = serde_json::from_str::<OllamaResponse>(&body) {
        return Ok(parsed.response);
    }
    if let Ok(err) = serde_json::from_str::<OllamaError>(&body) {
        return Err(format!("ollama rejected the request: {} (model: {})", err.error, model));
    }
    Err(format!(
        "unexpected ollama response: {}",
        body.chars().take(200).collect::<String>()
    ))
}
