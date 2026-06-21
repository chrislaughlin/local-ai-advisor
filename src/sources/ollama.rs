use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use std::time::Duration;

use crate::{
    models::{
        infer_family, infer_use_cases, is_safe_model_name, parse_parameter_size, ModelCandidate,
    },
    sources::ModelSource,
};

#[derive(Clone)]
pub struct OllamaLocalSource {
    client: reqwest::Client,
    base_url: String,
}

impl OllamaLocalSource {
    pub fn new(client: reqwest::Client) -> Self {
        Self {
            client,
            base_url: "http://127.0.0.1:11434".into(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct TagsResponse {
    #[serde(default)]
    models: Vec<OllamaModel>,
}

#[derive(Debug, Deserialize)]
struct OllamaModel {
    name: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    details: OllamaDetails,
}

#[derive(Debug, Default, Deserialize)]
struct OllamaDetails {
    parameter_size: Option<String>,
    quantization_level: Option<String>,
    family: Option<String>,
}

#[async_trait]
impl ModelSource for OllamaLocalSource {
    async fn fetch_models(&self) -> Result<Vec<ModelCandidate>> {
        let response = self
            .client
            .get(format!("{}/api/tags", self.base_url))
            .timeout(Duration::from_secs(2))
            .send()
            .await
            .context("Ollama API is not reachable")?
            .error_for_status()
            .context("Ollama API returned an error")?
            .json::<TagsResponse>()
            .await
            .context("Ollama returned invalid model metadata")?;

        Ok(response
            .models
            .into_iter()
            .filter(|model| is_safe_model_name(&model.name))
            .map(|model| {
                let file_size_gb = if model.size > 0 {
                    Some(model.size as f64 / 1024_f64.powi(3))
                } else {
                    None
                };
                let params = model
                    .details
                    .parameter_size
                    .as_deref()
                    .and_then(parse_parameter_size)
                    .or_else(|| parse_parameter_size(&model.name));
                let family = model
                    .details
                    .family
                    .unwrap_or_else(|| infer_family(&model.name));
                let use_cases = infer_use_cases(&model.name, &family);
                let mut candidate = ModelCandidate {
                    id: format!("ollama/{}", model.name),
                    display_name: model.name.clone(),
                    source: "ollama-local".into(),
                    repo_id: None,
                    ollama_name: Some(model.name),
                    family: family.clone(),
                    parameter_size_billion: params,
                    quantization: model.details.quantization_level,
                    file_size_gb,
                    minimum_ram_gb: 0.0,
                    recommended_ram_gb: 0.0,
                    strengths: vec!["Already installed locally".into()],
                    weaknesses: vec![],
                    use_cases,
                    downloads: None,
                    likes: None,
                    last_modified: None,
                    installed_locally: true,
                    install_command: None,
                };
                candidate.refresh_estimates();
                candidate
            })
            .collect())
    }
}
