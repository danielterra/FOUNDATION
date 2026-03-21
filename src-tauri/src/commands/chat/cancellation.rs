use std::sync::Mutex;
use std::collections::HashMap;
use tokio::sync::oneshot;

struct ConvCancel {
    cancelled: bool,
    tx: Option<oneshot::Sender<()>>,
}

pub struct AiCancellationState {
    convs: Mutex<HashMap<String, ConvCancel>>,
}

impl AiCancellationState {
    pub fn new() -> Self {
        Self { convs: Mutex::new(HashMap::new()) }
    }

    /// Call at the start of a new AI execution for a conversation. Resets the cancelled flag
    /// and returns a receiver that resolves immediately when `cancel()` is called.
    pub fn begin(&self, conv_id: &str) -> oneshot::Receiver<()> {
        let (tx, rx) = oneshot::channel();
        let mut map = self.convs.lock().unwrap();
        if let Some(old) = map.get_mut(conv_id) {
            old.cancelled = true;
            if let Some(old_tx) = old.tx.take() {
                let _ = old_tx.send(());
            }
        }
        map.insert(conv_id.to_string(), ConvCancel { cancelled: false, tx: Some(tx) });
        rx
    }

    pub fn cancel(&self, conv_id: &str) {
        let mut map = self.convs.lock().unwrap();
        if let Some(c) = map.get_mut(conv_id) {
            c.cancelled = true;
            if let Some(tx) = c.tx.take() {
                let _ = tx.send(());
            }
        }
    }

    pub fn is_cancelled(&self, conv_id: &str) -> bool {
        self.convs.lock().unwrap()
            .get(conv_id)
            .map_or(false, |c| c.cancelled)
    }
}
