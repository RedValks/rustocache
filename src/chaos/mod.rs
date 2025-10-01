//! Chaos engineering and adversarial testing framework for RustoCache
//!
//! This module provides comprehensive testing capabilities including:
//! - Chaos injection (failures, delays, network partitions)
//! - Adversarial access patterns (hotspots, thundering herd, cache stampede)
//! - Mathematical rigor (statistical analysis, confidence intervals)
//! - Load testing (concurrent access, memory pressure, resource exhaustion)

pub mod adversarial_patterns;
pub mod chaos_driver;
pub mod chaos_injector;
pub mod mathematical_analysis;
// pub mod load_generator; // Temporarily disabled due to trait object issues

pub use adversarial_patterns::{AccessPattern, AdversarialPattern, Operation, PatternGenerator};
pub use chaos_driver::ChaosDriver;
pub use chaos_injector::{ChaosConfig, ChaosInjector, FailureMode};
pub use mathematical_analysis::{ConfidenceInterval, PerformanceMetrics, StatisticalAnalyzer};
// pub use load_generator::{LoadGenerator, LoadPattern, ConcurrencyLevel};
