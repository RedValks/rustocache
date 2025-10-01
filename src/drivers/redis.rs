use async_trait::async_trait;
use redis::{AsyncCommands, Client};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

use crate::error::{CacheError, CacheResult};
use crate::traits::{CacheDriver, CacheEntry};

/// Configuration for Redis driver
#[derive(Debug, Clone)]
pub struct RedisDriverConfig {
    /// Redis connection URL
    pub url: String,
    /// Key prefix for namespacing
    pub prefix: Option<String>,
    /// Default TTL for entries
    pub default_ttl: Option<Duration>,
    /// Connection pool size
    pub pool_size: usize,
    /// Connection timeout
    pub connect_timeout: Duration,
    /// Command timeout
    pub command_timeout: Duration,
}

impl Default for RedisDriverConfig {
    fn default() -> Self {
        Self {
            url: "redis://localhost:6379".to_string(),
            prefix: None,
            default_ttl: None,
            pool_size: 10,
            connect_timeout: Duration::from_secs(5),
            command_timeout: Duration::from_secs(30),
        }
    }
}

/// Redis cache driver with connection pooling
pub struct RedisDriver<T> {
    client: Client,
    connection: Arc<RwLock<Option<redis::aio::Connection>>>,
    config: RedisDriverConfig,
    _phantom: std::marker::PhantomData<T>,
}

impl<T> RedisDriver<T>
where
    T: Serialize + for<'de> Deserialize<'de> + Send + Sync + 'static,
{
    pub async fn new(config: RedisDriverConfig) -> CacheResult<Self> {
        let client = Client::open(config.url.as_str()).map_err(CacheError::Redis)?;

        // Test connection
        let mut conn = client
            .get_async_connection()
            .await
            .map_err(CacheError::Redis)?;

        // Test connection with a simple command
        let _: String = redis::cmd("PING")
            .query_async(&mut conn)
            .await
            .map_err(CacheError::Redis)?;

        Ok(Self {
            client,
            connection: Arc::new(RwLock::new(Some(conn))),
            config,
            _phantom: std::marker::PhantomData,
        })
    }

    /// Get a connection from the pool
    async fn get_connection(&self) -> CacheResult<redis::aio::Connection> {
        let mut conn_guard = self.connection.write().await;

        if let Some(conn) = conn_guard.take() {
            Ok(conn)
        } else {
            // Create new connection
            self.client
                .get_async_connection()
                .await
                .map_err(CacheError::Redis)
        }
    }

    /// Return connection to the pool
    async fn return_connection(&self, conn: redis::aio::Connection) {
        let mut conn_guard = self.connection.write().await;
        *conn_guard = Some(conn);
    }

    /// Build the full key with prefix
    fn build_key(&self, key: &str) -> String {
        if let Some(prefix) = &self.config.prefix {
            format!("{}:{}", prefix, key)
        } else {
            key.to_string()
        }
    }

    /// Serialize value to bytes
    fn serialize_value(&self, entry: &CacheEntry<T>) -> CacheResult<Vec<u8>> {
        bincode::serde::encode_to_vec(entry, bincode::config::standard()).map_err(|e| {
            CacheError::Generic {
                message: format!("Serialization failed: {}", e),
            }
        })
    }

    /// Deserialize value from bytes
    fn deserialize_value(&self, data: &[u8]) -> CacheResult<CacheEntry<T>> {
        bincode::serde::decode_from_slice(data, bincode::config::standard())
            .map(|(entry, _)| entry)
            .map_err(|e| CacheError::Generic {
                message: format!("Deserialization failed: {}", e),
            })
    }
}

