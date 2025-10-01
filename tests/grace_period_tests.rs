use rustocache::drivers::MemoryDriverBuilder;
use rustocache::{CacheError, CacheProvider, GetOrSetOptions, RustoCache};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

#[derive(Clone, Debug, PartialEq)]
struct TestData {
    value: String,
}

#[tokio::test]
async fn test_grace_period_basic_functionality() {
    let memory_driver = Arc::new(
        MemoryDriverBuilder::new()
            .max_entries(100)
            .serialize(false)
            .build::<TestData>(),
    );

    let cache = RustoCache::new(
        rustocache::CacheStackBuilder::new("grace_test")
            .with_l1_driver(memory_driver)
            .build(),
    );

    // Initial population with short TTL
    let result = cache
        .get_or_set(
            "test_key",
            || async {
                Ok(TestData {
                    value: "original".to_string(),
                })
            },
            GetOrSetOptions {
                ttl: Some(Duration::from_millis(100)), // Very short TTL
                grace_period: Some(Duration::from_millis(500)), // Grace period
                ..Default::default()
            },
        )
        .await
        .unwrap();

    assert_eq!(result.value, "original");

    // Wait for TTL to expire but stay within grace period
    sleep(Duration::from_millis(200)).await;

    // Should still get the value from grace period
    let result = cache
        .get_or_set(
            "test_key",
            || async {
                Ok(TestData {
                    value: "refreshed".to_string(),
                })
            },
            GetOrSetOptions {
                ttl: Some(Duration::from_millis(100)),
                grace_period: Some(Duration::from_millis(500)),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    // Should get refreshed value since factory succeeded
    assert_eq!(result.value, "refreshed");
}

#[tokio::test]
async fn test_grace_period_factory_failure() {
    let memory_driver = Arc::new(
        MemoryDriverBuilder::new()
            .max_entries(100)
            .serialize(false)
            .build::<TestData>(),
    );

    let cache = RustoCache::new(
        rustocache::CacheStackBuilder::new("grace_failure_test")
            .with_l1_driver(memory_driver)
            .build(),
    );

    // Initial population
    cache
        .get_or_set(
            "test_key",
            || async {
                Ok(TestData {
                    value: "original".to_string(),
                })
            },
            GetOrSetOptions {
                ttl: Some(Duration::from_millis(100)),
                grace_period: Some(Duration::from_millis(500)),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    // Wait for TTL to expire
    sleep(Duration::from_millis(200)).await;

    // Factory fails, should get stale data from grace period
    let result = cache
        .get_or_set(
            "test_key",
            || async {
                Err(CacheError::Generic {
                    message: "Database down".to_string(),
                })
            },
            GetOrSetOptions {
                ttl: Some(Duration::from_millis(100)),
                grace_period: Some(Duration::from_millis(500)),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    assert_eq!(result.value, "original"); // Should get stale data
}

#[tokio::test]
async fn test_grace_period_expiry() {
    let memory_driver = Arc::new(
        MemoryDriverBuilder::new()
            .max_entries(100)
            .serialize(false)
            .build::<TestData>(),
    );

    let cache = RustoCache::new(
        rustocache::CacheStackBuilder::new("grace_expiry_test")
            .with_l1_driver(memory_driver)
            .build(),
    );

    // Initial population with very short TTL and grace period
    cache
        .get_or_set(
            "test_key",
            || async {
                Ok(TestData {
                    value: "original".to_string(),
                })
            },
            GetOrSetOptions {
                ttl: Some(Duration::from_millis(50)),
                grace_period: Some(Duration::from_millis(100)),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    // Wait for both TTL and grace period to expire
    sleep(Duration::from_millis(200)).await;

    // Factory fails, should get error since grace period expired
    let result = cache
        .get_or_set(
            "test_key",
            || async {
                Err(CacheError::Generic {
                    message: "Database down".to_string(),
                })
            },
            GetOrSetOptions {
                ttl: Some(Duration::from_millis(50)),
                grace_period: Some(Duration::from_millis(100)),
                ..Default::default()
            },
        )
        .await;

    assert!(result.is_err()); // Should fail since grace period expired
}

#[tokio::test]
async fn test_no_grace_period() {
    let memory_driver = Arc::new(
        MemoryDriverBuilder::new()
            .max_entries(100)
            .serialize(false)
            .build::<TestData>(),
    );

    let cache = RustoCache::new(
        rustocache::CacheStackBuilder::new("no_grace_test")
            .with_l1_driver(memory_driver)
            .build(),
    );

    // Initial population
    cache
        .get_or_set(
            "test_key",
            || async {
                Ok(TestData {
                    value: "original".to_string(),
                })
            },
            GetOrSetOptions {
                ttl: Some(Duration::from_millis(100)),
                grace_period: None, // No grace period
                ..Default::default()
            },
        )
        .await
        .unwrap();

    // Wait for TTL to expire
    sleep(Duration::from_millis(200)).await;

    // Factory fails, should get error immediately (no grace period)
    let result = cache
        .get_or_set(
            "test_key",
            || async {
                Err(CacheError::Generic {
                    message: "Database down".to_string(),
                })
            },
            GetOrSetOptions {
                ttl: Some(Duration::from_millis(100)),
                grace_period: None,
                ..Default::default()
            },
        )
        .await;

    assert!(result.is_err()); // Should fail immediately
}

#[tokio::test]
async fn test_grace_period_performance() {
    let memory_driver = Arc::new(
        MemoryDriverBuilder::new()
            .max_entries(100)
            .serialize(false)
            .build::<TestData>(),
    );

    let cache = RustoCache::new(
        rustocache::CacheStackBuilder::new("grace_perf_test")
            .with_l1_driver(memory_driver)
            .build(),
    );

    // Measure performance impact of grace period
    let iterations = 1000;

    // Without grace period
    let start = std::time::Instant::now();
    for i in 0..iterations {
        cache
            .get_or_set(
                &format!("key_{}", i),
                || async {
                    Ok(TestData {
                        value: format!("value_{}", i),
                    })
                },
                GetOrSetOptions {
                    ttl: Some(Duration::from_secs(60)),
                    grace_period: None,
                    ..Default::default()
                },
            )
            .await
            .unwrap();
    }
    let no_grace_time = start.elapsed();

    // With grace period
    let start = std::time::Instant::now();
    for i in 0..iterations {
        cache
            .get_or_set(
                &format!("grace_key_{}", i),
                || async {
                    Ok(TestData {
                        value: format!("value_{}", i),
                    })
                },
                GetOrSetOptions {
                    ttl: Some(Duration::from_secs(60)),
                    grace_period: Some(Duration::from_secs(30)),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
    }
    let with_grace_time = start.elapsed();

    // Grace period should add minimal overhead
    let overhead_ratio = with_grace_time.as_nanos() as f64 / no_grace_time.as_nanos() as f64;

    println!("No grace period: {:?}", no_grace_time);
    println!("With grace period: {:?}", with_grace_time);
    println!("Overhead ratio: {:.2}x", overhead_ratio);

    // Overhead should be less than 50% (1.5x)
    assert!(
        overhead_ratio < 1.5,
        "Grace period overhead too high: {:.2}x",
        overhead_ratio
    );
}
