use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::cli::UseCase;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelCandidate {
    pub id: String,
    pub display_name: String,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ollama_name: Option<String>,
    pub family: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameter_size_billion: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantization: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_size_gb: Option<f64>,
    pub minimum_ram_gb: f64,
    pub recommended_ram_gb: f64,
    pub strengths: Vec<String>,
    pub weaknesses: Vec<String>,
    pub use_cases: Vec<UseCase>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub downloads: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub likes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<DateTime<Utc>>,
    pub installed_locally: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_command: Option<String>,
}

impl ModelCandidate {
    pub fn refresh_estimates(&mut self) {
        if let Some(size) = self.file_size_gb {
            self.minimum_ram_gb = round1(size * 1.25);
            self.recommended_ram_gb = round1(size * 1.5);
        } else if let Some(params) = self.parameter_size_billion {
            let bytes_per_billion = quantization_gb_per_billion(self.quantization.as_deref());
            let overhead = if params <= 3.0 {
                1.0
            } else if params <= 14.0 {
                2.0
            } else {
                3.0
            };
            let base = params * bytes_per_billion + overhead;
            self.minimum_ram_gb = round1(base);
            self.recommended_ram_gb = round1(base * 1.2);
        }
    }

    pub fn set_ollama_command(&mut self) {
        self.install_command = self
            .ollama_name
            .as_deref()
            .filter(|name| is_safe_model_name(name))
            .map(|name| format!("ollama pull {name}"));
    }
}

pub fn quantization_gb_per_billion(quantization: Option<&str>) -> f64 {
    let value = quantization.unwrap_or("Q4").to_ascii_uppercase();
    if value.starts_with("Q2") || value.starts_with("Q3") {
        0.42
    } else if value.starts_with("Q5") {
        0.65
    } else if value.starts_with("Q6") {
        0.8
    } else if value.starts_with("Q8") {
        1.1
    } else {
        0.5
    }
}

pub fn parse_quantization(name: &str) -> Option<String> {
    let upper = name.to_ascii_uppercase();
    const QUANTS: &[&str] = &[
        "Q4_K_M", "Q5_K_M", "Q3_K_M", "Q3_K_S", "Q4_K_S", "Q5_K_S", "Q6_K", "Q2_K", "Q4_0", "Q5_0",
        "Q8_0",
    ];
    QUANTS
        .iter()
        .find(|quant| upper.contains(**quant))
        .map(|quant| (*quant).to_string())
}

pub fn parse_parameter_size(name: &str) -> Option<f64> {
    let re = Regex::new(r"(?i)(?:^|[-_./])([0-9]+(?:\.[0-9]+)?)b(?:$|[-_./])").ok()?;
    re.captures(name)
        .and_then(|captures| captures.get(1))
        .and_then(|value| value.as_str().parse().ok())
}

pub fn infer_family(name: &str) -> String {
    let name = name.to_ascii_lowercase();
    [
        "qwen",
        "llama",
        "mistral",
        "gemma",
        "deepseek",
        "phi",
        "starcoder",
    ]
    .into_iter()
    .find(|family| name.contains(family))
    .unwrap_or("other")
    .to_string()
}

pub fn infer_use_cases(name: &str, family: &str) -> Vec<UseCase> {
    let lower = name.to_ascii_lowercase();
    let mut cases = vec![UseCase::Chat];
    if lower.contains("code")
        || lower.contains("coder")
        || ["qwen", "deepseek", "starcoder"].contains(&family)
    {
        cases.push(UseCase::Coding);
        cases.push(UseCase::Agent);
    }
    if ["qwen", "llama", "deepseek", "mistral"].contains(&family) {
        cases.push(UseCase::Reasoning);
    }
    cases.sort_by_key(|case| *case as u8);
    cases.dedup();
    cases
}

pub fn is_safe_model_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 160
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || ":._/-".contains(character))
        && !name.starts_with('-')
        && !name.contains("..")
}

pub fn model_key(model: &ModelCandidate) -> String {
    if let Some(name) = &model.ollama_name {
        return format!("ollama:{}", normalize_ollama_name(name));
    }
    format!(
        "{}:{}",
        model
            .repo_id
            .as_deref()
            .unwrap_or(&model.id)
            .to_ascii_lowercase(),
        model.quantization.as_deref().unwrap_or("")
    )
}

pub fn normalize_ollama_name(name: &str) -> String {
    name.trim_end_matches(":latest").to_ascii_lowercase()
}

pub fn merge_models(groups: impl IntoIterator<Item = Vec<ModelCandidate>>) -> Vec<ModelCandidate> {
    let mut merged: Vec<ModelCandidate> = Vec::new();
    for model in groups.into_iter().flatten() {
        let key = model_key(&model);
        if let Some(existing) = merged.iter_mut().find(|item| model_key(item) == key) {
            if model.installed_locally {
                existing.installed_locally = true;
            }
            if existing.downloads.is_none() {
                existing.downloads = model.downloads;
            }
            if existing.likes.is_none() {
                existing.likes = model.likes;
            }
            if existing.repo_id.is_none() {
                existing.repo_id = model.repo_id;
            }
        } else {
            merged.push(model);
        }
    }
    merged
}

fn round1(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gguf_quantizations() {
        assert_eq!(
            parse_quantization("model.Q4_K_M.gguf").as_deref(),
            Some("Q4_K_M")
        );
        assert_eq!(parse_quantization("foo-q8_0.gguf").as_deref(), Some("Q8_0"));
        assert_eq!(parse_quantization("unquantized.gguf"), None);
    }

    #[test]
    fn parses_parameter_sizes_without_confusing_quantization() {
        assert_eq!(
            parse_parameter_size("Qwen2.5-Coder-7B-Instruct-Q4_K_M.gguf"),
            Some(7.0)
        );
        assert_eq!(parse_parameter_size("deepseek-coder-6.7b.gguf"), Some(6.7));
        assert_eq!(parse_parameter_size("model-Q4_K_M.gguf"), None);
    }

    #[test]
    fn rejects_unsafe_pull_names() {
        assert!(is_safe_model_name("qwen2.5-coder:7b"));
        assert!(!is_safe_model_name("model; rm -rf /"));
        assert!(!is_safe_model_name("--help"));
    }
}
