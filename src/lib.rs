pub mod cache_stack;
pub mod chaos;
pub mod drivers;
pub mod error;
pub mod simd;
pub mod traits;

// Re-exports for convenience
pub use cache_stack::{CacheStack, CacheStackBuilder, CacheStats};
pub use error::{CacheError, CacheResult};
pub use traits::{CacheDriver, CacheEntry, CacheProvider, GetOrSetOptions};

/// Main RustoCache struct - the entry point for the library
#[derive(Clone)]
pub struct RustoCache<T> {
    stack: CacheStack<T>,
}

impl<T> RustoCache<T>
where
    T: Clone + Send + Sync + 'static,
{
    pub fn new(stack: CacheStack<T>) -> Self {
        Self { stack }
    }

    pub fn builder<S: Into<String>>(name: S) -> CacheStackBuilder<T> {
        CacheStackBuilder::new(name)
    }

    pub async fn get_stats(&self) -> CacheStats {
        self.stack.get_stats().await
    }

    /// Get multiple keys at once
    pub async fn get_many(&self, keys: &[&str]) -> CacheResult<Vec<Option<T>>> {
        // For now, use individual gets since CacheProvider doesn't have get_many
        // In the future, this could be optimized to use the underlying driver's get_many
        let mut results = Vec::with_capacity(keys.len());
        for key in keys {
            results.push(self.get(key).await?);
        }
        Ok(results)
    }

    /// Set multiple key-value pairs at once
    pub async fn set_many(
        &self,
        entries: &[(&str, T, Option<std::time::Duration>)],
    ) -> CacheResult<()> {
        // For now, use individual sets since CacheProvider doesn't have set_many
        // In the future, this could be optimized to use the underlying driver's set_many
        for (key, value, ttl) in entries {
            self.set(key, value.clone(), *ttl).await?;
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl<T> CacheProvider for RustoCache<T>
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
        self.stack.get_or_set(key, factory, options).await
    }

    async fn get(&self, key: &str) -> CacheResult<Option<Self::Value>> {
        self.stack.get(key).await
    }

    async fn set(
        &self,
        key: &str,
        value: Self::Value,
        ttl: Option<std::time::Duration>,
    ) -> CacheResult<()> {
        self.stack.set(key, value, ttl).await
    }

    async fn delete(&self, key: &str) -> CacheResult<bool> {
        self.stack.delete(key).await
    }

    async fn delete_by_tag(&self, tags: &[&str]) -> CacheResult<u64> {
        self.stack.delete_by_tag(tags).await
    }

    async fn clear(&self) -> CacheResult<()> {
        self.stack.clear().await
    }
}
