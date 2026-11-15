use crate::ai::providers::{ContentBlock, MessageContent, UsageInfo};
use crate::ai::{GenerateRequest, GenerateResponse, ToolCall};
use futures_util::StreamExt;
use serde_json::{json, Value};
use std::collections::HashSet;
use tauri::Emitter;

pub struct OpenAIResponsesProvider {
    bearer_token: String,
    model: String,
    client: reqwest::Client,
}

enum StreamEvent {
    TextDelta(String),
    Completed(Value),
    Ignored,
}

impl OpenAIResponsesProvider {
    pub fn new(bearer_token: String, model: String, timeout_secs: u64) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            bearer_token,
            model,
            client,
        }
    }

    fn request_body(&self, request: &GenerateRequest, stream: bool) -> Result<Value, String> {
        let mut instructions = Vec::new();
        if let Some(system) = &request.system {
            instructions.push(system.clone());
        }
        if let Some(context) = &request.blackboard_context {
            instructions.push(context.clone());
        }

        let mut input = Vec::new();
        for message in &request.messages {
            match &message.content {
                MessageContent::Text(text) => {
                    input.push(json!({"role": message.role, "content": text}))
                }
                MessageContent::ContentBlocks(blocks) => {
                    for block in blocks {
                        match block {
                            ContentBlock::Text { text } => input.push(json!({"role": message.role, "content": text})),
                            ContentBlock::ToolUse { id, name, input: arguments } => input.push(json!({
                                "type":"function_call", "call_id":id, "name":name,
                                "arguments":arguments.to_string()
                            })),
                            ContentBlock::ToolResult { tool_use_id, content, is_error: _ } => input.push(json!({
                                "type":"function_call_output", "call_id":tool_use_id,
                                "output": if let Some(text) = content.as_str() { text.to_string() } else { content.to_string() }
                            })),
                            ContentBlock::Image { source } => input.push(json!({
                                "role": message.role,
                                "content": [{"type":"input_image", "image_url":format!("data:{};base64,{}", source.media_type, source.data)}]
                            })),
                            ContentBlock::Document { .. } | ContentBlock::Thinking { .. } | ContentBlock::RedactedThinking { .. } => {}
                        }
                    }
                }
            }
        }

        let mut body = json!({
            "model": self.model,
            "input": input,
            "store": false,
            "stream": stream,
            "max_output_tokens": request.max_tokens.unwrap_or(4096),
        });
        if !instructions.is_empty() {
            body["instructions"] = json!(instructions.join("\n\n"));
        }
        if let Some(temperature) = request.temperature {
            body["temperature"] = json!(temperature);
        }
        if let Some(tools) = &request.tools {
            if !tools.is_empty() {
                body["tools"] = json!(tools
                    .iter()
                    .map(|tool| json!({
                        "type":"function", "name":tool.name, "description":tool.description,
                        "parameters":tool.input_schema, "strict":false
                    }))
                    .collect::<Vec<_>>());
                body["tool_choice"] = request.tool_choice.clone().unwrap_or(json!("auto"));
            }
        }
        Ok(body)
    }

    fn api_error(status: reqwest::StatusCode, body: &str) -> String {
        let parsed: Value = serde_json::from_str(body).unwrap_or(Value::Null);
        let code = parsed["error"]["code"].as_str().unwrap_or("");
        let message = parsed["error"]["message"].as_str().unwrap_or(body);
        if status.as_u16() == 429
            && (code.contains("usage")
                || code.contains("quota")
                || message.to_lowercase().contains("usage limit"))
        {
            format!("CHATGPT_USAGE_LIMIT: {message}")
        } else {
            format!("OpenAI Responses API error {}: {}", status, message)
        }
    }

    pub async fn generate(&self, request: GenerateRequest) -> Result<GenerateResponse, String> {
        let body = self.request_body(&request, false)?;
        let response = self
            .client
            .post("https://api.openai.com/v1/responses")
            .bearer_auth(&self.bearer_token)
            .json(&body)
            .send()
            .await
            .map_err(|error| format!("OpenAI Responses request failed: {error}"))?;
        let status = response.status();
        let payload = response
            .text()
            .await
            .map_err(|error| format!("Could not read OpenAI response: {error}"))?;
        if !status.is_success() {
            return Err(Self::api_error(status, &payload));
        }
        let response: Value = serde_json::from_str(&payload)
            .map_err(|error| format!("Invalid OpenAI Responses payload: {error}"))?;
        Self::parse_completed(&response)
    }

    pub async fn generate_stream(
        &self,
        request: GenerateRequest,
        app: &tauri::AppHandle,
        conversation_id: &str,
    ) -> Result<GenerateResponse, String> {
        let body = self.request_body(&request, true)?;
        let response = self
            .client
            .post("https://api.openai.com/v1/responses")
            .bearer_auth(&self.bearer_token)
            .json(&body)
            .send()
            .await
            .map_err(|error| format!("OpenAI Responses stream failed: {error}"))?;
        let status = response.status();
        if !status.is_success() {
            let payload = response.text().await.unwrap_or_default();
            return Err(Self::api_error(status, &payload));
        }
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        let mut content = String::new();
        let mut completed = None;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|error| format!("OpenAI stream read failed: {error}"))?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));
            while let Some(end) = buffer.find("\n\n") {
                let event_block = buffer[..end].to_string();
                buffer.drain(..end + 2);
                let data = event_block
                    .lines()
                    .filter_map(|line| line.strip_prefix("data:"))
                    .map(str::trim)
                    .collect::<String>();
                if data.is_empty() {
                    continue;
                }
                let event: Value = serde_json::from_str(&data).unwrap_or(Value::Null);
                match Self::classify_stream_event(&event)? {
                    StreamEvent::TextDelta(delta) => {
                        content.push_str(&delta);
                        app.emit(
                            "chat-ai-delta",
                            json!({"conversationId":conversation_id,"type":"text","text":delta}),
                        )
                        .ok();
                    }
                    StreamEvent::Completed(response) => completed = Some(response),
                    StreamEvent::Ignored => {}
                }
            }
        }
        let mut result = Self::parse_completed(completed.as_ref().ok_or_else(|| {
            "OpenAI Responses stream ended without response.completed".to_string()
        })?)?;
        if !content.is_empty() {
            result.content = content;
        }
        Ok(result)
    }

    fn classify_stream_event(event: &Value) -> Result<StreamEvent, String> {
        match event["type"].as_str().unwrap_or("") {
            "response.output_text.delta" => Ok(event["delta"]
                .as_str()
                .map(|delta| StreamEvent::TextDelta(delta.to_string()))
                .unwrap_or(StreamEvent::Ignored)),
            "response.completed" => Ok(StreamEvent::Completed(event["response"].clone())),
            "response.failed" | "error" => Err(format!(
                "OpenAI Responses failed: {}",
                event["response"]["error"]["message"]
                    .as_str()
                    .or(event["message"].as_str())
                    .unwrap_or("unspecified provider error")
            )),
            // Function-call deltas are provisional. Only response.completed is authoritative.
            _ => Ok(StreamEvent::Ignored),
        }
    }

    fn parse_completed(response: &Value) -> Result<GenerateResponse, String> {
        let mut content = String::new();
        let mut tool_calls = Vec::new();
        let mut seen_call_ids = HashSet::new();
        for item in response["output"].as_array().into_iter().flatten() {
            match item["type"].as_str().unwrap_or("") {
                "message" => {
                    for part in item["content"].as_array().into_iter().flatten() {
                        if part["type"] == "output_text" {
                            content.push_str(part["text"].as_str().unwrap_or(""));
                        }
                    }
                }
                "function_call" => {
                    let call_id = item["call_id"]
                        .as_str()
                        .filter(|id| !id.is_empty())
                        .ok_or_else(|| "OpenAI function call is missing its call_id".to_string())?;
                    if !seen_call_ids.insert(call_id.to_string()) {
                        continue;
                    }
                    let name = item["name"]
                        .as_str()
                        .filter(|name| !name.is_empty())
                        .ok_or_else(|| {
                            format!("OpenAI function call '{call_id}' is missing its name")
                        })?;
                    let arguments = item["arguments"].as_str().ok_or_else(|| {
                        format!("OpenAI function call '{name}' ({call_id}) is missing arguments")
                    })?;
                    if arguments.trim().is_empty() {
                        return Err(format!(
                            "OpenAI function call '{name}' ({call_id}) returned empty arguments; expected a JSON object"
                        ));
                    }
                    let input: Value = serde_json::from_str(arguments).map_err(|error| {
                        format!("Invalid arguments for OpenAI function call '{name}' ({call_id}): {error}")
                    })?;
                    tool_calls.push(ToolCall {
                        id: call_id.to_string(),
                        name: name.to_string(),
                        input,
                    });
                }
                _ => {}
            }
        }
        let usage = response
            .get("usage")
            .filter(|value| !value.is_null())
            .map(|value| UsageInfo {
                input_tokens: value["input_tokens"].as_u64().unwrap_or(0) as u32,
                output_tokens: value["output_tokens"].as_u64().unwrap_or(0) as u32,
                cache_creation_input_tokens: 0,
                cache_read_input_tokens: 0,
            });
        Ok(GenerateResponse {
            content,
            stop_reason: Some(
                if tool_calls.is_empty() {
                    "end_turn"
                } else {
                    "tool_calls"
                }
                .to_string(),
            ),
            tool_calls,
            thinking_blocks: vec![],
            usage,
            model_used: response["model"].as_str().map(str::to_string),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{OpenAIResponsesProvider, StreamEvent};
    use serde_json::json;

    #[test]
    fn parses_tool_calls_from_completed_response_only() {
        let response = json!({
            "model": "test-model",
            "output": [{
                "type": "function_call",
                "call_id": "call-1",
                "name": "describe_individual",
                "arguments": "{\"iris\":[\"foundation:Person_1\"]}"
            }]
        });

        let parsed = OpenAIResponsesProvider::parse_completed(&response).unwrap();
        assert_eq!(parsed.tool_calls.len(), 1);
        assert_eq!(parsed.tool_calls[0].id, "call-1");
        assert_eq!(parsed.tool_calls[0].input["iris"][0], "foundation:Person_1");
    }

    #[test]
    fn completed_response_is_canonical_after_sse_argument_updates() {
        let sse_events = [
            json!({"type":"response.output_item.added", "output_index":0, "item":{"type":"function_call", "call_id":"call-1", "name":"describe_individual", "arguments":""}}),
            json!({"type":"response.function_call_arguments.delta", "output_index":0, "delta":"{\"iris\":[\"foundation:Person_1\"]}"}),
        ];
        for event in sse_events {
            assert!(matches!(
                OpenAIResponsesProvider::classify_stream_event(&event).unwrap(),
                StreamEvent::Ignored
            ));
        }
        let completed = json!({
            "output": [{
                "type": "function_call",
                "call_id": "call-1",
                "name": "describe_individual",
                "arguments": "{\"iris\":[\"foundation:Person_1\"]}"
            }]
        });
        let parsed = OpenAIResponsesProvider::parse_completed(&completed).unwrap();
        assert_eq!(parsed.tool_calls.len(), 1);
        assert_eq!(parsed.tool_calls[0].id, "call-1");
    }

    #[test]
    fn rejects_missing_empty_and_invalid_arguments() {
        for (arguments, expected) in [
            (None, "is missing arguments"),
            (Some(""), "returned empty arguments"),
            (Some("{"), "Invalid arguments for OpenAI function call"),
        ] {
            let mut call = json!({
                "type": "function_call",
                "call_id": "call-1",
                "name": "assert_individual"
            });
            if let Some(arguments) = arguments {
                call["arguments"] = json!(arguments);
            }
            let error =
                OpenAIResponsesProvider::parse_completed(&json!({"output": [call]})).unwrap_err();
            assert!(error.contains(expected), "unexpected error: {error}");
        }
    }

    #[test]
    fn preserves_distinct_tool_calls_and_deduplicates_repeated_call_ids() {
        let response = json!({
            "output": [
                {"type":"function_call", "call_id":"call-1", "name":"search", "arguments":"{\"query\":\"a\"}"},
                {"type":"function_call", "call_id":"call-1", "name":"search", "arguments":"{\"query\":\"a\"}"},
                {"type":"function_call", "call_id":"call-2", "name":"describe_individual", "arguments":"{\"iris\":[\"foundation:Person_1\"]}"}
            ]
        });

        let parsed = OpenAIResponsesProvider::parse_completed(&response).unwrap();
        assert_eq!(parsed.tool_calls.len(), 2);
        assert_eq!(parsed.tool_calls[0].id, "call-1");
        assert_eq!(parsed.tool_calls[1].id, "call-2");
    }
}
