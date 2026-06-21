use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};

#[derive(Debug, Parser)]
#[command(
    name = "local-ai-advisor",
    version,
    about = "Find local AI models that fit your machine"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Inspect this machine without changing it.
    Scan(FormatArgs),
    /// Recommend models for this machine.
    Recommend(RecommendArgs),
    /// Inspect or refresh the model catalogue.
    Catalog {
        #[command(subcommand)]
        command: CatalogCommand,
    },
    /// Show local Ollama status and useful pull commands.
    Ollama(FormatArgs),
    /// Explain the recommendation rules.
    Explain,
}

#[derive(Debug, Args)]
pub struct FormatArgs {
    #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
    pub format: OutputFormat,
}

#[derive(Debug, Args)]
pub struct RecommendArgs {
    #[arg(long, value_enum, default_value_t = UseCase::Chat)]
    pub use_case: UseCase,

    /// Force a public metadata refresh before recommending.
    #[arg(long, conflicts_with = "offline")]
    pub online: bool,

    /// Never access public model sources. Local Ollama access is still allowed.
    #[arg(long, conflicts_with = "online")]
    pub offline: bool,

    #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
    pub format: OutputFormat,
}

#[derive(Debug, Subcommand)]
pub enum CatalogCommand {
    /// Force a refresh from public sources.
    Refresh(FormatArgs),
    /// Search the cached catalogue (plus the built-in fallback).
    Search {
        query: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        format: OutputFormat,
    },
}

#[derive(Debug, Clone, Copy, Default, ValueEnum, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum UseCase {
    Coding,
    #[default]
    Chat,
    Agent,
    Reasoning,
}

impl std::fmt::Display for UseCase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", format!("{self:?}").to_lowercase())
    }
}

#[derive(Debug, Clone, Copy, Default, ValueEnum, PartialEq, Eq)]
pub enum OutputFormat {
    #[default]
    Human,
    Json,
}
