use crate::error::CacheResult;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Represents a cache entry with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry<T> {
    pub value: T,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub ttl: Option<Duration>,
    pub tags: Vec<String>,
}

impl<T> CacheEntry<T> {
    pub fn new(value: T, ttl: Option<Duration>) -> Self {
        Self {
            value,
            created_at: chrono::Utc::now(),
            ttl,
            tags: Vec::new(),
        }
    }

    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    pub fn is_expired(&self) -> bool {
        if let Some(ttl) = self.ttl {
            let elapsed = chrono::Utc::now().signed_duration_since(self.created_at);
            elapsed.to_std().unwrap_or(Duration::ZERO) > ttl
        } else {
            false
        }
    }

    /// Check if entry is within grace period (expired but still usable)
    pub fn is_within_grace_period(&self, grace_period: Duration) -> bool {
        if let Some(ttl) = self.ttl {
            let elapsed = chrono::Utc::now().signed_duration_since(self.created_at);
            let elapsed_std = elapsed.to_std().unwrap_or(Duration::ZERO);

            // Entry is expired but within grace period
            elapsed_std > ttl && elapsed_std <= (ttl + grace_period)
        } else {
            false
        }
    }

    /// Get time remaining until grace period expires
    pub fn grace_period_remaining(&self, grace_period: Duration) -> Option<Duration> {
        if let Some(ttl) = self.ttl {
            let elapsed = chrono::Utc::now().signed_duration_since(self.created_at);
            let elapsed_std = elapsed.to_std().unwrap_or(Duration::ZERO);
            let grace_expiry = ttl + grace_period;

            if elapsed_std < grace_expiry {
                Some(grace_expiry - elapsed_std)
            } else {
                None
            }
        } else {
            None
        }
    }
}

/// Core cache driver trait that all implementations must follow
#[async_trait]
pub trait CacheDriver: Send + Sync {
    type Value: Send + Sync;

    /// Get a value from the cache
    async fn get(&self, key: &str) -> CacheResult<Option<Self::Value>>;

    /// Set a value in the cache with optional TTL
    async fn set(&self, key: &str, value: Self::Value, ttl: Option<Duration>) -> CacheResult<()>;

    /// Delete a key from the cache
    async fn delete(&self, key: &str) -> CacheResult<bool>;

    /// Check if a key exists
    async fn has(&self, key: &str) -> CacheResult<bool>;

    /// Clear all entries
    async fn clear(&self) -> CacheResult<()>;

    /// Get multiple keys at once
    async fn get_many(&self, keys: &[&str]) -> CacheResult<Vec<Option<Self::Value>>>;

    /// Set multiple key-value pairs at once
    async fn set_many(&self, entries: &[(&str, Self::Value, Option<Duration>)]) -> CacheResult<()>;

    /// Delete multiple keys at once
    async fn delete_many(&self, keys: &[&str]) -> CacheResult<u64>;

    /// Get value with grace period support (returns value even if expired but within grace period)
    async fn get_with_grace_period(
        &self,
        key: &str,
        grace_period: Duration,
    ) -> CacheResult<Option<Self::Value>> {
        // Default implementation falls back to regular get
        self.get(key).await
    }
}

/// Factory function type for creating cache entries
pub type CacheFactory<T> = Box<dyn Fn() -> T + Send + Sync>;

/// Options for get_or_set operations
#[derive(Debug, Clone, Default)]
pub struct GetOrSetOptions {
    /// Time to live for the cache entry
    pub ttl: Option<Duration>,
    /// Tags for grouping cache entries
    pub tags: Vec<String>,
    /// Grace period for serving stale data when factory fails
    pub grace_period: Option<Duration>,
    /// Timeout for the factory function
    pub timeout: Option<Duration>,
    /// Background refresh threshold (refresh when TTL < threshold)
    pub refresh_threshold: Option<Duration>,
    /// Enable stampede protection (only one factory per key)
    pub stampede_protection: bool,
}

// Remove the manual Default implementation since we have #[derive(Default)]

/// High-level cache provider interface
#[async_trait]
pub trait CacheProvider: Send + Sync {
    type Value: Send + Sync;

    /// Get or set a value using a factory function
    async fn get_or_set<F, Fut>(
        &self,
        key: &str,
        factory: F,
        options: GetOrSetOptions,
    ) -> CacheResult<Self::Value>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = CacheResult<Self::Value>> + Send;

    /// Get a value from the cache
    async fn get(&self, key: &str) -> CacheResult<Option<Self::Value>>;

    /// Set a value in the cache
    async fn set(&self, key: &str, value: Self::Value, ttl: Option<Duration>) -> CacheResult<()>;

    /// Delete a key from the cache
    async fn delete(&self, key: &str) -> CacheResult<bool>;

    /// Delete by tags
    async fn delete_by_tag(&self, tags: &[&str]) -> CacheResult<u64>;

    /// Clear the entire cache
    async fn clear(&self) -> CacheResult<()>;
}
