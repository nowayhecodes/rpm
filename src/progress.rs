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
        let (sender, receiver) = broadcast::channel(32);
        
        let reporter = Arc::new(Self {
            total: AtomicUsize::new(total),
            completed: AtomicUsize::new(0),
            sender,
        });

        (reporter, receiver)
    }

    pub fn progress(&self) -> f32 {
        let completed = self.completed.load(Ordering::Relaxed) as f32;
        let total = self.total.load(Ordering::Relaxed) as f32;
        completed / total
    }

    pub fn report_progress(&self, event: ProgressEvent) {
        match &event {
            ProgressEvent::Downloaded { .. } |
            ProgressEvent::Verified { .. } |
            ProgressEvent::Installed { .. } => {
                self.completed.fetch_add(1, Ordering::Relaxed);
            }
            _ => {}
        }
        
        let _ = self.sender.send(event);
    }
} 