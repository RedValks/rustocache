use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Statistical analyzer for performance metrics with mathematical rigor
pub struct StatisticalAnalyzer {
    samples: VecDeque<f64>,
    max_samples: usize,
    start_time: Instant,
}

impl StatisticalAnalyzer {
    pub fn new(max_samples: usize) -> Self {
        Self {
            samples: VecDeque::with_capacity(max_samples),
            max_samples,
            start_time: Instant::now(),
        }
    }

    /// Add a sample (latency in nanoseconds)
    pub fn add_sample(&mut self, latency_ns: f64) {
        if self.samples.len() >= self.max_samples {
            self.samples.pop_front();
        }
        self.samples.push_back(latency_ns);
    }

    /// Calculate comprehensive performance metrics
    pub fn calculate_metrics(&self) -> PerformanceMetrics {
        if self.samples.is_empty() {
            return PerformanceMetrics::default();
        }

        let mut sorted_samples: Vec<f64> = self.samples.iter().cloned().collect();
        sorted_samples.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let n = sorted_samples.len() as f64;
        let sum: f64 = sorted_samples.iter().sum();
        let mean = sum / n;

        // Calculate variance and standard deviation
        let variance = sorted_samples
            .iter()
            .map(|x| (x - mean).powi(2))
            .sum::<f64>()
            / n;
        let std_dev = variance.sqrt();

        // Calculate percentiles
        let p50 = self.percentile(&sorted_samples, 0.50);
        let p90 = self.percentile(&sorted_samples, 0.90);
        let p95 = self.percentile(&sorted_samples, 0.95);
        let p99 = self.percentile(&sorted_samples, 0.99);
        let p999 = self.percentile(&sorted_samples, 0.999);

        // Calculate confidence intervals (95% CI for mean)
        let confidence_interval = self.calculate_confidence_interval(&sorted_samples, 0.95);

        // Calculate throughput (ops/sec)
        let elapsed = self.start_time.elapsed().as_secs_f64();
        let throughput = if elapsed > 0.0 { n / elapsed } else { 0.0 };

        // Detect anomalies using IQR method
        let q1 = self.percentile(&sorted_samples, 0.25);
        let q3 = self.percentile(&sorted_samples, 0.75);
        let iqr = q3 - q1;
        let lower_bound = q1 - 1.5 * iqr;
        let upper_bound = q3 + 1.5 * iqr;

        let anomalies = sorted_samples
            .iter()
            .filter(|&&x| x < lower_bound || x > upper_bound)
            .count();

        // Calculate coefficient of variation (CV)
        let cv = if mean > 0.0 { std_dev / mean } else { 0.0 };

        PerformanceMetrics {
            sample_count: n as usize,
            mean_latency_ns: mean,
            median_latency_ns: p50,
            std_deviation_ns: std_dev,
            min_latency_ns: sorted_samples[0],
            max_latency_ns: sorted_samples[sorted_samples.len() - 1],
            p90_latency_ns: p90,
            p95_latency_ns: p95,
            p99_latency_ns: p99,
            p999_latency_ns: p999,
            throughput_ops_sec: throughput,
            confidence_interval,
            anomaly_count: anomalies,
            coefficient_variation: cv,
            total_duration: self.start_time.elapsed(),
        }
    }

    /// Calculate percentile value
    fn percentile(&self, sorted_samples: &[f64], percentile: f64) -> f64 {
        if sorted_samples.is_empty() {
            return 0.0;
        }

        let index = (percentile * (sorted_samples.len() - 1) as f64).round() as usize;
        sorted_samples[index.min(sorted_samples.len() - 1)]
    }

    /// Calculate confidence interval for the mean
    fn calculate_confidence_interval(
        &self,
        samples: &[f64],
        confidence: f64,
    ) -> ConfidenceInterval {
        if samples.len() < 2 {
            return ConfidenceInterval {
                lower: 0.0,
                upper: 0.0,
                confidence,
            };
        }

        let n = samples.len() as f64;
        let mean = samples.iter().sum::<f64>() / n;
        let variance = samples.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0);
        let std_error = (variance / n).sqrt();

