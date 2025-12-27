use crate::ai;
use tauri::{AppHandle, Manager};

#[tauri::command]
pub async fn ai__initialize(app: AppHandle) -> Result<(), String> {
    use std::path::PathBuf;

    // In development mode, use resources/ from CARGO_MANIFEST_DIR
    // In production, use the bundled resources
    let model_path = if cfg!(debug_assertions) {
        // Development mode - use src-tauri/resources/
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("phi-4-mini-instruct-q4_k_m.gguf")
    } else {
        // Production mode - use Tauri's resource resolution
        app.path()
            .resolve("phi-4-mini-instruct-q4_k_m.gguf", tauri::path::BaseDirectory::Resource)
            .map_err(|e| format!("Failed to resolve model path: {}", e))?
    };

    if !model_path.exists() {
        return Err(format!("Model not found at: {:?}", model_path));
    }

    super::log_backend(&app, "info", &format!("Initializing AI model from: {:?}", model_path));

    ai::initialize_ai(model_path)?;

    super::log_backend(&app, "info", "AI model initialized successfully");

    Ok(())
}

#[tauri::command]
pub async fn ai__generate(
    app: AppHandle,
    prompt: String,
    max_tokens: Option<u32>,
) -> Result<String, String> {
    let max_tokens = max_tokens.unwrap_or(100);

    super::log_backend(&app, "info", &format!("Generating AI response for prompt: {}", prompt));

    let response = ai::generate_response(&prompt, max_tokens)?;

    super::log_backend(&app, "info", &format!("AI response generated: {} chars", response.len()));

    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::AI_INSTANCE;

    #[test]
    fn test_ai_generate_without_initialization() {
        // Clear AI instance to ensure clean state
        {
            let mut instance = AI_INSTANCE.lock().unwrap();
            *instance = None;
        }

        // Try to generate without initializing
        let result = ai::generate_response("Hello", 10);

        // Should fail because AI not initialized
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("AI not initialized"));
    }

    #[test]
    fn test_ai_generate_response_parameter_handling() {
        // Clear AI instance
        {
            let mut instance = AI_INSTANCE.lock().unwrap();
            *instance = None;
        }

        // Test with different max_tokens values
        let result1 = ai::generate_response("Test", 0);
        assert!(result1.is_err()); // Will fail due to not initialized

        let result2 = ai::generate_response("Test", 100);
        assert!(result2.is_err()); // Will fail due to not initialized

        // Both should fail with same error (not initialized)
        assert_eq!(result1.unwrap_err(), result2.unwrap_err());
    }
}
