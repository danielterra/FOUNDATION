use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State, Emitter};
use tiktoken_rs::cl100k_base;
use std::sync::OnceLock;
use std::fs;

use crate::eavto::DbExecutor;
use crate::owl::{Individual, Object, Thing};
use crate::ai::functions::{self, FunctionCall, FunctionResult};
use crate::commands::log_backend;

// Global tokenizer cache - initialized once and reused
static TOKENIZER: OnceLock<tiktoken_rs::CoreBPE> = OnceLock::new();

fn get_tokenizer() -> &'static tiktoken_rs::CoreBPE {
    TOKENIZER.get_or_init(|| {
        let start = std::time::Instant::now();
        let bpe = cl100k_base().expect("Failed to load tokenizer");
        let _elapsed = start.elapsed();
        bpe
    })
}

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
pub struct AttachmentInfo {
    pub iri: String,
    pub file_iri: Option<String>,
    pub file_name: String,
    pub mime_type: String,
    pub file_size: i64,
    pub base64_data: Option<String>,
    pub file_path: Option<String>,
    pub attached_at: String,
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
    pub attachments: Vec<AttachmentInfo>,
    pub token_count: Option<i64>,
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

/// Attach a file to be sent with a message
#[tauri::command]
#[allow(non_snake_case)]
pub async fn chat__attach_file(
    file_path: String,
    file_name: String,
    mime_type: String,
    executor: State<'_, DbExecutor>,
) -> Result<String, String> {
    executor.write(move |conn| {
        // Get user's Documents directory
        let home_dir = dirs::home_dir()
            .ok_or_else(|| "Failed to get home directory".to_string())?;

        let attachments_dir = home_dir.join("Documents").join("Foundation").join("attachments");

        // Create attachments directory if it doesn't exist
        fs::create_dir_all(&attachments_dir)
            .map_err(|e| format!("Failed to create attachments directory: {}", e))?;

        // Read file content
        let file_content = fs::read(&file_path)
            .map_err(|e| format!("Failed to read file: {}", e))?;

        let file_size = file_content.len() as i64;

        // Check file size limit (30 MB for Claude)
        if file_size > 30 * 1024 * 1024 {
            return Err("File size exceeds 30 MB limit".to_string());
        }

        // Generate unique filename using timestamp
        let timestamp = chrono::Utc::now().timestamp_millis();
        let path = std::path::Path::new(&file_name);
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("bin");
        let file_stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("file");
        let stored_filename = format!("{}_{}.{}", timestamp, sanitize_filename(file_stem), extension);
        let stored_path = attachments_dir.join(&stored_filename);

        // Copy file to attachments directory
        fs::copy(&file_path, &stored_path)
            .map_err(|e| format!("Failed to copy file: {}", e))?;


        let now = chrono::Utc::now().to_rfc3339();

        // Step 1: Create File entity
        let file_iri = format!("foundation:File_{}", timestamp);
        let file = Individual::new(&file_iri);

        // Determine FileType based on MIME type
        let file_type_iri = match mime_type.as_str() {
            "application/pdf" => "foundation:FileType_PDF",
            "image/jpeg" | "image/jpg" => "foundation:FileType_JPEG",
            "image/png" => "foundation:FileType_PNG",
            "image/gif" => "foundation:FileType_GIF",
            "image/webp" => "foundation:FileType_WEBP",
            "image/svg+xml" => "foundation:FileType_SVG",
            "image/bmp" => "foundation:FileType_BMP",
            "image/tiff" => "foundation:FileType_TIFF",
            "video/mp4" => "foundation:FileType_MP4",
            "video/x-msvideo" => "foundation:FileType_AVI",
            "video/quicktime" => "foundation:FileType_MOV",
            "video/webm" => "foundation:FileType_WEBM",
            "audio/mpeg" => "foundation:FileType_MP3",
            "audio/wav" => "foundation:FileType_WAV",
            "audio/ogg" => "foundation:FileType_OGG",
            "text/plain" => "foundation:FileType_TXT",
            "application/msword" => "foundation:FileType_DOC",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => "foundation:FileType_DOCX",
            "application/zip" => "foundation:FileType_ZIP",
            "application/vnd.rar" => "foundation:FileType_RAR",
            "application/x-7z-compressed" => "foundation:FileType_7Z",
            _ => "foundation:FileType_PDF", // Default fallback
        };

        file.assert(
            conn,
            "foundation:File",
            &file_name,
            "chat",
            "chat"
        ).map_err(|e| format!("Failed to create File: {}", e))?;

        // Add file properties
        file.add_property(
            conn,
            "foundation:fileName",
            Object::Literal {
                value: file_name.clone(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            },
            "chat"
        ).map_err(|e| format!("Failed to set fileName: {}", e))?;

        file.add_property(
            conn,
            "foundation:filePath",
            Object::Literal {
                value: format!("file://{}", stored_path.to_string_lossy()),
                datatype: Some("xsd:anyURI".to_string()),
                language: None,
            },
            "chat"
        ).map_err(|e| format!("Failed to set filePath: {}", e))?;

        file.add_property(
            conn,
            "foundation:fileSize",
            Object::Integer(file_size),
            "chat"
        ).map_err(|e| format!("Failed to set fileSize: {}", e))?;

        file.add_property(
            conn,
            "foundation:hasFileType",
            Object::Iri(file_type_iri.to_string()),
            "chat"
        ).map_err(|e| format!("Failed to set hasFileType: {}", e))?;

        file.add_property(
            conn,
            "foundation:uploadDate",
            Object::Literal {
                value: now.clone(),
                datatype: Some("xsd:dateTime".to_string()),
                language: None,
            },
            "chat"
        ).map_err(|e| format!("Failed to set uploadDate: {}", e))?;


        // Step 2: Create Attachment entity (wrapper)
        let attachment_iri = format!("foundation:Attachment_{}", timestamp);
        let attachment = Individual::new(&attachment_iri);

        attachment.assert(
            conn,
            "foundation:Attachment",
            &format!("Attachment: {}", file_name),
            "chat",
            "chat"
        ).map_err(|e| format!("Failed to create Attachment: {}", e))?;

        // Link Attachment to File
        attachment.add_property(
            conn,
            "foundation:attachesFile",
            Object::Iri(file_iri.clone()),
            "chat"
        ).map_err(|e| format!("Failed to set attachesFile: {}", e))?;

        // Add attachment timestamp
        attachment.add_property(
            conn,
            "foundation:attachedAt",
            Object::Literal {
                value: now,
                datatype: Some("xsd:dateTime".to_string()),
                language: None,
            },
            "chat"
        ).map_err(|e| format!("Failed to set attachedAt: {}", e))?;


        Ok(attachment_iri)
    }).await
}

/// Sanitize filename to avoid filesystem issues
fn sanitize_filename(filename: &str) -> String {
    filename
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' { c } else { '_' })
        .collect()
}

/// Helper function to load attachment info with File details
fn load_attachment_info(
    conn: &rusqlite::Connection,
    attachment_iri: &str,
) -> Result<AttachmentInfo, String> {

    let attachment_ind = Individual::get(conn, attachment_iri)
        .map_err(|e| format!("Failed to load Attachment: {}", e))?;

    // Get attachedAt from Attachment
    let attached_at = attachment_ind.properties.iter()
        .find(|(k, _)| k == "foundation:attachedAt")
        .and_then(|(_, v)| v.as_literal())
        .unwrap_or_default();

    // Get File IRI via attachesFile
    let file_iri = attachment_ind.properties.iter()
        .find(|(k, _)| k == "foundation:attachesFile")
        .and_then(|(_, v)| v.as_iri());

    super::log_backend("debug", &format!("[ATTACH] Loading attachment {}, file_iri: {:?}", attachment_iri, file_iri));

    if let Some(file_iri_str) = file_iri {
        // Load File entity
        if let Ok(file_ind) = Individual::get(conn, file_iri_str) {
            super::log_backend("debug", &format!("[ATTACH] Loaded File entity with {} properties", file_ind.properties.len()));
            let file_name = file_ind.properties.iter()
                .find(|(k, _)| k == "foundation:fileName")
                .and_then(|(_, v)| v.as_literal())
                .unwrap_or_default();

            let file_size = file_ind.properties.iter()
                .find(|(k, _)| k == "foundation:fileSize")
                .and_then(|(_, v)| if let Object::Integer(i) = v { Some(*i) } else { None })
                .unwrap_or(0);

            let file_path_uri = file_ind.properties.iter()
                .find(|(k, _)| k == "foundation:filePath")
                .and_then(|(_, v)| v.as_literal());

            // Extract file path from URI (remove "file://" prefix if present)
            let file_path = file_path_uri.map(|uri| {
                uri.strip_prefix("file://").unwrap_or(&uri).to_string()
            });

            // Get FileType to determine MIME type
            let file_type_iri = file_ind.properties.iter()
                .find(|(k, _)| k == "foundation:hasFileType")
                .and_then(|(_, v)| v.as_iri());


            let mime_type = if let Some(ft_iri) = file_type_iri {
                if let Ok(file_type_ind) = Individual::get(conn, ft_iri) {
                    let mt = file_type_ind.properties.iter()
                        .find(|(k, _)| k == "foundation:mimeType")
                        .and_then(|(_, v)| v.as_literal())
                        .unwrap_or_else(|| "application/octet-stream".to_string());
                    mt
                } else {
                    super::log_backend("warn", &format!("[ATTACH] Failed to load FileType: {}", ft_iri));
                    "application/octet-stream".to_string()
                }
            } else {
                super::log_backend("warn", "[ATTACH] No FileType IRI found");
                "application/octet-stream".to_string()
            };

            return Ok(AttachmentInfo {
                iri: attachment_iri.to_string(),
                file_iri: Some(file_iri_str.to_string()),
                file_name,
                mime_type,
                file_size,
                base64_data: None,
                file_path,
                attached_at,
            });
        } else {
            super::log_backend("warn", &format!("[ATTACH] Failed to load File entity: {}", file_iri_str));
        }
    }

    // Fallback: Try old schema (backward compatibility)
    let file_name = attachment_ind.properties.iter()
        .find(|(k, _)| k == "foundation:fileName")
        .and_then(|(_, v)| v.as_literal())
        .unwrap_or_else(|| "unknown".to_string());

    let mime_type = attachment_ind.properties.iter()
        .find(|(k, _)| k == "foundation:mimeType")
        .and_then(|(_, v)| v.as_literal())
        .unwrap_or_else(|| "application/octet-stream".to_string());

    let file_size = attachment_ind.properties.iter()
        .find(|(k, _)| k == "foundation:fileSize")
        .and_then(|(_, v)| if let Object::Integer(i) = v { Some(*i) } else { None })
        .unwrap_or(0);

    let file_path = attachment_ind.properties.iter()
        .find(|(k, _)| k == "foundation:filePath")
        .and_then(|(_, v)| v.as_literal());

    Ok(AttachmentInfo {
        iri: attachment_iri.to_string(),
        file_iri: None,
        file_name,
        mime_type,
        file_size,
        base64_data: None,
        file_path,
        attached_at,
    })
}

/// Send a message from user to AI assistant
#[tauri::command]
#[allow(non_snake_case)]
pub async fn chat__send_message(
    app: AppHandle,
    content: String,
    latitude: Option<f64>,
    longitude: Option<f64>,
    attachment_iris: Option<Vec<String>>,
    executor: State<'_, DbExecutor>,
) -> Result<String, String> {
    // Prevent creating empty messages
    if content.trim().is_empty() && attachment_iris.as_ref().map_or(true, |a| a.is_empty()) {
        return Err("Cannot create empty message - content and attachments are both empty".to_string());
    }

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
                value: content.clone(),
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
        let msg_type = if attachment_iris.is_some() && !attachment_iris.as_ref().unwrap().is_empty() {
            if content.is_empty() {
                "image" // or "file" depending on mime type
            } else {
                "mixed" // text + attachments
            }
        } else {
            "text"
        };

        message.add_property(
            conn,
            "foundation:messageType",
            Object::Literal {
                value: msg_type.to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            },
            "chat"
        ).map_err(|e| format!("Failed to set messageType: {}", e))?;

        // Link attachments to message
        if let Some(attachment_list) = attachment_iris {
            for attachment_iri in attachment_list {
                message.add_property(
                    conn,
                    "foundation:hasAttachment",
                    Object::Iri(attachment_iri),
                    "chat"
                ).map_err(|e| format!("Failed to link attachment: {}", e))?;
            }
        }

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

        // Calculate and store token count for this message
        let bpe = get_tokenizer();
        let token_count = count_tokens_with_bpe(bpe, &content);

        message.add_property(
            conn,
            "foundation:tokenCount",
            Object::Integer(token_count as i64),
            "chat"
        ).map_err(|e| format!("Failed to set tokenCount: {}", e))?;


        Ok(message_iri)
    }).await.map(|iri| {
        // Emit event to notify frontend
        use tauri::Emitter;
        let _ = app.emit("chat-message-added", ());
        iri
    })
}

