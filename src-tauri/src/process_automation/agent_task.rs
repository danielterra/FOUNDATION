use tauri::{AppHandle, Manager};

use crate::ai::{AIAssistant, GenerateRequest, ChatMessage};
use crate::ai::providers::{MessageContent, ContentBlock, ClaudeTool};
use crate::ai::functions::{get_claude_tools, ToolCall as FunctionToolCall, execute_tool as execute_fn};
use crate::owl::{DbExecutor, get_literal_property, get_iri_property, get_all_iri_properties, Individual, Object};

use super::executor::ExecutionContext;

type Result<T> = std::result::Result<T, String>;

const MAX_TOOL_LOOPS: usize = 50;
const DEFAULT_MAX_TOKENS: u32 = 4096;
const DEFAULT_TEMPERATURE: f32 = 0.3;
const DEFAULT_TIMEOUT_SECS: u64 = 180;

fn task_complete_tool() -> ClaudeTool {
    ClaudeTool {
        name: "task_complete".to_string(),
        description: "Signal explicit completion of this AgentTask. Call this to end the task with a typed outcome. If you do not call this, the task will time out as a failure.".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "success": {
                    "type": "boolean",
                    "description": "true if the task succeeded, false if it failed."
                },
                "output_iris": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "IRIs produced by this task, passed as inputIRIs to the next step. Can be empty."
                },
                "message": {
                    "type": "string",
                    "description": "Optional message summarising the outcome or failure reason."
                }
            },
            "required": ["success"]
        }),
    }
}

fn interpolate(template: &str, ctx: &ExecutionContext) -> String {
    let mut result = template.to_string();
    for (key, value) in ctx {
        result = result.replace(&format!("{{{{{}}}}}", key), value);
    }
    result
}

fn content_to_json(content: &MessageContent) -> String {
    match content {
        MessageContent::ContentBlocks(blocks) => {
            serde_json::to_string(blocks).unwrap_or_default()
        }
        MessageContent::Text(text) => {
            serde_json::to_string(&serde_json::json!([{"type": "text", "text": text}]))
                .unwrap_or_default()
        }
    }
}

async fn persist_conversation(
    executor: &DbExecutor,
    step_iri: &str,
    task_label: &str,
    model: &str,
    messages: &[ChatMessage],
) {
    let base_ms = chrono::Utc::now().timestamp_millis();
    let conv_iri = format!("foundation:AIConversation_{}", base_ms);
    let conv_label = format!("{} — Execution", task_label);

    let step = step_iri.to_string();
    let conv = conv_iri.clone();
    let label = conv_label.clone();
    executor.write(move |conn| {
        let ind = Individual::new(&conv);
        ind.assert(conn, "foundation:AIConversation", &label, "smart_toy", "process_automation")
            .map_err(|e| e.to_string())?;
        let link = crate::eavto::Triple::new(
            &step,
            "foundation:hasConversation",
            Object::Iri(conv.clone()),
        );
        crate::eavto::store::assert_triples(conn, &[link], "process_automation")
            .map_err(|e| e.to_string())?;
        Ok(conv)
    }).await.ok();

    for (i, msg) in messages.iter().enumerate() {
        let msg_iri = format!("foundation:AIConversationMessage_{}", base_ms + 1 + i as i64);
        let role = msg.role.clone();
        let content_json = content_to_json(&msg.content);
        let conv_iri_for_msg = conv_iri.clone();
        let model_str = model.to_string();

        executor.write(move |conn| {
            let ind = Individual::new(&msg_iri);
            ind.assert(conn, "foundation:AIConversationMessage", &role, "chat", "process_automation")
                .map_err(|e| e.to_string())?;

            let mut triples = vec![
                crate::eavto::Triple::new(
                    &msg_iri, "foundation:role",
                    Object::Literal { value: role.clone(), datatype: Some("xsd:string".to_string()), language: None },
                ),
                crate::eavto::Triple::new(
                    &msg_iri, "foundation:content",
                    Object::Literal { value: content_json, datatype: Some("xsd:string".to_string()), language: None },
                ),
                crate::eavto::Triple::new(
                    &msg_iri, "foundation:partOfConversation",
                    Object::Iri(conv_iri_for_msg),
                ),
            ];
            if role == "assistant" && !model_str.is_empty() {
                triples.push(crate::eavto::Triple::new(
                    &msg_iri, "foundation:model",
                    Object::Literal { value: model_str, datatype: Some("xsd:string".to_string()), language: None },
                ));
            }

            crate::eavto::store::assert_triples(conn, &triples, "process_automation")
                .map_err(|e| e.to_string())?;
            Ok(msg_iri)
        }).await.ok();
    }
}

