use rustocache::drivers::MemoryDriverBuilder;
use rustocache::{CacheProvider, GetOrSetOptions, RustoCache};
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone, Debug, PartialEq)]
struct User {
    id: u64,
    name: String,
    email: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing for logging
    tracing_subscriber::fmt::init();

    println!("🦀 RustoCache Basic Usage Example");
    println!("================================");

    // Create a high-performance memory cache
    let memory_driver = Arc::new(
        MemoryDriverBuilder::new()
            .max_entries(1_000)
            .serialize(false) // Zero-copy for maximum performance
            .default_ttl(Duration::from_secs(300))
            .build(),
    );

    let cache_stack = rustocache::CacheStackBuilder::new("example")
        .with_l1_driver(memory_driver)
        .build();

    let cache = RustoCache::new(cache_stack);

    println!("\n1. Testing get_or_set with factory function:");

    // Simulate expensive database lookup
    let user = cache
        .get_or_set(
            "user:123",
            || async {
                println!("   📡 Simulating database fetch...");
                tokio::time::sleep(Duration::from_millis(50)).await;
                Ok(User {
                    id: 123,
                    name: "John Doe".to_string(),
                    email: "john@example.com".to_string(),
                })
            },
            GetOrSetOptions {
                ttl: Some(Duration::from_secs(60)),
                ..Default::default()
            },
        )
        .await?;

    println!("   ✅ Retrieved user: {:?}", user);

    // Second call should hit cache
    println!("\n2. Testing cache hit (should be instant):");
    let start = std::time::Instant::now();

    let cached_user = cache
        .get_or_set(
            "user:123",
            || async {
                println!("   ❌ This should not be called!");
                Ok(User {
                    id: 999,
                    name: "Should not see this".to_string(),
                    email: "error@example.com".to_string(),
                })
            },
            GetOrSetOptions::default(),
        )
        .await?;

    let duration = start.elapsed();
    println!("   ⚡ Cache hit in {:?}", duration);
    println!("   ✅ Cached user: {:?}", cached_user);

    // Test direct cache operations
    println!("\n3. Testing direct cache operations:");

    let jane = User {
        id: 456,
        name: "Jane Smith".to_string(),
        email: "jane@example.com".to_string(),
    };

    cache
        .set("user:456", jane.clone(), Some(Duration::from_secs(30)))
        .await?;
    println!("   ✅ Set user:456");

    let retrieved = cache.get("user:456").await?;
    println!("   ✅ Retrieved: {:?}", retrieved);

    // Test cache statistics
    println!("\n4. Cache Statistics:");
    let stats = cache.get_stats().await;
    println!("   📊 L1 Hits: {}", stats.l1_hits);
    println!("   📊 L1 Misses: {}", stats.l1_misses);
    println!("   📊 L1 Hit Rate: {:.2}%", stats.l1_hit_rate() * 100.0);
    println!("   📊 Total Sets: {}", stats.sets);

    // Performance test
    println!("\n5. Performance Test (10,000 operations):");
    let start = std::time::Instant::now();

    for i in 0..10_000 {
        let key = format!("perf_test:{}", i % 100); // Reuse keys for cache hits
        cache
            .get_or_set(
                &key,
                || async {
                    Ok(User {
                        id: i,
                        name: format!("User {}", i),
                        email: format!("user{}@example.com", i),
                    })
                },
                GetOrSetOptions::default(),
            )
            .await?;
    }

    let duration = start.elapsed();
    let ops_per_sec = 10_000.0 / duration.as_secs_f64();

    println!("   ⚡ Completed 10,000 operations in {:?}", duration);
    println!("   🚀 Performance: {:.0} ops/sec", ops_per_sec);

    // Final stats
    let final_stats = cache.get_stats().await;
    println!("\n6. Final Statistics:");
    println!("   📊 Total L1 Hits: {}", final_stats.l1_hits);
    println!("   📊 Total L1 Misses: {}", final_stats.l1_misses);
    println!(
        "   📊 Overall Hit Rate: {:.2}%",
        final_stats.hit_rate() * 100.0
    );

    println!("\n🎉 RustoCache example completed successfully!");

    Ok(())
}
