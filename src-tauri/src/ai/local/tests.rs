use super::*;
use crate::ai::providers::{ContentBlock, MessageContent};

const MODEL_E4B: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/resources/models/gemma-4-E4B-it-Q4_K_M.gguf"
);

fn speak_tool() -> ToolDefinition {
    ToolDefinition {
        name: "speak".to_string(),
        description: "Respond to the user with text".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {"message": {"type": "string"}},
            "required": ["message"]
        }),
    }
}

fn describe_tool() -> ToolDefinition {
    ToolDefinition {
        name: "describe_individual".to_string(),
        description: "Fetch full details of an entity by IRI".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {"iris": {"type": "array", "items": {"type": "string"}}},
            "required": ["iris"]
        }),
    }
}

fn search_tool() -> ToolDefinition {
    ToolDefinition {
        name: "search".to_string(),
        description: "Search for entities by label or type".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "query": {"type": "string"},
                "class_iri": {"type": "string"}
            },
            "required": []
        }),
    }
}

fn build_turns(
    system: &str,
    tools: &[ToolDefinition],
    turns: Vec<(&str, MessageContent)>,
) -> Vec<(String, String)> {
    let mut messages: Vec<(String, String)> = vec![
        ("system".to_string(), format!("{}{}", system, format_tools_for_prompt(tools))),
    ];
    for (role, content) in turns {
        let text = extract_text_content(&content);
        if text.is_empty() { continue; }
        if let Some(last) = messages.last_mut() {
            if last.0 == role && role != "system" {
                last.1.push('\n');
                last.1.push_str(&text);
                continue;
            }
        }
        messages.push((role.to_string(), text));
    }
    messages
}

async fn run_and_parse(model_path: &str, messages: &[(String, String)])
    -> (String, Option<String>, Option<String>)
{
    let model = load_model(model_path).await.expect("failed to load model");
    let mut raw = String::new();
    run_inference(&model, messages, |p| { raw.push_str(&p); true })
        .expect("inference failed");
    println!("\n--- raw output ---\n{:?}\n---", raw);
    let tc = parse_all_tool_calls_from_output(&raw).into_iter().next();
    let name = tc.as_ref().map(|t| t.name.clone());
    let msg = tc.as_ref().and_then(|t| t.input["message"].as_str().map(|s| s.to_string()));
    (raw, name, msg)
}

async fn run_and_parse_all(model_path: &str, messages: &[(String, String)])
    -> (String, Vec<ToolCall>)
{
    let model = load_model(model_path).await.expect("failed to load model");
    let mut raw = String::new();
    run_inference(&model, messages, |p| { raw.push_str(&p); true })
        .expect("inference failed");
    println!("\n--- raw output (all calls) ---\n{:?}\n---", raw);
    let calls = parse_all_tool_calls_from_output(&raw);
    (raw, calls)
}

fn messages_with_injection(system: &str, user: &str) -> Vec<(String, String)> {
    let tools = vec![speak_tool()];
    let full_system = format!(
        "{}\n\nYou operate in a tool loop. Text responses are NEVER shown to the user \
         — use the speak tool to respond. NEVER output raw text.{}",
        system, format_tools_for_prompt(&tools)
    );
    vec![
        ("system".to_string(), full_system),
        ("user".to_string(), user.to_string()),
    ]
}

// =========================================================================
// Text extraction
// =========================================================================

#[test]
fn extract_text_from_text_variant() {
    let content = MessageContent::Text("Hello world".to_string());
    assert_eq!(extract_text_content(&content), "Hello world");
}

#[test]
fn extract_text_from_content_blocks_includes_text_and_tool_use() {
    let content = MessageContent::ContentBlocks(vec![
        ContentBlock::Text { text: "first line".to_string() },
        ContentBlock::ToolUse {
            id: "tu_1".to_string(),
            name: "search".to_string(),
            input: serde_json::json!({"query": "test"}),
        },
        ContentBlock::Text { text: "second line".to_string() },
    ]);
    let result = extract_text_content(&content);
    assert!(result.contains("first line"));
    assert!(result.contains("search"));
    assert!(result.contains("second line"));
}

