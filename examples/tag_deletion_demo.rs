use rustocache::drivers::memory::MemoryDriverBuilder;
use rustocache::{CacheProvider, CacheStackBuilder, GetOrSetOptions};
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🏷️  RustoCache Tag-Based Deletion Demo");
    println!("=====================================\n");

    // Create a cache stack with L1 memory driver
    let memory_driver = Arc::new(MemoryDriverBuilder::new().build::<String>());
    let cache = CacheStackBuilder::new("tag_demo")
        .with_l1_driver(memory_driver)
        .build();

    println!("📝 Setting up cache entries with different tags...\n");

    // Set up different cache entries with tags
    let user_profile_options = GetOrSetOptions {
        ttl: Some(Duration::from_secs(300)),
        tags: vec!["user".to_string(), "profile".to_string()],
        grace_period: None,
        timeout: Some(Duration::from_secs(30)),
        refresh_threshold: None,
        stampede_protection: false,
    };

    let user_settings_options = GetOrSetOptions {
        ttl: Some(Duration::from_secs(300)),
        tags: vec!["user".to_string(), "settings".to_string()],
        grace_period: None,
        timeout: Some(Duration::from_secs(30)),
        refresh_threshold: None,
        stampede_protection: false,
    };

    let product_options = GetOrSetOptions {
        ttl: Some(Duration::from_secs(600)),
        tags: vec!["product".to_string(), "catalog".to_string()],
        grace_period: None,
        timeout: Some(Duration::from_secs(30)),
        refresh_threshold: None,
        stampede_protection: false,
    };

    let session_options = GetOrSetOptions {
        ttl: Some(Duration::from_secs(1800)),
        tags: vec!["session".to_string(), "auth".to_string()],
        grace_period: None,
        timeout: Some(Duration::from_secs(30)),
        refresh_threshold: None,
        stampede_protection: false,
    };

    // Populate cache with tagged entries
    cache
        .get_or_set(
            "user:123:profile",
            || async { Ok("John Doe - Software Engineer".to_string()) },
            user_profile_options,
        )
        .await?;

    cache
        .get_or_set(
            "user:123:settings",
            || async { Ok("{\"theme\": \"dark\", \"notifications\": true }".to_string()) },
            user_settings_options,
        )
        .await?;

    cache
        .get_or_set(
            "user:456:profile",
            || async { Ok("Jane Smith - Product Manager".to_string()) },
            GetOrSetOptions {
                ttl: Some(Duration::from_secs(300)),
                tags: vec!["user".to_string()],
                grace_period: None,
                timeout: Some(Duration::from_secs(30)),
                refresh_threshold: None,
                stampede_protection: false,
            },
        )
        .await?;

    cache
        .get_or_set(
            "product:789",
            || async { Ok("Laptop - High Performance Computing".to_string()) },
            product_options,
        )
        .await?;

    cache
        .get_or_set(
            "product:101",
            || async { Ok("Mouse - Wireless Gaming".to_string()) },
            GetOrSetOptions {
                ttl: Some(Duration::from_secs(600)),
                tags: vec!["product".to_string(), "catalog".to_string()],
                grace_period: None,
                timeout: Some(Duration::from_secs(30)),
                refresh_threshold: None,
                stampede_protection: false,
            },
        )
        .await?;

    cache
        .get_or_set(
            "session:abc123",
            || async { Ok("Active session for user:123".to_string()) },
            session_options,
        )
        .await?;

    // Verify all entries exist
    println!("✅ Cache populated with entries:");
    let entries = vec![
        "user:123:profile",
        "user:123:settings",
        "user:456:profile",
        "product:789",
        "product:101",
        "session:abc123",
    ];

    for key in &entries {
        if let Some(value) = cache.get(key).await? {
            println!("   📄 {}: {}", key, value);
        }
    }

    println!("\n🎯 Demonstrating tag-based deletion...\n");

    // Delete all user-related entries
    println!("🗑️  Deleting all entries with 'user' tag...");
    let deleted_count = cache.delete_by_tag(&["user"]).await?;
    println!("   ✅ Deleted {} entries", deleted_count);

    // Verify user entries are gone
    println!("\n📊 Checking remaining entries:");
    for key in &entries {
        match cache.get(key).await? {
            Some(value) => println!("   ✅ {}: {}", key, value),
            None => println!("   ❌ {}: DELETED", key),
        }
    }

    // Delete product entries
    println!("\n🗑️  Deleting all entries with 'product' tag...");
    let deleted_count = cache.delete_by_tag(&["product"]).await?;
    println!("   ✅ Deleted {} entries", deleted_count);

    // Delete by multiple tags (should delete session)
    println!("\n🗑️  Deleting entries with 'session' OR 'auth' tags...");
    let deleted_count = cache.delete_by_tag(&["session", "auth"]).await?;
    println!("   ✅ Deleted {} entries", deleted_count);

    // Final verification - should be empty
    println!("\n📊 Final cache state:");
    let mut remaining = 0;
    for key in &entries {
        match cache.get(key).await? {
            Some(value) => {
                println!("   ✅ {}: {}", key, value);
                remaining += 1;
            }
            None => println!("   ❌ {}: DELETED", key),
        }
    }

    if remaining == 0 {
        println!("\n🎉 All entries successfully deleted using tag-based deletion!");
    } else {
        println!("\n⚠️  {} entries remain in cache", remaining);
    }

    // Show cache statistics
    let stats = cache.get_stats().await;
    println!("\n📈 Cache Statistics:");
    println!("   🎯 L1 Hits: {}", stats.l1_hits);
    println!("   ❌ L1 Misses: {}", stats.l1_misses);
    println!("   💾 Sets: {}", stats.sets);
    println!("   🗑️  Deletes: {}", stats.deletes);
    println!("   📊 Hit Rate: {:.2}%", stats.hit_rate() * 100.0);

    println!("\n🏆 Tag-based deletion demo completed successfully!");
    println!("   This feature allows you to efficiently delete related cache entries");
    println!("   by their semantic tags rather than individual keys.");

    Ok(())
}
