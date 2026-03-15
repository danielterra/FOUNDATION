use tauri::{AppHandle, Manager};

use crate::ai::{AIAssistant, GenerateRequest, ChatMessage};
use crate::ai::providers::{MessageContent, ContentBlock};
use crate::ai::functions::{get_claude_tools, ToolCall as FunctionToolCall, execute_tool as execute_fn};
use crate::owl::{DbExecutor, get_literal_property, get_iri_property, Individual};

use super::executor::ExecutionContext;

type Result<T> = std::result::Result<T, String>;

const MAX_TOOL_LOOPS: usize = 50;
const DEFAULT_MAX_TOKENS: u32 = 4096;
const DEFAULT_TEMPERATURE: f32 = 0.3;
const DEFAULT_TIMEOUT_SECS: u64 = 180;

fn interpolate(template: &str, ctx: &ExecutionContext) -> String {
    let mut result = template.to_string();
    for (key, value) in ctx {
        result = result.replace(&format!("{{{{{}}}}}", key), value);
    }
    result
}

/// Executes a bpmn_AgentTask node headlessly.
/// Reads the assignedAgent's API key and model, builds a prompt,
/// runs the agentic tool loop until end_turn, and returns the final text output.
pub async fn execute_agent_task(
    app: &AppHandle,
    node_iri: &str,
    ctx: &ExecutionContext,
) -> Result<String> {
    let executor = app.state::<DbExecutor>();

    let (label, description, agent_iri) = executor
        .read({
            let node_iri = node_iri.to_string();
            move |conn| async move {
                let label = get_literal_property(&conn, &node_iri, "rdfs:label").await
                    .map_err(|e| e.to_string())?
                    .unwrap_or_else(|| node_iri.clone());

                let description = get_literal_property(&conn, &node_iri, "rdfs:comment").await
                    .map_err(|e| e.to_string())?
                    .unwrap_or_default();

                let agent_iri = get_iri_property(&conn, &node_iri, "foundation:assignedAgent").await
                    .map_err(|e| e.to_string())?
                    .unwrap_or_else(|| "foundation:LocalAIAssistant".to_string());

                Ok((label, description, agent_iri))
            }
        })
        .await?;

    let (api_key, model_identifier, system_prompt, timeout_secs) = executor
        .read({
            let agent_iri = agent_iri.clone();
            move |conn| async move {
                let service_iri = get_iri_property(&conn, &agent_iri, "foundation:usesService").await
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| format!("Agent {} has no usesService", agent_iri))?;

                let api_key_iri = get_iri_property(&conn, &service_iri, "foundation:apiKey").await
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| "API key not configured".to_string())?;

                let api_key = get_literal_property(&conn, &api_key_iri, "foundation:credentialValue").await
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| "API key has no value".to_string())?;

                let model_iri = get_iri_property(&conn, &agent_iri, "foundation:usesModel").await
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| format!("Agent {} has no usesModel", agent_iri))?;

                let model_identifier = get_literal_property(&conn, &model_iri, "foundation:modelIdentifier").await
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| format!("Model {} has no modelIdentifier", model_iri))?;

                let system_prompt = Individual::get(&conn, &agent_iri).await
                    .ok()
                    .flatten()
                    .and_then(|ind| {
                        ind.properties.iter()
                            .find(|(k, _)| k == "foundation:basePrompt")
                            .and_then(|(_, v)| v.as_literal())
                    })
                    .unwrap_or_default();

                let timeout_secs = get_literal_property(&conn, &agent_iri, "foundation:requestTimeout").await
                    .ok()
                    .flatten()
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(DEFAULT_TIMEOUT_SECS);

                Ok((api_key, model_identifier, system_prompt, timeout_secs))
            }
        })
        .await?;

    let resolved_description = interpolate(&description, ctx);
    let task_prompt = format!(
        "You are executing the automation task: **{}**\n\n{}\n\nComplete this task and respond with the result.",
        label,
        resolved_description
    );

    let mut messages: Vec<ChatMessage> = vec![ChatMessage {
        role: "user".to_string(),
        content: MessageContent::ContentBlocks(vec![
            ContentBlock::Text { text: task_prompt },
        ]),
    }];

    let tools = get_claude_tools();
    let provider = crate::ai::providers::ClaudeProvider::with_model(
        api_key.clone(),
        model_identifier.clone(),
        timeout_secs,
    );
    let assistant = AIAssistant::new(Box::new(provider));

    let mut last_text = String::new();

    for _ in 0..MAX_TOOL_LOOPS {
        let request = GenerateRequest {
            messages: messages.clone(),
            max_tokens: Some(DEFAULT_MAX_TOKENS),
            temperature: Some(DEFAULT_TEMPERATURE),
            system: Some(system_prompt.clone()),
            blackboard_context: None,
            tools: Some(tools.clone()),
            supports_web_tools: false,
        };

        let response = assistant.generate(request).await
            .map_err(|e| format!("Agent task API error: {}", e))?;

        let stop_reason = response.stop_reason.clone().unwrap_or_else(|| "end_turn".to_string());
        last_text = response.content.clone();

        let mut assistant_blocks: Vec<ContentBlock> = Vec::new();
        if !response.content.is_empty() {
            assistant_blocks.push(ContentBlock::Text { text: response.content.clone() });
        }
        for tc in &response.tool_calls {
            assistant_blocks.push(ContentBlock::ToolUse {
                id: tc.id.clone(),
                name: tc.name.clone(),
                input: tc.input.clone(),
            });
        }
        messages.push(ChatMessage {
            role: "assistant".to_string(),
            content: MessageContent::ContentBlocks(assistant_blocks),
        });

        if stop_reason != "tool_use" {
            break;
        }

        let mut result_blocks: Vec<ContentBlock> = Vec::new();
        for tc in &response.tool_calls {
            let call = FunctionToolCall {
                name: tc.name.clone(),
                arguments: tc.input.clone(),
            };
            let tc_id = tc.id.clone();
            let app_clone = app.clone();
            let result_json = executor
                .write(move |conn| async move {
                    let r = execute_fn(&conn, &call, Some(&app_clone)).await;
                    serde_json::to_string(&r).map_err(|e| e.to_string())
                })
                .await
                .unwrap_or_else(|e| format!("\"{}\"", e));

            let tool_result: crate::ai::functions::ToolResult =
                serde_json::from_str(&result_json).unwrap_or(crate::ai::functions::ToolResult {
                    success: false,
                    result: None,
                    error: Some(result_json.clone()),
                    concept: None,
                });

            let content = if tool_result.success {
                tool_result.result
                    .map(|v| serde_json::to_string(&v).unwrap_or_else(|_| v.to_string()))
                    .unwrap_or_default()
            } else {
                tool_result.error.unwrap_or_default()
            };

            result_blocks.push(ContentBlock::ToolResult {
                tool_use_id: tc_id,
                content,
                is_error: Some(!tool_result.success),
            });
        }

        messages.push(ChatMessage {
            role: "user".to_string(),
            content: MessageContent::ContentBlocks(result_blocks),
        });
    }

    Ok(last_text)
}