        // Use t-distribution critical value (approximation for large n)
        let t_critical = if n > 30.0 {
            1.96 // Normal approximation for 95% CI
        } else {
            2.0 + 0.5 / n // Rough t-distribution approximation
        };

        let margin_error = t_critical * std_error;

        ConfidenceInterval {
            lower: mean - margin_error,
            upper: mean + margin_error,
            confidence,
        }
    }

    /// Perform statistical tests for performance regression
    pub fn detect_regression(
        &self,
        baseline_metrics: &PerformanceMetrics,
        threshold: f64,
    ) -> RegressionAnalysis {
        let current_metrics = self.calculate_metrics();

        // Calculate relative change in mean latency
        let latency_change = if baseline_metrics.mean_latency_ns > 0.0 {
            (current_metrics.mean_latency_ns - baseline_metrics.mean_latency_ns)
                / baseline_metrics.mean_latency_ns
        } else {
            0.0
        };

        // Calculate relative change in throughput
        let throughput_change = if baseline_metrics.throughput_ops_sec > 0.0 {
            (current_metrics.throughput_ops_sec - baseline_metrics.throughput_ops_sec)
                / baseline_metrics.throughput_ops_sec
        } else {
            0.0
        };

        // Determine if there's a significant regression
        let is_regression = latency_change > threshold || throughput_change < -threshold;

        // Calculate effect size (Cohen's d for latency)
        let pooled_std = ((baseline_metrics.std_deviation_ns.powi(2)
            + current_metrics.std_deviation_ns.powi(2))
            / 2.0)
            .sqrt();
        let effect_size = if pooled_std > 0.0 {
            (current_metrics.mean_latency_ns - baseline_metrics.mean_latency_ns) / pooled_std
        } else {
            0.0
        };

        RegressionAnalysis {
            is_regression,
            latency_change_percent: latency_change * 100.0,
            throughput_change_percent: throughput_change * 100.0,
            effect_size,
            confidence: if effect_size.abs() > 0.8 {
                "High"
            } else if effect_size.abs() > 0.5 {
                "Medium"
            } else {
                "Low"
            }
            .to_string(),
            baseline_metrics: baseline_metrics.clone(),
            current_metrics,
        }
    }

    /// Calculate statistical power for detecting performance differences
    pub fn calculate_statistical_power(&self, effect_size: f64, _alpha: f64) -> f64 {
        // Simplified power calculation for t-test
        let n = self.samples.len() as f64;
        if n < 2.0 {
            return 0.0;
        }

        // This is a simplified approximation
        let z_alpha = 1.96; // For alpha = 0.05
        let z_beta = effect_size * (n / 2.0).sqrt() - z_alpha;

        // Approximate power using normal CDF
        if z_beta <= -3.0 {
            0.0
        } else if z_beta >= 3.0 {
            1.0
        } else {
            0.5 + 0.5 * (z_beta / 3.0) // Rough approximation
        }
    }
}

/// Comprehensive performance metrics with statistical rigor
#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    pub sample_count: usize,
    pub mean_latency_ns: f64,
    pub median_latency_ns: f64,
    pub std_deviation_ns: f64,
    pub min_latency_ns: f64,
    pub max_latency_ns: f64,
    pub p90_latency_ns: f64,
    pub p95_latency_ns: f64,
    pub p99_latency_ns: f64,
    pub p999_latency_ns: f64,
    pub throughput_ops_sec: f64,
    pub confidence_interval: ConfidenceInterval,
    pub anomaly_count: usize,
    pub coefficient_variation: f64,
    pub total_duration: Duration,
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self {
            sample_count: 0,
            mean_latency_ns: 0.0,
            median_latency_ns: 0.0,
            std_deviation_ns: 0.0,
            min_latency_ns: 0.0,
            max_latency_ns: 0.0,
            p90_latency_ns: 0.0,
            p95_latency_ns: 0.0,
            p99_latency_ns: 0.0,
            p999_latency_ns: 0.0,
            throughput_ops_sec: 0.0,
            confidence_interval: ConfidenceInterval {
                lower: 0.0,
                upper: 0.0,
                confidence: 0.95,
            },
            anomaly_count: 0,
            coefficient_variation: 0.0,
            total_duration: Duration::from_secs(0),
        }
    }
}

