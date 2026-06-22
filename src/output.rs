use anyhow::Result;
use serde::Serialize;

use crate::{
    advisor::RecommendationOutput, cache::CacheMetadata, cli::OutputFormat,
    hardware::HardwareProfile, models::ModelCandidate,
};

pub fn print_hardware(hardware: &HardwareProfile, format: OutputFormat) -> Result<()> {
    if format == OutputFormat::Json {
        println!("{}", serde_json::to_string_pretty(hardware)?);
        return Ok(());
    }
    print_hardware_summary(hardware);
    if !hardware.warnings.is_empty() {
        println!("\nWarnings:");
        for warning in &hardware.warnings {
            println!("- {warning}");
        }
    }
    Ok(())
}

pub fn print_recommendations(output: &RecommendationOutput, format: OutputFormat) -> Result<()> {
    if format == OutputFormat::Json {
        println!("{}", serde_json::to_string_pretty(output)?);
        return Ok(());
    }

    print_hardware_summary(&output.hardware);
    println!("\nRecommended models for {}:\n", output.use_case);
    if output.recommendations.is_empty() {
        println!("No catalogue models fit the safe memory budget.");
    }
    for item in &output.recommendations {
        println!("{}. {}", item.rank, item.model.display_name);
        let cases = item
            .best_for
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        println!("   Best for: {cases}");
        println!("   Expected performance: {}", item.expected_performance);
        println!("   Memory estimate: {}", item.memory_estimate);
        println!("   Why: {}", item.why);
        if let Some(command) = &item.model.install_command {
            println!("   Install: {command}");
        } else if let Some(repo) = &item.model.repo_id {
            println!("   Source: https://huggingface.co/{repo}");
        }
        println!();
    }

    if !output.avoided_models.is_empty() {
        println!("Avoid:");
        for model in &output.avoided_models {
            println!("- {}: {}", model.model, model.reason);
        }
    }
    println!(
        "\nCatalogue: {} ({})",
        output.cache.status, output.cache.path
    );
    println!("Estimates are approximate; context length and other running apps affect real usage.");
    if !output.warnings.is_empty() {
        println!("\nWarnings:");
        for warning in &output.warnings {
            println!("- {warning}");
        }
        if output.hardware.available_ram_gb + 0.5 < output.hardware.safe_usable_memory_gb()
            && !output.hardware.top_memory_processes.is_empty()
        {
            println!("- Top memory users you may want to close:");
            for process in &output.hardware.top_memory_processes {
                println!(
                    "  - {} (PID {}): {:.1} GB",
                    process.name, process.pid, process.memory_gb
                );
            }
            println!("  Save your work and verify the process before terminating it; prefer quitting the app normally.");
        }
    }
    Ok(())
}

#[derive(Serialize)]
struct CatalogOutput<'a> {
    models: &'a [ModelCandidate],
    count: usize,
    cache: &'a CacheMetadata,
}

pub fn print_catalogue(
    models: &[ModelCandidate],
    cache: &CacheMetadata,
    format: OutputFormat,
) -> Result<()> {
    if format == OutputFormat::Json {
        let output = CatalogOutput {
            models,
            count: models.len(),
            cache,
        };
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{} model entries ({})", models.len(), cache.status);
        for model in models {
            let params = model
                .parameter_size_billion
                .map(|value| {
                    if value.fract() == 0.0 {
                        format!("{value:.0}B")
                    } else {
                        format!("{value:.1}B")
                    }
                })
                .unwrap_or_else(|| "unknown size".into());
            let quant = model
                .quantization
                .as_deref()
                .unwrap_or("unknown quantization");
            println!(
                "- {} — {}, {}, {:.1}–{:.1} GB RAM [{}]",
                model.display_name,
                params,
                quant,
                model.minimum_ram_gb,
                model.recommended_ram_gb,
                model.source
            );
        }
    }
    Ok(())
}

#[derive(Serialize)]
struct OllamaOutput<'a> {
    installed: bool,
    api_reachable: bool,
    installed_models: &'a [ModelCandidate],
    suggested_pull_commands: Vec<&'a str>,
}

pub fn print_ollama(
    hardware: &HardwareProfile,
    installed_models: &[ModelCandidate],
    recommendations: &RecommendationOutput,
    format: OutputFormat,
) -> Result<()> {
    let commands: Vec<&str> = recommendations
        .recommendations
        .iter()
        .filter(|item| !item.model.installed_locally)
        .filter_map(|item| item.model.install_command.as_deref())
        .take(3)
        .collect();
    if format == OutputFormat::Json {
        let output = OllamaOutput {
            installed: hardware.ollama.installed,
            api_reachable: hardware.ollama.running,
            installed_models,
            suggested_pull_commands: commands,
        };
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    println!(
        "Ollama installed: {}",
        if hardware.ollama.installed {
            "yes"
        } else {
            "no"
        }
    );
    println!(
        "Local API reachable: {}",
        if hardware.ollama.running { "yes" } else { "no" }
    );
    if !hardware.ollama.installed {
        println!("Install Ollama from https://ollama.com/download");
    } else if !hardware.ollama.running {
        println!("Start Ollama to inspect locally installed models.");
    }
    if !installed_models.is_empty() {
        println!("\nInstalled models:");
        for model in installed_models {
            println!("- {}", model.display_name);
        }
    }
    if !commands.is_empty() {
        println!("\nSuggested pulls (not executed):");
        for command in commands {
            println!("- {command}");
        }
    }
    Ok(())
}

fn print_hardware_summary(hardware: &HardwareProfile) {
    println!("Hardware summary:");
    println!("- OS: {} ({})", hardware.os, hardware.architecture);
    println!(
        "- CPU: {} ({} logical cores)",
        hardware.cpu_model, hardware.cpu_cores
    );
    if hardware.unified_memory {
        println!("- Memory: {:.1} GB unified memory", hardware.total_ram_gb);
    } else {
        println!("- Memory: {:.1} GB", hardware.total_ram_gb);
    }
    println!("- Available memory: {:.1} GB", hardware.available_ram_gb);
    println!("- Free disk space: {:.1} GB", hardware.disk_free_gb);
    for gpu in &hardware.gpus {
        if let Some(vram) = gpu.vram_gb {
            println!("- GPU: {} ({vram:.1} GB VRAM)", gpu.name);
        } else {
            println!("- GPU: {}", gpu.name);
        }
    }
    let ollama = match (hardware.ollama.installed, hardware.ollama.running) {
        (true, true) => "installed and running",
        (true, false) => "installed, API not reachable",
        (false, true) => "API reachable (executable not found in PATH)",
        (false, false) => "not detected",
    };
    println!("- Ollama: {ollama}");
    println!(
        "- llama.cpp: {}",
        if hardware.llama_cpp_installed {
            "detected"
        } else {
            "not detected"
        }
    );
}
