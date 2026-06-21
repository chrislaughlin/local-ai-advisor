use std::{fs, path::PathBuf, time::Duration};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    error::AdvisorError,
    models::{merge_models, ModelCandidate},
    sources::{static_fallback::fallback_models, ModelSource},
};

pub const DEFAULT_CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedCatalogue {
    pub schema_version: u32,
    pub fetched_at: DateTime<Utc>,
    pub models: Vec<ModelCandidate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheMetadata {
    pub path: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fetched_at: Option<DateTime<Utc>>,
    pub stale: bool,
}

#[derive(Debug, Clone)]
pub struct CacheStore {
    path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogueMode {
    Offline,
    PreferCache,
    ForceRefresh,
}

#[derive(Debug, Clone)]
pub struct LoadedCatalogue {
    pub models: Vec<ModelCandidate>,
    pub cache: CacheMetadata,
    pub warnings: Vec<String>,
}

impl CacheStore {
    pub fn discover() -> Result<Self> {
        let path = dirs::cache_dir()
            .ok_or(AdvisorError::CacheDirectoryUnavailable)?
            .join("local-ai-advisor")
            .join("catalog.json");
        Ok(Self { path })
    }

    #[cfg(test)]
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn load(&self) -> Result<Option<CachedCatalogue>> {
        if !self.path.exists() {
            return Ok(None);
        }
        let contents = fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read cache {}", self.path.display()))?;
        let cache = serde_json::from_str(&contents)
            .with_context(|| format!("invalid cache {}", self.path.display()))?;
        Ok(Some(cache))
    }

    pub fn save(&self, models: Vec<ModelCandidate>) -> Result<CachedCatalogue> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create cache directory {}", parent.display())
            })?;
        }
        let catalogue = CachedCatalogue {
            schema_version: 1,
            fetched_at: Utc::now(),
            models,
        };
        let temp_path = self.path.with_extension("json.tmp");
        let json =
            serde_json::to_vec_pretty(&catalogue).context("failed to serialize catalogue cache")?;
        fs::write(&temp_path, json)
            .with_context(|| format!("failed to write temporary cache {}", temp_path.display()))?;
        if cfg!(windows) && self.path.exists() {
            fs::remove_file(&self.path).with_context(|| {
                format!("failed to replace existing cache {}", self.path.display())
            })?;
        }
        fs::rename(&temp_path, &self.path)
            .with_context(|| format!("failed to replace cache {}", self.path.display()))?;
        Ok(catalogue)
    }

    pub fn is_stale(cache: &CachedCatalogue) -> bool {
        let age = Utc::now().signed_duration_since(cache.fetched_at);
        age.to_std()
            .map(|value| value > DEFAULT_CACHE_TTL)
            .unwrap_or(true)
    }

    pub fn metadata(
        &self,
        cache: Option<&CachedCatalogue>,
        status: impl Into<String>,
    ) -> CacheMetadata {
        CacheMetadata {
            path: self.path.display().to_string(),
            status: status.into(),
            fetched_at: cache.map(|value| value.fetched_at),
            stale: cache.map(Self::is_stale).unwrap_or(false),
        }
    }
}

pub async fn load_catalogue(
    store: &CacheStore,
    remote: &dyn ModelSource,
    mode: CatalogueMode,
) -> Result<LoadedCatalogue> {
    let mut warnings = Vec::new();
    let cached = match store.load() {
        Ok(value) => value,
        Err(error) => {
            warnings.push(format!("Ignoring unreadable model cache: {error}"));
            None
        }
    };

    if mode == CatalogueMode::Offline {
        let status = if cached.is_some() {
            "offline-cache"
        } else {
            "static-fallback"
        };
        let models = merge_models([
            cached
                .as_ref()
                .map(|value| value.models.clone())
                .unwrap_or_default(),
            fallback_models(),
        ]);
        return Ok(LoadedCatalogue {
            models,
            cache: store.metadata(cached.as_ref(), status),
            warnings,
        });
    }

    if mode == CatalogueMode::PreferCache {
        if let Some(cache) = cached.as_ref().filter(|cache| !CacheStore::is_stale(cache)) {
            return Ok(LoadedCatalogue {
                models: merge_models([cache.models.clone(), fallback_models()]),
                cache: store.metadata(Some(cache), "fresh-cache"),
                warnings,
            });
        }
    }

    match remote.fetch_models().await {
        Ok(models) if !models.is_empty() => {
            let saved = match store.save(models.clone()) {
                Ok(cache) => Some(cache),
                Err(error) => {
                    warnings.push(format!("Fresh metadata could not be cached: {error}"));
                    None
                }
            };
            Ok(LoadedCatalogue {
                models: merge_models([models, fallback_models()]),
                cache: store.metadata(saved.as_ref(), "refreshed"),
                warnings,
            })
        }
        Ok(_) => fallback_after_refresh_failure(
            store,
            cached,
            warnings,
            "Public model source returned no usable GGUF models.",
        ),
        Err(error) => fallback_after_refresh_failure(
            store,
            cached,
            warnings,
            &format!("Public metadata refresh failed: {error}"),
        ),
    }
}

fn fallback_after_refresh_failure(
    store: &CacheStore,
    cached: Option<CachedCatalogue>,
    mut warnings: Vec<String>,
    warning: &str,
) -> Result<LoadedCatalogue> {
    warnings.push(warning.to_string());
    let status = if cached.is_some() {
        "stale-cache-fallback"
    } else {
        "static-fallback"
    };
    Ok(LoadedCatalogue {
        models: merge_models([
            cached
                .as_ref()
                .map(|value| value.models.clone())
                .unwrap_or_default(),
            fallback_models(),
        ]),
        cache: store.metadata(cached.as_ref(), status),
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::static_fallback::fallback_models;

    #[test]
    fn round_trips_cached_catalogue() {
        let directory = tempfile::tempdir().unwrap();
        let store = CacheStore::at(directory.path().join("catalog.json"));
        let saved = store.save(fallback_models()).unwrap();
        let loaded = store.load().unwrap().unwrap();
        assert_eq!(saved.models.len(), loaded.models.len());
        assert_eq!(loaded.schema_version, 1);
    }

    struct FailingSource;

    #[async_trait::async_trait]
    impl ModelSource for FailingSource {
        async fn fetch_models(&self) -> Result<Vec<ModelCandidate>> {
            anyhow::bail!("simulated outage")
        }
    }

    #[tokio::test]
    async fn failed_network_uses_static_fallback() {
        let directory = tempfile::tempdir().unwrap();
        let store = CacheStore::at(directory.path().join("catalog.json"));
        let loaded = load_catalogue(&store, &FailingSource, CatalogueMode::ForceRefresh)
            .await
            .unwrap();
        assert!(!loaded.models.is_empty());
        assert_eq!(loaded.cache.status, "static-fallback");
        assert!(loaded.warnings[0].contains("simulated outage"));
    }
}
