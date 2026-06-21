use anyhow::Result;
use async_trait::async_trait;

use crate::{cli::UseCase, models::ModelCandidate, sources::ModelSource};

#[derive(Debug, Default)]
pub struct StaticFallbackSource;

#[async_trait]
impl ModelSource for StaticFallbackSource {
    async fn fetch_models(&self) -> Result<Vec<ModelCandidate>> {
        Ok(fallback_models())
    }
}

pub fn fallback_models() -> Vec<ModelCandidate> {
    vec![
        model(
            "llama3.2:1b",
            "llama",
            1.0,
            1.6,
            2.2,
            &[UseCase::Chat, UseCase::Agent],
            "Tiny and responsive",
            "Limited reasoning depth",
        ),
        model(
            "llama3.2:3b",
            "llama",
            3.0,
            2.8,
            4.0,
            &[UseCase::Chat, UseCase::Agent],
            "Good general chat for its size",
            "Less capable on hard tasks",
        ),
        model(
            "qwen2.5-coder:1.5b",
            "qwen",
            1.5,
            2.0,
            2.8,
            &[UseCase::Coding, UseCase::Agent],
            "Very fast code completion",
            "Limited repository-scale reasoning",
        ),
        model(
            "qwen2.5-coder:3b",
            "qwen",
            3.0,
            3.0,
            4.2,
            &[UseCase::Coding, UseCase::Agent],
            "Fast coding and tool loops",
            "Can struggle with complex changes",
        ),
        model(
            "qwen2.5-coder:7b",
            "qwen",
            7.0,
            5.5,
            7.0,
            &[UseCase::Coding, UseCase::Agent, UseCase::Reasoning],
            "Strong practical coding",
            "Slower on CPU-only laptops",
        ),
        model(
            "qwen2.5-coder:14b",
            "qwen",
            14.0,
            9.5,
            12.0,
            &[UseCase::Coding, UseCase::Reasoning],
            "Better difficult coding and reasoning",
            "Needs more memory and compute",
        ),
        model(
            "mistral:7b",
            "mistral",
            7.0,
            5.5,
            7.0,
            &[UseCase::Chat, UseCase::Reasoning],
            "Balanced general assistant",
            "Older than newer specialist models",
        ),
        model(
            "deepseek-coder:6.7b",
            "deepseek",
            6.7,
            5.5,
            7.0,
            &[UseCase::Coding, UseCase::Agent, UseCase::Reasoning],
            "Capable code generation",
            "May be less predictable in tool loops",
        ),
        model_with_params(
            "phi3.5",
            "phi",
            3.8,
            3.2,
            4.5,
            &[UseCase::Chat, UseCase::Agent],
            "Compact general assistant",
            "Smaller knowledge and reasoning capacity",
        ),
        model(
            "gemma2:2b",
            "gemma",
            2.0,
            2.4,
            3.4,
            &[UseCase::Chat, UseCase::Agent],
            "Efficient everyday chat",
            "Not a coding specialist",
        ),
        model(
            "gemma2:9b",
            "gemma",
            9.0,
            7.0,
            9.0,
            &[UseCase::Chat, UseCase::Reasoning],
            "High-quality chat at a moderate size",
            "Can be slow without acceleration",
        ),
    ]
}

#[allow(clippy::too_many_arguments)]
fn model(
    name: &str,
    family: &str,
    params: f64,
    minimum: f64,
    recommended: f64,
    use_cases: &[UseCase],
    strength: &str,
    weakness: &str,
) -> ModelCandidate {
    model_with_params(
        name,
        family,
        params,
        minimum,
        recommended,
        use_cases,
        strength,
        weakness,
    )
}

#[allow(clippy::too_many_arguments)]
fn model_with_params(
    name: &str,
    family: &str,
    params: f64,
    minimum: f64,
    recommended: f64,
    use_cases: &[UseCase],
    strength: &str,
    weakness: &str,
) -> ModelCandidate {
    let mut candidate = ModelCandidate {
        id: format!("ollama/{name}"),
        display_name: name.to_string(),
        source: "static-fallback".into(),
        repo_id: None,
        ollama_name: Some(name.to_string()),
        family: family.into(),
        parameter_size_billion: Some(params),
        quantization: Some("Q4_K_M".into()),
        file_size_gb: None,
        minimum_ram_gb: minimum,
        recommended_ram_gb: recommended,
        strengths: vec![strength.into()],
        weaknesses: vec![weakness.into()],
        use_cases: use_cases.to_vec(),
        downloads: None,
        likes: None,
        last_modified: None,
        installed_locally: false,
        install_command: None,
    };
    candidate.set_ollama_command();
    candidate
}
