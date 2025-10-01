use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;

use crate::error::CacheResult;
use crate::traits::CacheProvider;
use super::adversarial_patterns::{PatternGenerator, AdversarialPattern, AccessPattern, Operation};
use super::mathematical_analysis::{StatisticalAnalyzer, PerformanceMetrics};

/// Load generation patterns for stress testing
#[derive(Debug, Clone)]
pub enum LoadPattern {
    /// Constant rate of requests
    ConstantRate { ops_per_second: u64 },
    /// Gradually increasing load
    Ramp { start_ops: u64, end_ops: u64, duration: Duration },
    /// Sudden spike in traffic
    Spike { baseline_ops: u64, spike_ops: u64, spike_duration: Duration },
    /// Bursty traffic with quiet periods
    Bursty { burst_ops: u64, burst_duration: Duration, quiet_duration: Duration },
    /// Sinusoidal load pattern
    Sinusoidal { base_ops: u64, amplitude: u64, period: Duration },
    /// Chaotic load with random variations
    Chaotic { min_ops: u64, max_ops: u64 },
}

/// Concurrency levels for load testing
#[derive(Debug, Clone)]
pub enum ConcurrencyLevel {
    /// Fixed number of concurrent operations
    Fixed(usize),
    /// Dynamic concurrency based on response time
    Adaptive { min: usize, max: usize, target_latency_ms: u64 },
    /// Unlimited concurrency (dangerous!)
    Unlimited,
}

/// Load generator for comprehensive cache testing
pub struct LoadGenerator<C, T> {
    cache: Arc<C>,
    pattern_generator: PatternGenerator,
    load_pattern: LoadPattern,
    concurrency: ConcurrencyLevel,
    analyzer: StatisticalAnalyzer,
    start_time: Instant,
    _phantom: std::marker::PhantomData<T>,
}

