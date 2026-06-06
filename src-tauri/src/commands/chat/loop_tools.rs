use crate::ai::providers::ToolDefinition;
use super::settings::AgentConfig;

pub fn inject_reason_into_schema(schema: &mut serde_json::Value) {
    let Some(obj) = schema.as_object_mut() else { return; };

    let properties = obj
        .entry("properties")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .unwrap();
    properties.insert(
        "reason".to_string(),
        serde_json::json!({
            "type": "string",
            "description": "Short PT-BR description (≤6 words) of why you are calling this tool — shown to the user as the action label."
        }),
    );
    let required = obj
        .entry("required")
        .or_insert_with(|| serde_json::json!([]))
        .as_array_mut()
        .unwrap();
    let already = required.iter().any(|v| v.as_str() == Some("reason"));
    if !already {
        required.push(serde_json::Value::String("reason".to_string()));
    }

    make_object_strict(obj);
}

fn make_object_strict(obj: &mut serde_json::Map<String, serde_json::Value>) {
    let Some(props) = obj.get("properties").and_then(|v| v.as_object()).cloned() else {
        return;
    };

    let originally_required: std::collections::HashSet<String> = obj
        .get("required")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    let required = obj
        .entry("required")
        .or_insert_with(|| serde_json::json!([]))
        .as_array_mut()
        .unwrap();

    for key in props.keys() {
        if !originally_required.contains(key) {
            required.push(serde_json::Value::String(key.clone()));
        }
    }

    let props_mut = obj.get_mut("properties").unwrap().as_object_mut().unwrap();
    for (key, prop_val) in props_mut.iter_mut() {
        let Some(prop_obj) = prop_val.as_object_mut() else { continue; };

        if !originally_required.contains(key) {
            if let Some(type_val) = prop_obj.get("type").cloned() {
                match type_val {
                    serde_json::Value::String(s) if s != "null" => {
                        prop_obj.insert("type".to_string(), serde_json::json!([s, "null"]));
                    }
                    serde_json::Value::Array(arr)
                        if !arr.iter().any(|v| v.as_str() == Some("null")) =>
                    {
                        let mut new_arr = arr;
                        new_arr.push(serde_json::json!("null"));
                        prop_obj.insert("type".to_string(), serde_json::Value::Array(new_arr));
                    }
                    _ => {}
                }
            }
        }

        if prop_obj.get("type").and_then(|t| t.as_str()) == Some("object")
            || (prop_obj.get("type").and_then(|t| t.as_array()).map_or(false, |a| {
                a.iter().any(|v| v.as_str() == Some("object"))
            }))
        {
            prop_obj.insert("additionalProperties".to_string(), serde_json::json!(false));
            make_object_strict(prop_obj);
        }

        if prop_obj.get("type").and_then(|t| t.as_str()) == Some("array")
            || (prop_obj.get("type").and_then(|t| t.as_array()).map_or(false, |a| {
                a.iter().any(|v| v.as_str() == Some("array"))
            }))
        {
            if let Some(items) = prop_obj.get_mut("items").and_then(|v| v.as_object_mut()) {
                if items.get("type").and_then(|t| t.as_str()) == Some("object") {
                    items.insert("additionalProperties".to_string(), serde_json::json!(false));
                    make_object_strict(items);
                }
            }
        }
    }

    obj.insert("additionalProperties".to_string(), serde_json::json!(false));
}

pub fn build_conversation_tools() -> Vec<ToolDefinition> {
    let mut tools = crate::ai::functions::get_tool_definitions();

    tools.push(ToolDefinition {
        name: "ask_question".to_string(),
        description: "Ask the user a structured question and wait for their answer. Use only when a user decision is required to proceed. The conversation pauses until the user responds.".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "question": {
                    "type": "string",
                    "description": "The question to ask the user."
                },
                "type": {
                    "type": "string",
                    "enum": ["single", "multi", "text"],
                    "description": "'single' for one option button, 'multi' for checkboxes, 'text' for a free-text field."
                },
                "options": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "List of options. Required for 'single' and 'multi'. Omit for 'text'."
                }
            },
            "required": ["question", "type"]
        }),
    });

    for tool in tools.iter_mut() {
        inject_reason_into_schema(&mut tool.input_schema);
    }

    tools
}

pub fn build_system_prompt(
    agent_config: &AgentConfig,
    foundation_context: &str,
    widget_context: &str,
    camera_count: Option<usize>,
    conversation_id: &str,
) -> String {
    let delegation_section = format!(
        "## Delegating Tasks\n\
         Your current conversation IRI is: `{}`. When you create a `foundation:Task` and \
         want the result returned here automatically, set `foundation:delegatedFromConversation` \
         to this IRI on the task.\n\
         For the task to be executed, it must either have `foundation:scheduledAt` set to a \
         future datetime, or have its `foundation:hasStatus` set to `foundation:InProgress`. \
         Without one of these, the task will remain pending and never run.",
        conversation_id
    );

    let tool_reason_instruction = "## Tool calls\n\
        The `reason` field on each tool call must be a ≤6-word PT-BR description of your intent; \
        it is shown to the user as the action label.";

    let response_rule = "## Response rule\n\
        You MUST always end your turn with a text message to the user. \
        Never finish silently after tool calls — always confirm what was done, \
        summarize the result, or ask the next question. \
        An empty final turn is a failure.";

    let base = format!(
        "{}\n\n{}\n\n{}\n\n{}\n\n{}\n\n{}\n\n{}",
        foundation_context,
        agent_config.base_system_prompt,
        widget_context,
        agent_config.system_prompt,
        delegation_section,
        tool_reason_instruction,
        response_rule,
    );
    match camera_count {
        Some(n) => format!(
            "{}\n\n[Camera Vision] {} webcam snapshot{} of the user were captured during the typing of this message and are included as image blocks at the start of the user's message, in chronological order. Use them to read the evolution of the user's facial expression, posture, and emotional energy over the course of composing the message, and calibrate your tone and depth of response accordingly. Do not mention the camera or the images to the user unless they explicitly ask.",
            base, n, if n == 1 { "" } else { "s" }
        ),
        None => base,
    }
}

