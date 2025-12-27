use serde::{Deserialize, Serialize};
use tauri::State;

use crate::eavto::{DbExecutor, query};
use crate::owl::{Individual, Object, Thing};

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
                value: now,
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
    content: String,
    executor: State<'_, DbExecutor>,
) -> Result<Vec<MessageInfo>, String> {
    // First, send the user message
    let user_message_iri = chat__send_message(content.clone(), executor.clone()).await?;

    // Generate AI response using the AI module
    let ai_response = crate::ai::generate_response(&content, 150)
        .map_err(|e| format!("Failed to generate AI response: {}", e))?;

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
