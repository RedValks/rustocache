#![allow(deprecated)]
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rustocache::{
    drivers::{MemoryDriverBuilder, MemoryDriverSIMDBuilder},
    CacheProvider, CacheStackBuilder, GetOrSetOptions, RustoCache,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Runtime;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct BenchData {
    id: u64,
    name: String,
    data: Vec<u8>,
    metadata: std::collections::HashMap<String, String>,
}

impl BenchData {
    fn new(id: u64) -> Self {
        let mut metadata = std::collections::HashMap::new();
        metadata.insert("created_at".to_string(), "2024-01-01".to_string());
        metadata.insert("version".to_string(), "1.0".to_string());

        Self {
            id,
            name: format!("bench_item_{}", id),
            data: vec![0u8; 512], // 512 bytes of data
            metadata,
        }
    }
}

async fn create_standard_cache() -> RustoCache<BenchData> {
    let memory_driver = Arc::new(
        MemoryDriverBuilder::new()
            .max_entries(50_000)
            .serialize(false)
            .build(),
    );

    let stack = CacheStackBuilder::new("standard_bench")
        .with_l1_driver(memory_driver)
        .build();

    RustoCache::new(stack)
}

async fn create_simd_cache() -> RustoCache<BenchData> {
    let memory_driver = Arc::new(
        MemoryDriverSIMDBuilder::new()
            .max_entries(50_000)
            .serialize(false)
            .build(),
    );

    let stack = CacheStackBuilder::new("simd_bench")
        .with_l1_driver(memory_driver)
        .build();

    RustoCache::new(stack)
}

fn bench_bulk_operations_standard(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let cache = rt.block_on(create_standard_cache());

    c.bench_function("standard_bulk_set_1000", |b| {
        b.iter(|| {
            rt.block_on(async {
                let entries: Vec<(String, BenchData, Option<Duration>)> = (0..1000)
                    .map(|i| {
                        (
                            format!("key_{}", i),
                            BenchData::new(i),
                            Some(Duration::from_secs(300)),
                        )
                    })
                    .collect();

                let key_refs: Vec<(&str, BenchData, Option<Duration>)> = entries
                    .iter()
                    .map(|(k, v, ttl)| (k.as_str(), v.clone(), *ttl))
                    .collect();

                let result = cache.set_many(&key_refs).await;
                black_box(result)
            })
        })
    });
}

fn bench_bulk_operations_simd(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let cache = rt.block_on(create_simd_cache());

    c.bench_function("simd_bulk_set_1000", |b| {
        b.iter(|| {
            rt.block_on(async {
                let entries: Vec<(String, BenchData, Option<Duration>)> = (0..1000)
                    .map(|i| {
                        (
                            format!("key_{}", i),
                            BenchData::new(i),
                            Some(Duration::from_secs(300)),
                        )
                    })
                    .collect();

                let key_refs: Vec<(&str, BenchData, Option<Duration>)> = entries
                    .iter()
                    .map(|(k, v, ttl)| (k.as_str(), v.clone(), *ttl))
                    .collect();

                let result = cache.set_many(&key_refs).await;
                black_box(result)
            })
        })
    });
}

fn bench_bulk_get_standard(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let cache = rt.block_on(create_standard_cache());

    // Pre-populate cache
    rt.block_on(async {
        for i in 0..1000 {
            let key = format!("key_{}", i);
            cache
                .set(&key, BenchData::new(i), Some(Duration::from_secs(300)))
                .await
                .unwrap();
        }
    });

    c.bench_function("standard_bulk_get_1000", |b| {
        b.iter(|| {
            rt.block_on(async {
                let keys: Vec<String> = (0..1000).map(|i| format!("key_{}", i)).collect();
                let key_refs: Vec<&str> = keys.iter().map(|s| s.as_str()).collect();

                let result = cache.get_many(&key_refs).await;
                black_box(result)
            })
        })
    });
}

fn bench_bulk_get_simd(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let cache = rt.block_on(create_simd_cache());

    // Pre-populate cache
    rt.block_on(async {
        for i in 0..1000 {
            let key = format!("key_{}", i);
            cache
                .set(&key, BenchData::new(i), Some(Duration::from_secs(300)))
                .await
                .unwrap();
        }
    });

    c.bench_function("simd_bulk_get_1000", |b| {
        b.iter(|| {
            rt.block_on(async {
                let keys: Vec<String> = (0..1000).map(|i| format!("key_{}", i)).collect();
                let key_refs: Vec<&str> = keys.iter().map(|s| s.as_str()).collect();

                let result = cache.get_many(&key_refs).await;
                black_box(result)
            })
        })
    });
}

fn bench_expiration_cleanup_standard(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let cache = rt.block_on(create_standard_cache());

    c.bench_function("standard_expiration_cleanup", |b| {
        b.iter(|| {
            rt.block_on(async {
                // Set entries with very short TTL
                for i in 0..1000 {
                    let key = format!("expire_key_{}", i);
                    cache
                        .set(&key, BenchData::new(i), Some(Duration::from_millis(1)))
                        .await
                        .unwrap();
                }

                // Wait for expiration
                tokio::time::sleep(Duration::from_millis(5)).await;

                // Trigger cleanup by accessing cache
                let result = cache.get("trigger_cleanup").await;
                black_box(result)
            })
        })
    });
}

fn bench_expiration_cleanup_simd(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let cache = rt.block_on(create_simd_cache());

    c.bench_function("simd_expiration_cleanup", |b| {
        b.iter(|| {
            rt.block_on(async {
                // Set entries with very short TTL
                for i in 0..1000 {
                    let key = format!("expire_key_{}", i);
                    cache
                        .set(&key, BenchData::new(i), Some(Duration::from_millis(1)))
                        .await
                        .unwrap();
                }

                // Wait for expiration
                tokio::time::sleep(Duration::from_millis(5)).await;

                // Trigger cleanup by accessing cache
                let result = cache.get("trigger_cleanup").await;
                black_box(result)
            })
        })
    });
}

fn bench_single_operations_comparison(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let standard_cache = rt.block_on(create_standard_cache());
    let simd_cache = rt.block_on(create_simd_cache());

    let mut group = c.benchmark_group("single_operation_comparison");

    group.bench_function("standard_single_get_or_set", |b| {
        b.iter(|| {
            rt.block_on(async {
                let key = format!("key_{}", fastrand::u64(0..1000));
                let result = standard_cache
                    .get_or_set(
                        &key,
                        || async { Ok(BenchData::new(fastrand::u64(0..10000))) },
                        GetOrSetOptions::default(),
                    )
                    .await;
                black_box(result)
            })
        })
    });

    group.bench_function("simd_single_get_or_set", |b| {
        b.iter(|| {
            rt.block_on(async {
                let key = format!("key_{}", fastrand::u64(0..1000));
                let result = simd_cache
                    .get_or_set(
                        &key,
                        || async { Ok(BenchData::new(fastrand::u64(0..10000))) },
                        GetOrSetOptions::default(),
                    )
                    .await;
                black_box(result)
            })
        })
    });

    group.finish();
}

fn bench_high_contention_workload(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let standard_cache = rt.block_on(create_standard_cache());
    let simd_cache = rt.block_on(create_simd_cache());

    let mut group = c.benchmark_group("high_contention_workload");

    // Simulate high contention with many operations on overlapping key sets
    group.bench_function("standard_high_contention", |b| {
        b.iter(|| {
            rt.block_on(async {
                let mut handles = Vec::new();

                for _ in 0..10 {
                    let cache_clone = standard_cache.clone();
                    let handle = tokio::spawn(async move {
                        for i in 0..100 {
                            let key = format!("hotkey_{}", i % 20); // High contention on 20 keys
                            let _ = cache_clone
                                .get_or_set(
                                    &key,
                                    || async { Ok(BenchData::new(i)) },
                                    GetOrSetOptions::default(),
                                )
                                .await;
                        }
                    });
                    handles.push(handle);
                }

                for handle in handles {
                    let _ = handle.await;
                }
            })
        })
    });

    group.bench_function("simd_high_contention", |b| {
        b.iter(|| {
            rt.block_on(async {
                let mut handles = Vec::new();

                for _ in 0..10 {
                    let cache_clone = simd_cache.clone();
                    let handle = tokio::spawn(async move {
                        for i in 0..100 {
                            let key = format!("hotkey_{}", i % 20); // High contention on 20 keys
                            let _ = cache_clone
                                .get_or_set(
                                    &key,
                                    || async { Ok(BenchData::new(i)) },
                                    GetOrSetOptions::default(),
                                )
                                .await;
                        }
                    });
                    handles.push(handle);
                }

                for handle in handles {
                    let _ = handle.await;
                }
            })
        })
    });

    group.finish();
}

criterion_group!(
    simd_benches,
    bench_bulk_operations_standard,
    bench_bulk_operations_simd,
    bench_bulk_get_standard,
    bench_bulk_get_simd,
    bench_expiration_cleanup_standard,
    bench_expiration_cleanup_simd,
    bench_single_operations_comparison,
    bench_high_contention_workload
);

criterion_main!(simd_benches);