#[test]
fn extract_text_tool_use_produces_native_format() {
    let content = MessageContent::ContentBlocks(vec![
        ContentBlock::ToolUse {
            id: "tu_1".to_string(),
            name: "describe_individual".to_string(),
            input: serde_json::json!({"iris": ["foundation:X"]}),
        },
    ]);
    let result = extract_text_content(&content);
    assert!(result.contains("<|tool_call>call:describe_individual"), "should use the native format");
    assert!(result.contains("<tool_call|>"), "should close with <tool_call|>");
    assert!(result.contains("foundation:X"), "should include the args");
}

#[test]
fn extract_text_tool_result_produces_readable_text() {
    let content = MessageContent::ContentBlocks(vec![
        ContentBlock::ToolResult {
            tool_use_id: "tu_1".to_string(),
            content: serde_json::json!("Tool result goes here"),
            is_error: None,
        },
    ]);
    let result = extract_text_content(&content);
    assert!(result.contains("Tool result goes here"), "should include the result content");
}

// =========================================================================
// Python speak() parser
// =========================================================================

#[test]
fn parser_python_extracts_message_double_quotes() {
    let raw = r#"speak(message="Hi! How can I help?")"#;
    assert_eq!(parse_python_speak_output(raw), "Hi! How can I help?");
}

#[test]
fn parser_python_extracts_message_single_quotes() {
    let raw = "speak(message='Hello world')";
    assert_eq!(parse_python_speak_output(raw), "Hello world");
}

#[test]
fn parser_python_removes_end_of_turn_marker() {
    let raw = "speak(message=\"Response\")\n<end_of_turn>";
    assert_eq!(parse_python_speak_output(raw), "Response");
}

#[test]
fn parser_python_removes_eos_marker() {
    let raw = "speak(message=\"Response\")<eos>";
    assert_eq!(parse_python_speak_output(raw), "Response");
}

#[test]
fn parser_python_handles_escaped_quotes() {
    let raw = r#"speak(message="He said \"hi\" to me")"#;
    assert_eq!(parse_python_speak_output(raw), r#"He said "hi" to me"#);
}

#[test]
fn parser_python_uses_last_speak_when_model_writes_reasoning_first() {
    let raw = "Let me think...\nspeak(message=\"Final response\")";
    assert_eq!(parse_python_speak_output(raw), "Final response");
}

#[test]
fn parser_python_fallback_to_clean_text_without_speak() {
    let raw = "Response without wrapper\n<end_of_turn>";
    assert_eq!(parse_python_speak_output(raw), "Response without wrapper");
}

#[test]
fn parser_python_fallback_empty_string() {
    assert_eq!(parse_python_speak_output(""), "");
}

#[test]
fn parser_python_speak_with_iris_none_extracts_only_message() {
    let raw = r#"speak(message="Hi!", iris=None)"#;
    assert_eq!(parse_python_speak_output(raw), "Hi!");
}

// =========================================================================
// Parser — Gemma 4 native format (<|tool_call>call:name{args}<tool_call|>)
// =========================================================================

#[test]
fn parse_tool_call_speak_basic() {
    let raw = "<|tool_call>call:speak{message:<|\"|>Hi!<|\"|>}<tool_call|>";
    let tc = parse_all_tool_calls_from_output(raw).into_iter().next().expect("should parse speak");
    assert_eq!(tc.name, "speak");
    assert_eq!(tc.input["message"], "Hi!");
}

#[test]
fn parse_tool_call_search() {
    let raw = "<|tool_call>call:search{query:<|\"|>capital of Brazil<|\"|>}<tool_call|>";
    let tc = parse_all_tool_calls_from_output(raw).into_iter().next().expect("should parse search");
    assert_eq!(tc.name, "search");
    assert_eq!(tc.input["query"], "capital of Brazil");
}

#[test]
fn parse_tool_call_ignores_eot_markers() {
    let raw = "<|tool_call>call:speak{message:<|\"|>ok<|\"|>}<tool_call|>\n<end_of_turn>";
    let tc = parse_all_tool_calls_from_output(raw).into_iter().next().expect("should parse with EOT");
    assert_eq!(tc.name, "speak");
    assert_eq!(tc.input["message"], "ok");
}

#[test]
fn parse_tool_call_without_closing_tag() {
    let raw = "<|tool_call>call:search{query:<|\"|>test<|\"|>}";
    let tc = parse_all_tool_calls_from_output(raw).into_iter().next().expect("should parse without closing tag");
    assert_eq!(tc.name, "search");
}

#[test]
fn parse_tool_call_returns_empty_without_tag() {
    assert!(parse_all_tool_calls_from_output("text without a tag").is_empty());
    assert!(parse_all_tool_calls_from_output("").is_empty());
}

#[test]
fn parse_tool_call_returns_empty_without_native_marker() {
    assert!(parse_all_tool_calls_from_output("call:speak{message:<|\"|>ok<|\"|>}").is_empty());
}

#[test]
fn parse_tool_call_multiple_returns_all() {
    let raw = "<|tool_call>call:search{query:<|\"|>x<|\"|>}<tool_call|>\n<|tool_call>call:speak{message:<|\"|>ok<|\"|>}<tool_call|>";
    let calls = parse_all_tool_calls_from_output(raw);
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].name, "search");
    assert_eq!(calls[1].name, "speak");
}

