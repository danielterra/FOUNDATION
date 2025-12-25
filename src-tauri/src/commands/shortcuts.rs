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

