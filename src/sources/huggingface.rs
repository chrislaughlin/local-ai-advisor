use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::Url;
use serde::Deserialize;
use std::{collections::BTreeMap, time::Duration};

use crate::{
    models::{
        infer_family, infer_use_cases, is_safe_model_name, parse_parameter_size,
        parse_quantization, ModelCandidate,
    },
    sources::ModelSource,
};

#[derive(Clone)]
pub struct HuggingFaceGgufSource {
    client: reqwest::Client,
    api_base: Url,
    repository_limit: usize,
}

impl HuggingFaceGgufSource {
    pub fn new(client: reqwest::Client) -> Self {
        Self {
            client,
            api_base: Url::parse("https://huggingface.co/api/").expect("valid Hugging Face URL"),
            repository_limit: 40,
        }
    }

    fn models_url(&self) -> Result<Url> {
        let mut url = self
            .api_base
            .join("models")
            .context("invalid model API URL")?;
        url.query_pairs_mut()
            .append_pair("filter", "gguf")
            .append_pair("sort", "downloads")
            .append_pair("direction", "-1")
            .append_pair("limit", &self.repository_limit.to_string())
            .append_pair("full", "true");
        Ok(url)
    }

    fn tree_url(&self, repo_id: &str) -> Result<Url> {
        let mut url = self.api_base.clone();
        {
            let mut segments = url
                .path_segments_mut()
                .map_err(|_| anyhow::anyhow!("invalid Hugging Face API base URL"))?;
            segments.pop_if_empty().push("models");
            for segment in repo_id.split('/') {
                segments.push(segment);
            }
            segments.push("tree").push("main");
        }
        url.query_pairs_mut().append_pair("recursive", "true");
        Ok(url)
    }
}

#[derive(Debug, Deserialize)]
struct HubModel {
    id: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(rename = "pipeline_tag", default)]
    pipeline_tag: Option<String>,
    #[serde(default)]
    downloads: Option<u64>,
    #[serde(default)]
    likes: Option<u64>,
    #[serde(rename = "lastModified", default)]
    last_modified: Option<DateTime<Utc>>,
    #[serde(default)]
    siblings: Vec<HubSibling>,
}

#[derive(Debug, Clone, Deserialize)]
struct HubSibling {
    #[serde(rename = "rfilename", alias = "path")]
    path: String,
    #[serde(default)]
    size: Option<u64>,
}

#[async_trait]
impl ModelSource for HuggingFaceGgufSource {
    async fn fetch_models(&self) -> Result<Vec<ModelCandidate>> {
        let repositories = self
            .client
            .get(self.models_url()?)
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .context("failed to query Hugging Face")?
            .error_for_status()
            .context("Hugging Face model search failed")?
            .json::<Vec<HubModel>>()
            .await
            .context("Hugging Face returned invalid search metadata")?;

        let mut candidates = Vec::new();
        for repository in repositories {
            if !is_safe_model_name(&repository.id) || !is_relevant_repository(&repository) {
                continue;
            }
            let mut files = repository.siblings.clone();
            if !files.iter().any(|file| {
                file.path.to_ascii_lowercase().ends_with(".gguf") && file.size.is_some()
            }) {
                if let Ok(response) = self
                    .client
                    .get(self.tree_url(&repository.id)?)
                    .timeout(Duration::from_secs(6))
                    .send()
                    .await
                {
                    if response.status().is_success() {
                        if let Ok(tree) = response.json::<Vec<HubSibling>>().await {
                            files = tree;
                        }
                    }
                }
            }

            let ggufs: Vec<HubSibling> = files
                .into_iter()
                .filter(|file| {
                    file.path.to_ascii_lowercase().ends_with(".gguf")
                        && file.path.len() <= 240
                        && file.path.chars().all(|character| {
                            character.is_ascii_alphanumeric() || "-_./".contains(character)
                        })
                })
                .collect();
            let mut grouped: BTreeMap<String, Vec<HubSibling>> = BTreeMap::new();
            for file in ggufs {
                let key = parse_quantization(&file.path).unwrap_or_else(|| "GGUF".into());
                grouped.entry(key).or_default().push(file);
            }
            let mut variants: Vec<(String, Vec<HubSibling>)> = grouped.into_iter().collect();
            variants.sort_by_key(|(quant, _)| {
                quant_preference(if quant == "GGUF" {
                    None
                } else {
                    Some(quant.as_str())
                })
            });

            for (quant, files) in variants.into_iter().take(2) {
                let quantization = (quant != "GGUF").then_some(quant);
                let params = files
                    .iter()
                    .find_map(|file| parse_parameter_size(&file.path))
                    .or_else(|| parse_parameter_size(&repository.id));
                let family = infer_family(&repository.id);
                let size = files
                    .iter()
                    .map(|file| file.size)
                    .collect::<Option<Vec<_>>>()
                    .map(|sizes| sizes.into_iter().sum::<u64>() as f64 / 1024_f64.powi(3));
                let mut candidate = ModelCandidate {
                    id: format!(
                        "hf:{}/{}",
                        repository.id,
                        quantization.as_deref().unwrap_or("GGUF")
                    ),
                    display_name: format!(
                        "{} ({})",
                        repository.id,
                        quantization.as_deref().unwrap_or("GGUF")
                    ),
                    source: "huggingface".into(),
                    repo_id: Some(repository.id.clone()),
                    ollama_name: None,
                    family: family.clone(),
                    parameter_size_billion: params,
                    quantization,
                    file_size_gb: size,
                    minimum_ram_gb: 0.0,
                    recommended_ram_gb: 0.0,
                    strengths: vec!["Popular public GGUF release".into()],
                    weaknesses: vec!["Requires a compatible GGUF runner and manual download".into()],
                    use_cases: infer_use_cases(&repository.id, &family),
                    downloads: repository.downloads,
                    likes: repository.likes,
                    last_modified: repository.last_modified,
                    installed_locally: false,
                    install_command: None,
                };
                candidate.refresh_estimates();
                if candidate.minimum_ram_gb > 0.0 {
                    candidates.push(candidate);
                }
            }
        }
        Ok(candidates)
    }
}

fn is_relevant_repository(repository: &HubModel) -> bool {
    let id = repository.id.to_ascii_lowercase();
    if id.contains("embed")
        || matches!(
            repository.pipeline_tag.as_deref(),
            Some("feature-extraction" | "sentence-similarity")
        )
    {
        return false;
    }
    let known_family = [
        "llama",
        "qwen",
        "mistral",
        "gemma",
        "deepseek",
        "phi",
        "coder",
        "code",
        "starcoder",
    ]
    .iter()
    .any(|term| id.contains(term));
    let generation_metadata = repository.pipeline_tag.as_deref() == Some("text-generation")
        || repository
            .tags
            .iter()
            .any(|tag| tag.eq_ignore_ascii_case("text-generation"));
    known_family || generation_metadata
}

fn quant_preference(quantization: Option<&str>) -> u8 {
    match quantization {
        Some("Q4_K_M") => 0,
        Some("Q5_K_M") => 1,
        Some("Q4_0") => 2,
        Some("Q6_K") => 3,
        Some("Q8_0") => 4,
        Some(_) => 5,
        None => 6,
    }
}