#[async_trait]
impl<T> CacheDriver for RedisDriver<T>
where
    T: Serialize + for<'de> Deserialize<'de> + Clone + Send + Sync + 'static,
{
    type Value = T;

    async fn get(&self, key: &str) -> CacheResult<Option<Self::Value>> {
        let mut conn = self.get_connection().await?;
        let full_key = self.build_key(key);

        let result: Option<Vec<u8>> = conn.get(&full_key).await.map_err(CacheError::Redis)?;

        self.return_connection(conn).await;

        if let Some(data) = result {
            let entry = self.deserialize_value(&data)?;

            if entry.is_expired() {
                // Remove expired entry
                self.delete(key).await?;
                Ok(None)
            } else {
                Ok(Some(entry.value))
            }
        } else {
            Ok(None)
        }
    }

    async fn set(&self, key: &str, value: Self::Value, ttl: Option<Duration>) -> CacheResult<()> {
        let mut conn = self.get_connection().await?;
        let full_key = self.build_key(key);
        let ttl = ttl.or(self.config.default_ttl);

        let entry = CacheEntry::new(value, ttl);
        let serialized = self.serialize_value(&entry)?;

        if let Some(ttl) = ttl {
            let ttl_secs = ttl.as_secs();
            if ttl_secs > 0 {
                let _: () = conn
                    .set_ex(&full_key, serialized, ttl_secs)
                    .await
                    .map_err(CacheError::Redis)?;
            } else {
                let _: () = conn
                    .set(&full_key, serialized)
                    .await
                    .map_err(CacheError::Redis)?;
            }
        } else {
            let _: () = conn
                .set(&full_key, serialized)
                .await
                .map_err(CacheError::Redis)?;
        }

        self.return_connection(conn).await;
        Ok(())
    }

    async fn delete(&self, key: &str) -> CacheResult<bool> {
        let mut conn = self.get_connection().await?;
        let full_key = self.build_key(key);

        let deleted: u32 = conn.del(&full_key).await.map_err(CacheError::Redis)?;

        self.return_connection(conn).await;
        Ok(deleted > 0)
    }

    async fn has(&self, key: &str) -> CacheResult<bool> {
        let mut conn = self.get_connection().await?;
        let full_key = self.build_key(key);

        let exists: bool = conn.exists(&full_key).await.map_err(CacheError::Redis)?;

        self.return_connection(conn).await;
        Ok(exists)
    }

    async fn clear(&self) -> CacheResult<()> {
        let mut conn = self.get_connection().await?;

        if let Some(prefix) = &self.config.prefix {
            // Delete all keys with prefix
            let pattern = format!("{}:*", prefix);
            let keys: Vec<String> = conn.keys(&pattern).await.map_err(CacheError::Redis)?;

            if !keys.is_empty() {
                let _: u64 = conn.del(&keys).await.map_err(CacheError::Redis)?;
            }
        } else {
            // Flush entire database (dangerous!)
            let _: () = redis::cmd("FLUSHDB")
                .query_async(&mut conn)
                .await
                .map_err(CacheError::Redis)?;
        }

        self.return_connection(conn).await;
        Ok(())
    }

    async fn get_many(&self, keys: &[&str]) -> CacheResult<Vec<Option<Self::Value>>> {
        if keys.is_empty() {
            return Ok(Vec::new());
        }

        let mut conn = self.get_connection().await?;
        let full_keys: Vec<String> = keys.iter().map(|k| self.build_key(k)).collect();

        let results: Vec<Option<Vec<u8>>> =
            conn.mget(&full_keys).await.map_err(CacheError::Redis)?;

        self.return_connection(conn).await;

        let mut values = Vec::with_capacity(results.len());

        for (i, result) in results.into_iter().enumerate() {
            if let Some(data) = result {
                let entry = self.deserialize_value(&data)?;

                if entry.is_expired() {
                    // Remove expired entry
                    self.delete(keys[i]).await?;
                    values.push(None);
                } else {
                    values.push(Some(entry.value));
                }
            } else {
                values.push(None);
            }
        }

        Ok(values)
    }

    async fn set_many(&self, entries: &[(&str, Self::Value, Option<Duration>)]) -> CacheResult<()> {
        // For simplicity, use individual sets
        // In production, you might want to use Redis pipelines
        for (key, value, ttl) in entries {
            self.set(key, value.clone(), *ttl).await?;
        }

        Ok(())
    }

    async fn delete_many(&self, keys: &[&str]) -> CacheResult<u64> {
        if keys.is_empty() {
            return Ok(0);
        }

        let mut conn = self.get_connection().await?;
        let full_keys: Vec<String> = keys.iter().map(|k| self.build_key(k)).collect();

        let deleted: u64 = conn.del(&full_keys).await.map_err(CacheError::Redis)?;

        self.return_connection(conn).await;
        Ok(deleted)
    }
}

/// Builder for Redis driver
pub struct RedisDriverBuilder {
    config: RedisDriverConfig,
}

impl RedisDriverBuilder {
    pub fn new() -> Self {
        Self {
            config: RedisDriverConfig::default(),
        }
    }

    pub fn url<S: Into<String>>(mut self, url: S) -> Self {
        self.config.url = url.into();
        self
    }

    pub fn prefix<S: Into<String>>(mut self, prefix: S) -> Self {
        self.config.prefix = Some(prefix.into());
        self
    }

    pub fn default_ttl(mut self, ttl: Duration) -> Self {
        self.config.default_ttl = Some(ttl);
        self
    }

    pub fn pool_size(mut self, size: usize) -> Self {
        self.config.pool_size = size;
        self
    }

    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.config.connect_timeout = timeout;
        self
    }

    pub fn command_timeout(mut self, timeout: Duration) -> Self {
        self.config.command_timeout = timeout;
        self
    }

    pub async fn build<T>(self) -> CacheResult<RedisDriver<T>>
    where
        T: Serialize + for<'de> Deserialize<'de> + Send + Sync + 'static,
    {
        RedisDriver::new(self.config).await
    }
}

impl Default for RedisDriverBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct TestValue {
        id: u32,
        name: String,
    }

    #[tokio::test]
    #[ignore] // Requires Redis server
    async fn test_redis_driver_basic_operations() {
        let driver = RedisDriverBuilder::new()
            .url("redis://localhost:6379")
            .prefix("test")
            .build::<TestValue>()
            .await
            .unwrap();

        let test_value = TestValue {
            id: 1,
            name: "test".to_string(),
        };

        // Test set and get
        driver.set("key1", test_value.clone(), None).await.unwrap();
        let result = driver.get("key1").await.unwrap();
        assert_eq!(result, Some(test_value));

        // Test delete
        assert!(driver.delete("key1").await.unwrap());
        assert!(!driver.has("key1").await.unwrap());
    }
}
