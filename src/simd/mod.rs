/// SIMD-optimized operations for cache performance
pub mod vectorized_ops {
    use super::*;

    /// Check multiple cache entries for expiration using optimized batch operations
    /// This is a simplified version - real SIMD would use proper vector instructions
    pub fn check_expired_batch(timestamps: &[u64], ttls: &[u64], current_time: u64) -> Vec<bool> {
        let mut results = Vec::with_capacity(timestamps.len());

        // Process in chunks for better cache locality
        let chunks = timestamps.chunks_exact(4);
        let remainder = chunks.remainder();

        for (chunk_idx, chunk) in chunks.enumerate() {
            let ttl_start = chunk_idx * 4;
            // Process 4 items at once for better performance
            for (i, &timestamp) in chunk.iter().enumerate() {
                let ttl = ttls.get(ttl_start + i).copied().unwrap_or(0);
                results.push(current_time > timestamp + ttl);
            }
        }

        // Handle remainder
        let remainder_start = timestamps.len() - remainder.len();
        for (i, &timestamp) in remainder.iter().enumerate() {
            let ttl = ttls.get(remainder_start + i).copied().unwrap_or(0);
            results.push(current_time > timestamp + ttl);
        }

        results
    }

    /// Vectorized hash computation for multiple keys
    ///
    /// # Safety
    /// This function is marked unsafe as a placeholder for future SIMD optimizations.
    /// Currently, it only uses safe operations but may be optimized with unsafe SIMD in the future.
    #[cfg(target_arch = "x86_64")]
    pub unsafe fn hash_keys_batch(keys: &[&str]) -> Vec<u64> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hashes = Vec::with_capacity(keys.len());

        // Process keys in batches for better cache locality
        for chunk in keys.chunks(8) {
            for key in chunk {
                let mut hasher = DefaultHasher::new();
                key.hash(&mut hasher);
                hashes.push(hasher.finish());
            }
        }

        hashes
    }

    /// SIMD-optimized string comparison for tag matching
    ///
    /// # Safety
    /// This function is marked unsafe as a placeholder for future SIMD optimizations.
    /// Currently, it only uses safe operations but may be optimized with unsafe SIMD in the future.
    #[cfg(target_arch = "x86_64")]
    pub unsafe fn find_matching_tags(tags: &[String], target_tags: &[&str]) -> Vec<usize> {
        let mut matches = Vec::new();

        for (idx, tag) in tags.iter().enumerate() {
            for target in target_tags {
                if tag == *target {
                    matches.push(idx);
                    break;
                }
            }
        }

        matches
    }

    /// SIMD string equality check for short strings (up to 32 bytes)
    ///
    /// # Safety
    /// This function is marked unsafe as a placeholder for future SIMD optimizations.
    /// Currently, it only uses safe operations but may be optimized with unsafe SIMD in the future.
    #[cfg(target_arch = "x86_64")]
    pub unsafe fn simd_string_equals(a: &[u8], b: &[u8]) -> bool {
        a == b
    }
}

/// Bulk operations using SIMD optimizations
pub fn bulk_hash(keys: &[&str]) -> Vec<u64> {
    #[cfg(target_arch = "x86_64")]
    {
        unsafe { vectorized_ops::hash_keys_batch(keys) }
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        keys.iter()
            .map(|key| {
                let mut hasher = DefaultHasher::new();
                key.hash(&mut hasher);
                hasher.finish()
            })
            .collect()
    }
}

/// Check if expired using SIMD operations
pub fn check_expired_batch(timestamps: &[u64], ttls: &[u64], current_time: u64) -> Vec<bool> {
    vectorized_ops::check_expired_batch(timestamps, ttls, current_time)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bulk_hash() {
        let keys = vec!["key1", "key2", "key3"];
        let hashes = bulk_hash(&keys);
        assert_eq!(hashes.len(), 3);

        // Hashes should be consistent
        let hashes2 = bulk_hash(&keys);
        assert_eq!(hashes, hashes2);
    }

    #[test]
    fn test_simd_string_equals() {
        #[cfg(target_arch = "x86_64")]
        {
            unsafe {
                assert!(vectorized_ops::simd_string_equals(b"hello", b"hello"));
                assert!(!vectorized_ops::simd_string_equals(b"hello", b"world"));
                assert!(!vectorized_ops::simd_string_equals(b"hello", b"hell"));
            }
        }
    }

    #[test]
    fn test_check_expired_batch() {
        let timestamps = vec![1000, 2000, 3000, 4000];
        let ttls = vec![500, 1000, 1500, 2000];
        let current_time = 2500;

        let results = check_expired_batch(&timestamps, &ttls, current_time);

        // 1000 + 500 = 1500 < 2500 (expired)
        // 2000 + 1000 = 3000 > 2500 (not expired)
        // 3000 + 1500 = 4500 > 2500 (not expired)
        // 4000 + 2000 = 6000 > 2500 (not expired)
        assert_eq!(results, vec![true, false, false, false]);
    }
}