pub async fn execute_agent_task(
    app: &AppHandle,
    node_iri: &str,
    ctx: &ExecutionContext,
    step_iri: &str,
) -> Result<String> {
    let executor = app.state::<DbExecutor>();

    let (label, description, agent_iri, allowed_tool_names) = executor
        .read({
            let node_iri = node_iri.to_string();
            move |conn| {
                let label = get_literal_property(conn, &node_iri, "rdfs:label")
                    .map_err(|e| e.to_string())?
                    .unwrap_or_else(|| node_iri.clone());

                let description = get_literal_property(conn, &node_iri, "rdfs:comment")
                    .map_err(|e| e.to_string())?
                    .unwrap_or_default();

                let agent_iri = get_iri_property(conn, &node_iri, "foundation:assignedAgent")
                    .map_err(|e| e.to_string())?
                    .unwrap_or_else(|| "foundation:LocalAIAssistant".to_string());

                let tool_iris = get_all_iri_properties(conn, &node_iri, "foundation:allowedTools")
                    .unwrap_or_default();
                let allowed_tool_names: Vec<String> = tool_iris.iter()
                    .filter_map(|iri| get_literal_property(conn, iri, "rdfs:label").ok().flatten())
                    .collect();

                Ok((label, description, agent_iri, allowed_tool_names))
            }
        })
        .await?;

    let (api_key, model_identifier, system_prompt, timeout_secs) = executor
        .read({
            let agent_iri = agent_iri.clone();
            move |conn| {
                let service_iri = get_iri_property(conn, &agent_iri, "foundation:usesService")
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| format!("Agent {} has no usesService", agent_iri))?;

                let api_key_iri = get_iri_property(conn, &service_iri, "foundation:apiKey")
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| "API key not configured".to_string())?;

                let api_key = get_literal_property(conn, &api_key_iri, "foundation:credentialValue")
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| "API key has no value".to_string())?;

                let model_iri = get_iri_property(conn, &agent_iri, "foundation:usesModel")
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| format!("Agent {} has no usesModel", agent_iri))?;

                let model_identifier = get_literal_property(conn, &model_iri, "foundation:modelIdentifier")
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| format!("Model {} has no modelIdentifier", model_iri))?;

                let system_prompt = Individual::get(conn, &agent_iri)
                    .ok()
                    .flatten()
                    .and_then(|ind| {
                        ind.properties.iter()
                            .find(|(k, _)| k == "foundation:basePrompt")
                            .and_then(|(_, v)| v.as_literal())
                    })
                    .unwrap_or_default();

                let timeout_secs = get_literal_property(conn, &agent_iri, "foundation:requestTimeout")
                    .ok()
                    .flatten()
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(DEFAULT_TIMEOUT_SECS);

                Ok((api_key, model_identifier, system_prompt, timeout_secs))
            }
        })
        .await?;

    let resolved_description = interpolate(&description, ctx);

    let ctx_section = if ctx.is_empty() {
        String::new()
    } else {
        let ctx_snapshot: Vec<(String, String)> = ctx.iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        let entity_blocks = executor.read(move |conn| {
            let mut blocks = Vec::new();
            for (key, value) in &ctx_snapshot {
                if !value.contains(':') || value.contains(' ') {
                    continue;
                }
                if let Ok(Some(individual)) = Individual::get(conn, value) {
                    let type_labels: Vec<&str> = individual.types.iter()
                        .map(|t| t.label.as_str())
                        .collect();
                    let props: Vec<String> = individual.properties.iter()
                        .filter(|(k, _)| k != "rdf:type" && k != "foundation:hasStatus")
                        .map(|(k, v)| {
                            let val = match v {
                                Object::Iri(s) => s.clone(),
                                Object::Literal { value, .. } => value.clone(),
                                Object::Integer(i) => i.to_string(),
                                Object::Number(n) => n.to_string(),
                                Object::Boolean(b) => b.to_string(),
                                Object::DateTime(s) => s.clone(),
                                Object::Blank(s) => s.clone(),
                            };
                            format!("  {}: {}", k, val)
                        })
                        .collect();
                    let status_iri = individual.properties.iter()
                        .find(|(k, _)| k == "foundation:hasStatus")
                        .and_then(|(_, v)| if let Object::Iri(s) = v { Some(s.as_str()) } else { None });
                    let status_line = status_iri
                        .map(|s| format!("\n  status: {}", s))
                        .unwrap_or_default();
                    let comment_line = individual.comment.as_deref()
                        .map(|c| format!("\n  description: {}", c))
                        .unwrap_or_default();
                    blocks.push(format!(
                        "**{}** = `{}` ({})\n  label: {}{}{}\n{}",
                        key, individual.iri,
                        type_labels.join(", "),
                        individual.label.as_deref().unwrap_or(&individual.iri),
                        status_line,
                        comment_line,
                        props.join("\n")
                    ));
                } else {
                    blocks.push(format!("**{}** = `{}`", key, value));
                }
            }
            Ok::<_, String>(blocks)
        }).await.unwrap_or_default();

        format!("\n\n## Input Data\n{}", entity_blocks.join("\n\n"))
    };

    let task_prompt = format!(
        "You are executing the automation task: **{}**{}\n\n## Instructions\n{}\n\nComplete this task and respond with the result.",
        label,
        ctx_section,
        resolved_description
    );

    let mut messages: Vec<ChatMessage> = vec![ChatMessage {
        role: "user".to_string(),
        content: MessageContent::ContentBlocks(vec![
            ContentBlock::Text { text: task_prompt },
        ]),
    }];

    let mut tools: Vec<ClaudeTool> = if allowed_tool_names.is_empty() {
        get_claude_tools()
    } else {
        crate::ai::functions::get_available_tools()
            .into_iter()
            .filter(|t| allowed_tool_names.contains(&t.name))
            .map(|t| t.to_claude_tool())
            .collect()
    };
    tools.push(task_complete_tool());
    let provider = crate::ai::providers::ClaudeProvider::with_model(
        api_key.clone(),
        model_identifier.clone(),
        timeout_secs,
    );
    let assistant = AIAssistant::new(Box::new(provider));

    let mut last_text = String::new();
    let mut task_completion: Option<Result<(Vec<String>, String)>> = None;

    'outer: for _ in 0..MAX_TOOL_LOOPS {
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
            if tc.name == "task_complete" {
                let success = tc.input.get("success").and_then(|v| v.as_bool()).unwrap_or(true);
                let output_iris: Vec<String> = tc.input.get("output_iris")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                    .unwrap_or_default();
                let message = tc.input.get("message").and_then(|v| v.as_str()).unwrap_or("").to_string();

                result_blocks.push(ContentBlock::ToolResult {
                    tool_use_id: tc.id.clone(),
                    content: "Task completion acknowledged.".to_string(),
                    is_error: Some(false),
                });
                messages.push(ChatMessage {
                    role: "user".to_string(),
                    content: MessageContent::ContentBlocks(result_blocks),
                });

                task_completion = Some(if success {
                    Ok((output_iris, message))
                } else {
                    Err(if message.is_empty() { "Agent task marked as failed via task_complete".to_string() } else { message })
                });
                break 'outer;
            }

            let call = FunctionToolCall {
                name: tc.name.clone(),
                arguments: tc.input.clone(),
            };
            let tc_id = tc.id.clone();
            let app_clone = app.clone();
            let result_json = executor
                .write(move |conn| {
                    let r = execute_fn(conn, &call, Some(&app_clone), None);
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

        if task_completion.is_none() {
            messages.push(ChatMessage {
                role: "user".to_string(),
                content: MessageContent::ContentBlocks(result_blocks),
            });
        }
    }

    persist_conversation(&executor, step_iri, &label, &model_identifier, &messages).await;

    match task_completion {
        Some(Ok((output_iris, message))) => {
            let first = output_iris.first().cloned().unwrap_or_default();
            Ok(if !first.is_empty() { first } else { message })
        }
        Some(Err(e)) => Err(e),
        None => Ok(last_text),
    }
}
