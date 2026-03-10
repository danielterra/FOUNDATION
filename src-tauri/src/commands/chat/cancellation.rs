use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tokio::sync::oneshot;

pub struct AiCancellationState {
    cancelled: AtomicBool,
    cancel_tx: Mutex<Option<oneshot::Sender<()>>>,
}

impl AiCancellationState {
    pub fn new() -> Self {
        Self {
            cancelled: AtomicBool::new(false),
            cancel_tx: Mutex::new(None),
        }
    }

    /// Call at the start of a new AI execution. Resets the flag and returns a
    /// receiver that resolves immediately when `cancel()` is called.
    pub fn begin(&self) -> oneshot::Receiver<()> {
        let (tx, rx) = oneshot::channel();
        *self.cancel_tx.lock().unwrap() = Some(tx);
        self.cancelled.store(false, Ordering::SeqCst);
        rx
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        if let Some(tx) = self.cancel_tx.lock().unwrap().take() {
            let _ = tx.send(());
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}
