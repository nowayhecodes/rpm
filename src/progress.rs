use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast;

#[derive(Debug, Clone)]
pub enum ProgressEvent {
    Started { total: usize },
    Downloaded { package: String },
    Verified { package: String },
    Installed { package: String },
    Failed { package: String, error: String },
    Completed,
}

pub struct ProgressReporter {
    total: AtomicUsize,
    completed: AtomicUsize,
    sender: broadcast::Sender<ProgressEvent>,
}

impl ProgressReporter {
    pub fn new(total: usize) -> (Arc<Self>, broadcast::Receiver<ProgressEvent>) {
        let (sender, receiver) = broadcast::channel(64);

        let reporter = Arc::new(Self {
            total: AtomicUsize::new(total),
            completed: AtomicUsize::new(0),
            sender,
        });

        (reporter, receiver)
    }

    /// Returns the fraction of work completed (0.0–1.0).
    pub fn progress(&self) -> f32 {
        let total = self.total.load(Ordering::Relaxed);
        if total == 0 {
            return 0.0;
        }
        self.completed.load(Ordering::Relaxed) as f32 / total as f32
    }

    /// Adjusts the known total upward as new transitive packages are discovered.
    pub fn add_total(&self, count: usize) {
        self.total.fetch_add(count, Ordering::Relaxed);
    }

    /// Replaces the current total with an exact value.
    pub fn set_total(&self, total: usize) {
        self.total.store(total, Ordering::Relaxed);
    }

    pub fn report_progress(&self, event: ProgressEvent) {
        match &event {
            ProgressEvent::Downloaded { .. }
            | ProgressEvent::Verified { .. }
            | ProgressEvent::Installed { .. } => {
                self.completed.fetch_add(1, Ordering::Relaxed);
            }
            _ => {}
        }

        let _ = self.sender.send(event);
    }

    /// Subscribe to progress events. Call before starting an install to avoid
    /// missing early events.
    pub fn subscribe(&self) -> broadcast::Receiver<ProgressEvent> {
        self.sender.subscribe()
    }
}
