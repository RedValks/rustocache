// Removed unused async_trait import
use crate::error::{CacheError, CacheResult};
use fastrand;
use std::time::Duration;
use tokio::time::sleep;

/// Configuration for chaos injection
#[derive(Debug, Clone)]
pub struct ChaosConfig {
    /// Probability of failure (0.0 to 1.0)
    pub failure_probability: f64,
    /// Minimum delay in milliseconds
    pub min_delay_ms: u64,
    /// Maximum delay in milliseconds  
    pub max_delay_ms: u64,
    /// Types of failures to inject
    pub failure_modes: Vec<FailureMode>,
    /// Enable network partition simulation
    pub network_partition: bool,
    /// Memory pressure simulation
    pub memory_pressure: bool,
}

impl Default for ChaosConfig {
    fn default() -> Self {
        Self {
            failure_probability: 0.0,
            min_delay_ms: 0,
            max_delay_ms: 0,
            failure_modes: vec![FailureMode::Timeout, FailureMode::NetworkError],
            network_partition: false,
            memory_pressure: false,
        }
    }
}

/// Types of failures that can be injected
#[derive(Debug, Clone, PartialEq)]
pub enum FailureMode {
    /// Simulate timeout errors
    Timeout,
    /// Simulate network connectivity issues
    NetworkError,
    /// Simulate serialization failures
    SerializationError,
    /// Simulate out of memory conditions
    OutOfMemory,
    /// Simulate disk I/O failures
    IoError,
    /// Simulate corrupted data
    DataCorruption,
    /// Simulate partial failures (some operations succeed, others fail)
    PartialFailure,
}

/// Chaos injector that can simulate various failure conditions
pub struct ChaosInjector {
    config: ChaosConfig,
    partition_active: bool,
    memory_pressure_active: bool,
}

impl ChaosInjector {
    pub fn new(config: ChaosConfig) -> Self {
        Self {
            config,
            partition_active: false,
            memory_pressure_active: false,
        }
    }

    /// Create a chaos injector that always fails
    pub fn always_fail() -> Self {
        Self::new(ChaosConfig {
            failure_probability: 1.0,
            failure_modes: vec![
                FailureMode::Timeout,
                FailureMode::NetworkError,
                FailureMode::SerializationError,
            ],
            ..Default::default()
        })
    }

    /// Create a chaos injector with random delays
    pub fn with_delays(min_ms: u64, max_ms: u64) -> Self {
        Self::new(ChaosConfig {
            min_delay_ms: min_ms,
            max_delay_ms: max_ms,
            ..Default::default()
        })
    }

    /// Create a chaos injector that simulates network partitions
    pub fn with_network_partition() -> Self {
        Self::new(ChaosConfig {
            network_partition: true,
            failure_probability: 0.3,
            failure_modes: vec![FailureMode::NetworkError, FailureMode::Timeout],
            ..Default::default()
        })
    }

    /// Update the chaos configuration
    pub fn update_config(&mut self, config: ChaosConfig) {
        self.config = config;
    }

    /// Enable network partition simulation
    pub fn enable_network_partition(&mut self) {
        self.partition_active = true;
    }

    /// Disable network partition simulation
    pub fn disable_network_partition(&mut self) {
        self.partition_active = false;
    }

    /// Enable memory pressure simulation
    pub fn enable_memory_pressure(&mut self) {
        self.memory_pressure_active = true;
    }

