use async_trait::async_trait;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, warn};

use crate::error::{CacheError, CacheResult};
use crate::traits::{CacheDriver, CacheProvider, GetOrSetOptions};

/// Multi-tier cache stack that combines L1 (memory) and L2 (distributed) caches
#[derive(Clone)]
pub struct CacheStack<T> {
    l1_driver: Option<Arc<dyn CacheDriver<Value = T>>>,
    l2_driver: Option<Arc<dyn CacheDriver<Value = T>>>,
    name: String,
    stats: Arc<RwLock<CacheStats>>,
    /// Tag index: tag -> set of keys that have this tag
    tag_index: Arc<RwLock<HashMap<String, HashSet<String>>>>,
}

#[derive(Debug, Default, Clone)]
pub struct CacheStats {
    pub l1_hits: u64,
    pub l1_misses: u64,
    pub l2_hits: u64,
    pub l2_misses: u64,
    pub sets: u64,
    pub deletes: u64,
    pub errors: u64,
}

impl CacheStats {
    pub fn hit_rate(&self) -> f64 {
        let total_hits = self.l1_hits + self.l2_hits;
        let total_requests = total_hits + self.l1_misses + self.l2_misses;

        if total_requests == 0 {
            0.0
        } else {
            total_hits as f64 / total_requests as f64
        }
    }

    pub fn l1_hit_rate(&self) -> f64 {
        let total_l1_requests = self.l1_hits + self.l1_misses;

        if total_l1_requests == 0 {
            0.0
        } else {
            self.l1_hits as f64 / total_l1_requests as f64
        }
    }
}

