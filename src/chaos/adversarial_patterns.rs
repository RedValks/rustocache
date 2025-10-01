use fastrand;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Adversarial access patterns designed to stress-test cache performance
#[derive(Debug, Clone)]
pub enum AdversarialPattern {
    /// All requests hit the same key (worst case for LRU)
    Hotspot { key: String },
    /// Sequential access that defeats LRU caching
    Sequential { start: u64, end: u64 },
    /// Random access with no locality
    Random { key_space: u64 },
    /// Thundering herd - many concurrent requests for the same missing key
    ThunderingHerd { key: String, concurrency: usize },
    /// Cache stampede - coordinated invalidation followed by requests
    CacheStampede { keys: Vec<String> },
    /// Pathological LRU - access pattern that causes maximum evictions
    PathologicalLru { cache_size: usize },
    /// Memory bomb - large values that exhaust memory
    MemoryBomb { value_size: usize, count: usize },
    /// Zipfian distribution - realistic but skewed access pattern
    Zipfian { key_space: u64, alpha: f64 },
    /// Adversarial TTL - keys that expire at the worst possible time
    AdversarialTtl { base_ttl: Duration },
}

/// Access pattern generator for creating realistic workloads
pub struct AccessPattern {
    pub key: String,
    pub operation: Operation,
    pub timestamp: Instant,
    pub expected_result: ExpectedResult,
}

#[derive(Debug, Clone)]
pub enum Operation {
    Get,
    Set {
        value: Vec<u8>,
        ttl: Option<Duration>,
    },
    Delete,
    GetOrSet {
        factory_cost: Duration,
    },
}

#[derive(Debug, Clone)]
pub enum ExpectedResult {
    Hit,
    Miss,
    Error,
    Unknown,
}

/// Pattern generator that creates adversarial access patterns
pub struct PatternGenerator {
    pattern: AdversarialPattern,
    state: PatternState,
}

#[derive(Debug)]
struct PatternState {
    counter: u64,
    zipf_cache: Option<ZipfCache>,
    last_access_time: Instant,
}

#[derive(Debug)]
struct ZipfCache {
    probabilities: Vec<f64>,
    cumulative: Vec<f64>,
}

impl PatternGenerator {
    pub fn new(pattern: AdversarialPattern) -> Self {
        let mut state = PatternState {
            counter: 0,
            zipf_cache: None,
            last_access_time: Instant::now(),
        };

        // Pre-compute Zipfian distribution if needed
        if let AdversarialPattern::Zipfian { key_space, alpha } = &pattern {
            state.zipf_cache = Some(Self::compute_zipf_distribution(*key_space, *alpha));
        }

        Self { pattern, state }
    }

    /// Generate the next access pattern
    pub fn next_pattern(&mut self) -> AccessPattern {
        self.state.counter += 1;
        let now = Instant::now();

        let (key, operation, expected) = match &self.pattern {
            AdversarialPattern::Hotspot { key } => {
                (key.clone(), Operation::Get, ExpectedResult::Hit)
            }

            AdversarialPattern::Sequential { start, end } => {
                let key_id = start + (self.state.counter % (end - start));
                let key = format!("seq_{}", key_id);
                (key, Operation::Get, ExpectedResult::Miss)
            }

            AdversarialPattern::Random { key_space } => {
                let key_id = fastrand::u64(0..*key_space);
                let key = format!("random_{}", key_id);
                (key, Operation::Get, ExpectedResult::Unknown)
            }

            AdversarialPattern::ThunderingHerd { key, .. } => (
                key.clone(),
                Operation::GetOrSet {
                    factory_cost: Duration::from_millis(100),
                },
                ExpectedResult::Miss,
            ),

            AdversarialPattern::CacheStampede { keys } => {
                let key = keys[self.state.counter as usize % keys.len()].clone();
                if self.state.counter % 100 == 0 {
                    // Periodically invalidate
                    (key, Operation::Delete, ExpectedResult::Hit)
                } else {
                    (key, Operation::Get, ExpectedResult::Miss)
                }
            }

            AdversarialPattern::PathologicalLru { cache_size } => {
                // Access pattern that causes maximum LRU evictions
                let key_id = self.state.counter % (*cache_size as u64 + 1);
                let key = format!("lru_{}", key_id);
                (key, Operation::Get, ExpectedResult::Miss)
            }

            AdversarialPattern::MemoryBomb { value_size, count } => {
                let key_id = self.state.counter % (*count as u64);
                let key = format!("bomb_{}", key_id);
                let value = vec![0u8; *value_size];
                (
                    key,
                    Operation::Set {
                        value,
                        ttl: Some(Duration::from_secs(60)),
                    },
                    ExpectedResult::Unknown,
                )
            }

            AdversarialPattern::Zipfian { key_space, .. } => {
                let key_id = self.sample_zipfian(*key_space);
                let key = format!("zipf_{}", key_id);
                (key, Operation::Get, ExpectedResult::Unknown)
            }

            AdversarialPattern::AdversarialTtl { base_ttl } => {
                let key = format!("ttl_{}", self.state.counter);
                // TTL that expires right when we need it most
                let jitter = Duration::from_millis(fastrand::u64(1..=100));
                let ttl = if base_ttl > &jitter {
                    *base_ttl - jitter
                } else {
                    *base_ttl
                };
                (
                    key,
                    Operation::Set {
                        value: b"value".to_vec(),
                        ttl: Some(ttl),
                    },
                    ExpectedResult::Unknown,
                )
            }
        };

        AccessPattern {
            key,
            operation,
            timestamp: now,
            expected_result: expected,
        }
    }

