mod classifier;

pub use classifier::{classify_input, resolve_location, InputCategory};

use reqwest::Client;
use serde::{Deserialize, Serialize};

pub const OLLAMA_URL: &str = "http://localhost:11434/api/generate";
pub const PLAYER_MODEL: &str = "llama3.1:8b-instruct-q8_0";
pub const NPC_MODEL: &str = "llama3.1:8b-instruct-q8_0";
pub const CLASSIFIER_MODEL: &str = "llama3.2:3b"; // or whatever smaller model you have

#[derive(Serialize)]
pub struct OllamaRequest {
    pub model: String,
    pub prompt: String,
    pub stream: bool,
}

#[derive(Deserialize)]
pub struct OllamaResponse {
    pub response: String,
}

pub async fn call_ollama(model: &str, prompt: &str) -> Result<String, String> {
    let client = Client::new();

    let req = OllamaRequest {
        model: model.to_string(),
        prompt: prompt.to_string(),
        stream: false,
    };

    let res = client
        .post(OLLAMA_URL)
        .json(&req)
        .send()
        .await
        .map_err(|e| format!("ollama request failed: {}", e))?;

    let body: OllamaResponse = res
        .json()
        .await
        .map_err(|e| format!("failed to parse ollama response: {}", e))?;

    Ok(body.response)
}
