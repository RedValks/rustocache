#![allow(deprecated)]
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rustocache::{
    drivers::MemoryDriverBuilder, CacheProvider, CacheStackBuilder, GetOrSetOptions, RustoCache,
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
}

impl TestData {
    fn new(id: u64) -> Self {
        Self {
            id,
            name: format!("test_item_{}", id),
            data: vec![0u8; 1024], // 1KB of data
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

fn bench_memory_get_or_set(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let cache = rt.block_on(create_memory_cache());

    c.bench_function("RustoCache_L1_GetOrSet", |b| {
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

    c.bench_function("RustoCache_L1_Get", |b| {
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

    c.bench_function("RustoCache_L1_Set", |b| {
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

criterion_group!(
    benches,
    bench_memory_get_or_set,
    bench_memory_get,
    bench_memory_set
);
criterion_main!(benches);