/// Get recent messages from the main conversation - Tauri command wrapper
#[tauri::command]
#[allow(non_snake_case)]
pub async fn chat__get_recent_messages(
    limit: Option<usize>,
    executor: State<'_, DbExecutor>,
) -> Result<Vec<MessageInfo>, String> {
    get_recent_messages(limit.unwrap_or(10), executor.inner()).await
}

/// Get recent messages - core implementation
pub async fn get_recent_messages(
    limit: usize,
    executor: &DbExecutor,
) -> Result<Vec<MessageInfo>, String> {
    let limit_val = limit;

    executor.read(move |conn| {
        let conversation_iri = "foundation:MainChatConversation";

        // Find all messages that are part of this conversation
        let message_iris = Individual::find_by_class_and_properties(
            conn,
            "foundation:Message",
            &[("foundation:partOfConversation", conversation_iri)]
        ).map_err(|e| format!("Failed to find messages: {}", e))?;

        // Fetch full details for each message
        let mut messages_with_time: Vec<(MessageInfo, String)> = Vec::new();

        for message_iri in message_iris {
            // Load complete message individual
            let message = Individual::get(conn, &message_iri)
                .map_err(|e| format!("Failed to load message {}: {}", message_iri, e))?;

            // Get content
            let content = message.properties.iter()
                .find(|(k, _)| k == "foundation:content")
                .and_then(|(_, v)| v.as_literal())
                .unwrap_or_default();

            // Get sender
            let sender_iri = message.properties.iter()
                .find(|(k, _)| k == "foundation:sender")
                .and_then(|(_, v)| v.as_iri())
                .unwrap_or("unknown")
                .to_string();

            let sender_thing = Thing::get(conn, &sender_iri);

            // Get receiver
            let receiver_iri = message.properties.iter()
                .find(|(k, _)| k == "foundation:receiver")
                .and_then(|(_, v)| v.as_iri())
                .unwrap_or("unknown")
                .to_string();

            let receiver_thing = Thing::get(conn, &receiver_iri);

            // Get timestamp
            let sent_at = message.properties.iter()
                .find(|(k, _)| k == "foundation:sentAt")
                .and_then(|(_, v)| v.as_literal())
                .unwrap_or_default();

            // Load ToolUse and ToolResult entities linked to this message
            let (tool_use_data, tool_result_data) = load_tool_data_for_message(conn, &message_iri)
                .unwrap_or_else(|e| {
                    log_backend("warn", &format!("[CHAT] Failed to load tool data for {}: {}", message_iri, e));
                    (Vec::new(), Vec::new())
                });

            // Convert tool_use_data to ToolUseInfo
            let tool_uses: Vec<ToolUseInfo> = tool_use_data.into_iter()
                .map(|(iri, tool_name, tool_use_id, input, _order)| {
                    ToolUseInfo {
                        iri,
                        tool_name,
                        input,
                        tool_use_id,
                    }
                })
                .collect();

            // Convert tool_result_data to ToolResultInfo
            let tool_results: Vec<ToolResultInfo> = tool_result_data.into_iter()
                .map(|(result_of_iri, result_content, is_success)| {
                    // Only log failures
                    if !is_success {
                        log_backend("warn", &format!("[CHAT] Tool failed: {} - {}", result_of_iri, result_content));
                    }
                    ToolResultInfo {
                        iri: format!("foundation:ToolResult_{}", result_of_iri), // Reconstruct IRI (not ideal but works)
                        result_content,
                        is_success,
                        result_of_iri,
                    }
                })
                .collect();

            // Get token count
            let token_count = message.properties.iter()
                .find(|(k, _)| k == "foundation:tokenCount")
                .and_then(|(_, v)| match v {
                    Object::Integer(count) => Some(*count),
                    _ => None,
                });

            // Load Attachment entities linked to this message
            let attachment_iris: Vec<String> = message.properties.iter()
                .filter(|(k, _)| k == "foundation:hasAttachment")
                .filter_map(|(_, v)| v.as_iri().map(|s| s.to_string()))
                .collect();

            let mut attachments = Vec::new();
            // Only log if there are attachments
            if !attachment_iris.is_empty() {
                log_backend("info", &format!("[CHAT] Found {} attachments for {}", attachment_iris.len(), message_iri));
            }

            for attachment_iri in attachment_iris {
                match load_attachment_info(conn, &attachment_iri) {
                    Ok(attachment_info) => {
                        attachments.push(attachment_info);
                    }
                    Err(e) => {
                        log_backend("warn", &format!("[CHAT] Failed to load Attachment: {}", e));
                    }
                }
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
                    attachments,
                    token_count,
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

        // Load complete conversation individual
        let conv_ind = Individual::get(conn, conversation_iri)
            .map_err(|e| format!("Failed to load conversation: {}", e))?;

        // Get started_at
        let started_at = conv_ind.properties.iter()
            .find(|(k, _)| k == "foundation:startedAt")
            .and_then(|(_, v)| v.as_literal())
            .unwrap_or_default();

        // Get topic
        let topic = conv_ind.properties.iter()
            .find(|(k, _)| k == "foundation:topic")
            .and_then(|(_, v)| v.as_literal());

        // Get status
        let status = conv_ind.properties.iter()
            .find(|(k, _)| k == "foundation:conversationStatus")
            .and_then(|(_, v)| v.as_literal());

        // Count participants
        let participant_count = conv_ind.properties.iter()
            .filter(|(k, _)| k == "foundation:hasParticipant")
            .count();

        Ok(Some(ConversationInfo {
            iri: conversation_iri.to_string(),
            started_at,
            topic,
            status,
            participant_count,
        }))
    }).await
}

/// Recovery state enumeration
#[derive(Debug, Clone)]
pub enum RecoveryState {
    /// No recovery needed
    None,
    /// Executed N pending tools and created ToolResult message
    ExecutedTools(usize),
    /// Found ToolResults waiting for AI response - need to trigger AI
    AwaitingAIResponse,
}

/// Send a message and generate AI response
/// Check for tool_use blocks without corresponding tool_result and execute them
/// This handles interrupted sessions where the app was closed during tool execution
/// Returns the recovery state indicating what action was taken
pub async fn check_and_execute_pending_tools(
    app: AppHandle,
    executor: &DbExecutor,
) -> Result<RecoveryState, String> {
    // Get recent messages to check for pending tools
    let messages = get_recent_messages(10, executor).await?;

    super::log_backend("info", &format!("[RECOVERY] Loaded {} messages for recovery check", messages.len()));

    // Messages are in chronological order (oldest first), so last() gives us the most recent
    if let Some(last_msg) = messages.last() {
        super::log_backend("info", &format!("[RECOVERY] Last message: {} from {}", last_msg.iri, last_msg.sender_iri));
    } else {
        super::log_backend("warn", "[RECOVERY] No messages found in conversation");
    }

    let mut pending_count = 0;

    // Collect all tool_result IRIs from all messages to check what's already been executed
    let mut all_results: std::collections::HashSet<String> = std::collections::HashSet::new();
    for msg in &messages {
        for result in &msg.tool_results {
            all_results.insert(result.result_of_iri.clone());
        }
    }

    // Simple rule: The last message should ALWAYS be from the AI
    // If the last message is from the user, we need to trigger AI to respond
    // Messages are in chronological order, so .last() is the most recent
    if let Some(last_msg) = messages.last() {
        if last_msg.sender_iri == "foundation:ThisUser" {
            super::log_backend("warn", "[RECOVERY] Last message is from user - AI needs to respond");
            return Ok(RecoveryState::AwaitingAIResponse);
        }
    }

    // Find the most recent assistant message with tool_use
    for msg in messages.iter().rev() {
        if msg.sender_iri == "foundation:LocalAIAssistant" && !msg.tool_uses.is_empty() {

            // Collect pending tool executions
            let mut pending_tools: Vec<(usize, ToolUseInfo, String, FunctionResult)> = Vec::new();

            // Check each tool_use for missing tool_result
            for (idx, tool_use) in msg.tool_uses.iter().enumerate() {
                // Check if this tool_use already has a result (in any message)
                if all_results.contains(&tool_use.iri) {
                    continue;
                }

                // This tool_use has no result - it needs to be executed
                super::log_backend("warn", &format!("[RECOVERY] Found pending tool execution: {} ({})", tool_use.tool_name, tool_use.iri));

                // Parse the tool input
                let input_json: serde_json::Value = serde_json::from_str(&tool_use.input)
                    .unwrap_or(serde_json::json!({}));

                // Execute the tool
                let call = FunctionCall {
                    name: tool_use.tool_name.clone(),
                    arguments: input_json,
                };

                let app_clone = app.clone();
                let result_json = executor.write(move |conn| {
                    let result = functions::execute_function(conn, &call, Some(&app_clone));
                    serde_json::to_string(&result).map_err(|e| e.to_string())
                }).await.map_err(|e| format!("Failed to execute function: {}", e))?;

                let func_result: FunctionResult = serde_json::from_str(&result_json)
                    .map_err(|e| format!("Failed to parse result: {}", e))?;

                pending_tools.push((idx, tool_use.clone(), result_json, func_result));
                pending_count += 1;
            }

            // If we recovered any tool results, create message AND ToolResults in ONE transaction
            if !pending_tools.is_empty() {
                let result_count = pending_tools.len();

                // Create message and ALL ToolResults in a single transaction
                executor.write(move |conn| {
                    let timestamp = chrono::Utc::now().timestamp_millis();
                    let msg_iri = format!("foundation:Message_{}_recovery", timestamp);
                    let msg = Individual::new(&msg_iri);

                    // Create the message first
                    msg.assert(
                        conn,
                        "foundation:Message",
                        "Tool results message (recovered)",
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

                    // Now create all ToolResults WITH partOfMessage link
                    for (idx, tool_use, result_content, func_result) in pending_tools {
                        let tool_result_iri = format!("foundation:ToolResult_{}_{}_recovery", timestamp, idx);
                        let tool_result = Individual::new(&tool_result_iri);

                        tool_result.assert(
                            conn,
                            "foundation:ToolResult",
                            &format!("Tool result for {} (recovered)", tool_use.iri),
                            "ai",
                            "ai"
                        ).map_err(|e| format!("Failed to create ToolResult: {}", e))?;

                        tool_result.add_property(
                            conn,
                            "foundation:resultOf",
                            Object::Iri(tool_use.iri.clone()),
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
                                value: func_result.success.to_string(),
                                datatype: Some("xsd:boolean".to_string()),
                                language: None,
                            },
                            "ai"
                        ).map_err(|e| format!("Failed to set isSuccess: {}", e))?;

                        if let Some(err) = func_result.error {
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

                        // THIS IS THE KEY: Add partOfMessage immediately when creating the ToolResult
                        tool_result.add_property(
                            conn,
                            "foundation:partOfMessage",
                            Object::Iri(msg_iri.clone()),
                            "ai"
                        ).map_err(|e| format!("Failed to link ToolResult to message: {}", e))?;
                    }

                    // Set token count
                    let token_count = result_count as i64 * 100; // Rough estimate
                    msg.add_property(
                        conn,
                        "foundation:tokenCount",
                        Object::Integer(token_count),
                        "ai"
                    ).map_err(|e| format!("Failed to set tokenCount: {}", e))?;

                    Ok(msg_iri)
                }).await?;

                // Emit event to notify frontend
                use tauri::Emitter;
                let _ = app.emit("chat-message-added", ());

                super::log_backend("info", &format!("[RECOVERY] Created tool results message with {} results", result_count));
            }

            // Only check the most recent assistant message
            break;
        }
    }

    if pending_count > 0 {
        Ok(RecoveryState::ExecutedTools(pending_count))
    } else {
        Ok(RecoveryState::None)
    }
}

/// Tauri command to check for pending tool executions on startup and continue AI processing
#[tauri::command]
#[allow(non_snake_case)]
pub async fn chat__recover_pending_tools(
    app: AppHandle,
    executor: State<'_, DbExecutor>,
) -> Result<Option<Vec<MessageInfo>>, String> {
    super::log_backend("info", "[STARTUP] Checking for pending tool executions...");

    // Check and execute pending tools
    let recovery_state = check_and_execute_pending_tools(app.clone(), &executor).await?;

    match recovery_state {
        RecoveryState::ExecutedTools(count) => {
            super::log_backend("info", &format!("[STARTUP] Executed {} pending tool(s), returning updated messages", count));
            // Return recent messages so UI can update
            let messages = get_recent_messages(10, &executor).await?;
            Ok(Some(messages))
        }
        RecoveryState::AwaitingAIResponse => {
            super::log_backend("warn", "[STARTUP] Found ToolResults awaiting AI response - triggering AI to continue");
            // Trigger AI to continue the conversation by calling send_and_reply with empty content
            // This will load the existing history (including ToolResults) and generate AI response
            chat__send_and_reply(
                app.clone(),
                "".to_string(), // Empty content - we're continuing existing conversation
                None, // No location
                None,
                None, // No attachments
                executor.clone()
            ).await?;

            // Return updated messages
            let messages = get_recent_messages(10, &executor).await?;
            Ok(Some(messages))
        }
        RecoveryState::None => {
            super::log_backend("info", "[STARTUP] No pending tools found, conversation is up to date");
            Ok(None)
        }
    }
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn chat__send_and_reply(
    app: AppHandle,
    content: String,
    latitude: Option<f64>,
    longitude: Option<f64>,
    attachment_iris: Option<Vec<String>>,
    executor: State<'_, DbExecutor>,
) -> Result<Vec<MessageInfo>, String> {
    let _start_time = std::time::Instant::now();

    // Check if this is a recovery call (empty content) - if so, don't create a new user message
    let is_recovery_mode = content.is_empty() && attachment_iris.as_ref().map_or(true, |a| a.is_empty());

    // Emit event to frontend if this is recovery mode (so frontend can show loading indicator)
    if is_recovery_mode {
        app.emit("ai-processing-started", ()).ok();
    }

    // First, send the user message with location (skip if recovery mode)
    let _user_message_iri = if !is_recovery_mode {
        chat__send_message(app.clone(), content.clone(), latitude, longitude, attachment_iris.clone(), executor.clone()).await?
    } else {
        String::new() // Empty IRI for recovery mode
    };

    // Get user and AI information from database
    let _step_time = std::time::Instant::now();
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
        VALIDATE BEFORE CREATING:\n\
        - NEVER create concepts/things/properties without checking if they exist\n\
        - ALWAYS search first, then REUSE if found, or create if not found\n\
        - CRITICAL: Before creating connection_type, use remember_concept on domain/range to see existing properties\n\
        - Check remember_connection_types with domain/range filters for similar connections\n\
        - REUSE existing properties over creating duplicates (e.g., use hasAddress not hasLocation)\n\n\
        ATTACHMENTS & FILES:\n\
        - Attachments show as: [Attached Image/PDF: filename - File ID: foundation:File_XXX]\n\
        - NEVER create File entities - they already exist from attachments\n\
        - Icons can be: material name ('person'), file:// URL, http:// URL, or /absolute/path\n\n\
        BLACKBOARD:\n\
        - Show information visually (better than text) - use proactively\n\
        - Concepts have foundation:canBeDisplayedBy listing compatible widgets\n\
        - Widgets have foundation:canDisplay listing compatible concepts\n\
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
        super::log_backend("info", &format!("[TOKEN CONFIG] Querying default model..."));

        // Find default model using OWL abstractions
        let model_iris = Individual::find_by_class_and_properties(
            conn,
            "foundation:AIModel",
            &[("foundation:isDefaultModel", "true")]
        ).map_err(|e| format!("Failed to find default model: {}", e))?;

        super::log_backend("info", &format!("[TOKEN CONFIG] Found {} models with isDefaultModel=true", model_iris.len()));

        for model_iri in model_iris {
            super::log_backend("info", &format!("[TOKEN CONFIG] Checking model: {}", model_iri));

            // Load complete model individual
            if let Ok(model_ind) = Individual::get(conn, &model_iri) {
                // Get maxInputTokens property
                if let Some((_, token_value)) = model_ind.properties.iter()
                    .find(|(k, _)| k == "foundation:maxInputTokens")
                {
                    super::log_backend("info", &format!("[TOKEN CONFIG] Found maxInputTokens property: {:?}", token_value));
                    if let Object::Integer(token_val) = token_value {
                        super::log_backend("info", &format!("[TOKEN CONFIG] ✅ Using maxInputTokens from {}: {}", model_iri, token_val));
                        return Ok(*token_val as usize);
                    } else {
                        super::log_backend("warn", &format!("[TOKEN CONFIG] Object is not Integer: {:?}", token_value));
                    }
                } else {
                    super::log_backend("warn", &format!("[TOKEN CONFIG] No maxInputTokens property found for {}", model_iri));
                }
            } else {
                super::log_backend("warn", &format!("[TOKEN CONFIG] Failed to load model individual: {}", model_iri));
            }
        }

        super::log_backend("error", "[TOKEN CONFIG] ❌ FATAL: No valid default model found");
        Err("FATAL: Failed to load maxInputTokens configuration from database".to_string())
    }).await.expect("FATAL: Application cannot start without maxInputTokens configuration");

    super::log_backend("info", &format!("✅ Using maxInputTokens: {}", max_input_tokens));

    let conversation_iri = "foundation:MainChatConversation";
    let mut is_first_iteration = true;
    let mut skip_user_message = is_recovery_mode; // Flag for recovery mode - set if called with empty content

    // Check for pending tool executions (from interrupted sessions)
    super::log_backend("info", "[RECOVERY] Checking for pending tool executions...");
    let recovery_state = check_and_execute_pending_tools(app.clone(), &executor).await?;
    match recovery_state {
        RecoveryState::ExecutedTools(count) => {
            super::log_backend("info", &format!("[RECOVERY] Executed {} pending tools from interrupted session", count));
            // Tools were executed and user message was created - skip adding another one
            skip_user_message = true;
        }
        RecoveryState::AwaitingAIResponse => {
            super::log_backend("warn", "[RECOVERY] Found ToolResults awaiting AI response - triggering AI to continue");
            // ToolResults are already in the database as a user message
            skip_user_message = true;
        }
        RecoveryState::None => {
            // No recovery needed - proceed normally (unless already in recovery mode from empty content)
        }
    }

    // Main iteration loop - continues until AI stops making tool calls
    loop {
        // Load history respecting token limit
        let _load_start = std::time::Instant::now();
        let mut messages = load_history_with_limit(
            &app,
            &executor,
            max_input_tokens,
            &context,
            &tools,
            if is_first_iteration && !skip_user_message { &content } else { "" },
        ).await?;

        // Add current message at the end (only on first iteration if not in recovery mode)
        if is_first_iteration && !skip_user_message {

            // Load the just-created message WITH attachments from database
            let current_msg = get_recent_messages(1, &executor).await?;

            if let Some(msg_info) = current_msg.first() {

                // Build content blocks for the message
                let mut blocks = Vec::new();

                // Add text content
                if !msg_info.content.is_empty() {
                    blocks.push(crate::ai::providers::ContentBlock::Text {
                        text: msg_info.content.clone(),
                    });
                }

                // Add attachments (images and documents)
                for attachment in &msg_info.attachments {
                    if let Some(ref file_path) = attachment.file_path {
                        if attachment.mime_type.starts_with("image/") {
                            if let Ok(file_data) = std::fs::read(file_path) {
                                use base64::Engine;
                                let base64_data = base64::engine::general_purpose::STANDARD.encode(&file_data);
                                blocks.push(crate::ai::providers::ContentBlock::Image {
                                    source: crate::ai::providers::ImageSource {
                                        source_type: "base64".to_string(),
                                        media_type: attachment.mime_type.clone(),
                                        data: base64_data,
                                    },
                                });
                                super::log_backend("info", &format!("[CHAT] ✅ Added image to current message: {}", attachment.file_name));
                            } else {
                                super::log_backend("warn", &format!("[CHAT] ❌ Failed to read image file: {}", file_path));
                            }
                        } else if attachment.mime_type == "application/pdf" {
                            if let Ok(file_data) = std::fs::read(file_path) {
                                use base64::Engine;
                                let base64_data = base64::engine::general_purpose::STANDARD.encode(&file_data);
                                blocks.push(crate::ai::providers::ContentBlock::Document {
                                    source: crate::ai::providers::DocumentSource {
                                        source_type: "base64".to_string(),
                                        media_type: "application/pdf".to_string(),
                                        data: base64_data,
                                    },
                                });
                                super::log_backend("info", &format!("[CHAT] ✅ Added PDF document to current message: {}", attachment.file_name));
                            } else {
                                super::log_backend("warn", &format!("[CHAT] ❌ Failed to read PDF file: {}", file_path));
                            }
                        }
                    }
                }

                super::log_backend("debug", &format!("[CHAT] Built {} content blocks for user message (content_len={}, attachments={})",
                    blocks.len(), msg_info.content.len(), msg_info.attachments.len()));

                if blocks.is_empty() {
                    // Fallback to text-only if no blocks
                    super::log_backend("warn", "[CHAT] No blocks created, using text-only fallback");
                    messages.push(crate::ai::ChatMessage::text("user", content.clone()));
                } else {
                    messages.push(crate::ai::ChatMessage::with_blocks("user", blocks));
                }
            } else {
                // Fallback if message not found
                super::log_backend("warn", "[CHAT] Could not load current message, using text-only fallback");
                messages.push(crate::ai::ChatMessage::text("user", content.clone()));
            }

            is_first_iteration = false;
        }

        // Calculate total tokens being sent to API
        let bpe = get_tokenizer();
        let system_tokens = count_tokens_with_bpe(bpe, &context);
        let tools_tokens = count_tool_tokens(bpe, &tools);
        let messages_json = serde_json::to_string(&messages).unwrap_or_default();
        let messages_tokens = count_tokens_with_bpe(bpe, &messages_json);
        let total_tokens = system_tokens + tools_tokens + messages_tokens;

        super::log_backend("info", &format!(
            "[API REQUEST] Sending to Claude API: {} total tokens (system={}, tools={}, messages={})",
            total_tokens, system_tokens, tools_tokens, messages_tokens
        ));

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


        // Save AI message to database
        let ai_content = response.content.clone();
        let ai_content_for_save = ai_content.clone();
        let conversation_iri_clone = conversation_iri.to_string();
        let usage_info = response.usage.clone();

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

            // Save usage information if present
            if let Some(usage) = usage_info {
                ai_message.add_property(
                    conn,
                    "foundation:inputTokens",
                    Object::Integer(usage.input_tokens as i64),
                    "ai"
                ).map_err(|e| format!("Failed to set inputTokens: {}", e))?;

                ai_message.add_property(
                    conn,
                    "foundation:outputTokens",
                    Object::Integer(usage.output_tokens as i64),
                    "ai"
                ).map_err(|e| format!("Failed to set outputTokens: {}", e))?;

                if usage.cache_creation_input_tokens > 0 {
                    ai_message.add_property(
                        conn,
                        "foundation:cacheCreationInputTokens",
                        Object::Integer(usage.cache_creation_input_tokens as i64),
                        "ai"
                    ).map_err(|e| format!("Failed to set cacheCreationInputTokens: {}", e))?;
                }

                if usage.cache_read_input_tokens > 0 {
                    ai_message.add_property(
                        conn,
                        "foundation:cacheReadInputTokens",
                        Object::Integer(usage.cache_read_input_tokens as i64),
                        "ai"
                    ).map_err(|e| format!("Failed to set cacheReadInputTokens: {}", e))?;
                }
            }

            Ok(ai_message_iri)
        }).await?;

        // Calculate and store token count for assistant message
        // This includes text content + tool_use blocks that will be added
        {
            let bpe = get_tokenizer();
            let mut total_tokens = count_tokens_with_bpe(bpe, &ai_content) + 4; // base overhead

            // Add token count for each tool_use block
            for tool_call in &response.tool_calls {
                let tool_json = serde_json::to_string(tool_call).unwrap_or_default();
                total_tokens += count_tokens_with_bpe(bpe, &tool_json);
            }

            let msg_iri_for_tokens = ai_message_iri.clone();
            executor.write(move |conn| {
                let msg = Individual::new(&msg_iri_for_tokens);
                msg.add_property(
                    conn,
                    "foundation:tokenCount",
                    Object::Integer(total_tokens as i64),
                    "ai"
                ).map_err(|e| format!("Failed to set tokenCount: {}", e))?;
                Ok(msg_iri_for_tokens)
            }).await?;
        }

        // Emit event to notify frontend
        {
            use tauri::Emitter;
            let _ = app.emit("chat-message-added", ());
        }

        // If no tool calls, we're done - return the AI response
        if response.tool_calls.is_empty() {
            break;
        }

        // Execute tools and save results
        let mut tool_result_iris = Vec::new();

        for (idx, tool_call) in response.tool_calls.iter().enumerate() {

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

                    let tool_use_ind = Individual::new(tool_use_iri.clone());
                    tool_use_ind.add_property(
                        conn,
                        "foundation:partOfMessage",
                        Object::Iri(msg_iri),
                        "ai"
                    ).map_err(|e| format!("Failed to link ToolUse to message: {}", e))?;

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
            let _tool_results_message_iri = executor.write(move |conn| {
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
                for tool_result_iri in &tool_result_iris {
                    let tool_result_ind = Individual::new(tool_result_iri.clone());
                    tool_result_ind.add_property(
                        conn,
                        "foundation:partOfMessage",
                        Object::Iri(msg_iri.clone()),
                        "ai"
                    ).map_err(|e| format!("Failed to link ToolResult to message: {}", e))?;
                }

                // Calculate token count for tool_results message
                let bpe = get_tokenizer();
                let mut total_tokens = 4; // base overhead

                // Add token count for each tool_result block
                for tool_result_iri in &tool_result_iris {
                    // Load the result content to count tokens
                    if let Ok(result_ind) = Individual::get(conn, tool_result_iri) {
                        let result_content = result_ind.properties.iter()
                            .find(|(k, _)| k == "foundation:resultContent")
                            .and_then(|(_, v)| v.as_literal())
                            .unwrap_or_default();
                        total_tokens += count_tokens_with_bpe(bpe, &result_content);
                    }
                }

                msg.add_property(
                    conn,
                    "foundation:tokenCount",
                    Object::Integer(total_tokens as i64),
                    "ai"
                ).map_err(|e| format!("Failed to set tokenCount: {}", e))?;

                Ok(msg_iri)
            }).await?;

            // Emit event to notify frontend
            {
                use tauri::Emitter;
                let _ = app.emit("chat-message-added", ());
            }

        }

        // Loop continues - will reload history including tool_results message
    }

    // Return the latest messages (including both user and AI)
    chat__get_recent_messages(Some(2), executor).await
}