#[test]
fn parse_tool_call_with_iris_array() {
    let raw = "<|tool_call>call:speak{message:<|\"|>Here it is.<|\"|>,iris:[<|\"|>foundation:Task_1<|\"|>]}<tool_call|>";
    let tc = parse_all_tool_calls_from_output(raw).into_iter().next().expect("should parse speak with iris");
    assert_eq!(tc.name, "speak");
    assert_eq!(tc.input["message"], "Here it is.");
    assert_eq!(tc.input["iris"][0], "foundation:Task_1");
}

#[test]
fn parse_tool_call_legacy_format_as_fallback() {
    let raw = "tool_call\n{\"name\": \"speak\", \"input\": {\"message\": \"Hi!\"}}\n</tool_call>";
    let tc = parse_all_tool_calls_from_output(raw).into_iter().next().expect("legacy format should work as fallback");
    assert_eq!(tc.name, "speak");
    assert_eq!(tc.input["message"], "Hi!");
}

#[test]
fn parse_tool_call_xml_format_as_fallback() {
    let raw = r#"<tool_call>{"name": "speak", "input": {"message": "Hi!"}}</tool_call>"#;
    let tc = parse_all_tool_calls_from_output(raw).into_iter().next().expect("XML format should work as fallback");
    assert_eq!(tc.name, "speak");
    assert_eq!(tc.input["message"], "Hi!");
}

// =========================================================================
// Parser — parse_gemma_native_args
// =========================================================================

#[test]
fn gemma_args_simple_string() {
    let val = parse_gemma_native_args("{message:<|\"|>Hello world<|\"|>}");
    assert_eq!(val["message"], "Hello world");
}

#[test]
fn gemma_args_multiple_fields() {
    let val = parse_gemma_native_args("{query:<|\"|>test<|\"|>,class_iri:<|\"|>foundation:Task<|\"|>}");
    assert_eq!(val["query"], "test");
    assert_eq!(val["class_iri"], "foundation:Task");
}

#[test]
fn gemma_args_with_array() {
    let val = parse_gemma_native_args("{message:<|\"|>ok<|\"|>,iris:[<|\"|>foundation:Task_1<|\"|>]}");
    assert_eq!(val["message"], "ok");
    assert_eq!(val["iris"][0], "foundation:Task_1");
}

#[test]
fn gemma_args_array_multiple_items() {
    let val = parse_gemma_native_args("{iris:[<|\"|>foundation:A<|\"|>,<|\"|>foundation:B<|\"|>]}");
    assert_eq!(val["iris"][0], "foundation:A");
    assert_eq!(val["iris"][1], "foundation:B");
}

#[test]
fn gemma_args_empty() {
    let val = parse_gemma_native_args("{}");
    assert!(val.as_object().unwrap().is_empty());
}

