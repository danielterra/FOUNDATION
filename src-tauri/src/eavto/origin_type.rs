/// Origin Type
///
/// Represents the origin/provenance of triples (O dimension in EVTO)

/// Origin metadata
#[derive(Debug, Clone)]
#[allow(dead_code)] // Reserved for provenance tracking features
pub struct Origin {
    pub id: i64,
    pub name: String,
}

impl Origin {
    /// Create a new Origin
    pub fn new(id: i64, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_origin_new() {
        let origin = Origin::new(1, "test-origin");
        assert_eq!(origin.id, 1);
        assert_eq!(origin.name, "test-origin");
    }

    #[test]
    fn test_origin_new_with_string() {
        let origin = Origin::new(42, String::from("system"));
        assert_eq!(origin.id, 42);
        assert_eq!(origin.name, "system");
    }

    #[test]
    fn test_origin_clone() {
        let origin = Origin::new(10, "original");
        let cloned = origin.clone();

        assert_eq!(cloned.id, 10);
        assert_eq!(cloned.name, "original");
    }
}
