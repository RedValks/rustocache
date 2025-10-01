use rustocache::drivers::MemoryDriverBuilder;
use rustocache::{CacheProvider, GetOrSetOptions, RustoCache};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::sleep;

#[derive(Clone, Debug)]
struct TestData {
    id: u64,
    value: String,
}

static FACTORY_CALL_COUNT: AtomicU64 = AtomicU64::new(0);

async fn expensive_factory(delay_ms: u64) -> Result<TestData, rustocache::CacheError> {
    let call_id = FACTORY_CALL_COUNT.fetch_add(1, Ordering::SeqCst) + 1;
    println!("🏭 Factory #{} starting ({}ms delay)", call_id, delay_ms);

    sleep(Duration::from_millis(delay_ms)).await;

    println!("✅ Factory #{} completed", call_id);

    Ok(TestData {
        id: call_id,
        value: format!("Result from factory {}", call_id),
    })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🛡️  Simple Stampede Protection Demo");
    println!("===================================\n");

    let memory_driver = Arc::new(
        MemoryDriverBuilder::new()
            .max_entries(1000)
            .serialize(false)
            .build::<TestData>(),
    );

    let cache = RustoCache::new(
        rustocache::CacheStackBuilder::new("stampede_demo")
            .with_l1_driver(memory_driver)
            .build(),
    );

    println!("## Test 1: WITHOUT Stampede Protection");
    println!("======================================");

    FACTORY_CALL_COUNT.store(0, Ordering::SeqCst);
    let start = Instant::now();

    // 3 concurrent requests without stampede protection
    let cache1 = cache.clone();
    let cache2 = cache.clone();
    let cache3 = cache.clone();

    let (r1, r2, r3) = tokio::join!(
        cache1.get_or_set(
            "test_key",
            || expensive_factory(10),
            GetOrSetOptions {
                ttl: Some(Duration::from_secs(60)),
                stampede_protection: false,
                ..Default::default()
            },
        ),
        cache2.get_or_set(
            "test_key",
            || expensive_factory(10),
            GetOrSetOptions {
                ttl: Some(Duration::from_secs(60)),
                stampede_protection: false,
                ..Default::default()
            },
        ),
        cache3.get_or_set(
            "test_key",
            || expensive_factory(10),
            GetOrSetOptions {
                ttl: Some(Duration::from_secs(60)),
                stampede_protection: false,
                ..Default::default()
            },
        ),
    );

    let without_time = start.elapsed();
    let without_calls = FACTORY_CALL_COUNT.load(Ordering::SeqCst);

    println!("\nResults WITHOUT stampede protection:");
    println!("  ⏱️  Time: {:?}", without_time);
    println!("  🏭 Factory calls: {} (expected: 3)", without_calls);
    println!("  📊 Results: {} {} {}", r1?.id, r2?.id, r3?.id);

    // Clear cache
    cache.clear().await?;

    println!("\n## Test 2: WITH Stampede Protection");
    println!("===================================");

    FACTORY_CALL_COUNT.store(0, Ordering::SeqCst);
    let start = Instant::now();

    // 3 concurrent requests WITH stampede protection
    let cache1 = cache.clone();
    let cache2 = cache.clone();
    let cache3 = cache.clone();

    let (r1, r2, r3) = tokio::join!(
        cache1.get_or_set(
            "test_key",
            || expensive_factory(10),
            GetOrSetOptions {
                ttl: Some(Duration::from_secs(60)),
                stampede_protection: true,
                ..Default::default()
            },
        ),
        cache2.get_or_set(
            "test_key",
            || expensive_factory(10),
            GetOrSetOptions {
                ttl: Some(Duration::from_secs(60)),
                stampede_protection: true,
                ..Default::default()
            },
        ),
        cache3.get_or_set(
            "test_key",
            || expensive_factory(10),
            GetOrSetOptions {
                ttl: Some(Duration::from_secs(60)),
                stampede_protection: true,
                ..Default::default()
            },
        ),
    );

    let with_time = start.elapsed();
    let with_calls = FACTORY_CALL_COUNT.load(Ordering::SeqCst);

    println!("\nResults WITH stampede protection:");
    println!("  ⏱️  Time: {:?}", with_time);
    println!("  🏭 Factory calls: {} (expected: 1)", with_calls);
    let result1 = r1?;
    let result2 = r2?;
    let result3 = r3?;

    println!("  📊 Results: {} {} {}", result1.id, result2.id, result3.id);

    let all_same = result1.id == result2.id && result2.id == result3.id;
    println!(
        "  🎯 All same result: {}",
        if all_same { "✅ YES" } else { "❌ NO" }
    );

    println!("\n## Summary");
    println!("==========");
    println!(
        "  Without stampede protection: {} factory calls",
        without_calls
    );
    println!("  With stampede protection: {} factory calls", with_calls);
    println!(
        "  Efficiency improvement: {}x",
        without_calls / with_calls.max(1)
    );

    if with_calls == 1 && all_same {
        println!("  🎉 Stampede protection WORKING!");
    } else {
        println!("  ❌ Stampede protection needs debugging");
    }

    Ok(())
}
