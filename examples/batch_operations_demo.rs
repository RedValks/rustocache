use rustocache::drivers::memory::MemoryDriverBuilder;
use rustocache::{CacheProvider, RustoCache};
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("📦 RustoCache Batch Operations Demo");
    println!("===================================\n");

    // Create a RustoCache instance
    let memory_driver = Arc::new(MemoryDriverBuilder::new().build::<String>());
    let cache = RustoCache::new(
        rustocache::CacheStackBuilder::new("batch_demo")
            .with_l1_driver(memory_driver)
            .build(),
    );

    println!("🚀 Testing set_many operation...");

    // Test set_many
    let entries = vec![
        (
            "user:1",
            "Alice Johnson".to_string(),
            Some(Duration::from_secs(300)),
        ),
        (
            "user:2",
            "Bob Smith".to_string(),
            Some(Duration::from_secs(300)),
        ),
        (
            "user:3",
            "Charlie Brown".to_string(),
            Some(Duration::from_secs(300)),
        ),
        ("product:1", "Laptop".to_string(), None),
        ("product:2", "Mouse".to_string(), None),
    ];

    cache.set_many(&entries).await?;
    println!("   ✅ Set {} entries in batch", entries.len());

    println!("\n📥 Testing get_many operation...");

    // Test get_many
    let keys = vec![
        "user:1",
        "user:2",
        "user:3",
        "product:1",
        "product:2",
        "nonexistent",
    ];
    let results = cache.get_many(&keys).await?;

    println!("   📊 Retrieved {} results:", results.len());
    for (i, result) in results.iter().enumerate() {
        match result {
            Some(value) => println!("      ✅ {}: {}", keys[i], value),
            None => println!("      ❌ {}: NOT FOUND", keys[i]),
        }
    }

    println!("\n🔍 Verifying individual gets work the same...");

    // Verify with individual gets
    for key in &keys[..5] {
        // Skip the nonexistent key
        let individual_result = cache.get(key).await?;
        println!("   📄 {}: {:?}", key, individual_result);
    }

    // Test performance comparison
    println!("\n⚡ Performance comparison...");

    let test_keys: Vec<String> = (1..=100).map(|i| format!("perf_test:{}", i)).collect();
    let test_entries: Vec<(&str, String, Option<Duration>)> = test_keys
        .iter()
        .map(|k| (k.as_str(), format!("value_{}", k), None))
        .collect();

    // Batch set
    let start = std::time::Instant::now();
    cache.set_many(&test_entries).await?;
    let batch_set_time = start.elapsed();

    // Individual sets (for comparison)
    let individual_keys: Vec<String> = (101..=200).map(|i| format!("perf_test:{}", i)).collect();
    let start = std::time::Instant::now();
    for key in &individual_keys {
        cache.set(key, format!("value_{}", key), None).await?;
    }
    let individual_set_time = start.elapsed();

    println!("   📊 Batch set (100 items): {:?}", batch_set_time);
    println!(
        "   📊 Individual sets (100 items): {:?}",
        individual_set_time
    );

    // Batch get
    let test_key_refs: Vec<&str> = test_keys.iter().map(|s| s.as_str()).collect();
    let start = std::time::Instant::now();
    let _batch_results = cache.get_many(&test_key_refs).await?;
    let batch_get_time = start.elapsed();

    // Individual gets
    let start = std::time::Instant::now();
    for key in &test_keys {
        let _ = cache.get(key).await?;
    }
    let individual_get_time = start.elapsed();

    println!("   📊 Batch get (100 items): {:?}", batch_get_time);
    println!(
        "   📊 Individual gets (100 items): {:?}",
        individual_get_time
    );

    // Show cache statistics
    let stats = cache.get_stats().await;
    println!("\n📈 Final Cache Statistics:");
    println!("   🎯 L1 Hits: {}", stats.l1_hits);
    println!("   ❌ L1 Misses: {}", stats.l1_misses);
    println!("   💾 Sets: {}", stats.sets);
    println!("   🗑️  Deletes: {}", stats.deletes);
    println!("   📊 Hit Rate: {:.2}%", stats.hit_rate() * 100.0);

    println!("\n🎉 Batch operations demo completed successfully!");
    println!("   The get_many and set_many methods provide convenient batch operations");
    println!("   while maintaining the same interface as individual operations.");

    Ok(())
}
