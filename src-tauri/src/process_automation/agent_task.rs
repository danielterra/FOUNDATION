use tauri::{AppHandle, Emitter, Manager};

use crate::ai::{ChatMessage};
use crate::ai::providers::{MessageContent, ContentBlock, ClaudeTool};
use crate::ai::functions::get_claude_tools;
use crate::owl::{DbExecutor, get_literal_property, get_iri_property, get_all_iri_properties, Individual, Object};

use super::executor::ExecutionContext;
use super::tool_loop::{ToolLoopConfig, CompletionToolConfig, run_tool_loop};
use super::agent_runner::resolve_agent_config;

type Result<T> = std::result::Result<T, String>;

const DEFAULT_MAX_TOKENS: u32 = 4096;
const DEFAULT_TEMPERATURE: f32 = 0.3;

pub(super) fn to_storage_blocks(content: &MessageContent) -> Vec<crate::commands::chat_storage::ContentBlock> {
    use crate::commands::chat_storage::ContentBlock as S;
    use ContentBlock as A;

    let blocks = match content {
        MessageContent::ContentBlocks(b) => b.as_slice(),
        MessageContent::Text(t) => return vec![S::Text { text: t.clone() }],
    };

    blocks.iter().filter_map(|b| match b {
        A::Text { text } => Some(S::Text { text: text.clone() }),
        A::ToolUse { id, name, input } => Some(S::ToolUse {
            id: id.clone(), name: name.clone(), input: input.clone(), reason: None,
        }),
        A::ToolResult { tool_use_id, content, is_error, .. } => Some(S::ToolResult {
            tool_use_id: tool_use_id.clone(),
            content: match content {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            },
            is_error: *is_error,
            duration_ms: None,
        }),
        A::Thinking { thinking, signature } => Some(S::Thinking {
            thinking: thinking.clone(), signature: signature.clone(),
        }),
        A::RedactedThinking { data } => Some(S::RedactedThinking { data: data.clone() }),
        A::Image { .. } | A::Document { .. } => None,
    }).collect()
}

fn task_complete_tool(output_class: Option<&str>) -> ClaudeTool {
    let (description, output_iri_description) = match output_class {
        Some(class) => (
            format!("Signal explicit completion of this AgentTask. You MUST pass the IRI of a {} individual in output_iri — the executor forwards it to the next step as its input.", class),
            format!("The IRI of the {} individual produced by this task (e.g. 'foundation:Task_1234567890'). Must exist in the ontology before calling this tool.", class),
        ),
        None => (
            "Signal explicit completion of this AgentTask. Pass the single output IRI (the main entity produced by this task) in output_iri — the executor forwards it to the next step as its input.".to_string(),
            "The single IRI produced by this task, forwarded as input to the next step.".to_string(),
        ),
    };
    ClaudeTool {
        name: "task_complete".to_string(),
        description,
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "output_iri": { "type": "string", "description": output_iri_description },
                "message": { "type": "string", "description": "Optional message summarising the outcome." }
            },
            "required": []
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

pub async fn create_conversation(
    executor: &DbExecutor,
    origin_iri: &str,
    origin_property: &str,
    task_label: &str,
    agent_iri: Option<&str>,
) -> String {
    let conv_iri = format!("foundation:AIConversation_{}", chrono::Utc::now().timestamp_millis());
    let conv_label = format!("{} — Execution", task_label);
    let origin = origin_iri.to_string();
    let prop = origin_property.to_string();
    let conv = conv_iri.clone();
    let agent = agent_iri.map(|s| s.to_string());
    executor.write(move |conn| {
        let ind = Individual::new(&conv);
        ind.assert(conn, "foundation:AIConversation", &conv_label, "smart_toy", "process_automation")
            .map_err(|e| e.to_string())?;
        let mut triples = vec![
            crate::eavto::Triple::new(&conv, &prop, Object::Iri(origin)),
        ];
        if let Some(a) = agent {
            triples.push(crate::eavto::Triple::new(&conv, "foundation:handledBy", Object::Iri(a)));
        }
        crate::eavto::store::assert_triples(conn, &triples, "process_automation")
            .map_err(|e| e.to_string())?;
        Ok(conv)
    }).await.unwrap_or_default()
}

pub async fn persist_messages(
    executor: &DbExecutor,
    conv_iri: &str,
    messages: &[ChatMessage],
    model_identifier: &str,
) {
    for msg in messages {
        let role = msg.role.as_str();
        let model = if role == "assistant" { Some(model_identifier) } else { None };
        if let Err(e) = crate::commands::chat_storage::create_message(
            executor, conv_iri, role,
            to_storage_blocks(&msg.content),
            model, None, None,
        ).await {
            crate::commands::log_backend("error", &format!("[agent_task] persist_messages failed: {}", e));
        }
    }
}

pub async fn execute_agent_task(
    app: &AppHandle,
    node_iri: &str,
    ctx: &ExecutionContext,
    step_iri: &str,
    exec_iri: &str,
) -> Result<String> {
    let executor = app.state::<DbExecutor>();

    let (label, description, agent_iri, allowed_tool_names, output_class) = executor
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
                    .filter_map(|iri| get_literal_property(conn, iri, "foundation:functionName").ok().flatten())
                    .collect();

                let output_class = get_iri_property(conn, &node_iri, "foundation:outputClass")
                    .map_err(|e| e.to_string())?;

                Ok((label, description, agent_iri, allowed_tool_names, output_class))
            }
        })
        .await?;

    let agent_config = resolve_agent_config(&executor, &agent_iri).await?;

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

    let current_datetime = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let task_prompt = format!(
        "currentDateTime: {}\n\nYou are executing the automation task: **{}**{}\n\n## Instructions\n{}\n\nComplete this task and respond with the result.",
        current_datetime, label, ctx_section, resolved_description
    );

    let mut tools: Vec<ClaudeTool> = if allowed_tool_names.is_empty() {
        get_claude_tools()
    } else {
        crate::ai::functions::get_available_tools()
            .into_iter()
            .filter(|t| allowed_tool_names.contains(&t.name))
            .map(|t| t.to_claude_tool())
            .collect()
    };
    tools.push(task_complete_tool(output_class.as_deref()));

    let initial_messages = vec![ChatMessage {
        role: "user".to_string(),
        content: MessageContent::ContentBlocks(vec![
            ContentBlock::Text { text: task_prompt },
        ]),
    }];

    let loop_config = ToolLoopConfig {
        system: Some(agent_config.system_prompt),
        tools,
        max_iterations: 50,
        max_tokens: DEFAULT_MAX_TOKENS,
        temperature: DEFAULT_TEMPERATURE,
        completion_tool: Some(CompletionToolConfig {
            tool_name: "task_complete".to_string(),
            output_class: output_class.clone(),
        }),
        compaction_threshold: 0.80,
        context_window: 180_000,
        persist_to: None,
    };

    let output = run_tool_loop(&executor, &agent_config.provider, initial_messages, loop_config).await?;

    let conv_iri = create_conversation(&executor, step_iri, "foundation:generatedByStep", &label, Some(&agent_iri)).await;
    persist_messages(&executor, &conv_iri, &output.messages, &agent_config.model_identifier).await;

    app.emit("automation-step-message", serde_json::json!({
        "executionIri": exec_iri,
        "stepIri": step_iri,
        "lastText": output.last_text,
    })).ok();

    match output.completion {
        Some(c) => Ok(if !c.output_iri.is_empty() { c.output_iri } else { c.message }),
        None => Ok(output.last_text),
    }
}
