use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;

use super::chaos_injector::{ChaosConfig, ChaosInjector};
use crate::error::CacheResult;
use crate::traits::CacheDriver;

/// A wrapper around any cache driver that injects chaos
pub struct ChaosDriver<T> {
    inner: Arc<dyn CacheDriver<Value = T>>,
    chaos_injector: ChaosInjector,
}

impl<T> ChaosDriver<T>
where
    T: Clone + Send + Sync + 'static,
{
    pub fn new(inner: Arc<dyn CacheDriver<Value = T>>, config: ChaosConfig) -> Self {
        Self {
            inner,
            chaos_injector: ChaosInjector::new(config),
        }
    }

    /// Wrap a driver with always-failing chaos
    pub fn always_fail(inner: Arc<dyn CacheDriver<Value = T>>) -> Self {
        Self {
            inner,
            chaos_injector: ChaosInjector::always_fail(),
        }
    }

    /// Wrap a driver with random delays
    pub fn with_delays(inner: Arc<dyn CacheDriver<Value = T>>, min_ms: u64, max_ms: u64) -> Self {
        Self {
            inner,
            chaos_injector: ChaosInjector::with_delays(min_ms, max_ms),
        }
    }

    /// Wrap a driver with network partition simulation
    pub fn with_network_partition(inner: Arc<dyn CacheDriver<Value = T>>) -> Self {
        Self {
            inner,
            chaos_injector: ChaosInjector::with_network_partition(),
        }
    }

    /// Update the chaos configuration
    pub fn update_config(&mut self, config: ChaosConfig) {
        self.chaos_injector.update_config(config);
    }

    /// Enable network partition
    pub fn enable_network_partition(&mut self) {
        self.chaos_injector.enable_network_partition();
    }

    /// Disable network partition
    pub fn disable_network_partition(&mut self) {
        self.chaos_injector.disable_network_partition();
    }

    /// Enable memory pressure
    pub fn enable_memory_pressure(&mut self) {
        self.chaos_injector.enable_memory_pressure();
    }

    /// Get reference to the inner driver
    pub fn inner(&self) -> &Arc<dyn CacheDriver<Value = T>> {
        &self.inner
    }

    /// Get mutable reference to the chaos injector
    pub fn chaos_injector_mut(&mut self) -> &mut ChaosInjector {
        &mut self.chaos_injector
    }
}

#[async_trait]
impl<T> CacheDriver for ChaosDriver<T>
where
    T: Clone + Send + Sync + 'static,
{
    type Value = T;

    async fn get(&self, key: &str) -> CacheResult<Option<Self::Value>> {
        self.chaos_injector.inject_chaos().await?;
        self.inner.get(key).await
    }

    async fn set(&self, key: &str, value: Self::Value, ttl: Option<Duration>) -> CacheResult<()> {
        self.chaos_injector.inject_chaos().await?;
        self.inner.set(key, value, ttl).await
    }

    async fn delete(&self, key: &str) -> CacheResult<bool> {
        self.chaos_injector.inject_chaos().await?;
        self.inner.delete(key).await
    }

    async fn has(&self, key: &str) -> CacheResult<bool> {
        self.chaos_injector.inject_chaos().await?;
        self.inner.has(key).await
    }

    async fn clear(&self) -> CacheResult<()> {
        self.chaos_injector.inject_chaos().await?;
        self.inner.clear().await
    }

    async fn get_many(&self, keys: &[&str]) -> CacheResult<Vec<Option<Self::Value>>> {
        self.chaos_injector.inject_chaos().await?;
        self.inner.get_many(keys).await
    }

    async fn set_many(&self, entries: &[(&str, Self::Value, Option<Duration>)]) -> CacheResult<()> {
        self.chaos_injector.inject_chaos().await?;
        self.inner.set_many(entries).await
    }

    async fn delete_many(&self, keys: &[&str]) -> CacheResult<u64> {
        self.chaos_injector.inject_chaos().await?;
        self.inner.delete_many(keys).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drivers::memory::MemoryDriverBuilder;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_chaos_driver_basic_operations() {
        let memory_driver = Arc::new(MemoryDriverBuilder::new().build::<String>());
        let chaos_driver = ChaosDriver::new(memory_driver, ChaosConfig::default());

        // Should work normally with no chaos
        let result = chaos_driver.set("key1", "value1".to_string(), None).await;
        assert!(result.is_ok());

        let result = chaos_driver.get("key1").await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some("value1".to_string()));
    }

    #[tokio::test]
    async fn test_chaos_driver_always_fail() {
        let memory_driver = Arc::new(MemoryDriverBuilder::new().build::<String>());
        let chaos_driver = ChaosDriver::always_fail(memory_driver);

        // Should always fail
        let result = chaos_driver.set("key1", "value1".to_string(), None).await;
        assert!(result.is_err());

        let result = chaos_driver.get("key1").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_chaos_driver_with_delays() {
        let memory_driver = Arc::new(MemoryDriverBuilder::new().build::<String>());
        let chaos_driver = ChaosDriver::with_delays(memory_driver, 10, 20);

        let start = std::time::Instant::now();
        let result = chaos_driver.set("key1", "value1".to_string(), None).await;
        let duration = start.elapsed();

        assert!(result.is_ok());
        assert!(duration >= Duration::from_millis(10));
    }
}
