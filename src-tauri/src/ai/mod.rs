use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{LlamaModel, AddBos, Special};
use llama_cpp_2::token::data_array::LlamaTokenDataArray;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub struct AIAssistant {
    model: Arc<LlamaModel>,
    backend: LlamaBackend,
}

impl AIAssistant {
    pub fn new(model_path: PathBuf) -> Result<Self, String> {
        // Initialize backend
        let backend = LlamaBackend::init()
            .map_err(|e| format!("Failed to initialize LlamaBackend: {}", e))?;

        // Load model
        let model_params = LlamaModelParams::default();
        let model = Arc::new(LlamaModel::load_from_file(&backend, model_path, &model_params)
            .map_err(|e| format!("Failed to load model: {}", e))?);

        Ok(Self { model, backend })
    }

    pub fn generate(&self, prompt: &str, max_tokens: u32) -> Result<String, String> {
        // Create context
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(std::num::NonZero::new(2048));

        let mut ctx = self.model
            .new_context(&self.backend, ctx_params)
            .map_err(|e| format!("Failed to create context: {}", e))?;

        // Tokenize prompt
        let tokens = ctx.model
            .str_to_token(prompt, AddBos::Never)
            .map_err(|e| format!("Failed to tokenize: {}", e))?;

        // Create batch
        let mut batch = LlamaBatch::new(512, 1);

        for (i, token) in tokens.iter().enumerate() {
            let is_last = i == tokens.len() - 1;
            batch.add(*token, i as i32, &[0], is_last)
                .map_err(|e| format!("Failed to add token to batch: {}", e))?;
        }

        // Process prompt
        ctx.decode(&mut batch)
            .map_err(|e| format!("Failed to decode: {}", e))?;

        // Generate response
        let mut response = String::new();
        let mut n_cur = tokens.len();

        for _ in 0..max_tokens {
            // Get candidates and select token with highest logit (greedy sampling)
            let candidates = ctx.candidates();
            let candidates_vec: Vec<_> = candidates.collect();

            if candidates_vec.is_empty() {
                return Err("No candidates available".to_string());
            }

            // Find token with highest logit (greedy)
            let next_token = candidates_vec
                .iter()
                .max_by(|a, b| a.logit().partial_cmp(&b.logit()).unwrap())
                .map(|token_data| token_data.id())
                .ok_or("Failed to select token")?;

            // Check for EOS
            if ctx.model.is_eog_token(next_token) {
                break;
            }

            // Convert token to bytes
            let output_bytes = ctx.model
                .token_to_bytes(next_token, Special::Tokenize)
                .map_err(|e| format!("Failed to convert token: {}", e))?;

            response.push_str(&String::from_utf8_lossy(&output_bytes));

            // Prepare next batch
            batch.clear();
            batch.add(next_token, n_cur as i32, &[0], true)
                .map_err(|e| format!("Failed to add token: {}", e))?;

            ctx.decode(&mut batch)
                .map_err(|e| format!("Failed to decode: {}", e))?;

            n_cur += 1;
        }

        Ok(response)
    }
}

// Thread-safe global instance
lazy_static::lazy_static! {
    pub static ref AI_INSTANCE: Arc<Mutex<Option<AIAssistant>>> = Arc::new(Mutex::new(None));
}

pub fn initialize_ai(model_path: PathBuf) -> Result<(), String> {
    let assistant = AIAssistant::new(model_path)?;
    let mut instance = AI_INSTANCE.lock()
        .map_err(|e| format!("Failed to lock AI instance: {}", e))?;
    *instance = Some(assistant);
    Ok(())
}

