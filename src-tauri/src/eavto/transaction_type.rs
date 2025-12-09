/// Transaction Type
///
/// Represents a logical transaction (T dimension in EVTO)

/// Transaction metadata
#[derive(Debug, Clone)]
#[allow(dead_code)] // Reserved for future temporal queries
pub struct Transaction {
    pub tx: i64,
    pub origin: String,
    pub created_at: i64,
}

impl Transaction {
    /// Create a new Transaction
    pub fn new(tx: i64, origin: impl Into<String>, created_at: i64) -> Self {
        Self {
            tx,
            origin: origin.into(),
            created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_new() {
        let transaction = Transaction::new(1, "test-origin", 1000);
        assert_eq!(transaction.tx, 1);
        assert_eq!(transaction.origin, "test-origin");
        assert_eq!(transaction.created_at, 1000);
    }

    #[test]
    fn test_transaction_new_with_string() {
        let transaction = Transaction::new(42, String::from("system"), 123456789);
        assert_eq!(transaction.tx, 42);
        assert_eq!(transaction.origin, "system");
        assert_eq!(transaction.created_at, 123456789);
    }

    #[test]
    fn test_transaction_clone() {
        let transaction = Transaction::new(10, "original", 999);
        let cloned = transaction.clone();

        assert_eq!(cloned.tx, 10);
        assert_eq!(cloned.origin, "original");
        assert_eq!(cloned.created_at, 999);
    }

    #[test]
    fn test_transaction_with_negative_timestamp() {
        let transaction = Transaction::new(5, "retracted", -1);
        assert_eq!(transaction.tx, 5);
        assert_eq!(transaction.created_at, -1);
    }
}