// =========================================================================
// StreamProcessor
// =========================================================================

#[test]
fn stream_processor_emite_texto_normal() {
    let mut p = StreamProcessor::new();
    let (emit, stop) = p.feed("olá mundo");
    assert_eq!(emit.as_deref(), Some("olá mundo"));
    assert!(!stop);
}

#[test]
fn stream_processor_suprime_tool_call_nativo() {
    let mut p = StreamProcessor::new();
    let tokens = ["<|tool_call>", "call:speak{message:", "<|\"|>", "ok", "<|\"|>}", "<tool_call|>"];
    let mut emitted = String::new();
    for tok in tokens {
        let (emit, _) = p.feed(tok);
        if let Some(t) = emit { emitted.push_str(&t); }
    }
    assert!(emitted.is_empty(), "nenhum texto deve ter sido emitido: {:?}", emitted);
}

#[test]
fn stream_processor_suprime_tool_call_xml() {
    let mut p = StreamProcessor::new();
    let tokens = ["<tool", "_call>", "{\"name\":", "\"speak\",", "\"input\":", "{\"message\":\"ok\"}}", "</tool_call>"];
    let mut emitted = String::new();
    let mut stopped = false;
    for tok in tokens {
        let (emit, stop) = p.feed(tok);
        if let Some(t) = emit { emitted.push_str(&t); }
        if stop { stopped = true; break; }
    }
    assert!(emitted.is_empty(), "nenhum texto deve ter sido emitido: {:?}", emitted);
    assert!(!stopped, "não deve parar após </tool_call> — processamento continua");
}

#[test]
fn stream_processor_emite_texto_antes_de_tool_call_xml() {
    let mut p = StreamProcessor::new();
    let (emit, _) = p.feed("texto antes");
    assert_eq!(emit.as_deref(), Some("texto antes"));
    let (emit2, _) = p.feed("<tool_call>");
    assert!(emit2.is_none());
    assert_eq!(p.state, StreamState::InToolCall);
}

#[test]
fn stream_processor_falso_alarme_emite_lookahead() {
    let mut p = StreamProcessor::new();
    let (e1, _) = p.feed("<");
    assert!(e1.is_none());
    let (e2, _) = p.feed("p>algum html que nao é tool call longo o suficiente");
    assert!(e2.is_some(), "deve ter feito flush do lookahead");
    assert_eq!(p.state, StreamState::Text);
}

#[test]
fn stream_processor_flush_pending_text_retorna_lookahead_nao_emitido() {
    let mut p = StreamProcessor::new();
    p.feed("<poss");
    let pending = p.flush_pending_text();
    assert!(pending.is_some());
    assert!(pending.unwrap().contains("<poss"));
}

// =========================================================================
// Truncamento de contexto
// =========================================================================

#[test]
fn truncate_nao_remove_quando_cabem() {
    let mut msgs = vec![
        ("system".to_string(), "system prompt curto".to_string()),
        ("user".to_string(), "mensagem curta".to_string()),
    ];
    truncate_messages_if_needed(&mut msgs);
    assert_eq!(msgs.len(), 2);
}

#[test]
fn truncate_remove_mensagens_antigas_mantendo_sistema_e_ultima() {
    let large = "x".repeat(MAX_PROMPT_CHARS / 2 + 100);
    let mut msgs = vec![
        ("system".to_string(), "system".to_string()),
        ("user".to_string(), large.clone()),
        ("assistant".to_string(), "resposta".to_string()),
        ("user".to_string(), "ultima mensagem".to_string()),
    ];
    truncate_messages_if_needed(&mut msgs);
    assert_eq!(msgs[0].0, "system");
    assert_eq!(msgs.last().unwrap().1, "ultima mensagem");
    let total: usize = msgs.iter().map(|(_, c)| c.len()).sum();
    assert!(total <= MAX_PROMPT_CHARS, "total {} > limite {}", total, MAX_PROMPT_CHARS);
}

// =========================================================================
// format_tools_for_prompt
// =========================================================================

