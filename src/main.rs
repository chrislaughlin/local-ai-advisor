mod advisor;
mod cache;
mod cli;
mod error;
mod hardware;
mod models;
mod output;
mod sources;

use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;

use crate::{
    advisor::{recommend, EXPLANATION},
    cache::{load_catalogue, CacheStore, CatalogueMode},
    cli::{CatalogCommand, Cli, Commands},
    error::AdvisorError,
    models::{merge_models, ModelCandidate},
    sources::{
        huggingface::HuggingFaceGgufSource, ollama::OllamaLocalSource,
        static_fallback::StaticFallbackSource, ModelSource,
    },
};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let client = reqwest::Client::builder()
        .user_agent(concat!("local-ai-advisor/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(4))
        .timeout(Duration::from_secs(12))
        .build()
        .context("failed to create HTTP client")?;

    match cli.command {
        Commands::Scan(args) => {
            let profile = hardware::scan(&client).await;
            output::print_hardware(&profile, args.format)?;
        }
        Commands::Recommend(args) => {
            let profile = hardware::scan(&client).await;
            let cache = CacheStore::discover()?;
            let remote = HuggingFaceGgufSource::new(client.clone());
            let mode = if args.offline {
                CatalogueMode::Offline
            } else if args.online {
                CatalogueMode::ForceRefresh
            } else {
                CatalogueMode::PreferCache
            };
            let mut loaded = load_catalogue(&cache, &remote, mode).await?;
            let local =
                fetch_local_models(&client, profile.ollama.running, &mut loaded.warnings).await;
            loaded.models = merge_models([loaded.models, local]);
            let result = recommend(
                profile,
                loaded.models,
                args.use_case,
                loaded.cache,
                loaded.warnings,
            );
            output::print_recommendations(&result, args.format)?;
        }
        Commands::Catalog { command } => match command {
            CatalogCommand::Refresh(args) => {
                let cache = CacheStore::discover()?;
                let source = HuggingFaceGgufSource::new(client.clone());
                let models = source.fetch_models().await?;
                if models.is_empty() {
                    return Err(AdvisorError::EmptyPublicCatalogue.into());
                }
                let saved = cache.save(models.clone())?;
                let metadata = cache.metadata(Some(&saved), "refreshed");
                output::print_catalogue(&models, &metadata, args.format)?;
            }
            CatalogCommand::Search { query, format } => {
                let cache = CacheStore::discover()?;
                let cached = cache.load().unwrap_or(None);
                let fallback = StaticFallbackSource.fetch_models().await?;
                let models = merge_models([
                    cached
                        .as_ref()
                        .map(|value| value.models.clone())
                        .unwrap_or_default(),
                    fallback,
                ]);
                let query = query.to_ascii_lowercase();
                let matches: Vec<ModelCandidate> = models
                    .into_iter()
                    .filter(|model| {
                        model.display_name.to_ascii_lowercase().contains(&query)
                            || model.family.to_ascii_lowercase().contains(&query)
                            || model
                                .repo_id
                                .as_deref()
                                .is_some_and(|repo| repo.to_ascii_lowercase().contains(&query))
                    })
                    .collect();
                let metadata = cache.metadata(
                    cached.as_ref(),
                    if cached.is_some() {
                        "cache-search"
                    } else {
                        "static-fallback"
                    },
                );
                output::print_catalogue(&matches, &metadata, format)?;
            }
        },
        Commands::Ollama(args) => {
            let profile = hardware::scan(&client).await;
            let mut warnings = Vec::new();
            let installed =
                fetch_local_models(&client, profile.ollama.running, &mut warnings).await;
            let cache = CacheStore::discover()?;
            let remote = HuggingFaceGgufSource::new(client.clone());
            let loaded = load_catalogue(&cache, &remote, CatalogueMode::Offline).await?;
            let models = merge_models([loaded.models, installed.clone()]);
            let result = recommend(
                profile.clone(),
                models,
                cli::UseCase::Chat,
                loaded.cache,
                warnings,
            );
            output::print_ollama(&profile, &installed, &result, args.format)?;
        }
        Commands::Explain => println!("{EXPLANATION}"),
    }
    Ok(())
}

async fn fetch_local_models(
    client: &reqwest::Client,
    api_reachable: bool,
    warnings: &mut Vec<String>,
) -> Vec<ModelCandidate> {
    if !api_reachable {
        return Vec::new();
    }
    match OllamaLocalSource::new(client.clone()).fetch_models().await {
        Ok(models) => models,
        Err(error) => {
            warnings.push(format!("Could not read Ollama's installed models: {error}"));
            Vec::new()
        }
    }
}