impl<T> LoadGenerator<T>
where
    T: Clone + Send + Sync + 'static,
{
    pub fn new(
        cache: Arc<dyn CacheProvider<Value = T>>,
        adversarial_pattern: AdversarialPattern,
        load_pattern: LoadPattern,
        concurrency: ConcurrencyLevel,
    ) -> Self {
        Self {
            cache,
            pattern_generator: PatternGenerator::new(adversarial_pattern),
            load_pattern,
            concurrency,
            analyzer: StatisticalAnalyzer::new(100_000),
            start_time: Instant::now(),
        }
    }

    /// Run the load test for a specified duration
    pub async fn run_load_test(&mut self, duration: Duration) -> LoadTestResults<T> {
        let start_time = Instant::now();
        let mut handles = Vec::new();
        let mut total_operations = 0;
        let mut errors = 0;

        println!("🚀 Starting load test for {:?}", duration);

        while start_time.elapsed() < duration {
            let current_ops_per_second = self.calculate_current_rate(start_time.elapsed());
            let batch_size = (current_ops_per_second / 10).max(1); // 100ms batches
            
            let semaphore = match &self.concurrency {
                ConcurrencyLevel::Fixed(n) => Arc::new(Semaphore::new(*n)),
                ConcurrencyLevel::Adaptive { max, .. } => Arc::new(Semaphore::new(*max)),
                ConcurrencyLevel::Unlimited => Arc::new(Semaphore::new(usize::MAX)),
            };

            // Generate batch of operations
            let patterns = self.pattern_generator.next_batch(batch_size as usize);
            
            for pattern in patterns {
                let cache = Arc::clone(&self.cache);
                let semaphore = Arc::clone(&semaphore);
                
                let handle = tokio::spawn(async move {
                    let _permit = semaphore.acquire().await.unwrap();
                    let start = Instant::now();
                    
                    let result = Self::execute_pattern(cache, pattern).await;
                    let latency = start.elapsed();
                    
                    (result, latency)
                });
                
                handles.push(handle);
                total_operations += 1;
            }

            // Collect completed operations
            let mut completed_handles = Vec::new();
            for (i, handle) in handles.iter().enumerate() {
                if handle.is_finished() {
                    completed_handles.push(i);
                }
            }

            // Process completed operations in reverse order to maintain indices
            for &i in completed_handles.iter().rev() {
                let handle = handles.swap_remove(i);
                match handle.await {
                    Ok((result, latency)) => {
                        self.analyzer.add_sample(latency.as_nanos() as f64);
                        if result.is_err() {
                            errors += 1;
                        }
                    }
                    Err(_) => {
                        errors += 1;
                    }
                }
            }

            // Rate limiting
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        // Wait for remaining operations to complete
        for handle in handles {
            match handle.await {
                Ok((result, latency)) => {
                    self.analyzer.add_sample(latency.as_nanos() as f64);
                    if result.is_err() {
                        errors += 1;
                    }
                }
                Err(_) => {
                    errors += 1;
                }
            }
        }

        let metrics = self.analyzer.calculate_metrics();
        
        LoadTestResults {
            total_operations,
            successful_operations: total_operations - errors,
            failed_operations: errors,
            duration: start_time.elapsed(),
            performance_metrics: metrics,
            error_rate: errors as f64 / total_operations as f64,
        }
    }

    /// Execute a specific access pattern
    async fn execute_pattern(
        cache: Arc<dyn CacheProvider<Value = T>>,
        pattern: AccessPattern,
    ) -> CacheResult<()>
    where
        T: Default + From<Vec<u8>>,
    {
        match pattern.operation {
            Operation::Get => {
                cache.get(&pattern.key).await?;
            }
            Operation::Set { value, ttl } => {
                let cache_value = T::from(value);
                cache.set(&pattern.key, cache_value, ttl).await?;
            }
            Operation::Delete => {
                cache.delete(&pattern.key).await?;
            }
            Operation::GetOrSet { factory_cost } => {
                cache.get_or_set(
                    &pattern.key,
                    || async move {
                        // Simulate factory cost
                        tokio::time::sleep(factory_cost).await;
                        Ok(T::default())
                    },
                    crate::traits::GetOrSetOptions::default(),
                ).await?;
            }
        }
        Ok(())
    }

    /// Calculate current operation rate based on load pattern
    fn calculate_current_rate(&self, elapsed: Duration) -> u64 {
        match &self.load_pattern {
            LoadPattern::ConstantRate { ops_per_second } => *ops_per_second,
            
            LoadPattern::Ramp { start_ops, end_ops, duration } => {
                let progress = elapsed.as_secs_f64() / duration.as_secs_f64();
                let progress = progress.min(1.0);
                (*start_ops as f64 + (*end_ops as f64 - *start_ops as f64) * progress) as u64
            }
            
            LoadPattern::Spike { baseline_ops, spike_ops, spike_duration } => {
                if elapsed < *spike_duration {
                    *spike_ops
                } else {
                    *baseline_ops
                }
            }
            
            LoadPattern::Bursty { burst_ops, burst_duration, quiet_duration } => {
                let cycle_duration = *burst_duration + *quiet_duration;
                let cycle_position = elapsed.as_nanos() % cycle_duration.as_nanos();
                
                if cycle_position < burst_duration.as_nanos() {
                    *burst_ops
                } else {
                    0
                }
            }
            
            LoadPattern::Sinusoidal { base_ops, amplitude, period } => {
                let phase = 2.0 * std::f64::consts::PI * elapsed.as_secs_f64() / period.as_secs_f64();
                (*base_ops as f64 + *amplitude as f64 * phase.sin()) as u64
            }
            
            LoadPattern::Chaotic { min_ops, max_ops } => {
                fastrand::u64(*min_ops..=*max_ops)
            }
        }
    }

    /// Run a thundering herd test
    pub async fn run_thundering_herd_test(&mut self, concurrency: usize) -> LoadTestResults<T>
    where
        T: Default + From<Vec<u8>>,
    {
        println!("⚡ Running thundering herd test with {} concurrent requests", concurrency);
        
        let start_time = Instant::now();
        let mut handles = Vec::new();
        
        // Generate thundering herd patterns
        let patterns = self.pattern_generator.thundering_herd_batch();
        
        for _ in 0..concurrency {
            for pattern in &patterns {
                let cache = Arc::clone(&self.cache);
                let pattern = pattern.clone();
                
                let handle = tokio::spawn(async move {
                    let start = Instant::now();
                    let result = Self::execute_pattern(cache, pattern).await;
                    let latency = start.elapsed();
                    (result, latency)
                });
                
                handles.push(handle);
            }
        }

        let mut successful = 0;
        let mut failed = 0;
        
        for handle in handles {
            match handle.await {
                Ok((result, latency)) => {
                    self.analyzer.add_sample(latency.as_nanos() as f64);
                    if result.is_ok() {
                        successful += 1;
                    } else {
                        failed += 1;
                    }
                }
                Err(_) => {
                    failed += 1;
                }
            }
        }

        let total = successful + failed;
        let metrics = self.analyzer.calculate_metrics();
        
        LoadTestResults {
            total_operations: total,
            successful_operations: successful,
            failed_operations: failed,
            duration: start_time.elapsed(),
            performance_metrics: metrics,
            error_rate: failed as f64 / total as f64,
        }
    }

    /// Run a memory pressure test
    pub async fn run_memory_pressure_test(&mut self, value_size_mb: usize, count: usize) -> LoadTestResults<T>
    where
        T: Default + From<Vec<u8>>,
    {
        println!("💾 Running memory pressure test: {} MB values, {} count", value_size_mb, count);
        
        let start_time = Instant::now();
        let value_size_bytes = value_size_mb * 1024 * 1024;
        let mut successful = 0;
        let mut failed = 0;

        for i in 0..count {
            let key = format!("memory_test_{}", i);
            let value = vec![0u8; value_size_bytes];
            let cache_value = T::from(value);
            
            let operation_start = Instant::now();
            let result = self.cache.set(&key, cache_value, Some(Duration::from_secs(60))).await;
            let latency = operation_start.elapsed();
            
            self.analyzer.add_sample(latency.as_nanos() as f64);
            
            if result.is_ok() {
                successful += 1;
            } else {
                failed += 1;
                println!("❌ Memory pressure test failed at iteration {}: {:?}", i, result);
            }

            // Small delay to prevent overwhelming the system
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        let total = successful + failed;
        let metrics = self.analyzer.calculate_metrics();
        
        LoadTestResults {
            total_operations: total,
            successful_operations: successful,
            failed_operations: failed,
            duration: start_time.elapsed(),
            performance_metrics: metrics,
            error_rate: failed as f64 / total as f64,
        }
    }

    /// Get current performance metrics
    pub fn get_current_metrics(&self) -> PerformanceMetrics {
        self.analyzer.calculate_metrics()
    }
}

/// Results from a load test
#[derive(Debug)]
pub struct LoadTestResults<T> {
    pub total_operations: usize,
    pub successful_operations: usize,
    pub failed_operations: usize,
    pub duration: Duration,
    pub performance_metrics: PerformanceMetrics,
    pub error_rate: f64,
}

impl<T> LoadTestResults<T> {
    /// Generate a comprehensive test report
    pub fn generate_report(&self) -> String {
        format!(
            "🔬 Load Test Results Report\n\
             ═══════════════════════════\n\
             \n\
             📊 Operation Summary:\n\
             • Total Operations: {}\n\
             • Successful: {} ({:.2}%)\n\
             • Failed: {} ({:.2}%)\n\
             • Duration: {:.2}s\n\
             • Overall Throughput: {:.0} ops/sec\n\
             \n\
             ⚡ Performance Metrics:\n\
             {}\n\
             \n\
             🎯 Quality Assessment:\n\
             • Error Rate: {:.4}% {}\n\
             • Reliability: {}\n\
             • Performance Grade: {}",
            self.total_operations,
            self.successful_operations,
            (self.successful_operations as f64 / self.total_operations as f64) * 100.0,
            self.failed_operations,
            self.error_rate * 100.0,
            self.duration.as_secs_f64(),
            self.total_operations as f64 / self.duration.as_secs_f64(),
            self.performance_metrics.to_human_readable(),
            self.error_rate * 100.0,
            if self.error_rate < 0.001 { "✅" } else if self.error_rate < 0.01 { "⚠️" } else { "❌" },
            self.assess_reliability(),
            self.assess_performance_grade(),
        )
    }

    fn assess_reliability(&self) -> &'static str {
        match self.error_rate {
            rate if rate < 0.001 => "Excellent (99.9%+)",
            rate if rate < 0.01 => "Good (99%+)",
            rate if rate < 0.05 => "Fair (95%+)",
            _ => "Poor (<95%)",
        }
    }

    fn assess_performance_grade(&self) -> &'static str {
        let p99_ms = self.performance_metrics.p99_latency_ns / 1_000_000.0;
        match p99_ms {
            latency if latency < 1.0 => "A+ (Sub-millisecond)",
            latency if latency < 10.0 => "A (Single-digit ms)",
            latency if latency < 100.0 => "B (Double-digit ms)",
            latency if latency < 1000.0 => "C (Sub-second)",
            _ => "D (Multi-second)",
        }
    }

    /// Check if the test passed quality thresholds
    pub fn passes_quality_gate(&self, max_error_rate: f64, max_p99_latency_ms: f64) -> bool {
        let p99_ms = self.performance_metrics.p99_latency_ns / 1_000_000.0;
        self.error_rate <= max_error_rate && p99_ms <= max_p99_latency_ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drivers::memory::MemoryDriverBuilder;
    use crate::{CacheStackBuilder, RustoCache};

    async fn create_test_cache() -> Arc<RustoCache<Vec<u8>>> {
        let memory_driver = Arc::new(MemoryDriverBuilder::new().build());
        let stack = CacheStackBuilder::new("test").with_l1_driver(memory_driver).build();
        Arc::new(RustoCache::new(stack))
    }

    #[tokio::test]
    async fn test_constant_load() {
        let cache = create_test_cache().await;
        let mut generator = LoadGenerator::new(
            cache,
            AdversarialPattern::Random { key_space: 100 },
            LoadPattern::ConstantRate { ops_per_second: 100 },
            ConcurrencyLevel::Fixed(10),
        );

        let results = generator.run_load_test(Duration::from_millis(500)).await;
        
        assert!(results.total_operations > 0);
        assert!(results.performance_metrics.throughput_ops_sec > 0.0);
        println!("{}", results.generate_report());
    }

    #[tokio::test]
    async fn test_thundering_herd() {
        let cache = create_test_cache().await;
        let mut generator = LoadGenerator::new(
            cache,
            AdversarialPattern::ThunderingHerd { 
                key: "hot_key".to_string(), 
                concurrency: 50 
            },
            LoadPattern::ConstantRate { ops_per_second: 1000 },
            ConcurrencyLevel::Fixed(50),
        );

        let results = generator.run_thundering_herd_test(50).await;
        
        assert!(results.total_operations > 0);
        println!("Thundering Herd Test:\n{}", results.generate_report());
    }

    #[tokio::test]
    async fn test_memory_pressure() {
        let cache = create_test_cache().await;
        let mut generator = LoadGenerator::new(
            cache,
            AdversarialPattern::MemoryBomb { value_size: 1024, count: 100 },
            LoadPattern::ConstantRate { ops_per_second: 10 },
            ConcurrencyLevel::Fixed(1),
        );

        // Test with smaller values to avoid OOM in tests
        let results = generator.run_memory_pressure_test(1, 10).await; // 1MB x 10
        
        assert!(results.total_operations > 0);
        println!("Memory Pressure Test:\n{}", results.generate_report());
    }
}