impl PerformanceMetrics {
    /// Convert to human-readable format
    pub fn to_human_readable(&self) -> String {
        format!(
            "Performance Metrics:\n\
             Samples: {}\n\
             Mean Latency: {:.2} μs\n\
             Median Latency: {:.2} μs\n\
             P95 Latency: {:.2} μs\n\
             P99 Latency: {:.2} μs\n\
             Throughput: {:.0} ops/sec\n\
             Std Deviation: {:.2} μs\n\
             CV: {:.2}%\n\
             Anomalies: {}\n\
             95% CI: [{:.2}, {:.2}] μs",
            self.sample_count,
            self.mean_latency_ns / 1000.0,
            self.median_latency_ns / 1000.0,
            self.p95_latency_ns / 1000.0,
            self.p99_latency_ns / 1000.0,
            self.throughput_ops_sec,
            self.std_deviation_ns / 1000.0,
            self.coefficient_variation * 100.0,
            self.anomaly_count,
            self.confidence_interval.lower / 1000.0,
            self.confidence_interval.upper / 1000.0,
        )
    }
}

/// Confidence interval for statistical estimates
#[derive(Debug, Clone)]
pub struct ConfidenceInterval {
    pub lower: f64,
    pub upper: f64,
    pub confidence: f64,
}

/// Regression analysis results
#[derive(Debug)]
pub struct RegressionAnalysis {
    pub is_regression: bool,
    pub latency_change_percent: f64,
    pub throughput_change_percent: f64,
    pub effect_size: f64,
    pub confidence: String,
    pub baseline_metrics: PerformanceMetrics,
    pub current_metrics: PerformanceMetrics,
}

impl RegressionAnalysis {
    pub fn to_report(&self) -> String {
        format!(
            "Regression Analysis Report:\n\
             Regression Detected: {}\n\
             Latency Change: {:.2}%\n\
             Throughput Change: {:.2}%\n\
             Effect Size: {:.3}\n\
             Confidence: {}\n\
             \n\
             Baseline:\n{}\n\
             Current:\n{}",
            self.is_regression,
            self.latency_change_percent,
            self.throughput_change_percent,
            self.effect_size,
            self.confidence,
            self.baseline_metrics.to_human_readable(),
            self.current_metrics.to_human_readable(),
        )
    }
}

/// Advanced statistical tests for cache performance
pub struct AdvancedStatistics;

impl AdvancedStatistics {
    /// Perform Mann-Whitney U test for comparing two distributions
    pub fn mann_whitney_u_test(sample1: &[f64], sample2: &[f64]) -> f64 {
        if sample1.is_empty() || sample2.is_empty() {
            return 0.5; // No difference
        }

        let n1 = sample1.len();
        let n2 = sample2.len();
        let mut u1 = 0;

        for &x in sample1 {
            for &y in sample2 {
                if x < y {
                    u1 += 1;
                } else if x == y {
                    // Handle ties
                    u1 += 1; // Simplified tie handling
                }
            }
        }

        let u2 = n1 * n2 - u1;
        let u = u1.min(u2);

        // Convert to p-value approximation
        let mean_u = (n1 * n2) as f64 / 2.0;
        let std_u = ((n1 * n2 * (n1 + n2 + 1)) as f64 / 12.0).sqrt();

        if std_u == 0.0 {
            return 0.5;
        }

        let z = (u as f64 - mean_u) / std_u;

        // Rough p-value approximation
        if z.abs() > 2.58 {
            0.01 // p < 0.01
        } else if z.abs() > 1.96 {
            0.05 // p < 0.05
        } else {
            0.1 // p >= 0.05
        }
    }

