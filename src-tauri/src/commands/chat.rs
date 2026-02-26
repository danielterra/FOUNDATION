use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::eavto::{DbExecutor, query};
use crate::owl::{Individual, Object, Thing};
use crate::ai::functions::{self, FunctionCall};

#[derive(Debug, Serialize, Deserialize)]
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

        // Create message
        message.assert(
            conn,
            "foundation:Message",
            &content[..content.len().min(50)], // First 50 chars as label
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
            Object::Literal {
                value: now.clone(),
                datatype: Some("xsd:dateTime".to_string()),
                language: None,
            },
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
    // First, send the user message with location
    let user_message_iri = chat__send_message(content.clone(), latitude, longitude, executor.clone()).await?;

    // Get user and AI information from database
    let (user_name, ai_name) = executor.read(|conn| {
        let user_thing = Thing::get(conn, "foundation:ThisUser");
        let ai_thing = Thing::get(conn, "foundation:LocalAIAssistant");
        Ok((user_thing.label, ai_thing.label))
    }).await.map_err(|e| format!("Failed to get user/AI info: {}", e))?;

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
        "CONTEXT:\n\
        - Current date/time: {}\n\
        - System language: {}\n\
        - System locale: {} (country: {})\n\
        {}\
        - User IRI: foundation:ThisUser (name: {})\n\
        - Your IRI: foundation:LocalAIAssistant (name: {})\n\n",
        date_time,
        language,
        locale,
        country,
        location_info,
        user_name,
        ai_name
    );

    // Get recent conversation history (excluding the message we just saved)
    let history = chat__get_recent_messages(Some(10), executor.clone()).await?;

    // Build conversation history string (exclude the last message which is the one we just saved)
    let mut conversation_history = String::new();
    for msg in history.iter().rev().skip(1).rev() {
        let sender_label = &msg.sender_label;
        let msg_content = &msg.content;
        conversation_history.push_str(&format!("{}: {}\n", sender_label, msg_content));
    }

    // Build prompt with context, system instructions and function definitions
    let system_prompt = functions::get_system_prompt_with_functions();
    let context_clone = context.clone();
    let system_prompt_clone = system_prompt.clone();
    let content_clone = content.clone();

    let full_prompt = format!("{}{}{}{}: {}", context, system_prompt, conversation_history, user_name, content);

    // Log prompt size for monitoring
    super::log_backend(&app, "info", &format!("AI prompt length: {} chars, ~{} tokens (limit: 16384)",
        full_prompt.len(), full_prompt.len() / 4));

    // Build messages for Claude API
    let messages = vec![
        crate::ai::ChatMessage {
            role: "user".to_string(),
            content: format!("{}{}: {}", conversation_history, user_name, content),
        },
    ];

    let request = crate::ai::GenerateRequest {
        messages,
        max_tokens: Some(1024),
        temperature: Some(0.3),
        system: Some(format!("{}{}", context, system_prompt)),
    };

    // Generate AI response using the AI module with low temperature for precision
    let ai_raw_response = crate::ai::generate_response(request).await
        .map_err(|e| format!("Failed to generate AI response: {}", e))?;

    super::log_backend(&app, "info", &format!("AI raw response: {}", ai_raw_response));

    // Extract JSON from <json>...</json> tags if present
    let json_content = if let Some(start_idx) = ai_raw_response.find("<json>") {
        if let Some(end_idx) = ai_raw_response.find("</json>") {
            &ai_raw_response[start_idx + 6..end_idx]
        } else {
            ai_raw_response.trim()
        }
    } else {
        ai_raw_response.trim()
    };

    super::log_backend(&app, "info", &format!("Extracted content: {}", json_content));

    // Check if response is a function call (JSON format)
    let ai_response = if let Ok(function_call) = serde_json::from_str::<serde_json::Value>(json_content) {
        super::log_backend(&app, "info", "AI response parsed as JSON");
        // Check if it has "function" and "arguments" fields
        if let (Some(func_name), Some(args)) = (function_call.get("function"), function_call.get("arguments")) {
            if let Some(func_name_str) = func_name.as_str() {
                super::log_backend(&app, "info", &format!("Executing function: {} with args: {}", func_name_str, args));

                // Execute the function
                let call = FunctionCall {
                    name: func_name_str.to_string(),
                    arguments: args.clone(),
                };

                let func_result = executor.read(move |conn| {
                    Ok(functions::execute_function(conn, &call))
                }).await.map_err(|e| format!("Failed to execute function: {}", e))?;

                super::log_backend(&app, "info", &format!("Function executed. Success: {}", func_result.success));

                // Let AI interpret the function result
                if func_result.success {
                    if let Some(result) = &func_result.result {
                        // Build a new prompt with the function result for AI to interpret
                        let result_str = serde_json::to_string_pretty(result)
                            .unwrap_or_else(|_| result.to_string());

                        super::log_backend(&app, "info", &format!("Function result: {}", result_str));

                        // Build simple, direct interpretation prompt
                        let interpretation_prompt = format!(
                            "Question: {}\n\nData:\n{}\n\nProvide a direct answer based on the data:",
                            content_clone, result_str
                        );

                        super::log_backend(&app, "info", "Generating interpretation response...");

                        // Generate with higher temperature for more natural responses
                        let interpretation_request = crate::ai::GenerateRequest {
                            messages: vec![crate::ai::ChatMessage {
                                role: "user".to_string(),
                                content: interpretation_prompt,
                            }],
                            max_tokens: Some(512),
                            temperature: Some(0.7),
                            system: None,
                        };

                        match crate::ai::generate_response(interpretation_request).await {
                            Ok(interpreted) => {
                                super::log_backend(&app, "info", &format!("Interpretation generated: {}", interpreted));
                                interpreted
                            },
                            Err(e) => {
                                super::log_backend(&app, "error", &format!("Failed to generate interpretation: {}", e));
                                format!("Based on the data, here's what I found:\n{}", result_str)
                            }
                        }
                    } else {
                        super::log_backend(&app, "warn", "Function returned no result");
                        "I couldn't find any data.".to_string()
                    }
                } else {
                    let error_msg = func_result.error.unwrap_or_else(|| "Unknown error".to_string());
                    super::log_backend(&app, "error", &format!("Function error: {}", error_msg));
                    format!("Sorry, I encountered an error: {}", error_msg)
                }
            } else {
                super::log_backend(&app, "warn", "Function name is not a string");
                ai_raw_response
            }
        } else {
            super::log_backend(&app, "warn", "JSON doesn't have function/arguments fields");
            ai_raw_response
        }
    } else {
        super::log_backend(&app, "info", "AI response is not JSON (normal text response)");
        ai_raw_response
    };

    // Save AI response as a message
    executor.write(move |conn| {
        let conversation_iri = "foundation:MainChatConversation";

        // Create AI message
        let timestamp = chrono::Utc::now().timestamp_millis();
        let ai_message_iri = format!("foundation:Message_{}", timestamp);
        let ai_message = Individual::new(&ai_message_iri);

        let now = chrono::Utc::now().to_rfc3339();

        // Create message
        ai_message.assert(
            conn,
            "foundation:Message",
            &ai_response[..ai_response.len().min(50)],
            "chat",
            "ai"
        ).map_err(|e| format!("Failed to create AI message: {}", e))?;

        // Add content
        ai_message.add_property(
            conn,
            "foundation:content",
            Object::Literal {
                value: ai_response.clone(),
                datatype: Some("xsd:string".to_string()),
                language: Some("en".to_string()),
            },
            "ai"
        ).map_err(|e| format!("Failed to add AI content: {}", e))?;

        // Set sender (AI)
        ai_message.add_property(
            conn,
            "foundation:sender",
            Object::Iri("foundation:LocalAIAssistant".to_string()),
            "ai"
        ).map_err(|e| format!("Failed to set AI sender: {}", e))?;

        // Set receiver (User)
        ai_message.add_property(
            conn,
            "foundation:receiver",
            Object::Iri("foundation:ThisUser".to_string()),
            "ai"
        ).map_err(|e| format!("Failed to set AI receiver: {}", e))?;

        // Set timestamp
        ai_message.add_property(
            conn,
            "foundation:sentAt",
            Object::Literal {
                value: now,
                datatype: Some("xsd:dateTime".to_string()),
                language: None,
            },
            "ai"
        ).map_err(|e| format!("Failed to set AI sentAt: {}", e))?;

        // Link to conversation
        ai_message.add_property(
            conn,
            "foundation:partOfConversation",
            Object::Iri(conversation_iri.to_string()),
            "ai"
        ).map_err(|e| format!("Failed to link AI message to conversation: {}", e))?;

        // Set message type
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

    // Return the latest messages (including both user and AI)
    chat__get_recent_messages(Some(2), executor).await
}
