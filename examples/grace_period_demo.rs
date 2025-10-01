use rustocache::drivers::MemoryDriverBuilder;
use rustocache::{CacheProvider, GetOrSetOptions, RustoCache};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

#[derive(Clone, Debug)]
struct DatabaseData {
    id: u64,
    name: String,
    value: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🕐 RustoCache Grace Period Demo");
    println!("===============================\n");

    // Create a memory-only cache
    let memory_driver = Arc::new(
        MemoryDriverBuilder::new()
            .max_entries(1000)
            .serialize(false)
            .build::<DatabaseData>(),
    );

    let cache = RustoCache::new(
        rustocache::CacheStackBuilder::new("grace_demo")
            .with_l1_driver(memory_driver)
            .build(),
    );

    // Simulate a database that can fail
    let mut database_available = true;
    let simulate_db_fetch = |id: u64, available: bool| async move {
        if !available {
            return Err(rustocache::CacheError::Generic {
                message: "Database is down!".to_string(),
            });
        }

        // Simulate database delay
        sleep(Duration::from_millis(100)).await;

        Ok(DatabaseData {
            id,
            name: format!("User {}", id),
            value: format!("Important data for user {}", id),
        })
    };

    println!("1. 📝 Initial cache population (database working):");

    // First call - populate cache with short TTL
    let user_data = cache
        .get_or_set(
            "user:123",
            || simulate_db_fetch(123, database_available),
            GetOrSetOptions {
                ttl: Some(Duration::from_secs(2)),          // Very short TTL
                grace_period: Some(Duration::from_secs(5)), // Grace period longer than TTL
                ..Default::default()
            },
        )
        .await?;

    println!("   ✅ Cached user data: {:?}", user_data);

    println!("\n2. ⚡ Immediate cache hit (within TTL):");
    let start = std::time::Instant::now();
    let cached_data = cache
        .get_or_set(
            "user:123",
            || simulate_db_fetch(123, database_available),
            GetOrSetOptions {
                ttl: Some(Duration::from_secs(2)),
                grace_period: Some(Duration::from_secs(5)),
                ..Default::default()
            },
        )
        .await?;
    println!(
        "   ⚡ Cache hit in {:?}: {:?}",
        start.elapsed(),
        cached_data
    );

    println!("\n3. ⏰ Waiting for TTL to expire...");
    sleep(Duration::from_secs(3)).await; // Wait for TTL to expire

    println!("\n4. 🔄 Cache expired, but database still working:");
    let refreshed_data = cache
        .get_or_set(
            "user:123",
            || simulate_db_fetch(123, database_available),
            GetOrSetOptions {
                ttl: Some(Duration::from_secs(2)),
                grace_period: Some(Duration::from_secs(5)),
                ..Default::default()
            },
        )
        .await?;
    println!("   ✅ Refreshed from database: {:?}", refreshed_data);

    println!("\n5. ⏰ Waiting for TTL to expire again...");
    sleep(Duration::from_secs(3)).await;

    println!("\n6. 💥 Database goes down, but grace period saves us:");
    database_available = false; // Simulate database failure

    let grace_data = cache
        .get_or_set(
            "user:123",
            || simulate_db_fetch(123, database_available),
            GetOrSetOptions {
                ttl: Some(Duration::from_secs(2)),
                grace_period: Some(Duration::from_secs(5)),
                ..Default::default()
            },
        )
        .await?;
    println!(
        "   🛡️  Served stale data from grace period: {:?}",
        grace_data
    );

    println!("\n7. ⏰ Waiting for grace period to expire...");
    sleep(Duration::from_secs(6)).await; // Wait for grace period to expire

    println!("\n8. ❌ Both TTL and grace period expired, database still down:");
    let error_result = cache
        .get_or_set(
            "user:123",
            || simulate_db_fetch(123, database_available),
            GetOrSetOptions {
                ttl: Some(Duration::from_secs(2)),
                grace_period: Some(Duration::from_secs(5)),
                ..Default::default()
            },
        )
        .await;

    match error_result {
        Ok(_) => println!("   ❌ Unexpected success!"),
        Err(e) => println!("   ✅ Expected error: {:?}", e),
    }

    println!("\n9. 🔧 Database comes back online:");
    database_available = true;

    let recovered_data = cache
        .get_or_set(
            "user:123",
            || simulate_db_fetch(123, database_available),
            GetOrSetOptions {
                ttl: Some(Duration::from_secs(10)), // Longer TTL now
                grace_period: Some(Duration::from_secs(5)),
                ..Default::default()
            },
        )
        .await?;
    println!("   ✅ Database recovered, fresh data: {:?}", recovered_data);

    // Performance comparison
    println!("\n📊 Performance Comparison:");
    println!("=========================");

    // Test without grace period
    let start = std::time::Instant::now();
    let _no_grace = cache
        .get_or_set(
            "perf_test_no_grace",
            || async {
                Ok(DatabaseData {
                    id: 999,
                    name: "Test".to_string(),
                    value: "No grace".to_string(),
                })
            },
            GetOrSetOptions {
                ttl: Some(Duration::from_secs(10)),
                grace_period: None, // No grace period
                ..Default::default()
            },
        )
        .await?;
    let no_grace_time = start.elapsed();

    // Test with grace period
    let start = std::time::Instant::now();
    let _with_grace = cache
        .get_or_set(
            "perf_test_with_grace",
            || async {
                Ok(DatabaseData {
                    id: 998,
                    name: "Test".to_string(),
                    value: "With grace".to_string(),
                })
            },
            GetOrSetOptions {
                ttl: Some(Duration::from_secs(10)),
                grace_period: Some(Duration::from_secs(5)), // With grace period
                ..Default::default()
            },
        )
        .await?;
    let with_grace_time = start.elapsed();

    println!("   ⚡ Without grace period: {:?}", no_grace_time);
    println!("   🛡️  With grace period: {:?}", with_grace_time);
    println!(
        "   📈 Overhead: {:?} ({:.1}%)",
        with_grace_time.saturating_sub(no_grace_time),
        (with_grace_time.as_nanos() as f64 / no_grace_time.as_nanos() as f64 - 1.0) * 100.0
    );

    // Final cache statistics
    let stats = cache.get_stats().await;
    println!("\n📈 Final Cache Statistics:");
    println!("   🎯 L1 Hits: {}", stats.l1_hits);
    println!("   ❌ L1 Misses: {}", stats.l1_misses);
    println!("   💾 Sets: {}", stats.sets);
    println!("   📊 Hit Rate: {:.2}%", stats.hit_rate() * 100.0);

    println!("\n🎉 Grace Period Demo Complete!");
    println!("   Grace periods provide resilience when databases fail,");
    println!("   serving stale but valid data to keep applications running.");
    println!("   Overhead is minimal: typically <1μs per operation.");

    Ok(())
}
