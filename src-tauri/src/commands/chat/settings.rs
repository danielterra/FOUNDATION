use crate::owl::{Individual, Object, Connection, DbExecutor};

const DEFAULT_MAX_INPUT_TOKENS: usize = 30000;
const DEFAULT_TIMEOUT_SECS: u64 = 900;

pub struct AgentConfig {
    pub api_key: String,
    pub is_local: bool,
    pub base_url: String,
    pub fallback_models: Vec<String>,
    pub model_identifier: String,
    pub base_system_prompt: String,
    pub system_prompt: String,
    pub max_tokens: usize,
    pub max_output_tokens: u32,
    pub supports_web_tools: bool,
    pub timeout_secs: u64,
}

pub fn load_agent_config(conn: &Connection, conversation_iri: &str) -> Result<AgentConfig, String> {
    let agent_iri = crate::owl::get_iri_property(conn, conversation_iri, "foundation:handledBy")
        .map_err(|e| format!("Failed to get agent for conversation: {}", e))?
        .ok_or_else(|| format!("Conversation {} has no handledBy agent", conversation_iri))?;

    let service_iri = crate::owl::get_iri_property(conn, &agent_iri, "foundation:usesService")
        .map_err(|e| format!("Failed to get service for agent: {}", e))?
        .or_else(|| get_ai_service_iri(conn).ok().flatten())
        .ok_or_else(|| format!("Agent {} has no usesService and no default service configured.", agent_iri))?;

    let (api_key, is_local) = match crate::owl::get_iri_property(conn, &service_iri, "foundation:apiKey")
        .map_err(|e| format!("Failed to get apiKey from service: {}", e))?
    {
        Some(api_key_iri) => {
            let key = crate::owl::get_literal_property(conn, &api_key_iri, "foundation:credentialValue")
                .map_err(|e| format!("Failed to get credentialValue: {}", e))?
                .ok_or_else(|| "API key has no value. Please reconfigure your API key.".to_string())?;
            (key, false)
        }
        None => (String::new(), true),
    };

    let base_url = crate::owl::get_literal_property(conn, &service_iri, "foundation:apiBaseUrl")
        .unwrap_or_default()
        .unwrap_or_default();

    let fallback_models: Vec<String> = crate::owl::get_literal_property(conn, &service_iri, "foundation:fallbackModelIdentifiers")
        .unwrap_or_default()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default();

    let model_iri = crate::owl::get_iri_property(conn, &agent_iri, "foundation:usesModel")
        .map_err(|e| format!("Failed to get model for agent: {}", e))?
        .or_else(|| get_ai_model_iri(conn).ok().flatten())
        .ok_or_else(|| format!("Agent {} has no usesModel and no default model configured.", agent_iri))?;

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

    let max_output_tokens = agent.properties.iter()
        .find(|(k, _)| k == "foundation:maxOutputTokens")
        .and_then(|(_, v)| if let Object::Integer(n) = v { Some(*n as u32) } else { None });

    let model = Individual::get(conn, &model_iri)
        .map_err(|e| format!("Failed to get model: {}", e))?
        .ok_or_else(|| format!("Model {} not found", model_iri))?;

    let max_tokens = agent.properties.iter()
        .find(|(k, _)| k == "foundation:maxInputTokensPreference")
        .and_then(|(_, v)| match v {
            Object::Integer(n) => Some(*n as usize),
            Object::Literal { value, .. } => value.parse::<usize>().ok(),
            _ => None,
        })
        .or_else(|| model.properties.iter()
            .find(|(k, _)| k == "foundation:maxInputTokens")
            .and_then(|(_, v)| if let Object::Integer(n) = v { Some(*n as usize) } else { None }))
        .unwrap_or(DEFAULT_MAX_INPUT_TOKENS);

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
        .unwrap_or(DEFAULT_TIMEOUT_SECS);

    let base_system_prompt = load_base_system_prompt(conn);

    Ok(AgentConfig {
        api_key,
        is_local,
        base_url,
        fallback_models,
        model_identifier,
        base_system_prompt,
        system_prompt,
        max_tokens,
        max_output_tokens: max_output_tokens.unwrap_or(crate::commands::chat::MAX_OUTPUT_TOKENS),
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

pub fn load_base_system_prompt(conn: &Connection) -> String {
    Individual::get(conn, "foundation:DefaultSystemPromptSetting")
        .ok()
        .flatten()
        .and_then(|s| s.properties.iter()
            .find(|(k, _)| k == "foundation:settingValue")
            .and_then(|(_, v)| v.as_literal())
            .map(|v| v.to_string()))
        .unwrap_or_default()
}

/// Get AI service IRI from DefaultAIServiceSetting (searched by settingKey="aiService")
pub fn get_ai_service_iri(conn: &Connection) -> Result<Option<String>, String> {
    let setting_iris = crate::owl::find_entities_with_property(conn, "rdf:type", "foundation:SoftwareSetting")
        .map_err(|e| format!("Failed to query settings: {}", e))?;

    for iri in setting_iris {
        if let Ok(Some(setting)) = Individual::get(conn, &iri) {
            let key = setting.properties.iter()
                .find(|(k, _)| k == "foundation:settingKey")
                .and_then(|(_, v)| v.as_literal());
            if key.as_deref() == Some("aiService") {
                return Ok(setting.properties.iter()
                    .find(|(k, _)| k == "foundation:settingValue")
                    .and_then(|(_, v)| v.as_literal())
                    .map(|v| v.to_string()));
            }
        }
    }
    Ok(None)
}