impl<T> CacheStack<T>
where
    T: Clone + Send + Sync + 'static,
{
    pub fn new(name: String) -> Self {
        Self {
            l1_driver: None,
            l2_driver: None,
            name,
            stats: Arc::new(RwLock::new(CacheStats::default())),
            tag_index: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn with_l1_driver(mut self, driver: Arc<dyn CacheDriver<Value = T>>) -> Self {
        self.l1_driver = Some(driver);
        self
    }

    pub fn with_l2_driver(mut self, driver: Arc<dyn CacheDriver<Value = T>>) -> Self {
        self.l2_driver = Some(driver);
        self
    }

    pub async fn get_stats(&self) -> CacheStats {
        self.stats.read().await.clone()
    }

    /// Add tags for a key to the tag index
    async fn add_tags_to_index(&self, key: &str, tags: &[String]) {
        if tags.is_empty() {
            return;
        }

        let mut tag_index = self.tag_index.write().await;
        for tag in tags {
            tag_index
                .entry(tag.clone())
                .or_insert_with(HashSet::new)
                .insert(key.to_string());
        }
    }

    /// Remove a key from all tags in the tag index
    async fn remove_key_from_tags(&self, key: &str) {
        let mut tag_index = self.tag_index.write().await;
        let mut empty_tags = Vec::new();

        for (tag, keys) in tag_index.iter_mut() {
            keys.remove(key);
            if keys.is_empty() {
                empty_tags.push(tag.clone());
            }
        }

        // Clean up empty tag entries
        for tag in empty_tags {
            tag_index.remove(&tag);
        }
    }

    /// Get all keys that have any of the specified tags
    async fn get_keys_by_tags(&self, tags: &[&str]) -> HashSet<String> {
        let tag_index = self.tag_index.read().await;
        let mut result = HashSet::new();

        for tag in tags {
            if let Some(keys) = tag_index.get(*tag) {
                result.extend(keys.clone());
            }
        }

        result
    }

    /// Get value from cache stack (L1 first, then L2)
    async fn get_from_stack(&self, key: &str) -> CacheResult<Option<T>> {
        // Try L1 first
        if let Some(l1) = &self.l1_driver {
            match l1.get(key).await {
                Ok(Some(value)) => {
                    debug!("L1 cache hit for key: {}", key);
                    self.stats.write().await.l1_hits += 1;
                    return Ok(Some(value));
                }
                Ok(None) => {
                    debug!("L1 cache miss for key: {}", key);
                    self.stats.write().await.l1_misses += 1;
                }
                Err(e) => {
                    warn!("L1 cache error for key {}: {:?}", key, e);
                    self.stats.write().await.errors += 1;
                }
            }
        }

        // Try L2 if L1 missed
        if let Some(l2) = &self.l2_driver {
            match l2.get(key).await {
                Ok(Some(value)) => {
                    debug!("L2 cache hit for key: {}", key);
                    self.stats.write().await.l2_hits += 1;

                    // Backfill L1 cache
                    if let Some(l1) = &self.l1_driver {
                        if let Err(e) = l1.set(key, value.clone(), None).await {
                            warn!("Failed to backfill L1 cache for key {}: {:?}", key, e);
                        }
                    }

                    return Ok(Some(value));
                }
                Ok(None) => {
                    debug!("L2 cache miss for key: {}", key);
                    self.stats.write().await.l2_misses += 1;
                }
                Err(e) => {
                    warn!("L2 cache error for key {}: {:?}", key, e);
                    self.stats.write().await.errors += 1;
                }
            }
        }

        Ok(None)
    }

    /// Set value in both L1 and L2 caches with tags
    async fn set_in_stack_with_tags(
        &self,
        key: &str,
        value: T,
        ttl: Option<Duration>,
        tags: &[String],
    ) -> CacheResult<()> {
        // Add tags to index first
        self.add_tags_to_index(key, tags).await;

        // Then set in caches
        self.set_in_stack(key, value, ttl).await
    }

    /// Set value in both L1 and L2 caches
    async fn set_in_stack(&self, key: &str, value: T, ttl: Option<Duration>) -> CacheResult<()> {
        let mut errors = Vec::new();

        // Set in L1
        if let Some(l1) = &self.l1_driver {
            if let Err(e) = l1.set(key, value.clone(), ttl).await {
                warn!("Failed to set L1 cache for key {}: {:?}", key, e);
                errors.push(e);
            }
        }

        // Set in L2
        if let Some(l2) = &self.l2_driver {
            if let Err(e) = l2.set(key, value, ttl).await {
                warn!("Failed to set L2 cache for key {}: {:?}", key, e);
                errors.push(e);
            }
        }

        self.stats.write().await.sets += 1;

        // If both failed, return error
        if !errors.is_empty()
            && ((self.l1_driver.is_some() && self.l2_driver.is_some() && errors.len() == 2)
                || (self.l1_driver.is_some() && self.l2_driver.is_none() && errors.len() == 1)
                || (self.l1_driver.is_none() && self.l2_driver.is_some() && errors.len() == 1))
        {
            return Err(errors.into_iter().next().unwrap());
        }

        Ok(())
    }

    /// Delete from both L1 and L2 caches
    async fn delete_from_stack(&self, key: &str) -> CacheResult<bool> {
        let mut deleted = false;

        // Delete from L1
        if let Some(l1) = &self.l1_driver {
            match l1.delete(key).await {
                Ok(was_deleted) => deleted = deleted || was_deleted,
                Err(e) => warn!("Failed to delete from L1 cache for key {}: {:?}", key, e),
            }
        }

        // Delete from L2
        if let Some(l2) = &self.l2_driver {
            match l2.delete(key).await {
                Ok(was_deleted) => deleted = deleted || was_deleted,
                Err(e) => warn!("Failed to delete from L2 cache for key {}: {:?}", key, e),
            }
        }

        // Remove from tag index if deleted
        if deleted {
            self.remove_key_from_tags(key).await;
        }

        self.stats.write().await.deletes += 1;
        Ok(deleted)
    }
}

#[async_trait]
impl<T> CacheProvider for CacheStack<T>
where
    T: Clone + Send + Sync + 'static,
{
    type Value = T;

    async fn get_or_set<F, Fut>(
        &self,
        key: &str,
        factory: F,
        options: GetOrSetOptions,
    ) -> CacheResult<Self::Value>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = CacheResult<Self::Value>> + Send,
    {
        // Try to get from cache first
        if let Some(value) = self.get_from_stack(key).await? {
            return Ok(value);
        }

        // Cache miss - use factory to generate value
        debug!("Cache miss for key: {}, calling factory", key);

        let value = if let Some(timeout) = options.timeout {
            // Use timeout for factory execution
            match tokio::time::timeout(timeout, factory()).await {
                Ok(result) => result?,
                Err(_) => return Err(CacheError::Timeout),
            }
        } else {
            factory().await?
        };

        // Store in cache with tags
        self.set_in_stack_with_tags(key, value.clone(), options.ttl, &options.tags)
            .await?;

        Ok(value)
    }

    async fn get(&self, key: &str) -> CacheResult<Option<Self::Value>> {
        self.get_from_stack(key).await
    }

    async fn set(&self, key: &str, value: Self::Value, ttl: Option<Duration>) -> CacheResult<()> {
        self.set_in_stack(key, value, ttl).await
    }

    async fn delete(&self, key: &str) -> CacheResult<bool> {
        self.delete_from_stack(key).await
    }

    async fn delete_by_tag(&self, tags: &[&str]) -> CacheResult<u64> {
        if tags.is_empty() {
            return Ok(0);
        }

        // Get all keys that have any of the specified tags
        let keys_to_delete = self.get_keys_by_tags(tags).await;

        if keys_to_delete.is_empty() {
            return Ok(0);
        }

        let mut deleted_count = 0;

        // Delete each key from both L1 and L2 caches
        for key in &keys_to_delete {
            // Delete from L1
            if let Some(l1) = &self.l1_driver {
                match l1.delete(key).await {
                    Ok(was_deleted) => {
                        if was_deleted {
                            deleted_count += 1;
                        }
                    }
                    Err(e) => warn!("Failed to delete key {} from L1 cache: {:?}", key, e),
                }
            }

            // Delete from L2
            if let Some(l2) = &self.l2_driver {
                match l2.delete(key).await {
                    Ok(was_deleted) => {
                        if was_deleted && self.l1_driver.is_none() {
                            // Only count L2 deletions if there's no L1 cache
                            deleted_count += 1;
                        }
                    }
                    Err(e) => warn!("Failed to delete key {} from L2 cache: {:?}", key, e),
                }
            }

            // Remove key from tag index
            self.remove_key_from_tags(key).await;
        }

        // Update stats
        self.stats.write().await.deletes += deleted_count;

        debug!("Deleted {} keys by tags: {:?}", deleted_count, tags);
        Ok(deleted_count)
    }

    async fn clear(&self) -> CacheResult<()> {
        let mut errors = Vec::new();

        // Clear L1
        if let Some(l1) = &self.l1_driver {
            if let Err(e) = l1.clear().await {
                warn!("Failed to clear L1 cache: {:?}", e);
                errors.push(e);
            }
        }

        // Clear L2
        if let Some(l2) = &self.l2_driver {
            if let Err(e) = l2.clear().await {
                warn!("Failed to clear L2 cache: {:?}", e);
                errors.push(e);
            }
        }

        // Reset stats and clear tag index
        *self.stats.write().await = CacheStats::default();
        self.tag_index.write().await.clear();

        if !errors.is_empty() {
            return Err(errors.into_iter().next().unwrap());
        }

        Ok(())
    }
}

/// Builder for cache stack
pub struct CacheStackBuilder<T> {
    name: String,
    l1_driver: Option<Arc<dyn CacheDriver<Value = T>>>,
    l2_driver: Option<Arc<dyn CacheDriver<Value = T>>>,
}

impl<T> CacheStackBuilder<T>
where
    T: Clone + Send + Sync + 'static,
{
    pub fn new<S: Into<String>>(name: S) -> Self {
        Self {
            name: name.into(),
            l1_driver: None,
            l2_driver: None,
        }
    }

    pub fn with_l1_driver(mut self, driver: Arc<dyn CacheDriver<Value = T>>) -> Self {
        self.l1_driver = Some(driver);
        self
    }

    pub fn with_l2_driver(mut self, driver: Arc<dyn CacheDriver<Value = T>>) -> Self {
        self.l2_driver = Some(driver);
        self
    }

    pub fn build(self) -> CacheStack<T> {
        let mut stack = CacheStack::new(self.name);

        if let Some(l1) = self.l1_driver {
            stack = stack.with_l1_driver(l1);
        }

        if let Some(l2) = self.l2_driver {
            stack = stack.with_l2_driver(l2);
        }

        stack
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drivers::memory::MemoryDriverBuilder;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_cache_stack_l1_only() {
        let l1_driver: Arc<dyn CacheDriver<Value = String>> =
            Arc::new(MemoryDriverBuilder::new().build());

        let stack = CacheStackBuilder::new("test")
            .with_l1_driver(l1_driver)
            .build();

        // Test get_or_set
        let value = stack
            .get_or_set(
                "key1",
                || async { Ok("value1".to_string()) },
                GetOrSetOptions::default(),
            )
            .await
            .unwrap();

        assert_eq!(value, "value1");

        // Should hit cache on second call
        let value2 = stack
            .get_or_set(
                "key1",
                || async { Ok("different_value".to_string()) },
                GetOrSetOptions::default(),
            )
            .await
            .unwrap();

        assert_eq!(value2, "value1"); // Should return cached value

        // Check stats
        let stats = stack.get_stats().await;
        assert_eq!(stats.l1_hits, 1);
        assert_eq!(stats.l1_misses, 1);
    }

    #[tokio::test]
    async fn test_tag_based_deletion() {
        let l1_driver: Arc<dyn CacheDriver<Value = String>> =
            Arc::new(MemoryDriverBuilder::new().build());

        let stack = CacheStackBuilder::new("test")
            .with_l1_driver(l1_driver)
            .build();

        // Set values with tags
        let options1 = GetOrSetOptions {
            ttl: None,
            tags: vec!["user".to_string(), "profile".to_string()],
            grace_period: None,
            timeout: Some(Duration::from_secs(30)),
        };

        let options2 = GetOrSetOptions {
            ttl: None,
            tags: vec!["user".to_string(), "settings".to_string()],
            grace_period: None,
            timeout: Some(Duration::from_secs(30)),
        };

        let options3 = GetOrSetOptions {
            ttl: None,
            tags: vec!["product".to_string()],
            grace_period: None,
            timeout: Some(Duration::from_secs(30)),
        };

        // Set values using get_or_set with tags
        stack
            .get_or_set(
                "user:1",
                || async { Ok("user1_data".to_string()) },
                options1,
            )
            .await
            .unwrap();
        stack
            .get_or_set(
                "user:2",
                || async { Ok("user2_data".to_string()) },
                options2,
            )
            .await
            .unwrap();
        stack
            .get_or_set(
                "product:1",
                || async { Ok("product1_data".to_string()) },
                options3,
            )
            .await
            .unwrap();

        // Verify all values exist
        assert!(stack.get("user:1").await.unwrap().is_some());
        assert!(stack.get("user:2").await.unwrap().is_some());
        assert!(stack.get("product:1").await.unwrap().is_some());

        // Delete by "user" tag - should delete user:1 and user:2
        let deleted_count = stack.delete_by_tag(&["user"]).await.unwrap();
        assert_eq!(deleted_count, 2);

        // Verify user entries are deleted but product remains
        assert!(stack.get("user:1").await.unwrap().is_none());
        assert!(stack.get("user:2").await.unwrap().is_none());
        assert!(stack.get("product:1").await.unwrap().is_some());

        // Delete by "product" tag
        let deleted_count = stack.delete_by_tag(&["product"]).await.unwrap();
        assert_eq!(deleted_count, 1);

        // Verify product is also deleted
        assert!(stack.get("product:1").await.unwrap().is_none());

        // Try deleting by non-existent tag
        let deleted_count = stack.delete_by_tag(&["nonexistent"]).await.unwrap();
        assert_eq!(deleted_count, 0);
    }
}