#[test]
fn format_tools_vazio_retorna_vazio() {
    assert_eq!(format_tools_for_prompt(&[]), "");
}

#[test]
fn format_tools_contem_nome_e_formato() {
    let tools = vec![speak_tool()];
    let prompt = format_tools_for_prompt(&tools);
    assert!(prompt.contains("speak"));
    assert!(prompt.contains("<|tool_call>"));
    assert!(prompt.contains("<tool_call|>"));
    assert!(prompt.contains("message"));
}

#[test]
fn format_tools_multiplas_ferramentas() {
    let tools = vec![
        speak_tool(),
        ToolDefinition {
            name: "search".to_string(),
            description: "Busca no grafo".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {"query": {"type": "string"}},
                "required": ["query"]
            }),
        },
    ];
    let prompt = format_tools_for_prompt(&tools);
    assert!(prompt.contains("speak"));
    assert!(prompt.contains("search"));
    assert!(prompt.contains("query: string"));
}

// =========================================================================
// Testes de integração — requerem o modelo GGUF em disco
// =========================================================================

#[tokio::test]
#[ignore = "requires resources/models/gemma-4-E4B-it-Q4_K_M.gguf (~4.6 GB)"]
async fn integracao_modelo_carrega_e_cache_funciona() {
    let m1 = load_model(MODEL_E4B).await.expect("failed to load model");
    let m2 = load_model(MODEL_E4B).await.expect("falha no cache");
    assert!(Arc::ptr_eq(&m1, &m2), "segunda chamada deve retornar o mesmo Arc");
}

#[tokio::test]
#[ignore = "requires resources/models/gemma-4-E4B-it-Q4_K_M.gguf (~4.6 GB)"]
async fn integracao_modelo_usa_formato_tool_call() {
    let model = load_model(MODEL_E4B).await.expect("failed to load model");
    let msgs = messages_with_injection("Você é um assistente útil.", "Diga apenas 'olá'.");

    let mut raw = String::new();
    run_inference(&model, &msgs, |p| { raw.push_str(&p); true }).expect("inference failed");

    println!("raw output: {:?}", raw);

    let tc = parse_all_tool_calls_from_output(&raw).into_iter().next();
    assert!(tc.is_some(), "modelo não gerou tool call. Output: {:?}", raw);
    assert_eq!(tc.unwrap().name, "speak");
}

#[tokio::test]
#[ignore = "requires resources/models/gemma-4-E4B-it-Q4_K_M.gguf (~4.6 GB)"]
async fn integracao_parser_extrai_mensagem_limpa() {
    let model = load_model(MODEL_E4B).await.expect("failed to load model");
    let msgs = messages_with_injection("Você é um assistente útil.", "Diga apenas 'olá'.");

    let mut raw = String::new();
    run_inference(&model, &msgs, |p| { raw.push_str(&p); true }).expect("inference failed");

    println!("raw output: {:?}", raw);

    let tc = parse_all_tool_calls_from_output(&raw).into_iter().next();
    assert!(tc.is_some(), "modelo não gerou tool call. Output: {:?}", raw);
    let message = tc.unwrap().input["message"].as_str().unwrap_or("").to_string();

    println!("parsed message: {:?}", message);
    assert!(!message.is_empty(), "mensagem parseada está vazia");
    assert!(!message.contains("tool_call"), "tag tool_call vazou para a mensagem");
    assert!(!message.contains("<end_of_turn>"), "marcador EOT não foi removido");
}

#[tokio::test]
#[ignore = "requires resources/models/gemma-4-E4B-it-Q4_K_M.gguf (~4.6 GB)"]
async fn integracao_modelo_retorna_stats_de_tokens() {
    let model = load_model(MODEL_E4B).await.expect("failed to load model");
    let msgs = messages_with_injection("Você é um assistente útil.", "Diga apenas 'olá'.");

    let stats = run_inference(&model, &msgs, |_| true).expect("inference failed");

    assert!(stats.prompt_tokens > 0, "prompt_tokens deve ser > 0");
    assert!(stats.completion_tokens > 0, "completion_tokens deve ser > 0");
    println!("prompt: {} tokens, completion: {} tokens", stats.prompt_tokens, stats.completion_tokens);
}

