use crate::owl::{Individual, Object, Connection, DbExecutor};

pub async fn get_system_prompt(executor: &DbExecutor) -> Result<String, String> {
    executor.read(|conn| {
        let template = if let Ok(Some(setting)) = Individual::get(
            conn,
            "foundation:DefaultSystemPromptSetting",
        ) {
            if let Some(Object::Literal { value, .. }) = setting.properties.iter()
                .find(|(k, _)| k == "foundation:settingValue")
                .map(|(_, v)| v) {
                value.clone()
            } else {
                return Err("DefaultSystemPromptSetting has no settingValue".to_string());
            }
        } else {
            return Err(
                "Failed to get system prompt: DefaultSystemPromptSetting not found".to_string(),
            );
        };

        let user = Individual::get(conn, "foundation:ThisUser").ok().flatten();
        let ai = Individual::get(conn, "foundation:LocalAIAssistant").ok().flatten();

        let user_name = user.as_ref()
            .and_then(|u| u.properties.iter()
                .find(|(k, _)| k == "rdfs:label")
                .and_then(|(_, v)| match v {
                    Object::Literal { value, .. } => Some(value.clone()),
                    _ => None,
                }))
            .unwrap_or_else(|| "User".to_string());

        let ai_name = ai.as_ref()
            .and_then(|a| a.properties.iter()
                .find(|(k, _)| k == "rdfs:label")
                .and_then(|(_, v)| match v {
                    Object::Literal { value, .. } => Some(value.clone()),
                    _ => None,
                }))
            .unwrap_or_else(|| "NOVA".to_string());

        let language = Individual::get(conn, "foundation:DefaultLanguageSetting")
            .ok()
            .flatten()
            .and_then(|s| s.properties.iter()
                .find(|(k, _)| k == "foundation:settingValue")
                .and_then(|(_, v)| match v {
                    Object::Literal { value, .. } => Some(value.clone()),
                    _ => None,
                }))
            .unwrap_or_else(|| "English".to_string());

        let locale = Individual::get(conn, "foundation:DefaultLocaleSetting")
            .ok()
            .flatten()
            .and_then(|s| s.properties.iter()
                .find(|(k, _)| k == "foundation:settingValue")
                .and_then(|(_, v)| match v {
                    Object::Literal { value, .. } => Some(value.clone()),
                    _ => None,
                }))
            .unwrap_or_else(|| "en_US".to_string());

        let country = Individual::get(conn, "foundation:DefaultCountrySetting")
            .ok()
            .flatten()
            .and_then(|s| s.properties.iter()
                .find(|(k, _)| k == "foundation:settingValue")
                .and_then(|(_, v)| match v {
                    Object::Literal { value, .. } => Some(value.clone()),
                    _ => None,
                }))
            .unwrap_or_else(|| "United States".to_string());

        let location_info = Individual::get(conn, "foundation:DefaultLocationInfoSetting")
            .ok()
            .flatten()
            .and_then(|s| s.properties.iter()
                .find(|(k, _)| k == "foundation:settingValue")
                .and_then(|(_, v)| match v {
                    Object::Literal { value, .. } => Some(value.clone()),
                    _ => None,
                }))
            .unwrap_or_else(|| "".to_string());

        let prompt = template
            .replace("{user_name}", &user_name)
            .replace("{ai_name}", &ai_name)
            .replace("{language}", &language)
            .replace("{locale}", &locale)
            .replace("{country}", &country)
            .replace("{location_info}", &location_info);

        Ok(prompt)
    }).await
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

pub async fn get_supports_web_tools(executor: &DbExecutor) -> bool {
    executor.read(|conn| {
        let model_iri = match get_ai_model_iri(conn)? {
            Some(iri) => iri,
            None => return Ok(false),
        };
        let model = match Individual::get(conn, &model_iri).map_err(|e| e.to_string())? {
            Some(m) => m,
            None => return Ok(false),
        };
        let has_web_tools = model.properties.iter().any(|(k, v)| {
            k == "foundation:modelCapability"
                && matches!(v, Object::Literal { value, .. } if value == "web_tools")
        });
        Ok(has_web_tools)
    }).await.unwrap_or(false)
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
