use async_trait::async_trait;
use lru::LruCache;
use parking_lot::RwLock;
// Serde traits are used in generic bounds
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Duration;

use crate::error::CacheResult;
use crate::traits::{CacheDriver, CacheEntry};

/// Configuration for the memory driver
#[derive(Debug, Clone)]
pub struct MemoryDriverConfig {
    /// Maximum number of entries to store
    pub max_entries: usize,
    /// Whether to serialize values (false for zero-copy performance)
    pub serialize: bool,
    /// Default TTL for entries
    pub default_ttl: Option<Duration>,
}

impl Default for MemoryDriverConfig {
    fn default() -> Self {
        Self {
            max_entries: 10_000,
            serialize: false,
            default_ttl: None,
        }
    }
}

/// High-performance in-memory cache driver using LRU eviction
pub struct MemoryDriver<T> {
    cache: Arc<RwLock<LruCache<String, CacheEntry<T>>>>,
    tag_index: Arc<RwLock<HashMap<String, Vec<String>>>>,
    config: MemoryDriverConfig,
}

impl<T> MemoryDriver<T>
where
    T: Clone + Send + Sync + 'static,
{
    pub fn new(config: MemoryDriverConfig) -> Self {
        let capacity =
            NonZeroUsize::new(config.max_entries).unwrap_or(NonZeroUsize::new(1).unwrap());

        Self {
            cache: Arc::new(RwLock::new(LruCache::new(capacity))),
            tag_index: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// Clean up expired entries (but preserve those that might be within grace period)
    fn cleanup_expired(&self) {
        let mut cache = self.cache.write();
        let mut tag_index = self.tag_index.write();

        let max_grace_period = Duration::from_secs(3600); // 1 hour max grace period
        let expired_keys: Vec<String> = cache
            .iter()
            .filter_map(|(key, entry)| {
                if entry.is_expired() && !entry.is_within_grace_period(max_grace_period) {
                    Some(key.clone())
                } else {
                    None
                }
            })
            .collect();

        for key in expired_keys {
            if let Some(entry) = cache.pop(&key) {
                // Remove from tag index
                for tag in &entry.tags {
                    if let Some(keys) = tag_index.get_mut(tag) {
                        keys.retain(|k| k != &key);
                        if keys.is_empty() {
                            tag_index.remove(tag);
                        }
                    }
                }
            }
        }
    }

    /// Update tag index for a key
    fn update_tag_index(&self, key: &str, tags: &[String]) {
        let mut tag_index = self.tag_index.write();

        for tag in tags {
            tag_index
                .entry(tag.clone())
                .or_default()
                .push(key.to_string());
        }
    }

    /// Remove key from tag index
    fn remove_from_tag_index(&self, key: &str, tags: &[String]) {
        let mut tag_index = self.tag_index.write();

        for tag in tags {
            if let Some(keys) = tag_index.get_mut(tag) {
                keys.retain(|k| k != key);
                if keys.is_empty() {
                    tag_index.remove(tag);
                }
            }
        }
    }

    /// Get keys by tags
    pub fn get_keys_by_tags(&self, tags: &[&str]) -> Vec<String> {
        let tag_index = self.tag_index.read();
        let mut result = Vec::new();

        for tag in tags {
            if let Some(keys) = tag_index.get(*tag) {
                result.extend(keys.clone());
            }
        }

        result.sort();
        result.dedup();
        result
    }
}

#[async_trait]
impl<T> CacheDriver for MemoryDriver<T>
where
    T: Clone + Send + Sync + 'static,
{
    type Value = T;

    async fn get(&self, key: &str) -> CacheResult<Option<Self::Value>> {
        // Periodic cleanup (every 100th access)
        if fastrand::u32(0..100) == 0 {
            self.cleanup_expired();
        }

        let mut cache = self.cache.write();

        if let Some(entry) = cache.get(key) {
            if entry.is_expired() {
                // Check if we might need this for grace period (assume max 1 hour grace period)
                let max_grace_period = Duration::from_secs(3600);
                if !entry.is_within_grace_period(max_grace_period) {
                    // Only remove if it's beyond any reasonable grace period
                    let entry = cache.pop(key).unwrap();
                    self.remove_from_tag_index(key, &entry.tags);
                }
                Ok(None)
            } else {
                Ok(Some(entry.value.clone()))
            }
        } else {
            Ok(None)
        }
    }

    async fn set(&self, key: &str, value: Self::Value, ttl: Option<Duration>) -> CacheResult<()> {
        let ttl = ttl.or(self.config.default_ttl);
        let entry = CacheEntry::new(value, ttl);

        let mut cache = self.cache.write();

        // If key already exists, remove from tag index first
        if let Some(old_entry) = cache.peek(key) {
            self.remove_from_tag_index(key, &old_entry.tags);
        }

        cache.put(key.to_string(), entry);
        Ok(())
    }

    async fn has(&self, key: &str) -> CacheResult<bool> {
        let cache = self.cache.read();

        if let Some(entry) = cache.peek(key) {
            Ok(!entry.is_expired())
        } else {
            Ok(false)
        }
    }

    async fn delete(&self, key: &str) -> CacheResult<bool> {
        let mut cache = self.cache.write();
        
        if let Some(entry) = cache.pop(key) {
            self.remove_from_tag_index(key, &entry.tags);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn clear(&self) -> CacheResult<()> {
        let mut cache = self.cache.write();
        let mut tag_index = self.tag_index.write();

        cache.clear();
        tag_index.clear();

        Ok(())
    }

    async fn get_many(&self, keys: &[&str]) -> CacheResult<Vec<Option<Self::Value>>> {
        let mut results = Vec::with_capacity(keys.len());

        for key in keys {
            results.push(self.get(key).await?);
        }

        Ok(results)
    }

    async fn set_many(&self, entries: &[(&str, Self::Value, Option<Duration>)]) -> CacheResult<()> {
        for (key, value, ttl) in entries {
            self.set(key, value.clone(), *ttl).await?;
        }

        Ok(())
    }

    async fn delete_many(&self, keys: &[&str]) -> CacheResult<u64> {
        let mut deleted = 0;

        for key in keys {
            if self.delete(key).await? {
                deleted += 1;
            }
        }

        Ok(deleted)
    }

    async fn get_with_grace_period(&self, key: &str, grace_period: Duration) -> CacheResult<Option<Self::Value>> {
        // Periodic cleanup (every 100th access)
        if fastrand::u32(0..100) == 0 {
            self.cleanup_expired();
        }

        let mut cache = self.cache.write();

        if let Some(entry) = cache.get(key) {
            if entry.is_expired() {
                // Check if within grace period
                if entry.is_within_grace_period(grace_period) {
                    // Return stale data but don't remove from cache
                    return Ok(Some(entry.value.clone()));
                } else {
                    // Remove expired entry beyond grace period
                    let entry = cache.pop(key).unwrap();
                    self.remove_from_tag_index(key, &entry.tags);
                    Ok(None)
                }
            } else {
                // Return fresh data
                Ok(Some(entry.value.clone()))
            }
        } else {
            Ok(None)
        }
    }
}

/// Builder for memory driver
pub struct MemoryDriverBuilder {
    config: MemoryDriverConfig,
}

impl MemoryDriverBuilder {
    pub fn new() -> Self {
        Self {
            config: MemoryDriverConfig::default(),
        }
    }

    pub fn max_entries(mut self, max_entries: usize) -> Self {
        self.config.max_entries = max_entries;
        self
    }

    pub fn serialize(mut self, serialize: bool) -> Self {
        self.config.serialize = serialize;
        self
    }

    pub fn default_ttl(mut self, ttl: Duration) -> Self {
        self.config.default_ttl = Some(ttl);
        self
    }

    pub fn build<T>(self) -> MemoryDriver<T>
    where
        T: Clone + Send + Sync + 'static,
    {
        MemoryDriver::new(self.config)
    }
}

impl Default for MemoryDriverBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    // tokio_test not needed for these tests

    #[tokio::test]
    async fn test_memory_driver_basic_operations() {
        let driver = MemoryDriverBuilder::new()
            .max_entries(100)
            .build::<String>();

        // Test set and get
        driver
            .set("key1", "value1".to_string(), None)
            .await
            .unwrap();
        let result = driver.get("key1").await.unwrap();
        assert_eq!(result, Some("value1".to_string()));

        // Test has
        assert!(driver.has("key1").await.unwrap());
        assert!(!driver.has("nonexistent").await.unwrap());

        // Test delete
        assert!(driver.delete("key1").await.unwrap());
        assert!(!driver.has("key1").await.unwrap());
    }

    #[tokio::test]
    async fn test_memory_driver_ttl() {
        let driver = MemoryDriverBuilder::new().build::<String>();

        // Set with short TTL
        driver
            .set(
                "key1",
                "value1".to_string(),
                Some(Duration::from_millis(10)),
            )
            .await
            .unwrap();

        // Should exist immediately
        assert!(driver.has("key1").await.unwrap());

        // Wait for expiration
        tokio::time::sleep(Duration::from_millis(20)).await;

        // Should be expired
        assert!(!driver.has("key1").await.unwrap());
        let result = driver.get("key1").await.unwrap();
        assert_eq!(result, None);
    }
}
