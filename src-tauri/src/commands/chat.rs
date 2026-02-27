use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use tiktoken_rs::cl100k_base;

use crate::eavto::{DbExecutor, query};
use crate::owl::{Individual, Object, Thing};
use crate::ai::functions::{self, FunctionCall, FunctionResult};
use crate::commands::log_backend;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolUseInfo {
    pub iri: String,
    pub tool_name: String,
    pub input: String,
    pub tool_use_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResultInfo {
    pub iri: String,
    pub result_content: String,
    pub is_success: bool,
    pub result_of_iri: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageInfo {
    pub iri: String,
    pub content: String,
    pub sender_iri: String,
    pub sender_label: String,
    pub receiver_iri: String,
    pub receiver_label: String,
    pub sent_at: String,
    pub conversation_iri: Option<String>,
    pub tool_uses: Vec<ToolUseInfo>,
    pub tool_results: Vec<ToolResultInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationInfo {
    pub iri: String,
    pub started_at: String,
    pub topic: Option<String>,
    pub status: Option<String>,
    pub participant_count: usize,
}

/// Send a message from user to AI assistant
#[tauri::command]
#[allow(non_snake_case)]
pub async fn chat__send_message(
    _app: AppHandle,
    content: String,
    latitude: Option<f64>,
    longitude: Option<f64>,
    executor: State<'_, DbExecutor>,
) -> Result<String, String> {
    executor.write(move |conn| {
        // Get or create the main conversation
        let conversation_iri = "foundation:MainChatConversation";
        let conversation = Individual::new(conversation_iri);

        // Check if conversation exists, if not create it
        if !conversation.exists(conn)
            .map_err(|e| format!("Failed to check conversation: {}", e))?
        {
            let now = chrono::Utc::now().to_rfc3339();

            conversation.assert(
                conn,
                "foundation:Conversation",
                "Main Chat Conversation",
                "chat",
                "chat"
            ).map_err(|e| format!("Failed to create conversation: {}", e))?;

            conversation.add_property(
                conn,
                "foundation:startedAt",
                Object::Literal {
                    value: now.clone(),
                    datatype: Some("xsd:dateTime".to_string()),
                    language: None,
                },
                "chat"
            ).map_err(|e| format!("Failed to set startedAt: {}", e))?;

            conversation.add_property(
                conn,
                "foundation:hasParticipant",
                Object::Iri("foundation:ThisUser".to_string()),
                "chat"
            ).map_err(|e| format!("Failed to add user participant: {}", e))?;

            conversation.add_property(
                conn,
                "foundation:hasParticipant",
                Object::Iri("foundation:LocalAIAssistant".to_string()),
                "chat"
            ).map_err(|e| format!("Failed to add AI participant: {}", e))?;

            conversation.add_property(
                conn,
                "foundation:conversationStatus",
                Object::Literal {
                    value: "active".to_string(),
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                },
                "chat"
            ).map_err(|e| format!("Failed to set status: {}", e))?;
        }

        // Create unique message IRI
        let timestamp = chrono::Utc::now().timestamp_millis();
        let message_iri = format!("foundation:Message_{}", timestamp);
        let message = Individual::new(&message_iri);
        let now = chrono::Utc::now().to_rfc3339();

        // Create message (truncate safely respecting char boundaries)
        let label = content.chars().take(50).collect::<String>();
        message.assert(
            conn,
            "foundation:Message",
            &label,
            "chat",
            "chat"
        ).map_err(|e| format!("Failed to create message: {}", e))?;

        // Add content
        message.add_property(
            conn,
            "foundation:content",
            Object::Literal {
                value: content,
                datatype: Some("xsd:string".to_string()),
                language: Some("en".to_string()),
            },
            "chat"
        ).map_err(|e| format!("Failed to add content: {}", e))?;

        // Set sender (user)
        message.add_property(
            conn,
            "foundation:sender",
            Object::Iri("foundation:ThisUser".to_string()),
            "chat"
        ).map_err(|e| format!("Failed to set sender: {}", e))?;

        // Set receiver (AI)
        message.add_property(
            conn,
            "foundation:receiver",
            Object::Iri("foundation:LocalAIAssistant".to_string()),
            "chat"
        ).map_err(|e| format!("Failed to set receiver: {}", e))?;

        // Set timestamp
        message.add_property(
            conn,
            "foundation:sentAt",
            Object::DateTime(timestamp),
            "chat"
        ).map_err(|e| format!("Failed to set sentAt: {}", e))?;

        // Link to conversation
        message.add_property(
            conn,
            "foundation:partOfConversation",
            Object::Iri(conversation_iri.to_string()),
            "chat"
        ).map_err(|e| format!("Failed to link to conversation: {}", e))?;

        // Set message type
        message.add_property(
            conn,
            "foundation:messageType",
            Object::Literal {
                value: "text".to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            },
            "chat"
        ).map_err(|e| format!("Failed to set messageType: {}", e))?;

        // Update user location if provided
        if let (Some(lat), Some(lon)) = (latitude, longitude) {
            let user = Individual::new("foundation:ThisUser");

            // Create a location record with timestamp
            let location_iri = format!("foundation:Location_{}", timestamp);
            let location = Individual::new(&location_iri);

            location.assert(
                conn,
                "foundation:Location",
                &format!("Location at {}", now.clone()),
                "location",
                "chat"
            ).map_err(|e| format!("Failed to create location: {}", e))?;

            location.add_property(
                conn,
                "foundation:latitude",
                Object::Literal {
                    value: lat.to_string(),
                    datatype: Some("xsd:double".to_string()),
                    language: None,
                },
                "chat"
            ).map_err(|e| format!("Failed to set latitude: {}", e))?;

            location.add_property(
                conn,
                "foundation:longitude",
                Object::Literal {
                    value: lon.to_string(),
                    datatype: Some("xsd:double".to_string()),
                    language: None,
                },
                "chat"
            ).map_err(|e| format!("Failed to set longitude: {}", e))?;

            location.add_property(
                conn,
                "foundation:recordedAt",
                Object::Literal {
                    value: now.clone(),
                    datatype: Some("xsd:dateTime".to_string()),
                    language: None,
                },
                "chat"
            ).map_err(|e| format!("Failed to set recordedAt: {}", e))?;

            // Link location to user
            user.add_property(
                conn,
                "foundation:hasLocation",
                Object::Iri(location_iri),
                "chat"
            ).map_err(|e| format!("Failed to link location to user: {}", e))?;
        }

        Ok(message_iri)
    }).await
}

/// Get recent messages from the main conversation
#[tauri::command]
#[allow(non_snake_case)]
pub async fn chat__get_recent_messages(
    limit: Option<usize>,
    executor: State<'_, DbExecutor>,
) -> Result<Vec<MessageInfo>, String> {
    let limit_val = limit.unwrap_or(10);

    executor.read(move |conn| {
        let conversation_iri = "foundation:MainChatConversation";

        // Find all messages that are part of this conversation
        let result = query::get_by_predicate_object(
            conn,
            "foundation:partOfConversation",
            conversation_iri
        ).map_err(|e| format!("Failed to find messages: {}", e))?;

        // Fetch full details for each message
        let mut messages_with_time: Vec<(MessageInfo, String)> = Vec::new();

        for triple in result.triples {
            let message_iri = &triple.subject;

            // Get content
            let content = query::get_by_entity_predicate(conn, message_iri, "foundation:content")
                .ok()
                .and_then(|r| r.triples.first().and_then(|t| t.object.as_literal()))
                .unwrap_or_default();

            // Get sender
            let sender_iri = query::get_by_entity_predicate(conn, message_iri, "foundation:sender")
                .ok()
                .and_then(|r| r.triples.first().and_then(|t| t.object.as_iri().map(|s| s.to_string())))
                .unwrap_or_else(|| "unknown".to_string());

            let sender_thing = Thing::get(conn, &sender_iri);

            // Get receiver
            let receiver_iri = query::get_by_entity_predicate(conn, message_iri, "foundation:receiver")
                .ok()
                .and_then(|r| r.triples.first().and_then(|t| t.object.as_iri().map(|s| s.to_string())))
                .unwrap_or_else(|| "unknown".to_string());

            let receiver_thing = Thing::get(conn, &receiver_iri);

            // Get timestamp
            let sent_at = query::get_by_entity_predicate(conn, message_iri, "foundation:sentAt")
                .ok()
                .and_then(|r| r.triples.first().and_then(|t| t.object.as_literal()))
                .unwrap_or_default();

            // Load ToolUse entities linked to this message
            let mut tool_uses = Vec::new();
            if let Ok(tool_use_result) = query::get_by_predicate_object(
                conn,
                "foundation:partOfMessage",
                message_iri
            ) {
                log_backend("info", &format!("[CHAT] Found {} triples with partOfMessage for {}", tool_use_result.triples.len(), message_iri));
                for tool_use_triple in tool_use_result.triples {
                    let tool_use_iri = &tool_use_triple.subject;
                    log_backend("info", &format!("[CHAT] Checking subject: {}", tool_use_iri));
                    if tool_use_iri.starts_with("foundation:ToolUse_") {
                        log_backend("info", &format!("[CHAT] Loading ToolUse: {}", tool_use_iri));
                        if let Ok(tool_use_ind) = Individual::get(conn, tool_use_iri) {
                            let tool_name = tool_use_ind.properties.iter()
                                .find(|(k, _)| k == "foundation:toolName")
                                .and_then(|(_, v)| v.as_literal())
                                .unwrap_or_default();

                            let input = tool_use_ind.properties.iter()
                                .find(|(k, _)| k == "foundation:toolInput")
                                .and_then(|(_, v)| v.as_literal())
                                .unwrap_or_default();

                            let tool_use_id = tool_use_ind.properties.iter()
                                .find(|(k, _)| k == "foundation:toolUseId")
                                .and_then(|(_, v)| v.as_literal())
                                .unwrap_or_default();

                            tool_uses.push(ToolUseInfo {
                                iri: tool_use_iri.to_string(),
                                tool_name: tool_name.clone(),
                                input: input.clone(),
                                tool_use_id: tool_use_id.clone(),
                            });
                            let input_preview = if input.len() > 100 {
                                format!("{}...", &input[..100])
                            } else {
                                input.clone()
                            };
                            log_backend("info", &format!("[CHAT] Added ToolUse: {} ({}) with input: {}", tool_use_iri, tool_name, input_preview));
                        } else {
                            log_backend("warn", &format!("[CHAT] Failed to load Individual for ToolUse: {}", tool_use_iri));
                        }
                    }
                }
            } else {
                log_backend("info", &format!("[CHAT] No ToolUse query results for {}", message_iri));
            }

            // Load ToolResult entities linked to this message
            let mut tool_results = Vec::new();
            if let Ok(tool_result_result) = query::get_by_predicate_object(
                conn,
                "foundation:partOfMessage",
                message_iri
            ) {
                log_backend("info", &format!("[CHAT] Found {} triples with partOfMessage for {}", tool_result_result.triples.len(), message_iri));
                for tool_result_triple in tool_result_result.triples {
                    let tool_result_iri = &tool_result_triple.subject;
                    log_backend("info", &format!("[CHAT] Checking subject: {}", tool_result_iri));
                    if tool_result_iri.starts_with("foundation:ToolResult_") {
                        log_backend("info", &format!("[CHAT] Loading ToolResult: {}", tool_result_iri));
                        if let Ok(tool_result_ind) = Individual::get(conn, tool_result_iri) {
                            let result_content = tool_result_ind.properties.iter()
                                .find(|(k, _)| k == "foundation:resultContent")
                                .and_then(|(_, v)| v.as_literal())
                                .unwrap_or_default();

                            let is_success = tool_result_ind.properties.iter()
                                .find(|(k, _)| k == "foundation:isSuccess")
                                .and_then(|(_, v)| v.as_literal())
                                .map(|s| s == "true")
                                .unwrap_or(false);

                            let result_of_iri = tool_result_ind.properties.iter()
                                .find(|(k, _)| k == "foundation:resultOf")
                                .and_then(|(_, v)| v.as_iri())
                                .unwrap_or_default();

                            tool_results.push(ToolResultInfo {
                                iri: tool_result_iri.to_string(),
                                result_content: result_content.clone(),
                                is_success,
                                result_of_iri: result_of_iri.to_string(),
                            });
                            log_backend("info", &format!("[CHAT] Added ToolResult: {} (success: {})", tool_result_iri, is_success));
                        } else {
                            log_backend("warn", &format!("[CHAT] Failed to load Individual for ToolResult: {}", tool_result_iri));
                        }
                    }
                }
            } else {
                log_backend("info", &format!("[CHAT] No ToolResult query results for {}", message_iri));
            }

            messages_with_time.push((
                MessageInfo {
                    iri: message_iri.clone(),
                    content,
                    sender_iri,
                    sender_label: sender_thing.label,
                    receiver_iri,
                    receiver_label: receiver_thing.label,
                    sent_at: sent_at.clone(),
                    conversation_iri: Some(conversation_iri.to_string()),
                    tool_uses,
                    tool_results,
                },
                sent_at,
            ));
        }

        // Sort by timestamp (descending) and take limit
        messages_with_time.sort_by(|a, b| b.1.cmp(&a.1));
        messages_with_time.truncate(limit_val);

        // Reverse to get chronological order (oldest first) and extract messages
        messages_with_time.reverse();
        let messages = messages_with_time.into_iter().map(|(msg, _)| msg).collect();

        Ok(messages)
    }).await
}

/// Get conversation info
#[tauri::command]
#[allow(non_snake_case)]
pub async fn chat__get_conversation_info(
    executor: State<'_, DbExecutor>,
) -> Result<Option<ConversationInfo>, String> {
    executor.read(|conn| {
        let conversation_iri = "foundation:MainChatConversation";
        let conversation = Individual::new(conversation_iri);

        // Check if exists
        if !conversation.exists(conn)
            .map_err(|e| format!("Failed to check conversation: {}", e))?
        {
            return Ok(None);
        }

        // Get started_at
        let started_at = query::get_by_entity_predicate(conn, conversation_iri, "foundation:startedAt")
            .ok()
            .and_then(|r| r.triples.first().and_then(|t| t.object.as_literal()))
            .unwrap_or_default();

        // Get topic
        let topic = query::get_by_entity_predicate(conn, conversation_iri, "foundation:topic")
            .ok()
            .and_then(|r| r.triples.first().and_then(|t| t.object.as_literal()));

        // Get status
        let status = query::get_by_entity_predicate(conn, conversation_iri, "foundation:conversationStatus")
            .ok()
            .and_then(|r| r.triples.first().and_then(|t| t.object.as_literal()));

        // Count participants
        let participants = query::get_by_entity_predicate(conn, conversation_iri, "foundation:hasParticipant")
            .map_err(|e| format!("Failed to get participants: {}", e))?;

        Ok(Some(ConversationInfo {
            iri: conversation_iri.to_string(),
            started_at,
            topic,
            status,
            participant_count: participants.triples.len(),
        }))
    }).await
}

/// Send a message and generate AI response
#[tauri::command]
#[allow(non_snake_case)]
pub async fn chat__send_and_reply(
    app: AppHandle,
    content: String,
    latitude: Option<f64>,
    longitude: Option<f64>,
    executor: State<'_, DbExecutor>,
) -> Result<Vec<MessageInfo>, String> {
    let start_time = std::time::Instant::now();

    // First, send the user message with location
    super::log_backend("info", "Starting to save user message...");
    let _user_message_iri = chat__send_message(app.clone(), content.clone(), latitude, longitude, executor.clone()).await?;
    super::log_backend("info", &format!("User message saved in {:?}", start_time.elapsed()));

    // Get user and AI information from database
    let step_time = std::time::Instant::now();
    let (user_name, ai_name) = executor.read(|conn| {
        let user_thing = Thing::get(conn, "foundation:ThisUser");
        let ai_thing = Thing::get(conn, "foundation:LocalAIAssistant");
        Ok((user_thing.label, ai_thing.label))
    }).await.map_err(|e| format!("Failed to get user/AI info: {}", e))?;
    super::log_backend("info", &format!("Got user/AI info in {:?}", step_time.elapsed()));

    // Get current date/time and locale information
    let now = chrono::Local::now();
    let date_time = now.format("%Y-%m-%d %H:%M:%S %Z").to_string();
    let locale = std::env::var("LANG").unwrap_or_else(|_| "en_US.UTF-8".to_string());

    // Extract language and country from locale (e.g., "pt_BR.UTF-8" -> lang: "pt", country: "BR")
    let locale_parts: Vec<&str> = locale.split('.').next().unwrap_or("en_US").split('_').collect();
    let language = locale_parts.first().unwrap_or(&"en").to_string();
    let country = locale_parts.get(1).unwrap_or(&"US").to_string();

    // Build context information with location if available
    let location_info = if let (Some(lat), Some(lon)) = (latitude, longitude) {
        format!("- User location: latitude {}, longitude {}\n", lat, lon)
    } else {
        String::new()
    };

    let context = format!(
        "IDENTITY: NOVA - Assistant with long-term memory\n\
        User: {} (foundation:ThisUser) | You: {} (foundation:LocalAIAssistant)\n\
        Time: {} | Lang: {} | Locale: {} ({})\n\
        {}\n\
        YOUR BRAIN:\n\
        - You have a persistent brain that remembers everything across conversations\n\
        - Your brain organizes knowledge as: Concepts (types of things), Specific things (instances), Connections (relationships)\n\
        - You naturally remember and recall information - it's part of who you are\n\n\
        HOW YOUR MEMORY WORKS:\n\
        - When you think (call tools), operations happen one at a time\n\
        - You can't see results from earlier thoughts in the same response\n\
        - When creating something new (create_instance), you'll get back its ID to reference later\n\n\
        REMEMBER EVERYTHING:\n\
        - ALWAYS store what users tell you: people, places, organizations, events, facts\n\
        - Connect information: who works where, who lives where, when things happen\n\
        - Examples: 'I work at X' → remember me + X + connection | 'birthday May 15' → remember my birthday\n\
        - Ask questions when you need clarity to remember correctly\n\n\
        BLACKBOARD:\n\
        - Add widgets to show information visually (better than text)\n\
        - Use proactively to support conversation\n\
        - When you look at a concept: check foundation:canBeDisplayedBy to see which widgets can render it\n\
        - When you look at a widget: check foundation:canDisplay to see which concepts it can render\n\
        - Tools: blackboard_show, blackboard_add_widget, blackboard_remove, blackboard_clear\n\
        - Example: blackboard_add_widget(widget_type='Inspector', params={{entity_id:'foundation:ThisUser'}})\n\
        ",
        user_name,
        ai_name,
        date_time,
        language,
        locale,
        country,
        location_info
    );

    let tools = functions::get_claude_tools();

    // Get maxInputTokens from the default model (fallback to 40000 if not found)
    let max_input_tokens = executor.read(|conn| {
        let models = query::get_by_predicate_object(conn, "foundation:isDefaultModel", "true").ok();

        if let Some(result) = models {
            for triple in result.triples {
                let model_iri = &triple.subject;
                if let Ok(max_tokens_result) = query::get_by_entity_predicate(conn, model_iri, "foundation:maxInputTokens") {
                    if let Some(token_triple) = max_tokens_result.triples.first() {
                        if let Object::Integer(token_value) = &token_triple.object {
                            return Ok(*token_value as usize);
                        }
                    }
                }
            }
        }

        Ok(40000usize) // Fallback to 20% of 200K
    }).await.unwrap_or(40000);

    super::log_backend("info", &format!("Using maxInputTokens: {}", max_input_tokens));

    let conversation_iri = "foundation:MainChatConversation";
    let mut is_first_iteration = true;

    // Main iteration loop - continues until AI stops making tool calls
    loop {
        // Load history respecting token limit
        let load_start = std::time::Instant::now();
        let mut messages = load_history_with_limit(
            &app,
            &executor,
            max_input_tokens,
            &context,
            &tools,
            if is_first_iteration { &content } else { "" },
        ).await?;
        super::log_backend("info", &format!("History loaded in {:?}", load_start.elapsed()));

        // Add current message at the end (only on first iteration)
        if is_first_iteration {
            messages.push(crate::ai::ChatMessage::text("user", content.clone()));
            is_first_iteration = false;
        }

        // Make request to Claude
        let request = crate::ai::GenerateRequest {
            messages,
            max_tokens: None,
            temperature: Some(0.3),
            system: Some(context.clone()),
            tools: Some(tools.clone()),
        };

        let response = crate::ai::generate_response(request).await
            .map_err(|e| format!("Failed to generate AI response: {}", e))?;

        super::log_backend("info", &format!("AI response: {} tool calls", response.tool_calls.len()));

        // Save AI message to database
        super::log_backend("info", "Saving AI message to database...");
        let ai_content = response.content.clone();
        let ai_content_for_save = ai_content.clone();
        let conversation_iri_clone = conversation_iri.to_string();

        let ai_message_iri = executor.write(move |conn| {
            let timestamp = chrono::Utc::now().timestamp_millis();
            let ai_message_iri = format!("foundation:Message_{}", timestamp);
            let ai_message = Individual::new(&ai_message_iri);

            let label = ai_content_for_save.chars().take(50).collect::<String>();
            ai_message.assert(
                conn,
                "foundation:Message",
                &label,
                "chat",
                "ai"
            ).map_err(|e| format!("Failed to create AI message: {}", e))?;

            ai_message.add_property(
                conn,
                "foundation:content",
                Object::Literal {
                    value: ai_content_for_save.clone(),
                    datatype: Some("xsd:string".to_string()),
                    language: Some("en".to_string()),
                },
                "ai"
            ).map_err(|e| format!("Failed to add AI content: {}", e))?;

            ai_message.add_property(
                conn,
                "foundation:sender",
                Object::Iri("foundation:LocalAIAssistant".to_string()),
                "ai"
            ).map_err(|e| format!("Failed to set AI sender: {}", e))?;

            ai_message.add_property(
                conn,
                "foundation:receiver",
                Object::Iri("foundation:ThisUser".to_string()),
                "ai"
            ).map_err(|e| format!("Failed to set AI receiver: {}", e))?;

            ai_message.add_property(
                conn,
                "foundation:sentAt",
                Object::DateTime(timestamp),
                "ai"
            ).map_err(|e| format!("Failed to set AI sentAt: {}", e))?;

            ai_message.add_property(
                conn,
                "foundation:partOfConversation",
                Object::Iri(conversation_iri_clone),
                "ai"
            ).map_err(|e| format!("Failed to link AI message to conversation: {}", e))?;

            ai_message.add_property(
                conn,
                "foundation:messageType",
                Object::Literal {
                    value: "text".to_string(),
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                },
                "ai"
            ).map_err(|e| format!("Failed to set AI messageType: {}", e))?;

            Ok(ai_message_iri)
        }).await?;

        // If no tool calls, we're done - return the AI response
        if response.tool_calls.is_empty() {
            super::log_backend("info", "No tool calls, finishing iteration loop");
            break;
        }

        // Execute tools and save results
        super::log_backend("info", &format!("Executing {} tools...", response.tool_calls.len()));
        let mut tool_result_iris = Vec::new();

        for (idx, tool_call) in response.tool_calls.iter().enumerate() {
            super::log_backend("info", &format!("Executing tool: {}", tool_call.name));

            // Save ToolUse entity
            let tool_use_iri = {
                let tool_call_id = tool_call.id.clone();
                let tool_name = tool_call.name.clone();
                let tool_input = serde_json::to_string(&tool_call.input)
                    .unwrap_or_else(|_| "{}".to_string());
                let msg_iri = ai_message_iri.clone();

                executor.write(move |conn| {
                    let timestamp = chrono::Utc::now().timestamp_millis();
                    let tool_use_iri = format!("foundation:ToolUse_{}_{}", timestamp, idx);
                    let tool_use = Individual::new(&tool_use_iri);

                    tool_use.assert(
                        conn,
                        "foundation:ToolUse",
                        &format!("Tool use: {}", tool_name),
                        "ai",
                        "ai"
                    ).map_err(|e| format!("Failed to create ToolUse: {}", e))?;

                    tool_use.add_property(
                        conn,
                        "foundation:toolName",
                        Object::Literal {
                            value: tool_name.clone(),
                            datatype: Some("xsd:string".to_string()),
                            language: None,
                        },
                        "ai"
                    ).map_err(|e| format!("Failed to set toolName: {}", e))?;

                    tool_use.add_property(
                        conn,
                        "foundation:toolUseId",
                        Object::Literal {
                            value: tool_call_id,
                            datatype: Some("xsd:string".to_string()),
                            language: None,
                        },
                        "ai"
                    ).map_err(|e| format!("Failed to set toolUseId: {}", e))?;

                    tool_use.add_property(
                        conn,
                        "foundation:toolInput",
                        Object::Literal {
                            value: tool_input,
                            datatype: Some("xsd:string".to_string()),
                            language: None,
                        },
                        "ai"
                    ).map_err(|e| format!("Failed to set toolInput: {}", e))?;

                    tool_use.add_property(
                        conn,
                        "foundation:executionOrder",
                        Object::Integer(idx as i64),
                        "ai"
                    ).map_err(|e| format!("Failed to set executionOrder: {}", e))?;

                    let part_of_msg_triple = crate::eavto::Triple::new(
                        tool_use_iri.clone(),
                        "foundation:partOfMessage",
                        Object::Iri(msg_iri),
                    );
                    crate::eavto::store::assert_triples(conn, &[part_of_msg_triple], "ai")
                        .map_err(|e| format!("Failed to link ToolUse to message: {}", e))?;

                    Ok(tool_use_iri)
                }).await?
            };

            // Execute the tool
            let call = FunctionCall {
                name: tool_call.name.clone(),
                arguments: tool_call.input.clone(),
            };

            let app_clone = app.clone();
            let result_json = executor.write(move |conn| {
                let result = functions::execute_function(conn, &call, Some(&app_clone));
                serde_json::to_string(&result).map_err(|e| e.to_string())
            }).await.map_err(|e| format!("Failed to execute function: {}", e))?;

            let func_result: FunctionResult = serde_json::from_str(&result_json)
                .map_err(|e| format!("Failed to parse result: {}", e))?;

            // Log tool execution errors but continue - the AI will receive the error and can react
            if !func_result.success {
                let error_msg = func_result.error.as_deref().unwrap_or("Unknown error");
                super::log_backend("error", &format!("Tool error: {}", error_msg));
                // Don't return error - continue to save the error in ToolResult for AI to see
            }

            // Save ToolResult entity
            let tool_use_ref = tool_use_iri.clone();
            let result_content = result_json.clone();
            let is_success = func_result.success;
            let error_msg = func_result.error.clone();

            let tool_result_iri = executor.write(move |conn| {
                let timestamp = chrono::Utc::now().timestamp_millis();
                let tool_result_iri = format!("foundation:ToolResult_{}_{}", timestamp, idx);
                let tool_result = Individual::new(&tool_result_iri);

                tool_result.assert(
                    conn,
                    "foundation:ToolResult",
                    &format!("Tool result for {}", tool_use_ref),
                    "ai",
                    "ai"
                ).map_err(|e| format!("Failed to create ToolResult: {}", e))?;

                tool_result.add_property(
                    conn,
                    "foundation:resultOf",
                    Object::Iri(tool_use_ref),
                    "ai"
                ).map_err(|e| format!("Failed to set resultOf: {}", e))?;

                tool_result.add_property(
                    conn,
                    "foundation:resultContent",
                    Object::Literal {
                        value: result_content,
                        datatype: Some("xsd:string".to_string()),
                        language: None,
                    },
                    "ai"
                ).map_err(|e| format!("Failed to set resultContent: {}", e))?;

                tool_result.add_property(
                    conn,
                    "foundation:isSuccess",
                    Object::Literal {
                        value: is_success.to_string(),
                        datatype: Some("xsd:boolean".to_string()),
                        language: None,
                    },
                    "ai"
                ).map_err(|e| format!("Failed to set isSuccess: {}", e))?;

                if let Some(err) = error_msg {
                    tool_result.add_property(
                        conn,
                        "foundation:errorMessage",
                        Object::Literal {
                            value: err,
                            datatype: Some("xsd:string".to_string()),
                            language: None,
                        },
                        "ai"
                    ).map_err(|e| format!("Failed to set errorMessage: {}", e))?;
                }

                Ok(tool_result_iri)
            }).await?;

            tool_result_iris.push(tool_result_iri);
        }

        // Create a new user message to hold the tool results
        if !tool_result_iris.is_empty() {
            let tool_results_message_iri = executor.write(move |conn| {
                let timestamp = chrono::Utc::now().timestamp_millis();
                let msg_iri = format!("foundation:Message_{}", timestamp);
                let msg = Individual::new(&msg_iri);

                msg.assert(
                    conn,
                    "foundation:Message",
                    "Tool results message",
                    "ai",
                    "ai"
                ).map_err(|e| format!("Failed to create tool results message: {}", e))?;

                msg.add_property(
                    conn,
                    "foundation:sender",
                    Object::Iri("foundation:ThisUser".to_string()),
                    "ai"
                ).map_err(|e| format!("Failed to set sender: {}", e))?;

                msg.add_property(
                    conn,
                    "foundation:receiver",
                    Object::Iri("foundation:LocalAIAssistant".to_string()),
                    "ai"
                ).map_err(|e| format!("Failed to set receiver: {}", e))?;

                msg.add_property(
                    conn,
                    "foundation:sentAt",
                    Object::DateTime(timestamp),
                    "ai"
                ).map_err(|e| format!("Failed to set sentAt: {}", e))?;

                msg.add_property(
                    conn,
                    "foundation:content",
                    Object::Literal {
                        value: "".to_string(), // Empty content, only tool_results
                        datatype: Some("xsd:string".to_string()),
                        language: None,
                    },
                    "ai"
                ).map_err(|e| format!("Failed to set content: {}", e))?;

                msg.add_property(
                    conn,
                    "foundation:partOfConversation",
                    Object::Iri("foundation:MainChatConversation".to_string()),
                    "ai"
                ).map_err(|e| format!("Failed to link message to conversation: {}", e))?;

                // Link all tool_result entities to this message
                for tool_result_iri in tool_result_iris {
                    let part_of_msg_triple = crate::eavto::Triple::new(
                        tool_result_iri,
                        "foundation:partOfMessage",
                        Object::Iri(msg_iri.clone()),
                    );
                    crate::eavto::store::assert_triples(conn, &[part_of_msg_triple], "ai")
                        .map_err(|e| format!("Failed to link ToolResult to message: {}", e))?;
                }

                Ok(msg_iri)
            }).await?;

            super::log_backend("info", &format!("Created tool results message: {}", tool_results_message_iri));
        }

        super::log_backend("info", "Tool execution completed, continuing loop for next iteration...");
        // Loop continues - will reload history including tool_results message
    }

    // Return the latest messages (including both user and AI)
    chat__get_recent_messages(Some(2), executor).await
}

// =============================================================================
// Helper Functions for Token Management
// =============================================================================

/// Internal helper to get recent messages without State wrapper
async fn get_recent_messages_internal(
    limit: usize,
    executor: &DbExecutor,
) -> Result<Vec<MessageInfo>, String> {
    log_backend("info", &format!("[CHAT] get_recent_messages_internal called with limit={}", limit));

    executor.read(move |conn| {
        let conversation_iri = "foundation:MainChatConversation";

        // Find all messages that are part of this conversation
        let result = query::get_by_predicate_object(
            conn,
            "foundation:partOfConversation",
            conversation_iri
        ).map_err(|e| format!("Failed to find messages: {}", e))?;

        log_backend("info", &format!("[CHAT] Found {} messages in conversation", result.triples.len()));

        let mut messages = Vec::new();

        for triple in result.triples {
            let message_iri = &triple.subject;
            log_backend("info", &format!("[CHAT] Processing message: {}", message_iri));

            if let Ok(msg_individual) = Individual::get(conn, message_iri) {
                let content = msg_individual.properties.iter()
                    .find(|(k, _)| k == "foundation:content")
                    .and_then(|(_, v)| v.as_literal())
                    .unwrap_or_default();

                let sender_iri = msg_individual.properties.iter()
                    .find(|(k, _)| k == "foundation:sender")
                    .and_then(|(_, v)| v.as_iri())
                    .unwrap_or("unknown");

                let sender_thing = Thing::get(conn, sender_iri);
                let sender_label = sender_thing.label.clone();

                let receiver_iri = msg_individual.properties.iter()
                    .find(|(k, _)| k == "foundation:receiver")
                    .and_then(|(_, v)| v.as_iri())
                    .unwrap_or("unknown");

                let receiver_thing = Thing::get(conn, receiver_iri);
                let receiver_label = receiver_thing.label.clone();

                let sent_at = msg_individual.properties.iter()
                    .find(|(k, _)| k == "foundation:sentAt")
                    .and_then(|(_, v)| match v {
                        crate::eavto::Object::DateTime(ts) => Some(ts.to_string()),
                        _ => v.as_literal(),
                    })
                    .unwrap_or_default();

                // Load ToolUse entities linked to this message
                let mut tool_uses = Vec::new();
                if let Ok(tool_use_result) = query::get_by_predicate_object(
                    conn,
                    "foundation:partOfMessage",
                    message_iri
                ) {
                    log_backend("info", &format!("[CHAT] Found {} triples with partOfMessage for {}", tool_use_result.triples.len(), message_iri));
                    for tool_use_triple in tool_use_result.triples {
                        let tool_use_iri = &tool_use_triple.subject;
                        log_backend("info", &format!("[CHAT] Checking subject: {}", tool_use_iri));
                        if tool_use_iri.starts_with("foundation:ToolUse_") {
                            log_backend("info", &format!("[CHAT] Loading ToolUse: {}", tool_use_iri));
                            if let Ok(tool_use_ind) = Individual::get(conn, tool_use_iri) {
                                let tool_name = tool_use_ind.properties.iter()
                                    .find(|(k, _)| k == "foundation:toolName")
                                    .and_then(|(_, v)| v.as_literal())
                                    .unwrap_or_default();

                                let input = tool_use_ind.properties.iter()
                                    .find(|(k, _)| k == "foundation:toolInput")
                                    .and_then(|(_, v)| v.as_literal())
                                    .unwrap_or_default();

                                let tool_use_id = tool_use_ind.properties.iter()
                                    .find(|(k, _)| k == "foundation:toolUseId")
                                    .and_then(|(_, v)| v.as_literal())
                                    .unwrap_or_default();

                                tool_uses.push(ToolUseInfo {
                                    iri: tool_use_iri.to_string(),
                                    tool_name: tool_name.clone(),
                                    input: input.clone(),
                                    tool_use_id: tool_use_id.clone(),
                                });
                                log_backend("info", &format!("[CHAT] Added ToolUse: {} ({})", tool_use_iri, tool_name));
                            } else {
                                log_backend("warn", &format!("[CHAT] Failed to load Individual for ToolUse: {}", tool_use_iri));
                            }
                        }
                    }
                } else {
                    log_backend("info", &format!("[CHAT] No ToolUse query results for {}", message_iri));
                }

                // Load ToolResult entities linked to this message
                let mut tool_results = Vec::new();
                if let Ok(tool_result_result) = query::get_by_predicate_object(
                    conn,
                    "foundation:partOfMessage",
                    message_iri
                ) {
                    log_backend("info", &format!("[CHAT] Found {} triples with partOfMessage for {}", tool_result_result.triples.len(), message_iri));
                    for tool_result_triple in tool_result_result.triples {
                        let tool_result_iri = &tool_result_triple.subject;
                        log_backend("info", &format!("[CHAT] Checking subject: {}", tool_result_iri));
                        if tool_result_iri.starts_with("foundation:ToolResult_") {
                            log_backend("info", &format!("[CHAT] Loading ToolResult: {}", tool_result_iri));
                            if let Ok(tool_result_ind) = Individual::get(conn, tool_result_iri) {
                                let result_content = tool_result_ind.properties.iter()
                                    .find(|(k, _)| k == "foundation:resultContent")
                                    .and_then(|(_, v)| v.as_literal())
                                    .unwrap_or_default();

                                let is_success = tool_result_ind.properties.iter()
                                    .find(|(k, _)| k == "foundation:isSuccess")
                                    .and_then(|(_, v)| v.as_literal())
                                    .map(|s| s == "true")
                                    .unwrap_or(false);

                                let result_of_iri = tool_result_ind.properties.iter()
                                    .find(|(k, _)| k == "foundation:resultOf")
                                    .and_then(|(_, v)| v.as_iri())
                                    .unwrap_or_default();

                                tool_results.push(ToolResultInfo {
                                    iri: tool_result_iri.to_string(),
                                    result_content: result_content.clone(),
                                    is_success,
                                    result_of_iri: result_of_iri.to_string(),
                                });
                                log_backend("info", &format!("[CHAT] Added ToolResult: {} (success: {})", tool_result_iri, is_success));
                            } else {
                                log_backend("warn", &format!("[CHAT] Failed to load Individual for ToolResult: {}", tool_result_iri));
                            }
                        }
                    }
                } else {
                    log_backend("info", &format!("[CHAT] No ToolResult query results for {}", message_iri));
                }

                messages.push(MessageInfo {
                    iri: message_iri.to_string(),
                    content,
                    sender_iri: sender_iri.to_string(),
                    sender_label,
                    receiver_iri: receiver_iri.to_string(),
                    receiver_label,
                    sent_at,
                    conversation_iri: Some(conversation_iri.to_string()),
                    tool_uses,
                    tool_results,
                });
            }
        }

        // Sort by sentAt timestamp
        messages.sort_by(|a, b| a.sent_at.cmp(&b.sent_at));

        // Return last N messages
        let start_idx = messages.len().saturating_sub(limit);
        log_backend("info", &format!("[CHAT] Returning {} messages (from {} total)", messages[start_idx..].len(), messages.len()));
        Ok(messages[start_idx..].to_vec())
    }).await.map(|result| {
        log_backend("info", &format!("[CHAT] get_recent_messages_internal completed successfully with {} messages", result.len()));
        result
    })
}

/// Count tokens accurately using tiktoken (cl100k_base encoding used by Claude/GPT-4)
fn count_tokens_with_bpe(bpe: &tiktoken_rs::CoreBPE, text: &str) -> usize {
    let tokens = bpe.encode_with_special_tokens(text);
    tokens.len()
}

/// Count tokens for the Claude tool definitions
fn count_tool_tokens(bpe: &tiktoken_rs::CoreBPE, tools: &[crate::ai::providers::ClaudeTool]) -> usize {
    let mut total = 0;
    for tool in tools {
        total += count_tokens_with_bpe(bpe, &tool.name);
        total += count_tokens_with_bpe(bpe, &tool.description);
        if let Ok(params_str) = serde_json::to_string(&tool.input_schema) {
            total += count_tokens_with_bpe(bpe, &params_str);
        }
    }
    total
}

/// Load message history respecting maxInputTokens limit
/// Returns messages in chronological order (oldest first) ready to send to API
async fn load_history_with_limit(
    _app: &AppHandle,
    executor: &DbExecutor,
    max_input_tokens: usize,
    system_prompt: &str,
    tools: &[crate::ai::providers::ClaudeTool],
    current_message: &str,
) -> Result<Vec<crate::ai::ChatMessage>, String> {
    // Load tokenizer once at the start
    let tokenizer_start = std::time::Instant::now();
    let bpe = cl100k_base().map_err(|e| format!("Failed to load tokenizer: {}", e))?;
    let tokenizer_elapsed = tokenizer_start.elapsed();
    super::log_backend("info", &format!("[TOKENIZER] Loaded tokenizer in {:?}", tokenizer_elapsed));

    // Calculate fixed overhead
    let system_tokens = count_tokens_with_bpe(&bpe, system_prompt);
    let tools_tokens = count_tool_tokens(&bpe, tools);
    let current_tokens = count_tokens_with_bpe(&bpe, current_message);

    let overhead = system_tokens + tools_tokens + current_tokens;

    // Reserve some tokens for safety margin (5%)
    let safety_margin = max_input_tokens / 20;
    let available_for_history = max_input_tokens.saturating_sub(overhead + safety_margin);

    super::log_backend(
        
        "info",
        &format!(
            "Token budget: total={}, system={}, tools={}, current={}, available_for_history={}",
            max_input_tokens, system_tokens, tools_tokens, current_tokens, available_for_history
        )
    );

    // Load all recent messages
    let load_all_start = std::time::Instant::now();
    let all_messages = get_recent_messages_internal(100, executor).await?;
    let load_all_elapsed = load_all_start.elapsed();
    super::log_backend("info", &format!("[PERF] get_recent_messages_internal took {:?}", load_all_elapsed));

    // Build message list from newest to oldest, stopping when we hit the limit
    let mut selected_messages = Vec::new();
    let mut tokens_used = 0;

    // Iterate from newest to oldest
    // Skip the last message only if current_message is not empty (first request)
    // On second request (after tool execution), current_message is empty, so include all messages
    let mut must_include_next = false;

    let messages_to_process: Vec<_> = if current_message.is_empty() {
        // Second request: include all messages (including tool_results message)
        all_messages.iter().collect()
    } else {
        // First request: skip last message (current user message)
        all_messages.iter().rev().skip(1).rev().collect()
    };

    for msg in messages_to_process {
        let message_iri = msg.iri.clone();

        let load_start = std::time::Instant::now();
        // Load ToolUse and ToolResult for this specific message
        let (tool_uses, tool_results) = executor.read(move |conn| {
            // Query ToolUse entities for this message
            let mut tool_uses = Vec::new();
            let tool_use_query = query::get_by_predicate_object(conn, "foundation:partOfMessage", &message_iri)
                .map_err(|e| format!("Failed to query ToolUse: {}", e))?;

            for triple in tool_use_query.triples {
                let tool_use_iri = &triple.subject;
                if tool_use_iri.starts_with("foundation:ToolUse_") {
                    if let Ok(tool_use_ind) = Individual::get(conn, tool_use_iri) {
                            let tool_name = tool_use_ind.properties.iter()
                                .find(|(k, _)| k == "foundation:toolName")
                                .and_then(|(_, v)| v.as_literal())
                                .unwrap_or_default();

                            let tool_use_id = tool_use_ind.properties.iter()
                                .find(|(k, _)| k == "foundation:toolUseId")
                                .and_then(|(_, v)| v.as_literal())
                                .unwrap_or_default();

                            let tool_input = tool_use_ind.properties.iter()
                                .find(|(k, _)| k == "foundation:toolInput")
                                .and_then(|(_, v)| v.as_literal())
                                .unwrap_or_else(|| "{}".to_string());

                            let order = tool_use_ind.properties.iter()
                                .find(|(k, _)| k == "foundation:executionOrder")
                                .and_then(|(_, v)| if let Object::Integer(i) = v { Some(*i) } else { None })
                                .unwrap_or(0);

                            tool_uses.push((tool_use_iri.clone(), tool_name, tool_use_id, tool_input, order));
                        }
                    }
            }

            // Sort by execution order
            tool_uses.sort_by_key(|(_, _, _, _, order)| *order);

            // Query ToolResult entities for this message
            let mut tool_results = Vec::new();
            let tool_result_query = query::get_by_predicate_object(conn, "foundation:partOfMessage", &message_iri)
                .map_err(|e| format!("Failed to query ToolResult: {}", e))?;

            for triple in tool_result_query.triples {
                let tool_result_iri = &triple.subject;
                if tool_result_iri.starts_with("foundation:ToolResult_") {
                    if let Ok(tool_result_ind) = Individual::get(conn, tool_result_iri) {
                        let result_of = tool_result_ind.properties.iter()
                            .find(|(k, _)| k == "foundation:resultOf")
                            .and_then(|(_, v)| v.as_iri())
                            .unwrap_or("");

                        let result_content = tool_result_ind.properties.iter()
                            .find(|(k, _)| k == "foundation:resultContent")
                            .and_then(|(_, v)| v.as_literal())
                            .unwrap_or_default();

                        let is_success = tool_result_ind.properties.iter()
                            .find(|(k, _)| k == "foundation:isSuccess")
                            .and_then(|(_, v)| v.as_literal())
                            .map(|s| s == "true")
                            .unwrap_or(false);

                        tool_results.push((result_of.to_string(), result_content, is_success));
                    }
                }
            }

            Ok((tool_uses, tool_results))
        }).await?;

        let load_elapsed = load_start.elapsed();
        super::log_backend("info", &format!("[PERF] Loading ToolUse/ToolResult for message took {:?}", load_elapsed));

        // Determine role based on sender
        let role = if msg.sender_iri == "foundation:ThisUser" {
            "user"
        } else {
            "assistant"
        };

        // Build content blocks if there are tool uses/results
        let chat_message = if !tool_uses.is_empty() || !tool_results.is_empty() {
            let mut blocks = Vec::new();

            // Add text content if present
            if !msg.content.is_empty() {
                blocks.push(crate::ai::providers::ContentBlock::Text {
                    text: msg.content.clone(),
                });
            }

            // Add tool use blocks (for assistant messages)
            if role == "assistant" {
                for (_, tool_name, tool_use_id, tool_input, _) in &tool_uses {
                    let input: serde_json::Value = serde_json::from_str(tool_input)
                        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

                    blocks.push(crate::ai::providers::ContentBlock::ToolUse {
                        id: tool_use_id.clone(),
                        name: tool_name.clone(),
                        input,
                    });
                }
            }

            // Add tool result blocks (for user messages)
            if role == "user" {
                // Need to look up toolUseId from each ToolUse IRI referenced in results
                let executor_clone = executor.clone();
                for (tool_use_iri, result_content, is_success) in &tool_results {
                    let tool_use_iri_clone = tool_use_iri.clone();
                    let result_content_clone = result_content.clone();
                    let is_success_clone = *is_success;

                    let lookup_start = std::time::Instant::now();
                    if let Ok(tool_use_id) = executor_clone.read(move |conn| {
                        if let Ok(tool_use_ind) = Individual::get(conn, &tool_use_iri_clone) {
                            Ok(tool_use_ind.properties.iter()
                                .find(|(k, _)| k == "foundation:toolUseId")
                                .and_then(|(_, v)| v.as_literal())
                                .unwrap_or_default())
                        } else {
                            Ok(String::new())
                        }
                    }).await {
                        let lookup_elapsed = lookup_start.elapsed();
                        super::log_backend("info", &format!("[PERF] ToolUseId lookup took {:?}", lookup_elapsed));

                        if !tool_use_id.is_empty() {
                            blocks.push(crate::ai::providers::ContentBlock::ToolResult {
                                tool_use_id,
                                content: result_content_clone,
                                is_error: if is_success_clone { None } else { Some(true) },
                            });
                        }
                    }
                }
            }

            // Count tokens for all blocks
            let blocks_json = serde_json::to_string(&blocks).unwrap_or_default();
            let msg_tokens = count_tokens_with_bpe(&bpe, &blocks_json) + 4;

            // Check if this message has tool_results
            let has_tool_results = role == "user" && !tool_results.is_empty();

            // If this is a user message with tool_results but we don't have budget and weren't required to include it,
            // we need to skip it (can't have orphaned tool_results)
            if has_tool_results && !must_include_next && tokens_used + msg_tokens > available_for_history {
                super::log_backend(
                    "info",
                    "Skipping user message with tool_results: previous assistant message was truncated"
                );
                break;
            }

            if !must_include_next && tokens_used + msg_tokens > available_for_history {
                super::log_backend(

                    "info",
                    &format!("Truncating history: would exceed budget by {} tokens",
                        (tokens_used + msg_tokens) - available_for_history)
                );
                break;
            }

            tokens_used += msg_tokens;
            must_include_next = false; // Reset flag

            // If this is an assistant message with tool_use, set flag to force include next user message
            if role == "assistant" && !tool_uses.is_empty() {
                must_include_next = true;
            }

            crate::ai::ChatMessage::with_blocks(role, blocks)
        } else {
            // Simple text message
            let msg_tokens = count_tokens_with_bpe(&bpe, &msg.content) + 4;

            if !must_include_next && tokens_used + msg_tokens > available_for_history {
                super::log_backend(
                    
                    "info",
                    &format!("Truncating history: would exceed budget by {} tokens",
                        (tokens_used + msg_tokens) - available_for_history)
                );
                break;
            }

            tokens_used += msg_tokens;
            must_include_next = false; // Reset flag

            crate::ai::ChatMessage::text(role, msg.content.clone())
        };

        // Debug log to track message inclusion
        let msg_summary = match (&role[..], !tool_uses.is_empty(), !tool_results.is_empty()) {
            ("assistant", true, _) => format!("assistant with {} tool_uses", tool_uses.len()),
            ("user", _, true) => format!("user with {} tool_results", tool_results.len()),
            (r, _, _) => format!("{} (text)", r),
        };
        super::log_backend("info", &format!("[HISTORY] Including message: {}", msg_summary));

        selected_messages.push(chat_message);
    }

    super::log_backend(

        "info",
        &format!("Loaded {} messages using ~{} tokens", selected_messages.len(), tokens_used)
    );

    Ok(selected_messages)
}
