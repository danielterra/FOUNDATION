use serde::{Serialize, Deserialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyboardShortcut {
    pub keys: String,
    pub label: String,
}

/// Get all registered keyboard shortcuts
#[tauri::command]
pub fn shortcuts__get_all() -> String {
    let shortcuts = vec![
        KeyboardShortcut {
            keys: "CMD+F".to_string(),
            label: "Search".to_string(),
        },
        KeyboardShortcut {
            keys: "CMD+0".to_string(),
            label: "Recenter".to_string(),
        },
        KeyboardShortcut {
            keys: "CMD+R".to_string(),
            label: "Reload".to_string(),
        },
    ];

    serde_json::to_string(&shortcuts).unwrap_or_else(|_| "[]".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shortcuts_get_all_returns_json() {
        let result = shortcuts__get_all();
        assert!(!result.is_empty());

        // Parse to verify it's valid JSON
        let shortcuts: Vec<KeyboardShortcut> = serde_json::from_str(&result).unwrap();
        assert_eq!(shortcuts.len(), 3);
        assert_eq!(shortcuts[0].keys, "CMD+F");
        assert_eq!(shortcuts[0].label, "Search");
    }

    #[test]
    fn test_shortcut_serialization() {
        let shortcut = KeyboardShortcut {
            keys: "CMD+S".to_string(),
            label: "Save".to_string(),
        };

        let json = serde_json::to_string(&shortcut).unwrap();
        assert!(json.contains("keys"));
        assert!(json.contains("label"));
        assert!(json.contains("CMD+S"));
        assert!(json.contains("Save"));
    }
}