    /// Generate a batch of access patterns
    pub fn next_batch(&mut self, count: usize) -> Vec<AccessPattern> {
        (0..count).map(|_| self.next_pattern()).collect()
    }

    /// Generate patterns for thundering herd scenario
    pub fn thundering_herd_batch(&mut self) -> Vec<AccessPattern> {
        if let AdversarialPattern::ThunderingHerd { key, concurrency } = &self.pattern {
            let mut patterns = Vec::with_capacity(*concurrency);
            let now = Instant::now();

            for _ in 0..*concurrency {
                patterns.push(AccessPattern {
                    key: key.clone(),
                    operation: Operation::GetOrSet {
                        factory_cost: Duration::from_millis(100),
                    },
                    timestamp: now,
                    expected_result: ExpectedResult::Miss,
                });
            }

            patterns
        } else {
            vec![self.next_pattern()]
        }
    }

    /// Compute Zipfian distribution probabilities
    fn compute_zipf_distribution(n: u64, alpha: f64) -> ZipfCache {
        let mut probabilities = Vec::with_capacity(n as usize);
        let mut sum = 0.0;

        // Compute unnormalized probabilities
        for i in 1..=n {
            let prob = 1.0 / (i as f64).powf(alpha);
            probabilities.push(prob);
            sum += prob;
        }

        // Normalize and compute cumulative distribution
        let mut cumulative = Vec::with_capacity(n as usize);
        let mut cum_sum = 0.0;

        for prob in &mut probabilities {
            *prob /= sum;
            cum_sum += *prob;
            cumulative.push(cum_sum);
        }

        ZipfCache {
            probabilities,
            cumulative,
        }
    }

    /// Sample from Zipfian distribution
    fn sample_zipfian(&self, key_space: u64) -> u64 {
        if let Some(zipf_cache) = &self.state.zipf_cache {
            let r = fastrand::f64();

            // Binary search in cumulative distribution
            match zipf_cache.cumulative.binary_search_by(|&x| {
                if x < r {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Greater
                }
            }) {
                Ok(idx) => idx as u64,
                Err(idx) => idx as u64,
            }
        } else {
            fastrand::u64(0..key_space)
        }
    }

    /// Create a pattern that maximizes cache misses
    pub fn worst_case_misses(cache_size: usize) -> Self {
        Self::new(AdversarialPattern::PathologicalLru { cache_size })
    }

    /// Create a pattern that causes memory pressure
    pub fn memory_pressure(value_size: usize, count: usize) -> Self {
        Self::new(AdversarialPattern::MemoryBomb { value_size, count })
    }

    /// Create a realistic but challenging access pattern
    pub fn realistic_adversarial(key_space: u64) -> Self {
        Self::new(AdversarialPattern::Zipfian {
            key_space,
            alpha: 1.2, // Highly skewed distribution
        })
    }
}

/// Workload analyzer that detects pathological patterns
pub struct WorkloadAnalyzer {
    access_history: Vec<AccessPattern>,
    key_frequencies: HashMap<String, u64>,
    temporal_locality: f64,
    spatial_locality: f64,
}

impl Default for WorkloadAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkloadAnalyzer {
    pub fn new() -> Self {
        Self {
            access_history: Vec::new(),
            key_frequencies: HashMap::new(),
            temporal_locality: 0.0,
            spatial_locality: 0.0,
        }
    }

    /// Record an access pattern
    pub fn record_access(&mut self, pattern: AccessPattern) {
        *self.key_frequencies.entry(pattern.key.clone()).or_insert(0) += 1;
        self.access_history.push(pattern);

        // Keep only recent history to avoid memory bloat
        if self.access_history.len() > 10000 {
            self.access_history.drain(0..5000);
        }

        self.update_locality_metrics();
    }

    /// Analyze if the workload is adversarial
    pub fn is_adversarial(&self) -> bool {
        self.has_hotspot()
            || self.has_poor_temporal_locality()
            || self.has_poor_spatial_locality()
            || self.has_thundering_herd_pattern()
    }

