use thiserror::Error;

#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Redis error: {0}")]
    Redis(#[from] redis::RedisError),

    #[error("Key not found: {key}")]
    KeyNotFound { key: String },

    #[error("Cache operation timeout")]
    Timeout,

    #[error("Driver not available")]
    DriverUnavailable,

    #[error("Invalid TTL value: {ttl}")]
    InvalidTtl { ttl: u64 },

    #[error("Cache is full")]
    CacheFull,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Generic error: {message}")]
    Generic { message: String },
}

pub type CacheResult<T> = Result<T, CacheError>;