#[tokio::test]
#[ignore = "requires resources/models/gemma-4-E4B-it-Q4_K_M.gguf (~4.6 GB)"]
async fn integracao_modelo_responde_em_portugues() {
    let model = load_model(MODEL_E4B).await.expect("failed to load model");
    let msgs = messages_with_injection(
        "Você é um assistente útil. Responda sempre em português.",
        "Qual é a capital do Brasil?",
    );

    let mut raw = String::new();
    run_inference(&model, &msgs, |p| { raw.push_str(&p); true }).expect("inference failed");

    let message = parse_all_tool_calls_from_output(&raw)
        .into_iter()
        .next()
        .and_then(|tc| tc.input["message"].as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| parse_python_speak_output(&raw));

    println!("parsed: {:?}", message);
    let lower = message.to_lowercase();
    assert!(
        lower.contains("brasília") || lower.contains("brasilia"),
        "resposta não mencionou Brasília. Got: {:?}", message
    );
}

#[tokio::test]
#[ignore = "requires resources/models/gemma-4-E4B-it-Q4_K_M.gguf (~4.6 GB)"]
async fn integracao_modelo_nao_encontrado_retorna_erro_claro() {
    let result = load_model("/tmp/modelo_inexistente.gguf").await;
    let err = result.expect_err("esperava Err, mas load_model retornou Ok");
    assert!(err.contains("not found"), "erro deveria mencionar 'not found': {}", err);
}

// =========================================================================
// Testes de cenário: tool loop, artefatos de sistema, multi-turn
// =========================================================================

#[tokio::test]
#[ignore = "requires resources/models/gemma-4-E4B-it-Q4_K_M.gguf (~4.6 GB)"]
async fn cenario_speak_simples() {
    let tools = vec![speak_tool()];
    let msgs = build_turns(
        "You are a helpful assistant.",
        &tools,
        vec![("user", MessageContent::Text("What is 2 + 2?".into()))],
    );
    let (_, name, msg) = run_and_parse(MODEL_E4B, &msgs).await;
    assert_eq!(name.as_deref(), Some("speak"), "deve chamar speak");
    let msg = msg.expect("speak deve ter message");
    assert!(msg.contains('4'), "resposta deve conter '4'. Got: {:?}", msg);
}

#[tokio::test]
#[ignore = "requires resources/models/gemma-4-E4B-it-Q4_K_M.gguf (~4.6 GB)"]
async fn cenario_speak_apos_resultado_de_ferramenta() {
    let tools = vec![speak_tool(), search_tool()];
    let msgs = build_turns(
        "You are a helpful assistant.",
        &tools,
        vec![
            ("user", MessageContent::Text("Find all tasks due today.".into())),
            ("assistant", MessageContent::ContentBlocks(vec![
                ContentBlock::ToolUse {
                    id: "t1".into(),
                    name: "search".into(),
                    input: serde_json::json!({"query": "tasks due today"}),
                },
            ])),
            ("user", MessageContent::ContentBlocks(vec![
                ContentBlock::ToolResult {
                    tool_use_id: "t1".into(),
                    content: serde_json::json!({"entities": [{"label": "Pay rent", "scheduledAt": "2026-04-15"}], "total": 1}),
                    is_error: None,
                },
            ])),
        ],
    );
    let (_, name, msg) = run_and_parse(MODEL_E4B, &msgs).await;
    assert_eq!(name.as_deref(), Some("speak"), "deve chamar speak após ter o resultado");
    let msg = msg.expect("speak deve ter message");
    println!("speak message: {:?}", msg);
    assert!(!msg.is_empty(), "mensagem não pode estar vazia");
}

