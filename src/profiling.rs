use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::time::Duration;
use log::{info, warn};

pub struct MemoryProfile {
    allocated: Arc<AtomicUsize>,
    peak: Arc<AtomicUsize>,
    threshold: usize,
}

impl MemoryProfile {
    pub fn new(threshold: usize) -> Self {
        let allocated = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        
        let profile = Self {
            allocated: Arc::clone(&allocated),
            peak: Arc::clone(&peak),
            threshold,
        };

        profile.start_monitoring();
        profile
    }

    pub fn allocate(&self, size: usize) {
        let current = self.allocated.fetch_add(size, Ordering::SeqCst);
        let new_total = current + size;
        
        // Update peak if necessary
        let mut current_peak = self.peak.load(Ordering::SeqCst);
        while new_total > current_peak {
            match self.peak.compare_exchange(
                current_peak,
                new_total,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => break,
                Err(actual) => current_peak = actual,
            }
        }

        if new_total > self.threshold {
            warn!(
                "Memory usage exceeded threshold: current={}, threshold={}, peak={}",
                new_total, self.threshold, current_peak
            );
        }
    }

    pub fn deallocate(&self, size: usize) {
        self.allocated.fetch_sub(size, Ordering::SeqCst);
    }

    pub fn current_usage(&self) -> usize {
        self.allocated.load(Ordering::SeqCst)
    }

    pub fn peak_usage(&self) -> usize {
        self.peak.load(Ordering::SeqCst)
    }

    fn start_monitoring(&self) {
        let allocated = Arc::clone(&self.allocated);
        let peak = Arc::clone(&self.peak);
        let threshold = self.threshold;

        tokio::spawn(async move {
            let interval = Duration::from_secs(60);
            loop {
                tokio::time::sleep(interval).await;
                let current = allocated.load(Ordering::SeqCst);
                let peak_usage = peak.load(Ordering::SeqCst);
                
                info!(
                    "Memory usage - Current: {} bytes, Peak: {} bytes, Threshold: {} bytes",
                    current, peak_usage, threshold
                );
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio;

    #[tokio::test]
    async fn test_memory_profiling() {
        let profile = MemoryProfile::new(1000);

        // Test allocation
        profile.allocate(500);
        assert_eq!(profile.current_usage(), 500);

        // Test peak tracking
        profile.allocate(300);
        assert_eq!(profile.current_usage(), 800);
        assert_eq!(profile.peak_usage(), 800);

        // Test deallocation
        profile.deallocate(300);
        assert_eq!(profile.current_usage(), 500);
        assert_eq!(profile.peak_usage(), 800); // Peak should remain unchanged

        // Test threshold warning
        profile.allocate(600); // This should trigger a warning log
        assert_eq!(profile.current_usage(), 1100);
    }
} 