use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::{
    cache::CacheMetadata, cli::UseCase, hardware::HardwareProfile, models::ModelCandidate,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    pub rank: usize,
    pub model: ModelCandidate,
    pub score: i32,
    pub best_for: Vec<UseCase>,
    pub expected_performance: String,
    pub memory_estimate: String,
    pub why: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvoidedModel {
    pub model: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecommendationOutput {
    pub hardware: HardwareProfile,
    pub use_case: UseCase,
    pub recommendations: Vec<Recommendation>,
    pub avoided_models: Vec<AvoidedModel>,
    pub warnings: Vec<String>,
    pub cache: CacheMetadata,
    pub estimates_are_approximate: bool,
}

pub fn recommend(
    hardware: HardwareProfile,
    models: Vec<ModelCandidate>,
    use_case: UseCase,
    cache: CacheMetadata,
    mut warnings: Vec<String>,
) -> RecommendationOutput {
    let safe_memory = hardware.safe_usable_memory_gb();
    let mut accepted = Vec::new();
    let mut avoided = Vec::new();

    for model in models {
        if model.minimum_ram_gb > safe_memory {
            avoided.push(AvoidedModel {
                model: model.display_name.clone(),
                reason: format!(
                    "needs about {:.1} GB minimum; this machine's safe model budget is {:.1} GB",
                    model.minimum_ram_gb, safe_memory
                ),
            });
            continue;
        }
        accepted.push((score_model(&model, &hardware, use_case), model));
    }

    accepted.sort_by(|left, right| {
        right.0.cmp(&left.0).then_with(|| {
            left.1
                .recommended_ram_gb
                .partial_cmp(&right.1.recommended_ram_gb)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    });
    avoided.sort_by(|left, right| left.model.cmp(&right.model));

    let recommendations = accepted
        .into_iter()
        .take(5)
        .enumerate()
        .map(|(index, (score, model))| Recommendation {
            rank: index + 1,
            expected_performance: performance_label(&model, &hardware).into(),
            memory_estimate: format!(
                "{:.1}–{:.1} GB",
                model.minimum_ram_gb, model.recommended_ram_gb
            ),
            why: recommendation_reason(&model, &hardware, use_case),
            best_for: model.use_cases.clone(),
            model,
            score,
        })
        .collect();

    if hardware.available_ram_gb + 0.5 < safe_memory {
        warnings.push(format!(
            "Only {:.1} GB is currently available. Close memory-heavy apps before loading a larger model.",
            hardware.available_ram_gb
        ));
    }
    warnings.extend(hardware.warnings.clone());

    RecommendationOutput {
        hardware,
        use_case,
        recommendations,
        avoided_models: avoided.into_iter().take(8).collect(),
        warnings,
        cache,
        estimates_are_approximate: true,
    }
}

pub fn score_model(model: &ModelCandidate, hardware: &HardwareProfile, use_case: UseCase) -> i32 {
    let safe = hardware.safe_usable_memory_gb();
    let mut score = if model.recommended_ram_gb <= safe * 0.8 {
        30
    } else if model.recommended_ram_gb <= safe {
        15
    } else {
        5
    };

    let remaining = safe - model.minimum_ram_gb;
    if remaining < 2.0 || remaining < safe * 0.1 {
        score -= 20;
    }
    if model.installed_locally {
        score += 20;
    }

    let name = format!("{} {}", model.display_name, model.family).to_ascii_lowercase();
    let params = model.parameter_size_billion.unwrap_or(7.0);
    match use_case {
        UseCase::Coding => {
            if ["coder", "code", "qwen", "deepseek", "starcoder"]
                .iter()
                .any(|term| name.contains(term))
            {
                score += 30;
            }
            if model.use_cases.contains(&UseCase::Coding) {
                score += 12;
            }
            score += if (6.0..=10.0).contains(&params) {
                18
            } else if (10.0..=20.0).contains(&params) {
                14
            } else if params >= 3.0 {
                8
            } else {
                -3
            };
        }
        UseCase::Chat => {
            if ["llama", "mistral", "gemma", "phi"]
                .iter()
                .any(|term| name.contains(term))
            {
                score += 24;
            }
            if model.use_cases.contains(&UseCase::Chat) {
                score += 10;
            }
        }
        UseCase::Agent => {
            if (3.0..=8.0).contains(&params) {
                score += 28;
            } else if params < 3.0 {
                score += 14;
            } else if params > 14.0 {
                score -= 25;
            }
            if model.use_cases.contains(&UseCase::Agent) {
                score += 18;
            }
            if model.recommended_ram_gb <= safe * 0.5 {
                score += 10;
            }
        }
        UseCase::Reasoning => {
            if params >= 14.0 && model.recommended_ram_gb <= safe * 0.85 {
                score += 28;
            } else if params >= 7.0 {
                score += 16;
            }
            if ["qwen", "llama", "deepseek", "mistral"]
                .iter()
                .any(|term| name.contains(term))
            {
                score += 15;
            }
        }
    }

    if hardware.max_vram_gb().is_none() && !hardware.apple_silicon {
        if params > 14.0 {
            score -= 25;
        } else if params > 8.0 {
            score -= 10;
        }
    }
    if hardware
        .max_vram_gb()
        .is_some_and(|vram| model.recommended_ram_gb <= vram)
    {
        score += 12;
    }

    score += popularity_boost(model);
    score += recency_boost(model);
    score += match model.quantization.as_deref() {
        Some("Q4_K_M" | "Q5_K_M") => 8,
        Some("Q4_0" | "Q5_0" | "Q6_K") => 4,
        Some(value) if value.starts_with("Q2") => -10,
        Some(value) if value.starts_with("Q3") => -5,
        _ => 0,
    };
    score
}

fn popularity_boost(model: &ModelCandidate) -> i32 {
    let downloads = model.downloads.unwrap_or(0);
    let likes = model.likes.unwrap_or(0);
    (match downloads {
        1_000_000.. => 8,
        100_000.. => 6,
        10_000.. => 4,
        1_000.. => 2,
        _ => 0,
    }) + if likes >= 500 {
        3
    } else if likes >= 50 {
        1
    } else {
        0
    }
}

fn recency_boost(model: &ModelCandidate) -> i32 {
    let Some(modified) = model.last_modified else {
        return 0;
    };
    let days = Utc::now().signed_duration_since(modified).num_days();
    if days <= 90 {
        5
    } else if days <= 365 {
        2
    } else {
        0
    }
}

fn performance_label(model: &ModelCandidate, hardware: &HardwareProfile) -> &'static str {
    let params = model.parameter_size_billion.unwrap_or(7.0);
    let accelerated = hardware.apple_silicon
        || hardware
            .max_vram_gb()
            .is_some_and(|vram| model.minimum_ram_gb <= vram);
    if accelerated {
        if params <= 8.0 {
            "fast"
        } else if params <= 20.0 {
            "usable"
        } else {
            "slow"
        }
    } else if params <= 3.0 {
        "fast"
    } else if params <= 8.0 {
        "usable"
    } else {
        "slow"
    }
}

fn recommendation_reason(
    model: &ModelCandidate,
    hardware: &HardwareProfile,
    use_case: UseCase,
) -> String {
    if model.recommended_ram_gb > hardware.available_ram_gb {
        return format!(
            "A strong {} candidate for this hardware, but only {:.1} GB is available now; close other memory-heavy apps first.",
            use_case, hardware.available_ram_gb
        );
    }
    let fit = if model.recommended_ram_gb <= hardware.safe_usable_memory_gb() * 0.7 {
        "fits comfortably and leaves useful memory headroom"
    } else if model.recommended_ram_gb <= hardware.safe_usable_memory_gb() {
        "should fit, though it uses much of the safe memory budget"
    } else {
        "can fit at its minimum estimate, but context size may need to be reduced"
    };
    let local = if model.installed_locally {
        " It is already installed in Ollama."
    } else {
        ""
    };
    format!("A strong {} candidate that {}.{}", use_case, fit, local)
}

pub const EXPLANATION: &str = r#"How recommendations work

1. Memory fit
   The advisor reserves OS/application headroom: 2 GB on <=8 GB machines, 4 GB around
   16 GB, 6 GB around 24 GB, and 25% on 32 GB+ machines. Models whose minimum estimate
   exceeds the remaining budget are excluded.

2. Memory estimates
   Known GGUF size: minimum = file size × 1.25; recommended = file size × 1.5.
   Otherwise Q4 uses roughly 0.5 GB per billion parameters, Q5 0.65 GB, and Q8 1.1 GB,
   plus 1–3 GB for runtime, context, and working memory.

3. Ranking
   Models gain points for comfortable fit, use-case suitability, practical Q4_K_M/Q5_K_M
   quantization, popularity, recency, GPU fit, and being installed locally. Large models are
   penalized on CPU-only machines. Agent mode favors responsive 3B–8B models; reasoning mode
   rewards larger models only when they fit comfortably.

All memory and performance labels are approximate. Context length, runner, quantization,
prompt caching, thermals, and other running applications can materially change results."#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{hardware::mock_profile, sources::static_fallback::fallback_models};

    #[test]
    fn filters_models_that_exceed_safe_memory() {
        let hardware = mock_profile(8.0, false, None);
        let cache = CacheMetadata {
            path: "test".into(),
            status: "test".into(),
            fetched_at: None,
            stale: false,
        };
        let result = recommend(hardware, fallback_models(), UseCase::Chat, cache, vec![]);
        assert!(result
            .recommendations
            .iter()
            .all(|item| item.model.minimum_ram_gb <= 6.0));
        assert!(result
            .avoided_models
            .iter()
            .any(|item| item.model.contains("14b")));
    }

    #[test]
    fn coding_prefers_coder_models() {
        let hardware = mock_profile(24.0, true, None);
        let models = fallback_models();
        let qwen = models
            .iter()
            .find(|model| model.display_name == "qwen2.5-coder:7b")
            .unwrap();
        let mistral = models
            .iter()
            .find(|model| model.display_name == "mistral:7b")
            .unwrap();
        assert!(
            score_model(qwen, &hardware, UseCase::Coding)
                > score_model(mistral, &hardware, UseCase::Coding)
        );
    }

    #[test]
    fn coding_prefers_capable_7b_over_tiny_model_when_both_fit() {
        let hardware = mock_profile(24.0, true, None);
        let models = fallback_models();
        let tiny = models
            .iter()
            .find(|model| model.display_name == "qwen2.5-coder:1.5b")
            .unwrap();
        let capable = models
            .iter()
            .find(|model| model.display_name == "qwen2.5-coder:7b")
            .unwrap();
        assert!(
            score_model(capable, &hardware, UseCase::Coding)
                > score_model(tiny, &hardware, UseCase::Coding)
        );
    }

    #[test]
    fn agent_prefers_responsive_models() {
        let hardware = mock_profile(64.0, false, Some(24.0));
        let models = fallback_models();
        let small = models
            .iter()
            .find(|model| model.display_name == "qwen2.5-coder:7b")
            .unwrap();
        let large = models
            .iter()
            .find(|model| model.display_name == "qwen2.5-coder:14b")
            .unwrap();
        assert!(
            score_model(small, &hardware, UseCase::Agent)
                > score_model(large, &hardware, UseCase::Agent)
        );
    }

    #[test]
    fn json_has_required_top_level_shape() {
        let hardware = mock_profile(16.0, false, None);
        let cache = CacheMetadata {
            path: "test".into(),
            status: "offline".into(),
            fetched_at: None,
            stale: false,
        };
        let value = serde_json::to_value(recommend(
            hardware,
            fallback_models(),
            UseCase::Chat,
            cache,
            vec![],
        ))
        .unwrap();
        for key in [
            "hardware",
            "recommendations",
            "avoided_models",
            "warnings",
            "cache",
        ] {
            assert!(value.get(key).is_some(), "missing {key}");
        }
    }

    #[test]
    fn generates_safe_ollama_pull_command() {
        let mut model = fallback_models().remove(0);
        model.install_command = None;
        model.set_ollama_command();
        assert_eq!(
            model.install_command.as_deref(),
            Some("ollama pull llama3.2:1b")
        );
    }

    #[test]
    fn mocked_profiles_all_produce_recommendations() {
        for profile in [
            mock_profile(8.0, false, None),
            mock_profile(16.0, false, None),
            mock_profile(24.0, true, None),
            mock_profile(64.0, false, Some(24.0)),
        ] {
            let cache = CacheMetadata {
                path: "test".into(),
                status: "offline".into(),
                fetched_at: None,
                stale: false,
            };
            assert!(
                !recommend(profile, fallback_models(), UseCase::Chat, cache, vec![])
                    .recommendations
                    .is_empty()
            );
        }
    }
}
