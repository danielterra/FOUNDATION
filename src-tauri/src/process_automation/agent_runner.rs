use crate::ai::AiProvider;
use crate::owl::{DbExecutor, Individual, get_literal_property, get_iri_property};

type Result<T> = std::result::Result<T, String>;

const DEFAULT_TIMEOUT_SECS: u64 = 180;

pub struct AgentConfig {
    pub provider: AiProvider,
    pub model_identifier: String,
    pub system_prompt: String,
}

/// Resolves a fully configured AiProvider for the given agent IRI,
/// falling back to global service/model settings when the agent has none.
pub async fn resolve_agent_config(executor: &DbExecutor, agent_iri: &str) -> Result<AgentConfig> {
    let agent_iri = agent_iri.to_string();
    executor.read(move |conn| {
        let service_iri = get_iri_property(conn, &agent_iri, "foundation:usesService")
            .map_err(|e| e.to_string())?
            .or_else(|| crate::commands::chat::settings::get_ai_service_iri(conn).ok().flatten())
            .ok_or_else(|| format!("No AI service configured for agent {} and no global default", agent_iri))?;

        let api_key_iri = get_iri_property(conn, &service_iri, "foundation:apiKey")
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "API key not configured".to_string())?;

        let api_key = get_literal_property(conn, &api_key_iri, "foundation:credentialValue")
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "API key has no value".to_string())?;

        let model_iri = get_iri_property(conn, &agent_iri, "foundation:usesModel")
            .map_err(|e| e.to_string())?
            .or_else(|| crate::commands::chat::settings::get_ai_model_iri(conn).ok().flatten())
            .ok_or_else(|| format!("No AI model configured for agent {} and no global default", agent_iri))?;

        let model_identifier = get_literal_property(conn, &model_iri, "foundation:modelIdentifier")
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Model {} has no modelIdentifier", model_iri))?;

        let base_url = get_literal_property(conn, &service_iri, "foundation:apiBaseUrl")
            .unwrap_or_default()
            .unwrap_or_default();

        let fallback_models: Vec<String> = get_literal_property(conn, &service_iri, "foundation:fallbackModelIdentifiers")
            .unwrap_or_default()
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default();

        let timeout_secs = get_literal_property(conn, &agent_iri, "foundation:requestTimeout")
            .ok()
            .flatten()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(DEFAULT_TIMEOUT_SECS);

        let agent_base_prompt = Individual::get(conn, &agent_iri)
            .ok()
            .flatten()
            .and_then(|ind| ind.properties.iter()
                .find(|(k, _)| k == "foundation:basePrompt")
                .and_then(|(_, v)| v.as_literal()))
            .unwrap_or_default();

        let base_system_prompt = crate::commands::chat::settings::load_base_system_prompt(conn);
        let system_prompt = if agent_base_prompt.is_empty() {
            base_system_prompt
        } else {
            format!("{}\n\n{}", base_system_prompt, agent_base_prompt)
        };

        let provider = build_provider(&api_key, &model_identifier, &base_url, &fallback_models, timeout_secs);

        Ok(AgentConfig { provider, model_identifier, system_prompt })
    }).await
}

fn build_provider(
    api_key: &str,
    model_identifier: &str,
    base_url: &str,
    fallback_models: &[String],
    timeout_secs: u64,
) -> AiProvider {
    if base_url.contains("openrouter.ai") {
        AiProvider::OpenRouter(crate::ai::providers::OpenRouterProvider::with_model(
            api_key.to_string(),
            base_url.to_string(),
            model_identifier.to_string(),
            fallback_models.to_vec(),
            timeout_secs,
        ))
    } else {
        AiProvider::Claude(crate::ai::providers::ClaudeProvider::with_model(
            api_key.to_string(),
            model_identifier.to_string(),
            timeout_secs,
        ))
    }
}
