use thiserror::Error;

#[derive(Debug, Error)]
pub enum AdvisorError {
    #[error("no user cache directory is available on this system")]
    CacheDirectoryUnavailable,

    #[error("public model source returned no usable GGUF models")]
    EmptyPublicCatalogue,
}