    /// Check for hotspot patterns (single key getting >50% of requests)
    fn has_hotspot(&self) -> bool {
        if let Some(max_freq) = self.key_frequencies.values().max() {
            let total_accesses = self.access_history.len() as u64;
            *max_freq as f64 / total_accesses as f64 > 0.5
        } else {
            false
        }
    }

    /// Check for poor temporal locality
    fn has_poor_temporal_locality(&self) -> bool {
        self.temporal_locality < 0.3
    }

    /// Check for poor spatial locality  
    fn has_poor_spatial_locality(&self) -> bool {
        self.spatial_locality < 0.3
    }

    /// Check for thundering herd patterns
    fn has_thundering_herd_pattern(&self) -> bool {
        // Look for many concurrent requests to the same key
        if self.access_history.len() < 10 {
            return false;
        }

        let recent = &self.access_history[self.access_history.len() - 10..];
        let mut key_counts = HashMap::new();

        for pattern in recent {
            *key_counts.entry(&pattern.key).or_insert(0) += 1;
        }

        key_counts.values().any(|&count| count >= 5)
    }

    /// Update locality metrics
    fn update_locality_metrics(&mut self) {
        if self.access_history.len() < 2 {
            return;
        }

        // Simple temporal locality: how often do we re-access recent keys?
        let window_size = std::cmp::min(100, self.access_history.len());
        let recent_window = &self.access_history[self.access_history.len() - window_size..];

        let mut unique_keys = std::collections::HashSet::new();
        for pattern in recent_window {
            unique_keys.insert(&pattern.key);
        }

        self.temporal_locality = 1.0 - (unique_keys.len() as f64 / window_size as f64);

        // Simple spatial locality: how often are consecutive accesses to similar keys?
        let mut similar_consecutive = 0;
        for i in 1..recent_window.len() {
            if self.keys_are_similar(&recent_window[i - 1].key, &recent_window[i].key) {
                similar_consecutive += 1;
            }
        }

        self.spatial_locality = similar_consecutive as f64 / (recent_window.len() - 1) as f64;
    }

    /// Check if two keys are spatially similar (simple heuristic)
    fn keys_are_similar(&self, key1: &str, key2: &str) -> bool {
        // Simple similarity: same prefix or consecutive numbers
        if key1.len() >= 3 && key2.len() >= 3 {
            key1[..3] == key2[..3]
        } else {
            false
        }
    }

    /// Get workload statistics
    pub fn get_stats(&self) -> WorkloadStats {
        WorkloadStats {
            total_accesses: self.access_history.len(),
            unique_keys: self.key_frequencies.len(),
            temporal_locality: self.temporal_locality,
            spatial_locality: self.spatial_locality,
            hotspot_ratio: self.get_hotspot_ratio(),
            is_adversarial: self.is_adversarial(),
        }
    }

    fn get_hotspot_ratio(&self) -> f64 {
        if let Some(max_freq) = self.key_frequencies.values().max() {
            *max_freq as f64 / self.access_history.len() as f64
        } else {
            0.0
        }
    }
}

#[derive(Debug)]
pub struct WorkloadStats {
    pub total_accesses: usize,
    pub unique_keys: usize,
    pub temporal_locality: f64,
    pub spatial_locality: f64,
    pub hotspot_ratio: f64,
    pub is_adversarial: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hotspot_pattern() {
        let mut generator = PatternGenerator::new(AdversarialPattern::Hotspot {
            key: "hot_key".to_string(),
        });

        let pattern = generator.next_pattern();
        assert_eq!(pattern.key, "hot_key");
        assert!(matches!(pattern.operation, Operation::Get));
    }

    #[test]
    fn test_sequential_pattern() {
        let mut generator =
            PatternGenerator::new(AdversarialPattern::Sequential { start: 0, end: 10 });

        let patterns: Vec<_> = (0..5).map(|_| generator.next_pattern()).collect();

        for (i, pattern) in patterns.iter().enumerate() {
            assert_eq!(pattern.key, format!("seq_{}", i));
        }
    }

    #[test]
    fn test_workload_analyzer() {
        let mut analyzer = WorkloadAnalyzer::new();

        // Simulate hotspot pattern
        for _ in 0..100 {
            analyzer.record_access(AccessPattern {
                key: "hot_key".to_string(),
                operation: Operation::Get,
                timestamp: Instant::now(),
                expected_result: ExpectedResult::Hit,
            });
        }

        let stats = analyzer.get_stats();
        assert!(stats.is_adversarial);
        assert!(stats.hotspot_ratio > 0.9);
    }

    #[test]
    fn test_zipfian_distribution() {
        let mut generator = PatternGenerator::new(AdversarialPattern::Zipfian {
            key_space: 100,
            alpha: 1.0,
        });

        let patterns: Vec<_> = (0..1000).map(|_| generator.next_pattern()).collect();

        // Should generate valid keys
        for pattern in patterns {
            assert!(pattern.key.starts_with("zipf_"));
        }
    }
}