#[tokio::test]
#[ignore = "requires resources/models/gemma-4-E4B-it-Q4_K_M.gguf (~4.6 GB)"]
async fn cenario_sem_loop_de_ferramenta() {
    let tools = vec![speak_tool(), describe_tool()];
    let msgs = build_turns(
        "You are a helpful assistant.",
        &tools,
        vec![
            ("user", MessageContent::Text("Tell me about foundation:Task_123.".into())),
            ("assistant", MessageContent::ContentBlocks(vec![
                ContentBlock::ToolUse {
                    id: "t1".into(),
                    name: "describe_individual".into(),
                    input: serde_json::json!({"iris": ["foundation:Task_123"]}),
                },
            ])),
            ("user", MessageContent::ContentBlocks(vec![
                ContentBlock::ToolResult {
                    tool_use_id: "t1".into(),
                    content: serde_json::json!({"label": "Buy groceries", "status": "Pending", "scheduledAt": "2026-04-15"}),
                    is_error: None,
                },
            ])),
        ],
    );
    let (_, name, msg) = run_and_parse(MODEL_E4B, &msgs).await;
    assert_ne!(name.as_deref(), Some("describe_individual"),
        "não deve repetir describe_individual. Got: {:?}", name);
    assert_eq!(name.as_deref(), Some("speak"), "deve chamar speak com o resumo");
    let msg = msg.expect("speak deve ter message");
    println!("speak message: {:?}", msg);
    assert!(!msg.is_empty());
}

#[tokio::test]
#[ignore = "requires resources/models/gemma-4-E4B-it-Q4_K_M.gguf (~4.6 GB)"]
async fn cenario_multitool_na_mesma_resposta() {
    let tools = vec![speak_tool(), describe_tool()];
    let msgs = build_turns(
        "You are a helpful assistant. When the user asks to describe multiple entities, \
         call describe_individual for each entity in a single response — emit one <tool_call> \
         per entity, all in the same reply.",
        &tools,
        vec![
            ("user", MessageContent::Text(
                "Describe both foundation:Task_A and foundation:Task_B right now. \
                 Call describe_individual twice in this response.".into()
            )),
        ],
    );
    let (raw, calls) = run_and_parse_all(MODEL_E4B, &msgs).await;
    println!("tool calls ({} total): {:?}", calls.len(),
        calls.iter().map(|c| &c.name).collect::<Vec<_>>());
    assert!(
        calls.len() >= 2,
        "deve emitir pelo menos 2 tool_calls na mesma resposta. Obteve {} — raw: {:?}",
        calls.len(), raw
    );
    let names: Vec<&str> = calls.iter().map(|c| c.name.as_str()).collect();
    assert!(
        names.contains(&"describe_individual"),
        "deve chamar describe_individual. Obteve: {:?}", names
    );
}

#[tokio::test]
#[ignore = "requires resources/models/gemma-4-E4B-it-Q4_K_M.gguf (~4.6 GB)"]
async fn cenario_speak_sem_artefatos_de_sistema() {
    let tools = vec![speak_tool(), search_tool()];
    let msgs = build_turns(
        "You are a helpful assistant.",
        &tools,
        vec![
            ("user", MessageContent::Text("List my projects.".into())),
            ("assistant", MessageContent::ContentBlocks(vec![
                ContentBlock::ToolUse {
                    id: "t1".into(),
                    name: "search".into(),
                    input: serde_json::json!({"class_iri": "foundation:Project"}),
                },
            ])),
            ("user", MessageContent::ContentBlocks(vec![
                ContentBlock::ToolResult {
                    tool_use_id: "t1".into(),
                    content: serde_json::json!({"entities": [{"label": "FOUNDATION"}, {"label": "Website"}], "total": 2}),
                    is_error: None,
                },
            ])),
        ],
    );
    let (_, _, msg) = run_and_parse(MODEL_E4B, &msgs).await;
    let msg = msg.unwrap_or_default();
    println!("speak message: {:?}", msg);
    assert!(!msg.contains("<start_of_turn>"), "artefato <start_of_turn> no speak");
    assert!(!msg.contains("<end_of_turn>"), "artefato <end_of_turn> no speak");
    assert!(!msg.contains("[called "), "prefixo [called] vazou para o speak");
    assert!(!msg.contains("<result>"), "tag <result> vazou para o speak");
    assert!(!msg.contains("<action>"), "tag <action> vazou para o speak");
}
