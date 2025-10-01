#![allow(deprecated)]
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rustocache::{
    chaos::{
        mathematical_analysis::AdvancedStatistics, ChaosConfig, ChaosDriver, FailureMode,
        StatisticalAnalyzer,
    },
    drivers::MemoryDriverBuilder,
    CacheProvider, CacheStackBuilder, GetOrSetOptions, RustoCache,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Runtime;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct BenchmarkData {
    id: u64,
    payload: Vec<u8>,
    metadata: String,
}

impl Default for BenchmarkData {
    fn default() -> Self {
        Self {
            id: 0,
            payload: vec![0u8; 1024], // 1KB payload
            metadata: "benchmark_data".to_string(),
        }
    }
}

impl From<Vec<u8>> for BenchmarkData {
    fn from(payload: Vec<u8>) -> Self {
        Self {
            id: fastrand::u64(0..u64::MAX),
            payload,
            metadata: "converted_data".to_string(),
        }
    }
}

async fn create_pristine_cache() -> Arc<RustoCache<BenchmarkData>> {
    let memory_driver = Arc::new(
        MemoryDriverBuilder::new()
            .max_entries(10_000)
            .serialize(false)
            .build(),
    );

    let stack = CacheStackBuilder::new("pristine")
        .with_l1_driver(memory_driver)
        .build();

    Arc::new(RustoCache::new(stack))
}

async fn create_chaos_cache() -> Arc<RustoCache<BenchmarkData>> {
    let memory_driver = Arc::new(
        MemoryDriverBuilder::new()
            .max_entries(10_000)
            .serialize(false)
            .build(),
    );

    let chaos_driver = Arc::new(ChaosDriver::new(
        memory_driver,
        ChaosConfig {
            failure_probability: 0.05, // 5% failure rate
            min_delay_ms: 1,
            max_delay_ms: 10,
            failure_modes: vec![
                FailureMode::Timeout,
                FailureMode::NetworkError,
                FailureMode::PartialFailure,
            ],
            network_partition: false,
            memory_pressure: false,
        },
    ));

    let stack = CacheStackBuilder::new("chaos")
        .with_l1_driver(chaos_driver)
        .build();

    Arc::new(RustoCache::new(stack))
}

/// Benchmark: Hotspot Pattern (Worst Case for LRU)
fn bench_adversarial_hotspot(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let cache = rt.block_on(create_pristine_cache());

    c.bench_function("Adversarial_Hotspot_Pattern", |b| {
        b.iter(|| {
            rt.block_on(async {
                // All requests hit the same key - worst case for cache distribution
                let result = cache
                    .get_or_set(
                        "hotspot_key",
                        || async { Ok(BenchmarkData::default()) },
                        GetOrSetOptions::default(),
                    )
                    .await;
                black_box(result)
            })
        })
    });
}

/// Benchmark: Pathological LRU Access Pattern
fn bench_adversarial_lru_killer(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let cache = rt.block_on(create_pristine_cache());
    let cache_size = 1000;

    c.bench_function("Adversarial_LRU_Killer", |b| {
        let mut counter = 0u64;
        b.iter(|| {
            rt.block_on(async {
                // Access pattern that causes maximum LRU evictions
                let key_id = counter % (cache_size + 1);
                let key = format!("lru_killer_{}", key_id);
                counter += 1;

                let result = cache
                    .get_or_set(
                        &key,
                        || async { Ok(BenchmarkData::default()) },
                        GetOrSetOptions::default(),
                    )
                    .await;
                black_box(result)
            })
        })
    });
}

/// Benchmark: Random Access with No Locality
fn bench_adversarial_random_chaos(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let cache = rt.block_on(create_pristine_cache());

    c.bench_function("Adversarial_Random_Chaos", |b| {
        b.iter(|| {
            rt.block_on(async {
                // Completely random access with no spatial or temporal locality
                let key = format!("random_{}", fastrand::u64(0..1_000_000));
                let result = cache
                    .get_or_set(
                        &key,
                        || async { Ok(BenchmarkData::default()) },
                        GetOrSetOptions::default(),
                    )
                    .await;
                black_box(result)
            })
        })
    });
}

/// Benchmark: Chaos Engineering with Failures
fn bench_chaos_engineering(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let cache = rt.block_on(create_chaos_cache());

    c.bench_function("Chaos_Engineering_5pct_Failures", |b| {
        b.iter(|| {
            rt.block_on(async {
                let key = format!("chaos_{}", fastrand::u64(0..1000));
                let result = cache
                    .get_or_set(
                        &key,
                        || async { Ok(BenchmarkData::default()) },
                        GetOrSetOptions::default(),
                    )
                    .await;
                // Expect some failures due to chaos injection
                black_box(result)
            })
        })
    });
}

/// Benchmark: Memory Bomb Attack
fn bench_memory_bomb(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let cache = rt.block_on(create_pristine_cache());

    c.bench_function("Adversarial_Memory_Bomb", |b| {
        let mut counter = 0u64;
        b.iter(|| {
            rt.block_on(async {
                let key = format!("bomb_{}", counter % 100);
                counter += 1;

                // Large payload to stress memory management
                let large_data = BenchmarkData {
                    id: counter,
                    payload: vec![0u8; 10 * 1024], // 10KB payload
                    metadata: "memory_bomb_payload".to_string(),
                };

                let result = cache
                    .set(&key, large_data, Some(Duration::from_secs(60)))
                    .await;
                black_box(result)
            })
        })
    });
}

/// Benchmark: Zipfian Distribution (Realistic but Skewed)
fn bench_zipfian_distribution(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let cache = rt.block_on(create_pristine_cache());

    // Pre-compute Zipfian distribution for consistent benchmarking
    let mut zipf_keys = Vec::new();
    let key_space = 1000u64;
    let alpha = 1.2; // Highly skewed

    for _ in 0..10000 {
        // Simplified Zipfian sampling
        let r = fastrand::f64();
        let rank = ((1.0 - r).powf(-1.0 / alpha) - 1.0) as u64;
        let key_id = rank % key_space;
        zipf_keys.push(format!("zipf_{}", key_id));
    }

    c.bench_function("Adversarial_Zipfian_Distribution", |b| {
        let mut index = 0;
        b.iter(|| {
            rt.block_on(async {
                let key = &zipf_keys[index % zipf_keys.len()];
                index += 1;

                let result = cache
                    .get_or_set(
                        key,
                        || async { Ok(BenchmarkData::default()) },
                        GetOrSetOptions::default(),
                    )
                    .await;
                black_box(result)
            })
        })
    });
}

/// Comprehensive Adversarial Test Suite
fn bench_comprehensive_adversarial_suite(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("Comprehensive_Adversarial_Suite", |b| {
        b.iter(|| {
            rt.block_on(async {
                // Run multiple adversarial patterns in sequence
                let cache = create_pristine_cache().await;
                let mut total_ops = 0;

                // 1. Hotspot attack
                for _ in 0..10 {
                    let _ = cache
                        .get_or_set(
                            "hotspot",
                            || async { Ok(BenchmarkData::default()) },
                            GetOrSetOptions::default(),
                        )
                        .await;
                    total_ops += 1;
                }

                // 2. LRU killer
                for i in 0..10 {
                    let key = format!("lru_{}", i % 11);
                    let _ = cache.get(&key).await;
                    total_ops += 1;
                }

                // 3. Random chaos
                for _ in 0..10 {
                    let key = format!("random_{}", fastrand::u64(0..1000));
                    let _ = cache.get(&key).await;
                    total_ops += 1;
                }

                black_box(total_ops)
            })
        })
    });
}

/// Mathematical Rigor: Statistical Analysis Benchmark
fn bench_statistical_analysis(c: &mut Criterion) {
    c.bench_function("Mathematical_Statistical_Analysis", |b| {
        b.iter(|| {
            let mut analyzer = StatisticalAnalyzer::new(10000);

            // Generate sample data with realistic latency distribution
            for _ in 0..1000 {
                let base_latency = 1000.0; // 1μs base
                let noise = fastrand::f64() * 500.0; // Up to 0.5μs noise
                let outlier = if fastrand::f64() < 0.01 {
                    fastrand::f64() * 10000.0 // 1% outliers up to 10μs
                } else {
                    0.0
                };

                analyzer.add_sample(base_latency + noise + outlier);
            }

            let metrics = analyzer.calculate_metrics();
            black_box(metrics)
        })
    });
}

/// Advanced Statistical Tests
fn bench_advanced_statistics(c: &mut Criterion) {
    c.bench_function("Advanced_Statistical_Tests", |b| {
        b.iter(|| {
            // Generate two sample distributions
            let sample1: Vec<f64> = (0..1000).map(|_| fastrand::f64() * 1000.0).collect();
            let sample2: Vec<f64> = (0..1000)
                .map(|_| fastrand::f64() * 1200.0 + 100.0)
                .collect();

            // Mann-Whitney U test
            let p_value = AdvancedStatistics::mann_whitney_u_test(&sample1, &sample2);

            // Autocorrelation analysis
            let autocorr = AdvancedStatistics::autocorrelation(&sample1, 1);

            // Anomaly detection
            let anomalies = AdvancedStatistics::detect_anomalies(&sample1, 0.05);

            black_box((p_value, autocorr, anomalies.len()))
        })
    });
}

/// Concurrent Access Performance Test
fn bench_concurrent_access_performance(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("Concurrent_Access_Performance", |b| {
        b.iter(|| {
            rt.block_on(async {
                let cache = create_pristine_cache().await;
                let mut handles = Vec::new();

                // Simulate concurrent access
                for i in 0..50 {
                    let cache_clone = cache.clone();
                    let handle = tokio::spawn(async move {
                        let key = format!("concurrent_{}", i % 10);
                        cache_clone
                            .get_or_set(
                                &key,
                                || async { Ok(BenchmarkData::default()) },
                                GetOrSetOptions::default(),
                            )
                            .await
                    });
                    handles.push(handle);
                }

                let mut results = 0;
                for handle in handles {
                    if handle.await.is_ok() {
                        results += 1;
                    }
                }
                black_box(results)
            })
        })
    });
}

criterion_group!(
    adversarial_benches,
    bench_adversarial_hotspot,
    bench_adversarial_lru_killer,
    bench_adversarial_random_chaos,
    bench_chaos_engineering,
    bench_memory_bomb,
    bench_zipfian_distribution,
    bench_comprehensive_adversarial_suite,
    bench_statistical_analysis,
    bench_advanced_statistics,
    bench_concurrent_access_performance
);

criterion_main!(adversarial_benches);
