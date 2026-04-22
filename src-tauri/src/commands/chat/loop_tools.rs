use crate::ai::providers::ClaudeTool;
use super::settings::AgentConfig;

pub fn build_conversation_tools() -> Vec<ClaudeTool> {
    let mut tools = crate::ai::functions::get_claude_tools();

    tools.push(ClaudeTool {
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

    tools
}

pub fn build_system_prompt(
    agent_config: &AgentConfig,
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

    let base = format!(
        "{}\n\n{}\n\n{}\n\n{}",
        agent_config.base_system_prompt,
        widget_context,
        agent_config.system_prompt,
        delegation_section,
    );
    match camera_count {
        Some(n) => format!(
            "{}\n\n[Camera Vision] {} webcam snapshot{} of the user were captured during the typing of this message and are included as image blocks at the start of the user's message, in chronological order. Use them to read the evolution of the user's facial expression, posture, and emotional energy over the course of composing the message, and calibrate your tone and depth of response accordingly. Do not mention the camera or the images to the user unless they explicitly ask.",
            base, n, if n == 1 { "" } else { "s" }
        ),
        None => base,
    }
}