    /// Inject chaos before a cache operation
    pub async fn inject_chaos(&self) -> CacheResult<()> {
        // Inject delay first
        if self.config.max_delay_ms > 0 {
            let delay = if self.config.min_delay_ms == self.config.max_delay_ms {
                self.config.min_delay_ms
            } else {
                fastrand::u64(self.config.min_delay_ms..=self.config.max_delay_ms)
            };

            if delay > 0 {
                sleep(Duration::from_millis(delay)).await;
            }
        }

        // Check for network partition
        if self.partition_active && self.config.network_partition && fastrand::f64() < 0.5 {
            return Err(CacheError::Generic {
                message: "Network partition: node unreachable".to_string(),
            });
        }

        // Check for memory pressure
        if self.memory_pressure_active && self.config.memory_pressure && fastrand::f64() < 0.2 {
            return Err(CacheError::Generic {
                message: "Memory pressure: operation failed".to_string(),
            });
        }

        // Inject random failures
        if fastrand::f64() < self.config.failure_probability {
            let failure_mode =
                &self.config.failure_modes[fastrand::usize(0..self.config.failure_modes.len())];

            return Err(self.create_error(failure_mode));
        }

        Ok(())
    }

    /// Create an error based on the failure mode
    fn create_error(&self, mode: &FailureMode) -> CacheError {
        match mode {
            FailureMode::Timeout => CacheError::Timeout,
            FailureMode::NetworkError => CacheError::Generic {
                message: "Network connection failed".to_string(),
            },
            FailureMode::SerializationError => CacheError::Generic {
                message: "Serialization failed: corrupted data".to_string(),
            },
            FailureMode::OutOfMemory => CacheError::Generic {
                message: "Out of memory: allocation failed".to_string(),
            },
            FailureMode::IoError => CacheError::Io(std::io::Error::other("Disk I/O error")),
            FailureMode::DataCorruption => CacheError::Generic {
                message: "Data corruption detected".to_string(),
            },
            FailureMode::PartialFailure => CacheError::Generic {
                message: "Partial failure: some operations failed".to_string(),
            },
        }
    }

    /// Simulate a thundering herd scenario
    pub async fn simulate_thundering_herd(&self, operation_count: usize) -> CacheResult<()> {
        // Simulate many concurrent operations hitting the same resource
        let mut handles = Vec::new();

        for _ in 0..operation_count {
            let injector = self.clone();
            let handle = tokio::spawn(async move { injector.inject_chaos().await });
            handles.push(handle);
        }

        // Wait for all operations with potential failures
        let mut failures = 0;
        for handle in handles {
            match handle.await {
                Ok(result) => {
                    if result.is_err() {
                        failures += 1;
                    }
                }
                Err(_) => {
                    failures += 1;
                }
            }
        }

        if failures > operation_count / 2 {
            return Err(CacheError::Generic {
                message: format!(
                    "Thundering herd: {}/{} operations failed",
                    failures, operation_count
                ),
            });
        }

        Ok(())
    }

    /// Simulate cache stampede protection failure
    pub async fn simulate_cache_stampede(&self) -> CacheResult<()> {
        // Simulate the scenario where cache stampede protection fails
        if fastrand::f64() < 0.3 {
            return Err(CacheError::Generic {
                message: "Cache stampede: protection mechanism failed".to_string(),
            });
        }
        Ok(())
    }
}

impl Clone for ChaosInjector {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            partition_active: self.partition_active,
            memory_pressure_active: self.memory_pressure_active,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_chaos_injector_no_chaos() {
        let injector = ChaosInjector::new(ChaosConfig::default());
        let result = injector.inject_chaos().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_chaos_injector_always_fail() {
        let injector = ChaosInjector::always_fail();
        let result = injector.inject_chaos().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_chaos_injector_with_delays() {
        let injector = ChaosInjector::with_delays(10, 20);
        let start = std::time::Instant::now();
        let result = injector.inject_chaos().await;
        let duration = start.elapsed();

        assert!(result.is_ok());
        assert!(duration >= Duration::from_millis(10));
    }

    #[tokio::test]
    async fn test_thundering_herd_simulation() {
        let injector = ChaosInjector::new(ChaosConfig {
            failure_probability: 0.1,
            ..Default::default()
        });

        let result = injector.simulate_thundering_herd(100).await;
        // Should handle some failures gracefully
        assert!(result.is_ok() || result.is_err());
    }
}