// =============================================================================
// Helper Functions for Token Management
// =============================================================================


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

// =============================================================================
// History Builder - Robust Message Selection with Invariant Guarantees
// =============================================================================

/// Represents a unit of conversation that must be included/excluded atomically
#[derive(Debug, Clone)]
enum MessageUnit {
    /// Simple text message without tool interactions
    Simple {
        message: crate::ai::ChatMessage,
        tokens: usize,
        iri: String,
    },
    /// Atomic pair: assistant with tool_use + user with tool_result
    ToolPair {
        assistant_msg: crate::ai::ChatMessage,
        user_msg: crate::ai::ChatMessage,
        tokens: usize,
        assistant_iri: String,
        user_iri: String,
    },
    /// Assistant message with tool_use but NO corresponding tool_result (orphaned)
    /// These should NEVER be included as they violate API invariants
    OrphanedToolUse {
        tokens: usize,
    },
}

impl MessageUnit {
    fn tokens(&self) -> usize {
        match self {
            MessageUnit::Simple { tokens, .. } => *tokens,
            MessageUnit::ToolPair { tokens, .. } => *tokens,
            MessageUnit::OrphanedToolUse { tokens, .. } => *tokens,
        }
    }

    fn is_includable(&self) -> bool {
        !matches!(self, MessageUnit::OrphanedToolUse { .. })
    }

