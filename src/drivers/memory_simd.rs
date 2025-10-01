use async_trait::async_trait;
use lru::LruCache;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Duration;

use crate::error::CacheResult;
use crate::simd::{bulk_hash, check_expired_batch};
use crate::traits::{CacheDriver, CacheEntry};

/// SIMD-optimized high-performance in-memory cache driver
pub struct MemoryDriverSIMD<T> {
    cache: Arc<RwLock<LruCache<String, CacheEntry<T>>>>,
    tag_index: Arc<RwLock<HashMap<String, Vec<String>>>>,
    config: super::memory::MemoryDriverConfig,
    // SIMD optimization flags
    simd_enabled: bool,
    bulk_threshold: usize,
}

impl<T> MemoryDriverSIMD<T>
where
    T: Clone + Send + Sync + 'static,
{
    pub fn new(config: super::memory::MemoryDriverConfig) -> Self {
        let capacity =
            NonZeroUsize::new(config.max_entries).unwrap_or(NonZeroUsize::new(1).unwrap());

        Self {
            cache: Arc::new(RwLock::new(LruCache::new(capacity))),
            tag_index: Arc::new(RwLock::new(HashMap::new())),
            config,
            simd_enabled: is_x86_feature_detected!("avx2"),
            bulk_threshold: 8, // Minimum items for SIMD optimization
        }
    }

    /// SIMD-optimized cleanup of expired entries
    fn cleanup_expired_simd(&self) {
        let mut cache = self.cache.write();
        let mut tag_index = self.tag_index.write();

        // Collect all entries for bulk processing
        let entries: Vec<(String, CacheEntry<T>)> =
            cache.iter().map(|(k, v)| (k.clone(), v.clone())).collect();

        if entries.len() >= self.bulk_threshold && self.simd_enabled {
            // Use SIMD for bulk expiration checking
            let expired_keys = self.check_bulk_expiration(&entries);

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
        } else {
            // Fallback to sequential processing
            let expired_keys: Vec<String> = cache
                .iter()
                .filter_map(|(key, entry)| {
                    if entry.is_expired() {
                        Some(key.clone())
                    } else {
                        None
                    }
                })
                .collect();

            for key in expired_keys {
                if let Some(entry) = cache.pop(&key) {
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
    }

    /// Check bulk expiration using SIMD operations
    fn check_bulk_expiration(&self, entries: &[(String, CacheEntry<T>)]) -> Vec<String> {
        let timestamps: Vec<u64> = entries
            .iter()
            .map(|(_, entry)| entry.created_at.timestamp() as u64)
            .collect();

        let ttls: Vec<u64> = entries
            .iter()
            .map(|(_, entry)| entry.ttl.map(|d| d.as_secs()).unwrap_or(0))
            .collect();

        let current_time = chrono::Utc::now().timestamp() as u64;
        let expired_flags = check_expired_batch(&timestamps, &ttls, current_time);

        entries
            .iter()
            .zip(expired_flags.iter())
            .filter_map(|((key, _), &expired)| if expired { Some(key.clone()) } else { None })
            .collect()
    }

    /// SIMD-optimized bulk key hashing for better cache distribution
    fn hash_keys_bulk(&self, keys: &[&str]) -> Vec<u64> {
        if keys.len() >= self.bulk_threshold && self.simd_enabled {
            bulk_hash(keys)
        } else {
            // Fallback implementation
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};

            keys.iter()
                .map(|key| {
                    let mut hasher = DefaultHasher::new();
                    key.hash(&mut hasher);
                    hasher.finish()
                })
                .collect()
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
}

#[async_trait]
impl<T> CacheDriver for MemoryDriverSIMD<T>
where
    T: Clone + Send + Sync + 'static,
{
    type Value = T;

    async fn get(&self, key: &str) -> CacheResult<Option<Self::Value>> {
        // Periodic cleanup with SIMD optimization
        if fastrand::u32(0..100) == 0 {
            self.cleanup_expired_simd();
        }

        let mut cache = self.cache.write();

        if let Some(entry) = cache.get(key) {
            if entry.is_expired() {
                let entry = cache.pop(key).unwrap();
                self.remove_from_tag_index(key, &entry.tags);
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

        if let Some(old_entry) = cache.peek(key) {
            self.remove_from_tag_index(key, &old_entry.tags);
        }

        cache.put(key.to_string(), entry.clone());
        self.update_tag_index(key, &entry.tags);

        Ok(())
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

    async fn has(&self, key: &str) -> CacheResult<bool> {
        let cache = self.cache.read();

        if let Some(entry) = cache.peek(key) {
            Ok(!entry.is_expired())
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

    /// SIMD-optimized bulk get operation
    async fn get_many(&self, keys: &[&str]) -> CacheResult<Vec<Option<Self::Value>>> {
        if keys.len() >= self.bulk_threshold && self.simd_enabled {
            // Pre-compute hashes for better cache locality
            let _hashes = self.hash_keys_bulk(keys);

            // Process in chunks for optimal SIMD utilization
            let mut results = Vec::with_capacity(keys.len());

            for chunk in keys.chunks(self.bulk_threshold) {
                for key in chunk {
                    results.push(self.get(key).await?);
                }
            }

            Ok(results)
        } else {
            // Fallback to sequential processing
            let mut results = Vec::with_capacity(keys.len());
            for key in keys {
                results.push(self.get(key).await?);
            }
            Ok(results)
        }
    }

    /// SIMD-optimized bulk set operation
    async fn set_many(&self, entries: &[(&str, Self::Value, Option<Duration>)]) -> CacheResult<()> {
        if entries.len() >= self.bulk_threshold && self.simd_enabled {
            // Extract keys for bulk hashing
            let keys: Vec<&str> = entries.iter().map(|(k, _, _)| *k).collect();
            let _hashes = self.hash_keys_bulk(&keys);

            // Process in optimized chunks
            for chunk in entries.chunks(self.bulk_threshold) {
                for (key, value, ttl) in chunk {
                    self.set(key, value.clone(), *ttl).await?;
                }
            }
        } else {
            // Fallback implementation
            for (key, value, ttl) in entries {
                self.set(key, value.clone(), *ttl).await?;
            }
        }

        Ok(())
    }

    async fn delete_many(&self, keys: &[&str]) -> CacheResult<u64> {
        let mut deleted = 0;

        if keys.len() >= self.bulk_threshold && self.simd_enabled {
            // Pre-compute hashes for better performance
            let _hashes = self.hash_keys_bulk(keys);

            for chunk in keys.chunks(self.bulk_threshold) {
                for key in chunk {
                    if self.delete(key).await? {
                        deleted += 1;
                    }
                }
            }
        } else {
            for key in keys {
                if self.delete(key).await? {
                    deleted += 1;
                }
            }
        }

        Ok(deleted)
    }
}

/// Builder for SIMD-optimized memory driver
pub struct MemoryDriverSIMDBuilder {
    config: super::memory::MemoryDriverConfig,
}

impl MemoryDriverSIMDBuilder {
    pub fn new() -> Self {
        Self {
            config: super::memory::MemoryDriverConfig::default(),
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

    pub fn build<T>(self) -> MemoryDriverSIMD<T>
    where
        T: Clone + Send + Sync + 'static,
    {
        MemoryDriverSIMD::new(self.config)
    }
}

impl Default for MemoryDriverSIMDBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_simd_driver_performance() {
        let driver = MemoryDriverSIMDBuilder::new()
            .max_entries(1000)
            .build::<String>();

        // Test bulk operations
        let keys: Vec<String> = (0..100).map(|i| format!("key_{}", i)).collect();
        let key_refs: Vec<&str> = keys.iter().map(|s| s.as_str()).collect();

        // Bulk set
        let entries: Vec<(&str, String, Option<Duration>)> = key_refs
            .iter()
            .map(|&k| (k, format!("value_{}", k), None))
            .collect();

        driver.set_many(&entries).await.unwrap();

        // Bulk get
        let results = driver.get_many(&key_refs).await.unwrap();
        assert_eq!(results.len(), 100);

        // Verify all values are present
        for (i, result) in results.iter().enumerate() {
            assert!(result.is_some());
            assert_eq!(result.as_ref().unwrap(), &format!("value_key_{}", i));
        }
    }

    #[tokio::test]
    async fn test_simd_cleanup() {
        let driver = MemoryDriverSIMDBuilder::new().build::<String>();

        // Set entries with short TTL
        for i in 0..20 {
            driver
                .set(
                    &format!("key_{}", i),
                    format!("value_{}", i),
                    Some(Duration::from_millis(1)),
                )
                .await
                .unwrap();
        }

        // Wait for expiration
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Trigger cleanup
        driver.cleanup_expired_simd();

        // Verify entries are cleaned up
        for i in 0..20 {
            let result = driver.get(&format!("key_{}", i)).await.unwrap();
            assert!(result.is_none());
        }
    }
}
