pub mod huggingface;
pub mod ollama;
pub mod static_fallback;

use anyhow::Result;
use async_trait::async_trait;

use crate::models::ModelCandidate;

#[async_trait]
pub trait ModelSource: Send + Sync {
    async fn fetch_models(&self) -> Result<Vec<ModelCandidate>>;
}