    fn messages(&self) -> Vec<crate::ai::ChatMessage> {
        match self {
            MessageUnit::Simple { message, .. } => vec![message.clone()],
            MessageUnit::ToolPair { assistant_msg, user_msg, .. } => {
                vec![assistant_msg.clone(), user_msg.clone()]
            }
            MessageUnit::OrphanedToolUse { .. } => vec![],
        }
    }
}

/// Helper: Load tool_use and tool_result data for a message
fn load_tool_data_for_message(
    conn: &rusqlite::Connection,
    message_iri: &str,
) -> Result<(Vec<(String, String, String, String, i64)>, Vec<(String, String, bool)>), String> {
    // Query ToolUse entities using OWL abstractions
    let mut tool_uses = Vec::new();
    let tool_use_iris = Individual::find_by_class_and_properties(
        conn,
        "foundation:ToolUse",
        &[("foundation:partOfMessage", message_iri)]
    ).map_err(|e| format!("Failed to query ToolUse: {}", e))?;

    for tool_use_iri in tool_use_iris {
        if let Ok(tool_use_ind) = Individual::get(conn, &tool_use_iri) {
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
    tool_uses.sort_by_key(|(_, _, _, _, order)| *order);

    // Query ToolResult entities using OWL abstractions
    let mut tool_results = Vec::new();
    let tool_result_iris = Individual::find_by_class_and_properties(
        conn,
        "foundation:ToolResult",
        &[("foundation:partOfMessage", message_iri)]
    ).map_err(|e| format!("Failed to query ToolResult: {}", e))?;

    for tool_result_iri in tool_result_iris {
        if let Ok(tool_result_ind) = Individual::get(conn, &tool_result_iri) {
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

    Ok((tool_uses, tool_results))
}

/// Helper: Load only tool_results for a message
fn load_tool_results_for_message(
    conn: &rusqlite::Connection,
    message_iri: &str,
) -> Result<Vec<(String, String, bool)>, String> {
    let (_, tool_results) = load_tool_data_for_message(conn, message_iri)?;
    Ok(tool_results)
}

/// Helper: Get tool_use_id from a ToolUse IRI
async fn get_tool_use_id(executor: &DbExecutor, tool_use_iri: &str) -> Result<String, String> {
    let iri = tool_use_iri.to_string();
    executor.read(move |conn| {
        if let Ok(tool_use_ind) = Individual::get(conn, &iri) {
            Ok(tool_use_ind.properties.iter()
                .find(|(k, _)| k == "foundation:toolUseId")
                .and_then(|(_, v)| v.as_literal())
                .unwrap_or_default())
        } else {
            Ok(String::new())
        }
    }).await
}

/// Helper: Build ChatMessage with tool blocks
async fn build_chat_message_with_tools(
    msg: &MessageInfo,
    tool_uses: &[(String, String, String, String, i64)],
    tool_results: &[(String, String, bool)],
    role: &str,
    executor: &DbExecutor,
) -> Result<crate::ai::ChatMessage, String> {
    let mut blocks = Vec::new();

    // Add text content
    if !msg.content.is_empty() {
        blocks.push(crate::ai::providers::ContentBlock::Text {
            text: msg.content.clone(),
        });
    }

    // Add attachments (images and PDFs) for user messages
    if role == "user" && !msg.attachments.is_empty() {
        for attachment in &msg.attachments {
            if let Some(ref file_path) = attachment.file_path {
                if attachment.mime_type.starts_with("image/") {
                    if let Ok(file_data) = std::fs::read(file_path) {
                        use base64::Engine;
                        let base64_data = base64::engine::general_purpose::STANDARD.encode(&file_data);

                        // Add File IRI reference before the image so AI knows which File entity to use
                        if let Some(ref file_iri) = attachment.file_iri {
                            blocks.push(crate::ai::providers::ContentBlock::Text {
                                text: format!("[Attached Image: {} - File ID: {}]", attachment.file_name, file_iri),
                            });
                        }

                        blocks.push(crate::ai::providers::ContentBlock::Image {
                            source: crate::ai::providers::ImageSource {
                                source_type: "base64".to_string(),
                                media_type: attachment.mime_type.clone(),
                                data: base64_data,
                            },
                        });
                    }
                } else if attachment.mime_type == "application/pdf" {
                    if let Ok(file_data) = std::fs::read(file_path) {
                        use base64::Engine;
                        let base64_data = base64::engine::general_purpose::STANDARD.encode(&file_data);

                        // Add File IRI reference before the document so AI knows which File entity to use
                        if let Some(ref file_iri) = attachment.file_iri {
                            blocks.push(crate::ai::providers::ContentBlock::Text {
                                text: format!("[Attached PDF: {} - File ID: {}]", attachment.file_name, file_iri),
                            });
                        }

                        blocks.push(crate::ai::providers::ContentBlock::Document {
                            source: crate::ai::providers::DocumentSource {
                                source_type: "base64".to_string(),
                                media_type: "application/pdf".to_string(),
                                data: base64_data,
                            },
                        });
                    }
                }
            }
        }
    }

    // Add tool_use blocks (assistant only)
    if role == "assistant" {
        for (_, tool_name, tool_use_id, tool_input, _) in tool_uses {
            let input: serde_json::Value = serde_json::from_str(tool_input)
                .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

            blocks.push(crate::ai::providers::ContentBlock::ToolUse {
                id: tool_use_id.clone(),
                name: tool_name.clone(),
                input,
            });
        }
    }

    // Add tool_result blocks (user only)
    if role == "user" {
        for (tool_use_iri, result_content, is_success) in tool_results {
            if let Ok(tool_use_id) = get_tool_use_id(executor, tool_use_iri).await {
                if !tool_use_id.is_empty() {
                    blocks.push(crate::ai::providers::ContentBlock::ToolResult {
                        tool_use_id,
                        content: result_content.clone(),
                        is_error: if *is_success { None } else { Some(true) },
                    });
                }
            }
        }
    }

    Ok(crate::ai::ChatMessage::with_blocks(role, blocks))
}

/// Phase 1: Build message units from raw messages
/// Groups messages into atomic units (simple, tool pairs, or orphaned tool uses)
async fn build_message_units(
    messages: &[MessageInfo],
    executor: &DbExecutor,
) -> Result<Vec<MessageUnit>, String> {
    let mut units = Vec::new();
    let mut i = 0;

    while i < messages.len() {
        let msg = &messages[i];
        let message_iri = msg.iri.clone();

        // Load ToolUse and ToolResult for this message
        let (tool_uses, tool_results) = executor.read(move |conn| {
            load_tool_data_for_message(conn, &message_iri)
        }).await?;

        let role = if msg.sender_iri == "foundation:ThisUser" { "user" } else { "assistant" };

        // Case 1: Assistant message with tool_use
        if role == "assistant" && !tool_uses.is_empty() {
            // Build assistant message with tool_use blocks
            let assistant_chat_msg = build_chat_message_with_tools(
                msg, &tool_uses, &[], role, executor
            ).await?;

            let tool_use_ids: Vec<String> = tool_uses.iter()
                .map(|(_, _, id, _, _)| id.clone())
                .collect();

            // Look ahead to find corresponding user message with tool_results
            if i + 1 < messages.len() {
                let next_msg = &messages[i + 1];
                let next_iri = next_msg.iri.clone();

                let next_results = executor.read(move |conn| {
                    load_tool_results_for_message(conn, &next_iri)
                }).await?;

                let next_role = if next_msg.sender_iri == "foundation:ThisUser" { "user" } else { "assistant" };

                // Check if next message is user with matching tool_results
                if next_role == "user" && !next_results.is_empty() {
                    // Extract tool_use_ids from results
                    let mut result_tool_use_ids = Vec::new();
                    for (tool_use_iri, _, _) in &next_results {
                        if let Ok(id) = get_tool_use_id(executor, tool_use_iri).await {
                            if !id.is_empty() {
                                result_tool_use_ids.push(id);
                            }
                        }
                    }

                    // Check if the results match our tool_uses
                    let all_match = tool_use_ids.iter().all(|id| result_tool_use_ids.contains(id));

                    if all_match && tool_use_ids.len() == result_tool_use_ids.len() {
                        // Build user message with tool_result blocks
                        let user_chat_msg = build_chat_message_with_tools(
                            next_msg, &[], &next_results, "user", executor
                        ).await?;

                        // Use stored token counts (required)
                        let assistant_tokens = msg.token_count
                            .ok_or_else(|| format!("FATAL: Message {} missing tokenCount", msg.iri))?;

                        let user_tokens = next_msg.token_count
                            .ok_or_else(|| format!("FATAL: Message {} missing tokenCount", next_msg.iri))?;

                        let tokens = (assistant_tokens + user_tokens) as usize;

                        units.push(MessageUnit::ToolPair {
                            assistant_msg: assistant_chat_msg,
                            user_msg: user_chat_msg,
                            tokens,
                            assistant_iri: msg.iri.clone(),
                            user_iri: next_msg.iri.clone(),
                        });

                        super::log_backend("info", &format!(
                            "[UNIT] Created ToolPair: {} + {} ({} tokens)",
                            msg.iri, next_msg.iri, tokens
                        ));

                        i += 2; // Skip both messages
                        continue;
                    }
                }
            }

            // No matching tool_result found - this is an orphaned tool_use
            let tokens = msg.token_count
                .ok_or_else(|| format!("FATAL: Message {} missing tokenCount", msg.iri))? as usize;

            units.push(MessageUnit::OrphanedToolUse {
                tokens,
            });

            super::log_backend("warn", &format!(
                "[UNIT] Created OrphanedToolUse: {} ({} tool_uses, {} tokens) - WILL BE SKIPPED",
                msg.iri, tool_uses.len(), tokens
            ));

            i += 1;
            continue;
        }

        // Case 2: Simple message (no tool interactions or user with tool_results without prior tool_use)
        let chat_msg = build_chat_message_with_tools(
            msg, &tool_uses, &tool_results, role, executor
        ).await?;

        let tokens = msg.token_count
            .ok_or_else(|| format!("FATAL: Message {} missing tokenCount", msg.iri))? as usize;

        units.push(MessageUnit::Simple {
            message: chat_msg,
            tokens,
            iri: msg.iri.clone(),
        });

        super::log_backend("info", &format!(
            "[UNIT] Created Simple: {} ({} tokens)",
            msg.iri, tokens
        ));

        i += 1;
    }

    Ok(units)
}

/// Phase 2: Select message units within budget
/// Iterates newest->oldest, selecting only includable units that fit in budget
fn select_by_budget(
    units: Vec<MessageUnit>,
    available_tokens: usize,
) -> Vec<MessageUnit> {
    let mut selected = Vec::new();
    let mut tokens_used = 0;

    // Iterate newest to oldest
    for unit in units.into_iter().rev() {
        // Skip orphaned tool_use (never includable)
        if !unit.is_includable() {
            super::log_backend("warn", &format!(
                "[SELECT] Skipping OrphanedToolUse (violates API invariants)"
            ));
            continue;
        }

        let unit_tokens = unit.tokens();

        // Check if this unit fits in budget
        if tokens_used + unit_tokens > available_tokens {
            super::log_backend("info", &format!(
                "[SELECT] Budget exhausted: used={}, unit={}, available={}",
                tokens_used, unit_tokens, available_tokens
            ));
            break;
        }

        tokens_used += unit_tokens;

        match &unit {
            MessageUnit::Simple { iri, .. } => {
                super::log_backend("info", &format!(
                    "[SELECT] Including Simple: {} ({} tokens, total: {})",
                    iri, unit_tokens, tokens_used
                ));
            }
            MessageUnit::ToolPair { assistant_iri, user_iri, .. } => {
                super::log_backend("info", &format!(
                    "[SELECT] Including ToolPair: {} + {} ({} tokens, total: {})",
                    assistant_iri, user_iri, unit_tokens, tokens_used
                ));
            }
            _ => {}
        }

        selected.push(unit);
    }

    // Reverse to get chronological order (oldest first)
    selected.reverse();

    super::log_backend("info", &format!(
        "[SELECT] Selected {} units using {} tokens (budget: {})",
        selected.len(), tokens_used, available_tokens
    ));

    selected
}

/// Phase 3: Validate API invariants
/// Ensures the selected messages satisfy all Claude API requirements
fn validate_invariants(messages: &[crate::ai::ChatMessage]) -> Result<(), String> {
    use crate::ai::providers::{ContentBlock, MessageContent};

    for (i, msg) in messages.iter().enumerate() {
        // Extract content blocks
        let blocks = match &msg.content {
            MessageContent::ContentBlocks(blocks) => blocks.as_slice(),
            MessageContent::Text(_) => {
                // Text messages are always valid (non-empty)
                continue;
            }
        };

        // Invariant: Messages must not be empty
        if blocks.is_empty() {
            super::log_backend("error", &format!("[VALIDATE] Message {} ({}) has empty blocks", i, msg.role));
            return Err(format!("Message {} has empty content", i));
        }

        // Collect tool_use_ids from this message
        let mut tool_use_ids = Vec::new();
        for block in blocks {
            if let ContentBlock::ToolUse { id, .. } = block {
                tool_use_ids.push(id.clone());
            }
        }

        // Invariant: If assistant has tool_use, next message must be user with matching tool_results
        if !tool_use_ids.is_empty() {
            if i + 1 >= messages.len() {
                return Err(format!(
                    "Message {} has tool_use {:?} but no following message",
                    i, tool_use_ids
                ));
            }

            let next_msg = &messages[i + 1];

            // Verify next message is user role
            if next_msg.role != "user" {
                return Err(format!(
                    "Message {} has tool_use but next message is not user (role={})",
                    i, next_msg.role
                ));
            }

            let next_blocks = match &next_msg.content {
                MessageContent::ContentBlocks(blocks) => blocks.as_slice(),
                _ => {
                    return Err(format!(
                        "Message {} has tool_use but next message {} has no content blocks",
                        i, i + 1
                    ));
                }
            };

            // Collect tool_use_ids from tool_results in next message
            let mut result_ids = Vec::new();
            for block in next_blocks {
                if let ContentBlock::ToolResult { tool_use_id, .. } = block {
                    result_ids.push(tool_use_id.clone());
                }
            }

            // Verify all tool_uses have corresponding tool_results
            for id in &tool_use_ids {
                if !result_ids.contains(id) {
                    return Err(format!(
                        "Message {} has tool_use {} without corresponding tool_result in message {}",
                        i, id, i + 1
                    ));
                }
            }

            // Verify all tool_results have corresponding tool_uses
            for id in &result_ids {
                if !tool_use_ids.contains(id) {
                    return Err(format!(
                        "Message {} has tool_result {} without corresponding tool_use in message {}",
                        i + 1, id, i
                    ));
                }
            }
        }
    }

    super::log_backend("info", "[VALIDATE] ✅ All API invariants satisfied");
    Ok(())
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
    // Use cached tokenizer (loaded once globally)
    let bpe = get_tokenizer();

    // Calculate fixed overhead
    let system_tokens = count_tokens_with_bpe(bpe, system_prompt);
    let tools_tokens = count_tool_tokens(bpe, tools);
    let current_tokens = count_tokens_with_bpe(bpe, current_message);

    let overhead = system_tokens + tools_tokens + current_tokens;

    // Reserve some tokens for safety margin (5%)
    let safety_margin = max_input_tokens / 20;
    let available_for_history = max_input_tokens.saturating_sub(overhead + safety_margin);

    super::log_backend(
        "info",
        &format!(
            "[BUDGET] total={}, system={}, tools={}, current={}, available_for_history={}",
            max_input_tokens, system_tokens, tools_tokens, current_tokens, available_for_history
        )
    );

    // Load recent messages (50 is a good balance between performance and context)
    let load_all_start = std::time::Instant::now();
    let all_messages = get_recent_messages(50, executor).await?;
    let _load_all_elapsed = load_all_start.elapsed();

    // Determine which messages to process
    let messages_to_process: Vec<_> = if current_message.is_empty() {
        // Second request: include all messages (including tool_results message)
        all_messages
    } else {
        // First request: skip last message (current user message)
        all_messages.into_iter().rev().skip(1).rev().collect()
    };


    // PHASE 1: Build message units (atomic groups)
    let build_start = std::time::Instant::now();
    let units = build_message_units(&messages_to_process, executor).await?;
    let _build_elapsed = build_start.elapsed();

    // PHASE 2: Select units within budget
    let select_start = std::time::Instant::now();
    let selected_units = select_by_budget(units, available_for_history);
    let _select_elapsed = select_start.elapsed();

    // Extract messages from units
    let mut messages = Vec::new();
    for unit in selected_units {
        messages.extend(unit.messages());
    }


    // PHASE 3: Validate invariants
    super::log_backend("debug", &format!("[VALIDATE] Validating {} messages before sending to API", messages.len()));
    for (i, msg) in messages.iter().enumerate() {
        let content_summary = match &msg.content {
            crate::ai::providers::MessageContent::Text(t) => format!("Text({})", t.len()),
            crate::ai::providers::MessageContent::ContentBlocks(blocks) => format!("Blocks({})", blocks.len()),
        };
        super::log_backend("debug", &format!("[VALIDATE] Message {}: role={}, content={}", i, msg.role, content_summary));
    }
    validate_invariants(&messages)?;

    Ok(messages)
}

// End of history loading implementation
