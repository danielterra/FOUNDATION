use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::io::Write;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    data: Vec<Model>,
}

#[derive(Debug, Deserialize, Serialize)]
struct Model {
    id: String,
    #[serde(rename = "type")]
    model_type: String,
    display_name: String,
    created_at: String,
}

#[derive(Debug, Clone, Serialize)]
struct ModelPricing {
    input_price: f64,
    output_price: f64,
    cache_write_5min_price: f64,
    cache_write_1h_price: f64,
    cache_read_price: f64,
    batch_input_price: f64,
    batch_output_price: f64,
}

fn fetch_pricing_data() -> Result<HashMap<String, ModelPricing>> {
    println!("🔍 Using hardcoded pricing data from Claude docs (2026)...");

    let mut pricing_map: HashMap<String, ModelPricing> = HashMap::new();

    // Based on https://platform.claude.com/docs/en/about-claude/pricing (2026)
    // Prices are per million tokens (MTok)

    // Claude Sonnet 4.6 - $3 input / $15 output
    pricing_map.insert("Claude Sonnet 4.6".to_string(), ModelPricing {
        input_price: 3.0,
        output_price: 15.0,
        cache_write_5min_price: 3.75,      // 1.25x base
        cache_write_1h_price: 6.0,         // 2x base
        cache_read_price: 0.3,             // 0.1x base
        batch_input_price: 1.5,            // 50% off
        batch_output_price: 7.5,           // 50% off
    });

    // Claude Opus 4.6 - $5 input / $25 output
    pricing_map.insert("Claude Opus 4.6".to_string(), ModelPricing {
        input_price: 5.0,
        output_price: 25.0,
        cache_write_5min_price: 6.25,
        cache_write_1h_price: 10.0,
        cache_read_price: 0.5,
        batch_input_price: 2.5,
        batch_output_price: 12.5,
    });

    // Claude Haiku 4.5 - $1 input / $5 output
    pricing_map.insert("Claude Haiku 4.5".to_string(), ModelPricing {
        input_price: 1.0,
        output_price: 5.0,
        cache_write_5min_price: 1.25,
        cache_write_1h_price: 2.0,
        cache_read_price: 0.1,
        batch_input_price: 0.5,
        batch_output_price: 2.5,
    });

    // Claude Sonnet 4.5 - $3 input / $15 output
    pricing_map.insert("Claude Sonnet 4.5".to_string(), ModelPricing {
        input_price: 3.0,
        output_price: 15.0,
        cache_write_5min_price: 3.75,
        cache_write_1h_price: 6.0,
        cache_read_price: 0.3,
        batch_input_price: 1.5,
        batch_output_price: 7.5,
    });

    // Claude Opus 4.5 - $5 input / $25 output
    pricing_map.insert("Claude Opus 4.5".to_string(), ModelPricing {
        input_price: 5.0,
        output_price: 25.0,
        cache_write_5min_price: 6.25,
        cache_write_1h_price: 10.0,
        cache_read_price: 0.5,
        batch_input_price: 2.5,
        batch_output_price: 12.5,
    });

    // Claude Opus 4.1 - $15 input / $75 output (legacy)
    pricing_map.insert("Claude Opus 4.1".to_string(), ModelPricing {
        input_price: 15.0,
        output_price: 75.0,
        cache_write_5min_price: 18.75,
        cache_write_1h_price: 30.0,
        cache_read_price: 1.5,
        batch_input_price: 7.5,
        batch_output_price: 37.5,
    });

    // Claude Opus 4 - assume same as 4.5 series
    pricing_map.insert("Claude Opus 4".to_string(), ModelPricing {
        input_price: 5.0,
        output_price: 25.0,
        cache_write_5min_price: 6.25,
        cache_write_1h_price: 10.0,
        cache_read_price: 0.5,
        batch_input_price: 2.5,
        batch_output_price: 12.5,
    });

    // Claude Sonnet 4 - assume same as 4.5 series
    pricing_map.insert("Claude Sonnet 4".to_string(), ModelPricing {
        input_price: 3.0,
        output_price: 15.0,
        cache_write_5min_price: 3.75,
        cache_write_1h_price: 6.0,
        cache_read_price: 0.3,
        batch_input_price: 1.5,
        batch_output_price: 7.5,
    });

    // Claude Haiku 3 - legacy pricing (estimate based on pattern)
    pricing_map.insert("Claude Haiku 3".to_string(), ModelPricing {
        input_price: 1.0,
        output_price: 5.0,
        cache_write_5min_price: 1.25,
        cache_write_1h_price: 2.0,
        cache_read_price: 0.1,
        batch_input_price: 0.5,
        batch_output_price: 2.5,
    });

    println!("✅ Loaded pricing for {} models", pricing_map.len());
    Ok(pricing_map)
}