pub fn generate_response(prompt: &str, max_tokens: u32) -> Result<String, String> {
    let instance = AI_INSTANCE.lock()
        .map_err(|e| format!("Failed to lock AI instance: {}", e))?;

    match instance.as_ref() {
        Some(assistant) => assistant.generate(prompt, max_tokens),
        None => Err("AI not initialized".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::path::PathBuf;
    use std::time::Instant;

    fn get_model_path() -> PathBuf {
        // Model is in resources/ directory relative to src-tauri/
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("phi-4-mini-instruct-q4_k_m.gguf")
    }

    fn measure_generation<F>(test_name: &str, f: F) -> String
    where F: FnOnce() -> Result<String, String>
    {
        println!("\n=== {} ===", test_name);
        let start = Instant::now();
        let result = f().expect("Generation failed");
        let duration = start.elapsed();
        println!("⏱️  Generation time: {:.2}s ({} ms)",
                 duration.as_secs_f64(),
                 duration.as_millis());
        result
    }

    #[test]
    #[serial]
    #[ignore] // Run with: cargo test -- --ignored --nocapture
    fn test_model_initialization() {
        let model_path = get_model_path();

        println!("Testing model at: {:?}", model_path);
        assert!(model_path.exists(), "Model file not found at {:?}", model_path);

        // Initialize AI
        let result = initialize_ai(model_path);
        assert!(result.is_ok(), "Failed to initialize AI: {:?}", result.err());

        println!("✓ Model initialized successfully");
    }

    #[test]
    #[serial]
    #[ignore]
    fn test_basic_math_multiple_choice() {
        let model_path = get_model_path();
        initialize_ai(model_path).expect("Failed to initialize AI");

        let prompt = "Answer only with the letter of the correct option.\n\nQ: What is 2 + 2?\nA) 1\nB) 3\nC) 4\nD) 5\n\nAnswer:";
        println!("Prompt: {}", prompt);

        let response = measure_generation("Math Multiple Choice (2+2=4)", || {
            generate_response(prompt, 10)
        });

        println!("Response: {}", response);

        // Validate response is "C" (the correct answer)
        let response_trimmed = response.trim().to_uppercase();
        let first_char = response_trimmed.chars().next().unwrap_or(' ');

        assert!(
            first_char == 'C',
            "Expected answer 'C' (2+2=4), got: '{}' (first char: '{}')",
            response_trimmed,
            first_char
        );

        println!("✅ Test passed: Correct answer is 'C'");
    }

    #[test]
    #[serial]
    #[ignore]
    fn test_logic_multiple_choice() {
        let model_path = get_model_path();
        initialize_ai(model_path).expect("Failed to initialize AI");

        let prompt = "Answer only with the letter of the correct option.\n\nQ: If all cats are mammals, and Felix is a cat, then Felix is:\nA) A bird\nB) A mammal\nC) A fish\nD) A reptile\n\nAnswer:";
        println!("Prompt: {}", prompt);

        let response = measure_generation("Logic Multiple Choice (Felix)", || {
            generate_response(prompt, 10)
        });

        println!("Response: {}", response);

        let response_trimmed = response.trim().to_uppercase();
        let first_char = response_trimmed.chars().next().unwrap_or(' ');

        assert!(
            first_char == 'B',
            "Expected answer 'B' (mammal), got: '{}' (first char: '{}')",
            response_trimmed,
            first_char
        );

        println!("✅ Test passed: Correct answer is 'B'");
    }

    #[test]
    #[serial]
    #[ignore]
    fn test_general_knowledge_multiple_choice() {
        let model_path = get_model_path();
        initialize_ai(model_path).expect("Failed to initialize AI");

        let prompt = "Answer only with the letter of the correct option.\n\nQ: What color is the sky on a clear day?\nA) Red\nB) Blue\nC) Green\nD) Yellow\n\nAnswer:";
        println!("Prompt: {}", prompt);

        let response = measure_generation("General Knowledge (Sky Color)", || {
            generate_response(prompt, 10)
        });

        println!("Response: {}", response);

        let response_trimmed = response.trim().to_uppercase();
        let first_char = response_trimmed.chars().next().unwrap_or(' ');

        assert!(
            first_char == 'B',
            "Expected answer 'B' (blue), got: '{}' (first char: '{}')",
            response_trimmed,
            first_char
        );

        println!("✅ Test passed: Correct answer is 'B'");
    }

    #[test]
    #[serial]
    #[ignore]
    fn test_sequence_multiple_choice() {
        let model_path = get_model_path();
        initialize_ai(model_path).expect("Failed to initialize AI");

        let prompt = "Answer only with the letter of the correct option.\n\nQ: Complete the sequence: 2, 4, 6, 8, ?\nA) 9\nB) 10\nC) 11\nD) 12\n\nAnswer:";
        println!("Prompt: {}", prompt);

        let response = measure_generation("Sequence (2,4,6,8,?)", || {
            generate_response(prompt, 10)
        });

        println!("Response: {}", response);

        let response_trimmed = response.trim().to_uppercase();
        let first_char = response_trimmed.chars().next().unwrap_or(' ');

        assert!(
            first_char == 'B',
            "Expected answer 'B' (10), got: '{}' (first char: '{}')",
            response_trimmed,
            first_char
        );

        println!("✅ Test passed: Correct answer is 'B'");
    }

    #[test]
    #[serial]
    #[ignore]
    fn test_chat_format() {
        let model_path = get_model_path();
        initialize_ai(model_path).expect("Failed to initialize AI");

        // TinyLlama is a chat model, test with chat format
        let prompt = "<|system|>\nYou are a helpful assistant.</s>\n<|user|>\nWhat is 1+1?</s>\n<|assistant|>";
        println!("Prompt (chat format): {}", prompt);

        let response = measure_generation("Chat Format (1+1)", || {
            generate_response(prompt, 50)
        });

        println!("Response: {}", response);

        // Validate response contains "2"
        let response_lower = response.to_lowercase();
        assert!(
            response_lower.contains("2") || response_lower.contains("two"),
            "Expected answer to contain '2' or 'two', got: '{}'",
            response
        );

        println!("✅ Test passed: Response correctly contains '2'");
    }

    #[test]
    #[serial]
    #[ignore]
    fn test_multiple_choice_batch() {
        let model_path = get_model_path();
        initialize_ai(model_path).expect("Failed to initialize AI");

        // Use simple direct format (worked better in basic math test)
        let test_cases = vec![
            (
                "Math: 2+2",
                "Answer only with the letter of the correct option.\n\nQ: What is 2 + 2?\nA) 1\nB) 3\nC) 4\nD) 5\n\nAnswer:",
                'C'
            ),
            (
                "Math: 5+3",
                "Answer only with the letter of the correct option.\n\nQ: What is 5 + 3?\nA) 6\nB) 7\nC) 8\nD) 9\n\nAnswer:",
                'C'
            ),
            (
                "Logic: Dog legs",
                "Answer only with the letter of the correct option.\n\nQ: How many legs does a dog have?\nA) 2\nB) 3\nC) 4\nD) 6\n\nAnswer:",
                'C'
            ),
            (
                "Knowledge: Grass color",
                "Answer only with the letter of the correct option.\n\nQ: What color is grass?\nA) Red\nB) Blue\nC) Green\nD) Yellow\n\nAnswer:",
                'C'
            ),
            (
                "Sequence: 2,4,6,8",
                "Answer only with the letter of the correct option.\n\nQ: What number comes next: 2, 4, 6, 8, ?\nA) 9\nB) 10\nC) 11\nD) 12\n\nAnswer:",
                'B'
            ),
        ];

        let mut passed = 0;
        let total = test_cases.len();

        for (i, (description, prompt, correct_answer)) in test_cases.iter().enumerate() {
            let test_name = format!("Batch #{} - {}", i + 1, description);
            println!("\n{}", "=".repeat(60));
            println!("Test: {}", description);

            let response = measure_generation(&test_name, || {
                generate_response(prompt, 3)  // Just need 1 letter, use 3 tokens max
            });

            let response_trimmed = response.trim().to_uppercase();
            let first_char = response_trimmed.chars().next().unwrap_or(' ');

            println!("Expected: {} | Got: {}", correct_answer, first_char);

            if first_char == *correct_answer {
                passed += 1;
                println!("✅ PASS");
            } else {
                println!("❌ FAIL - Expected '{}', got '{}'", correct_answer, first_char);
                println!("Full response: {}", response_trimmed);
            }
        }

        println!("\n{}", "=".repeat(60));
        println!("📊 Results: {}/{} tests passed ({:.1}%)",
                 passed, total, (passed as f64 / total as f64) * 100.0);

        assert_eq!(
            passed, total,
            "Only {}/{} tests passed. Some answers were incorrect.",
            passed, total
        );

        println!("✅ All batch tests passed!");
    }
}
