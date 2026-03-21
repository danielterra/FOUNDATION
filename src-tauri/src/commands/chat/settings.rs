use crate::owl::{Individual, Object, Connection, DbExecutor};

pub struct AgentConfig {
    pub api_key: String,
    pub model_identifier: String,
    pub system_prompt: String,
    pub max_tokens: usize,
    pub supports_web_tools: bool,
    pub timeout_secs: u64,
}

pub fn load_agent_config(conn: &Connection, conversation_iri: &str) -> Result<AgentConfig, String> {
    let agent_iri = crate::owl::get_iri_property(conn, conversation_iri, "foundation:handledBy")
        .map_err(|e| format!("Failed to get agent for conversation: {}", e))?
        .ok_or_else(|| format!("Conversation {} has no handledBy agent", conversation_iri))?;

    let service_iri = crate::owl::get_iri_property(conn, &agent_iri, "foundation:usesService")
        .map_err(|e| format!("Failed to get service for agent: {}", e))?
        .ok_or_else(|| format!("Agent {} has no usesService", agent_iri))?;

    let api_key_iri = crate::owl::get_iri_property(conn, &service_iri, "foundation:apiKey")
        .map_err(|e| format!("Failed to get apiKey from service: {}", e))?
        .ok_or_else(|| "API key not configured. Please add your API key in Settings.".to_string())?;

    let api_key = crate::owl::get_literal_property(conn, &api_key_iri, "foundation:credentialValue")
        .map_err(|e| format!("Failed to get credentialValue: {}", e))?
        .ok_or_else(|| "API key has no value. Please reconfigure your API key.".to_string())?;

    let model_iri = crate::owl::get_iri_property(conn, &agent_iri, "foundation:usesModel")
        .map_err(|e| format!("Failed to get model for agent: {}", e))?
        .ok_or_else(|| format!("Agent {} has no usesModel", agent_iri))?;

    let model_identifier = crate::owl::get_literal_property(conn, &model_iri, "foundation:modelIdentifier")
        .map_err(|e| format!("Failed to get modelIdentifier: {}", e))?
        .ok_or_else(|| format!("Model {} has no modelIdentifier", model_iri))?;

    let agent = Individual::get(conn, &agent_iri)
        .map_err(|e| format!("Failed to get agent: {}", e))?
        .ok_or_else(|| format!("Agent {} not found", agent_iri))?;

    let system_prompt = agent.properties.iter()
        .find(|(k, _)| k == "foundation:basePrompt")
        .and_then(|(_, v)| v.as_literal())
        .unwrap_or_default();

    let model = Individual::get(conn, &model_iri)
        .map_err(|e| format!("Failed to get model: {}", e))?
        .ok_or_else(|| format!("Model {} not found", model_iri))?;

    let max_tokens = model.properties.iter()
        .find(|(k, _)| k == "foundation:maxInputTokens")
        .and_then(|(_, v)| if let Object::Integer(n) = v { Some(*n as usize) } else { None })
        .unwrap_or(30000);

    let supports_web_tools = model.properties.iter().any(|(k, v)| {
        k == "foundation:modelCapability"
            && matches!(v, Object::Literal { value, .. } if value == "web_tools")
    });

    let timeout_secs = Individual::get(conn, "foundation:DefaultAPIRequestTimeoutSetting")
        .ok()
        .flatten()
        .and_then(|s| s.properties.iter()
            .find(|(k, _)| k == "foundation:settingValue")
            .and_then(|(_, v)| v.as_literal())
            .and_then(|v| v.parse::<u64>().ok()))
        .unwrap_or(900);

    Ok(AgentConfig {
        api_key,
        model_identifier,
        system_prompt,
        max_tokens,
        supports_web_tools,
        timeout_secs,
    })
}

/// Get max input tokens with fallback logic:
/// 1. Check DefaultMaxInputTokensSetting (user updates this setting, not creates new one)
/// 2. Fall back to AIModel's maxInputTokens
pub async fn get_max_input_tokens(executor: &DbExecutor) -> Result<usize, String> {
    executor.read(|conn| {
        if let Ok(Some(setting)) =
            Individual::get(conn, "foundation:DefaultMaxInputTokensSetting")
        {
            if let Some(Object::Literal { value, .. }) = setting.properties.iter()
                .find(|(k, _)| k == "foundation:settingValue")
                .map(|(_, v)| v) {
                if let Ok(tokens) = value.parse::<usize>() {
                    return Ok(tokens);
                }
            }
        }

        let model_iri = get_ai_model_iri(conn)?;

        if let Some(iri) = model_iri {
            let model = Individual::get(conn, &iri)
                .map_err(|e| format!("Failed to get AI model: {}", e))?
                .ok_or_else(|| format!("Failed to get AI model: IRI not found"))?;

            if let Some(Object::Integer(max_tokens)) = model.properties.iter()
                .find(|(k, _)| k == "foundation:maxInputTokens")
                .map(|(_, v)| v) {
                return Ok(*max_tokens as usize);
            }
        }

        Err(concat!(
            "Failed to get max input tokens: DefaultMaxInputTokensSetting not found",
            " and no AI model configured",
        ).to_string())
    }).await
}

/// Get AI model IRI with fallback logic:
/// Check DefaultAIModelSetting (user updates this setting, not creates new one)
pub fn get_ai_model_iri(conn: &Connection) -> Result<Option<String>, String> {
    if let Ok(Some(setting)) = Individual::get(conn, "foundation:DefaultAIModelSetting") {
        if let Some(Object::Literal { value, .. }) = setting.properties.iter()
            .find(|(k, _)| k == "foundation:settingValue")
            .map(|(_, v)| v) {
            return Ok(Some(value.clone()));
        }
    }

    Ok(None)
}
