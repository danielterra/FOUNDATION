use std::sync::Mutex;
use std::collections::{HashMap, HashSet};
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

/// Tracks which conversations are currently being processed by process_conversation_queue.
/// Prevents concurrent processing loops for the same conversation.
pub struct ConversationProcessingState {
    active: Mutex<HashSet<String>>,
}

impl ConversationProcessingState {
    pub fn new() -> Self {
        Self { active: Mutex::new(HashSet::new()) }
    }

    /// Mark conversation as active. Returns true if it was idle (caller should start processing).
    pub fn try_acquire(&self, conv_id: &str) -> bool {
        self.active.lock().unwrap_or_else(|e| e.into_inner()).insert(conv_id.to_string())
    }

    /// Release the active slot for this conversation.
    pub fn release(&self, conv_id: &str) {
        self.active.lock().unwrap_or_else(|e| e.into_inner()).remove(conv_id);
    }

    pub fn is_active(&self, conv_id: &str) -> bool {
        self.active.lock().unwrap_or_else(|e| e.into_inner()).contains(conv_id)
    }
}
