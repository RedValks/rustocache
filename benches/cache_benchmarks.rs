#![allow(deprecated)]
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rustocache::{
    drivers::{MemoryDriverBuilder, RedisDriverBuilder},
    CacheProvider, CacheStackBuilder, GetOrSetOptions, RustoCache,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Runtime;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct TestData {
    id: u64,
    name: String,
    data: Vec<u8>,
    metadata: std::collections::HashMap<String, String>,
}

impl TestData {
    fn new(id: u64) -> Self {
        let mut metadata = std::collections::HashMap::new();
        metadata.insert("created_at".to_string(), "2024-01-01".to_string());
        metadata.insert("version".to_string(), "1.0".to_string());

        Self {
            id,
            name: format!("test_item_{}", id),
            data: vec![0u8; 1024], // 1KB of data
            metadata,
        }
    }
}

async fn create_memory_cache() -> RustoCache<TestData> {
    let memory_driver = Arc::new(
        MemoryDriverBuilder::new()
            .max_entries(10_000)
            .serialize(false)
            .build(),
    );

    let stack = CacheStackBuilder::new("memory_bench")
        .with_l1_driver(memory_driver)
        .build();

    RustoCache::new(stack)
}

async fn create_redis_cache() -> Result<RustoCache<TestData>, Box<dyn std::error::Error>> {
    let redis_driver = Arc::new(
        RedisDriverBuilder::new()
            .url("redis://localhost:6379")
            .prefix("rustocache_bench")
            .build()
            .await?,
    );

    let stack = CacheStackBuilder::new("redis_bench")
        .with_l2_driver(redis_driver)
        .build();

    Ok(RustoCache::new(stack))
}

async fn create_tiered_cache() -> Result<RustoCache<TestData>, Box<dyn std::error::Error>> {
    let memory_driver = Arc::new(
        MemoryDriverBuilder::new()
            .max_entries(1_000)
            .serialize(false)
            .build(),
    );

    let redis_driver = Arc::new(
        RedisDriverBuilder::new()
            .url("redis://localhost:6379")
            .prefix("rustocache_tiered_bench")
            .build()
            .await?,
    );

    let stack = CacheStackBuilder::new("tiered_bench")
        .with_l1_driver(memory_driver)
        .with_l2_driver(redis_driver)
        .build();

    Ok(RustoCache::new(stack))
}

fn bench_memory_get_or_set(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let cache = rt.block_on(create_memory_cache());

    c.bench_function("memory_get_or_set", |b| {
        b.iter(|| {
            rt.block_on(async {
                let key = format!("key_{}", fastrand::u64(0..1000));
                let result = cache
                    .get_or_set(
                        &key,
                        || async { Ok(TestData::new(fastrand::u64(0..10000))) },
                        GetOrSetOptions {
                            ttl: Some(Duration::from_secs(300)),
                            ..Default::default()
                        },
                    )
                    .await;
                black_box(result)
            })
        })
    });
}

fn bench_memory_get(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let cache = rt.block_on(create_memory_cache());

    // Pre-populate cache
    rt.block_on(async {
        for i in 0..1000 {
            let key = format!("key_{}", i);
            cache
                .set(&key, TestData::new(i), Some(Duration::from_secs(300)))
                .await
                .unwrap();
        }
    });

    c.bench_function("memory_get", |b| {
        b.iter(|| {
            rt.block_on(async {
                let key = format!("key_{}", fastrand::u64(0..1000));
                let result = cache.get(&key).await;
                black_box(result)
            })
        })
    });
}

fn bench_memory_set(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let cache = rt.block_on(create_memory_cache());

    c.bench_function("memory_set", |b| {
        b.iter(|| {
            rt.block_on(async {
                let key = format!("key_{}", fastrand::u64(0..10000));
                let data = TestData::new(fastrand::u64(0..10000));
                let result = cache.set(&key, data, Some(Duration::from_secs(300))).await;
                black_box(result)
            })
        })
    });
}

fn bench_redis_get_or_set(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    // Skip Redis benchmarks if Redis is not available
    let cache = match rt.block_on(create_redis_cache()) {
        Ok(cache) => cache,
        Err(_) => {
            println!("Skipping Redis benchmarks - Redis server not available");
            return;
        }
    };

    c.bench_function("redis_get_or_set", |b| {
        b.iter(|| {
            rt.block_on(async {
                let key = format!("key_{}", fastrand::u64(0..1000));
                let result = cache
                    .get_or_set(
                        &key,
                        || async { Ok(TestData::new(fastrand::u64(0..10000))) },
                        GetOrSetOptions {
                            ttl: Some(Duration::from_secs(300)),
                            ..Default::default()
                        },
                    )
                    .await;
                black_box(result)
            })
        })
    });
}

fn bench_tiered_get_or_set(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let cache = match rt.block_on(create_tiered_cache()) {
        Ok(cache) => cache,
        Err(_) => {
            println!("Skipping tiered cache benchmarks - Redis server not available");
            return;
        }
    };

    c.bench_function("tiered_get_or_set", |b| {
        b.iter(|| {
            rt.block_on(async {
                let key = format!("key_{}", fastrand::u64(0..1000));
                let result = cache
                    .get_or_set(
                        &key,
                        || async { Ok(TestData::new(fastrand::u64(0..10000))) },
                        GetOrSetOptions {
                            ttl: Some(Duration::from_secs(300)),
                            ..Default::default()
                        },
                    )
                    .await;
                black_box(result)
            })
        })
    });
}

fn bench_tiered_get(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let cache = match rt.block_on(create_tiered_cache()) {
        Ok(cache) => cache,
        Err(_) => {
            println!("Skipping tiered cache get benchmarks - Redis server not available");
            return;
        }
    };

    // Pre-populate cache
    rt.block_on(async {
        for i in 0..1000 {
            let key = format!("key_{}", i);
            cache
                .set(&key, TestData::new(i), Some(Duration::from_secs(300)))
                .await
                .unwrap();
        }
    });

    c.bench_function("tiered_get", |b| {
        b.iter(|| {
            rt.block_on(async {
                let key = format!("key_{}", fastrand::u64(0..1000));
                let result = cache.get(&key).await;
                black_box(result)
            })
        })
    });
}

fn bench_comparison(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let memory_cache = rt.block_on(create_memory_cache());
    let tiered_cache = match rt.block_on(create_tiered_cache()) {
        Ok(cache) => cache,
        Err(_) => {
            println!("Skipping tiered cache comparison benchmarks - Redis server not available");
            // Only run memory cache benchmarks
            let mut group = c.benchmark_group("cache_comparison");

            group.bench_function("L1_GetOrSet_RustoCache", |b| {
                b.iter(|| {
                    rt.block_on(async {
                        let key = format!("key_{}", fastrand::u64(0..100));
                        let result = memory_cache
                            .get_or_set(
                                &key,
                                || async { Ok(TestData::new(fastrand::u64(0..1000))) },
                                GetOrSetOptions::default(),
                            )
                            .await;
                        black_box(result)
                    })
                })
            });

            group.finish();
            return;
        }
    };

    let mut group = c.benchmark_group("cache_comparison");

    group.bench_function("L1_GetOrSet_RustoCache", |b| {
        b.iter(|| {
            rt.block_on(async {
                let key = format!("key_{}", fastrand::u64(0..100));
                let result = memory_cache
                    .get_or_set(
                        &key,
                        || async { Ok(TestData::new(fastrand::u64(0..1000))) },
                        GetOrSetOptions::default(),
                    )
                    .await;
                black_box(result)
            })
        })
    });

    group.bench_function("Tiered_GetOrSet_RustoCache", |b| {
        b.iter(|| {
            rt.block_on(async {
                let key = format!("key_{}", fastrand::u64(0..100));
                let result = tiered_cache
                    .get_or_set(
                        &key,
                        || async { Ok(TestData::new(fastrand::u64(0..1000))) },
                        GetOrSetOptions::default(),
                    )
                    .await;
                black_box(result)
            })
        })
    });

    // Pre-populate for get benchmarks
    rt.block_on(async {
        for i in 0..100 {
            let key = format!("key_{}", i);
            let data = TestData::new(i);
            memory_cache.set(&key, data.clone(), None).await.unwrap();
            tiered_cache.set(&key, data, None).await.unwrap();
        }
    });

    group.bench_function("Tiered_Get_RustoCache", |b| {
        b.iter(|| {
            rt.block_on(async {
                let key = format!("key_{}", fastrand::u64(0..100));
                let result = tiered_cache.get(&key).await;
                black_box(result)
            })
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_memory_get_or_set,
    bench_memory_get,
    bench_memory_set,
    bench_redis_get_or_set,
    bench_tiered_get_or_set,
    bench_tiered_get,
    bench_comparison
);

criterion_main!(benches);