fn main() -> Result<()> {
    // Load .env from project root
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();

    let env_path = project_root.join(".env");
    dotenv::from_path(&env_path).context("Failed to load .env file")?;

    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .context("ANTHROPIC_API_KEY not found in .env")?;

    println!("🔍 Fetching models from Claude API...");

    // Call Claude API to get models
    let client = reqwest::blocking::Client::new();
    let response = client
        .get("https://api.anthropic.com/v1/models")
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .send()
        .context("Failed to call Claude API")?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().unwrap_or_default();
        anyhow::bail!("API request failed with status {}: {}", status, error_text);
    }

    let models_response: ModelsResponse = response
        .json()
        .context("Failed to parse API response")?;

    println!("✅ Found {} models", models_response.data.len());

    // Fetch pricing data
    let pricing_data = fetch_pricing_data()
        .context("Failed to fetch pricing data")?;

    // Read current TTL file
    let ttl_path = project_root.join("core-ontology/AIModel.ttl");
    let current_content = fs::read_to_string(&ttl_path)
        .context("Failed to read AIModel.ttl")?;

    // Create backup
    let backup_path = project_root.join("core-ontology/AIModel.ttl.backup");
    fs::write(&backup_path, &current_content)
        .context("Failed to create backup")?;
    println!("📦 Backup created: {:?}", backup_path);

    // Prepare data for Claude CLI
    let models_json = serde_json::to_string_pretty(&models_response.data)
        .context("Failed to serialize models")?;

    let pricing_json = serde_json::to_string_pretty(&pricing_data)
        .context("Failed to serialize pricing")?;

    println!("\n🤖 Calling Claude CLI to update TTL file...");

    // Create prompt for Claude
    let prompt = format!(
        "OUTPUT ONLY THE COMPLETE TTL FILE. DO NOT ADD ANY EXPLANATIONS OR COMMENTS OUTSIDE THE TTL CONTENT.

Update the AIModel.ttl file with new models from the Claude API.

CRITICAL RULES:
1. PRESERVE all property definitions and headers
2. PRESERVE existing models not in the API - mark them as isDeprecated: true
3. For models from the API: update or create with isDeprecated: false
4. Mark first API model as isDefaultModel: true, others false
5. Add capabilities: Sonnet → coding/reasoning/analysis, Opus → writing/complex_tasks/creativity, Haiku → speed/efficiency
6. Convert IDs to IRI names (claude-sonnet-4-6 → ClaudeSonnet46)
7. Add ALL pricing properties to each API model from the pricing data (match by display name)
8. Keep deprecated models at the end of the file

Current TTL:
{}

Models from API (JSON):
{}

Pricing (JSON):
{}

OUTPUT THE COMPLETE TTL FILE NOW:",
        current_content, models_json, pricing_json
    );

    // Call Claude CLI
    let mut child = Command::new("claude")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to spawn claude command. Make sure Claude CLI is installed.")?;

    // Write prompt to stdin
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(prompt.as_bytes())
            .context("Failed to write to claude stdin")?;
    }

    // Wait for completion and get output
    let output = child.wait_with_output()
        .context("Failed to wait for claude command")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Claude CLI failed: {}", stderr);
    }

    let updated_content = String::from_utf8(output.stdout)
        .context("Failed to parse claude output as UTF-8")?;

    // Extract TTL from markdown code blocks if present
    let cleaned_content = extract_ttl_from_response(&updated_content);

    // Write updated content
    fs::write(&ttl_path, cleaned_content)
        .context("Failed to write updated AIModel.ttl")?;

    println!("✅ Updated: {:?}", ttl_path);
    println!("🎉 Models updated successfully!");

    Ok(())
}

fn extract_ttl_from_response(response: &str) -> String {
    // Try to extract content from ```turtle code blocks
    if let Some(start) = response.find("```turtle") {
        let content_start = start + "```turtle".len();
        // Skip newline after ```turtle
        let content_start = if response[content_start..].starts_with('\n') {
            content_start + 1
        } else {
            content_start
        };

        if let Some(end_pos) = response[content_start..].find("```") {
            let content = &response[content_start..content_start + end_pos];
            return content.trim().to_string();
        }
    }

    // Try to extract from generic ``` code blocks
    if let Some(start) = response.find("```") {
        let content_start = start + 3;
        // Skip language identifier line if present
        let content_start = if let Some(newline) = response[content_start..].find('\n') {
            content_start + newline + 1
        } else {
            content_start
        };

        if let Some(end_pos) = response[content_start..].find("```") {
            let content = &response[content_start..content_start + end_pos];
            return content.trim().to_string();
        }
    }

    // If no code blocks, return as-is
    response.trim().to_string()
}
