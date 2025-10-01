use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};
use rustocache::{RustoCache, CacheProvider, GetOrSetOptions};
use rustocache::drivers::MemoryDriverBuilder;
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Runtime;

#[derive(Clone, Debug)]
struct BenchData {
    value: String,
}

fn grace_period_benchmarks(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    // Setup cache
    let memory_driver = Arc::new(
        MemoryDriverBuilder::new()
            .max_entries(10000)
            .serialize(false)
            .build::<BenchData>()
    );
    
    let cache = RustoCache::new(
        rustocache::CacheStackBuilder::new("grace_bench")
            .with_l1_driver(memory_driver)
            .build()
    );

    let mut group = c.benchmark_group("grace_period_performance");
    
    // Benchmark: Normal operation (no grace period)
    group.bench_function("normal_get_or_set", |b| {
        b.to_async(&rt).iter(|| async {
            cache.get_or_set(
                "bench_key_normal",
                || async { Ok(BenchData { value: "test_value".to_string() }) },
                GetOrSetOptions {
                    ttl: Some(Duration::from_secs(60)),
                    grace_period: None,
                    ..Default::default()
                },
            ).await.unwrap()
        });
    });
    
    // Benchmark: With grace period (should have minimal overhead)
    group.bench_function("with_grace_period", |b| {
        b.to_async(&rt).iter(|| async {
            cache.get_or_set(
                "bench_key_grace",
                || async { Ok(BenchData { value: "test_value".to_string() }) },
                GetOrSetOptions {
                    ttl: Some(Duration::from_secs(60)),
                    grace_period: Some(Duration::from_secs(30)),
                    ..Default::default()
                },
            ).await.unwrap()
        });
    });

    // Benchmark: Grace period serving stale data
    group.bench_function("grace_period_stale_data", |b| {
        // Pre-populate with expired data
        rt.block_on(async {
            cache.get_or_set(
                "bench_key_stale",
                || async { Ok(BenchData { value: "stale_value".to_string() }) },
                GetOrSetOptions {
                    ttl: Some(Duration::from_millis(1)), // Very short TTL
                    grace_period: Some(Duration::from_secs(60)),
                    ..Default::default()
                },
            ).await.unwrap();
            
            // Wait for TTL to expire
            tokio::time::sleep(Duration::from_millis(10)).await;
        });
        
        b.to_async(&rt).iter(|| async {
            cache.get_or_set(
                "bench_key_stale",
                || async { 
                    // Simulate factory failure
                    Err(rustocache::CacheError::Generic { 
                        message: "Factory failed".to_string() 
                    })
                },
                GetOrSetOptions {
                    ttl: Some(Duration::from_millis(1)),
                    grace_period: Some(Duration::from_secs(60)),
                    ..Default::default()
                },
            ).await.unwrap() // Should succeed with stale data
        });
    });

    group.finish();

    // Benchmark different grace period durations
    let mut group = c.benchmark_group("grace_period_durations");
    
    let grace_periods = vec![
        Duration::from_secs(1),
        Duration::from_secs(10),
        Duration::from_secs(60),
        Duration::from_secs(300),
        Duration::from_secs(3600),
    ];
    
    for grace_period in grace_periods {
        group.bench_with_input(
            BenchmarkId::new("grace_duration", format!("{}s", grace_period.as_secs())),
            &grace_period,
            |b, &grace_period| {
                b.to_async(&rt).iter(|| async {
                    cache.get_or_set(
                        &format!("bench_key_duration_{}", grace_period.as_secs()),
                        || async { Ok(BenchData { value: "test_value".to_string() }) },
                        GetOrSetOptions {
                            ttl: Some(Duration::from_secs(60)),
                            grace_period: Some(grace_period),
                            ..Default::default()
                        },
                    ).await.unwrap()
                });
            },
        );
    }
    
    group.finish();

    // Benchmark concurrent grace period operations
    let mut group = c.benchmark_group("grace_period_concurrency");
    
    group.bench_function("concurrent_grace_period_ops", |b| {
        b.to_async(&rt).iter(|| async {
            let futures = (0..100).map(|i| {
                cache.get_or_set(
                    &format!("concurrent_key_{}", i),
                    || async { Ok(BenchData { value: format!("value_{}", i) }) },
                    GetOrSetOptions {
                        ttl: Some(Duration::from_secs(60)),
                        grace_period: Some(Duration::from_secs(30)),
                        ..Default::default()
                    },
                )
            });
            
            futures::future::join_all(futures).await;
        });
    });
    
    group.finish();
}

criterion_group!(benches, grace_period_benchmarks);
criterion_main!(benches);