    /// Calculate autocorrelation to detect patterns in latency
    pub fn autocorrelation(samples: &[f64], lag: usize) -> f64 {
        if samples.len() <= lag {
            return 0.0;
        }

        let n = samples.len() - lag;
        let mean: f64 = samples.iter().sum::<f64>() / samples.len() as f64;

        let mut numerator = 0.0;
        let mut denominator = 0.0;

        for i in 0..n {
            let x_i = samples[i] - mean;
            let x_i_lag = samples[i + lag] - mean;
            numerator += x_i * x_i_lag;
        }

        for &sample in samples {
            let x = sample - mean;
            denominator += x * x;
        }

        if denominator == 0.0 {
            0.0
        } else {
            numerator / denominator
        }
    }

    /// Detect performance anomalies using isolation forest approximation
    pub fn detect_anomalies(samples: &[f64], contamination: f64) -> Vec<usize> {
        if samples.is_empty() {
            return Vec::new();
        }

        let mut sorted_with_indices: Vec<(f64, usize)> = samples
            .iter()
            .enumerate()
            .map(|(i, &val)| (val, i))
            .collect();
        sorted_with_indices.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

        let anomaly_count = (samples.len() as f64 * contamination).ceil() as usize;
        let mut anomalies = Vec::new();

        // Simple approach: mark extreme values as anomalies
        for &(_, idx) in sorted_with_indices
            .iter()
            .take(anomaly_count.min(samples.len()))
        {
            anomalies.push(idx);
        }

        for &(_, idx) in sorted_with_indices
            .iter()
            .skip(samples.len().saturating_sub(anomaly_count))
        {
            anomalies.push(idx);
        }

        anomalies.sort();
        anomalies.dedup();
        anomalies
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_statistical_analyzer() {
        let mut analyzer = StatisticalAnalyzer::new(1000);

        // Add some sample data
        for i in 0..100 {
            analyzer.add_sample(1000.0 + i as f64 * 10.0); // 1000-1990 ns
        }

        let metrics = analyzer.calculate_metrics();
        assert_eq!(metrics.sample_count, 100);
        assert!(metrics.mean_latency_ns > 1000.0);
        assert!(metrics.throughput_ops_sec > 0.0);
        assert!(metrics.confidence_interval.lower < metrics.confidence_interval.upper);
    }

    #[test]
    fn test_percentile_calculation() {
        let analyzer = StatisticalAnalyzer::new(1000);
        let samples = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];

        assert_eq!(analyzer.percentile(&samples, 0.5), 5.0);
        assert_eq!(analyzer.percentile(&samples, 0.9), 9.0);
        assert_eq!(analyzer.percentile(&samples, 1.0), 10.0);
    }

    #[test]
    fn test_regression_detection() {
        let mut analyzer = StatisticalAnalyzer::new(1000);

        // Baseline performance
        for _ in 0..100 {
            analyzer.add_sample(1000.0);
        }
        let baseline = analyzer.calculate_metrics();

        // Degraded performance
        analyzer = StatisticalAnalyzer::new(1000);
        for _ in 0..100 {
            analyzer.add_sample(1500.0); // 50% slower
        }

        let regression = analyzer.detect_regression(&baseline, 0.1); // 10% threshold
        assert!(regression.is_regression);
        assert!(regression.latency_change_percent > 40.0);
    }

    #[test]
    fn test_mann_whitney_u_test() {
        let sample1 = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let sample2 = vec![6.0, 7.0, 8.0, 9.0, 10.0];

        let p_value = AdvancedStatistics::mann_whitney_u_test(&sample1, &sample2);
        assert!(p_value <= 0.05); // Should detect significant difference
    }

    #[test]
    fn test_autocorrelation() {
        let samples = vec![1.0, 2.0, 1.0, 2.0, 1.0, 2.0]; // Alternating pattern
        let autocorr = AdvancedStatistics::autocorrelation(&samples, 1);
        assert!(autocorr < 0.0); // Negative correlation at lag 1
    }
}
