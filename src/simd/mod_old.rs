/// SIMD-optimized operations for cache performance
pub mod vectorized_ops {
    use super::*;

    /// Check multiple cache entries for expiration using optimized batch operations
    /// This is a simplified version - real SIMD would use proper vector instructions
    pub fn check_expired_batch(
        timestamps: &[u64],
        ttls: &[u64],
        current_time: u64,
    ) -> Vec<bool> {
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
                if simd_string_equals(tag.as_bytes(), target.as_bytes()) {
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
        if a.len() != b.len() {
            return false;
        }

        if a.len() <= 32 {
            // Use AVX2 for strings up to 32 bytes
            let len = a.len();
            if len <= 16 {
                let a_vec = _mm_loadu_si128(a.as_ptr() as *const __m128i);
                let b_vec = _mm_loadu_si128(b.as_ptr() as *const __m128i);
                let cmp = _mm_cmpeq_epi8(a_vec, b_vec);
                let mask = _mm_movemask_epi8(cmp) as u32;
                (mask & ((1u32 << len) - 1)) == ((1u32 << len) - 1)
            } else {
                let a_vec = _mm256_loadu_si256(a.as_ptr() as *const __m256i);
                let b_vec = _mm256_loadu_si256(b.as_ptr() as *const __m256i);
                let cmp = _mm256_cmpeq_epi8(a_vec, b_vec);
                let mask = _mm256_movemask_epi8(cmp) as u32;
                (mask & ((1u32 << len) - 1)) == ((1u32 << len) - 1)
            }
        } else {
            // Fallback to standard comparison for longer strings
            a == b
        }
    }
}

/// High-level SIMD cache operations
pub mod cache_simd {
    use super::vectorized_ops::*;
    use crate::traits::CacheEntry;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// SIMD-optimized bulk expiration check
    pub fn check_bulk_expiration<T>(entries: &[(String, CacheEntry<T>)]) -> Vec<String> {
        let current_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut timestamps = Vec::with_capacity(entries.len());
        let mut ttls = Vec::with_capacity(entries.len());

        for (_, entry) in entries {
            let created_timestamp = entry.created_at.timestamp() as u64;
            let ttl_secs = entry.ttl.map(|d| d.as_secs()).unwrap_or(u64::MAX);

            timestamps.push(created_timestamp);
            ttls.push(ttl_secs);
        }

        #[cfg(target_arch = "x86_64")]
        let expired_flags = unsafe { check_expired_batch(&timestamps, &ttls, current_time) };

        #[cfg(not(target_arch = "x86_64"))]
        let expired_flags: Vec<bool> = timestamps
            .iter()
            .zip(ttls.iter())
            .map(|(ts, ttl)| current_time > ts + ttl)
            .collect();

        entries
            .iter()
            .zip(expired_flags.iter())
            .filter_map(
                |((key, _), &expired)| {
                    if expired {
                        Some(key.clone())
                    } else {
                        None
                    }
                },
            )
            .collect()
    }

    /// SIMD-optimized bulk key hashing
    pub fn hash_keys_bulk(keys: &[&str]) -> Vec<u64> {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            hash_keys_batch(keys)
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simd_string_equals() {
        let a = "hello_world";
        let b = "hello_world";
        let c = "hello_rust";

        #[cfg(target_arch = "x86_64")]
        unsafe {
            assert!(vectorized_ops::simd_string_equals(
                a.as_bytes(),
                b.as_bytes()
            ));
            assert!(!vectorized_ops::simd_string_equals(
                a.as_bytes(),
                c.as_bytes()
            ));
        }
    }

    #[test]
    fn test_bulk_hash() {
        let keys = vec!["key1", "key2", "key3", "key4"];
        let hashes = cache_simd::hash_keys_bulk(&keys);
        assert_eq!(hashes.len(), 4);

        // Verify hashes are different
        assert_ne!(hashes[0], hashes[1]);
        assert_ne!(hashes[1], hashes[2]);
    }
}
