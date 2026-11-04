use crate::owl::DbExecutor;
use crate::owl::{self, Individual, Object, Triple};
use chrono::{DateTime, SecondsFormat, Utc};
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Debug, PartialEq)]
struct ParameterCounts {
    total: Option<u64>,
    active: Option<u64>,
}

fn parse_parameter_quantity(token: &str, following: Option<&str>) -> Option<u64> {
    let token = token.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '.');
    let (number, multiplier) = match token.chars().last()?.to_ascii_lowercase() {
        'k' => (&token[..token.len() - 1], 1_000_f64),
        'm' => (&token[..token.len() - 1], 1_000_000_f64),
        'b' => (&token[..token.len() - 1], 1_000_000_000_f64),
        't' => (&token[..token.len() - 1], 1_000_000_000_000_f64),
        _ => {
            let unit = following?.trim_matches(|c: char| !c.is_ascii_alphabetic());
            let multiplier = match unit.to_ascii_lowercase().as_str() {
                "thousand" => 1_000_f64,
                "million" => 1_000_000_f64,
                "billion" => 1_000_000_000_f64,
                "trillion" => 1_000_000_000_000_f64,
                _ => return None,
            };
            (token, multiplier)
        }
    };
    let value = number.parse::<f64>().ok()? * multiplier;
    (value.is_finite() && value > 0.0 && value.fract() == 0.0 && value <= u64::MAX as f64)
        .then_some(value as u64)
}

fn parse_parameter_counts(description: &str) -> ParameterCounts {
    let parsed = description
        .split(|character| matches!(character, ',' | ';' | '\n'))
        .map(parse_parameter_counts_segment)
        .collect::<Vec<_>>();
    ParameterCounts {
        total: unique(parsed.iter().filter_map(|counts| counts.total).collect()),
        active: unique(parsed.iter().filter_map(|counts| counts.active).collect()),
    }
}

fn openrouter_context_length(value: Option<u64>) -> Result<Option<i64>, String> {
    value
        .map(i64::try_from)
        .transpose()
        .map_err(|_| "OpenRouter context length exceeds the supported range".to_string())
}

fn openrouter_price_per_million(value: Option<f64>) -> Option<f64> {
    value
        .filter(|price| price.is_finite() && *price >= 0.0)
        .map(|price| price * 1_000_000.0)
}

fn parse_parameter_counts_segment(description: &str) -> ParameterCounts {
    let tokens: Vec<&str> = description.split_whitespace().collect();
    let mut totals = Vec::new();
    let mut actives = Vec::new();

    for (index, token) in tokens.iter().enumerate() {
        let (quantity, consumed_unit) = if index + 1 < tokens.len() {
            match parse_parameter_quantity(token, Some(tokens[index + 1])) {
                Some(value)
                    if !token
                        .trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '.')
                        .chars()
                        .last()
                        .is_some_and(|c| {
                            matches!(c.to_ascii_lowercase(), 'k' | 'm' | 'b' | 't')
                        }) =>
                {
                    (Some(value), 1)
                }
                Some(value) => (Some(value), 0),
                None => (None, 0),
            }
        } else {
            (parse_parameter_quantity(token, None), 0)
        };
        let Some(quantity) = quantity else { continue };
        let start = index.saturating_sub(2);
        let end = (index + consumed_unit + 4).min(tokens.len());
        let context = tokens[start..end]
            .iter()
            .enumerate()
            .map(|(offset, word)| {
                (
                    start + offset,
                    word.trim_matches(|c: char| !c.is_ascii_alphabetic())
                        .to_ascii_lowercase(),
                )
            })
            .collect::<Vec<_>>();
        let parameter_is_nearby = context
            .iter()
            .filter(|(_, word)| {
                matches!(
                    word.as_str(),
                    "parameter" | "parameters" | "param" | "params"
                )
            })
            .any(|(position, _)| index.abs_diff(*position) <= 3);
        if !parameter_is_nearby {
            continue;
        }
        let mut labels = context
            .iter()
            .filter(|(_, word)| word == "total" || word == "active")
            .map(|(label_index, word)| (index.abs_diff(*label_index), word.as_str()))
            .collect::<Vec<_>>();
        labels.sort_by_key(|(distance, _)| *distance);
        let nearest_distance = labels.first().map(|(distance, _)| *distance);
        let nearest_labels: Vec<&str> = labels
            .iter()
            .take_while(|(distance, _)| Some(*distance) == nearest_distance)
            .map(|(_, label)| *label)
            .collect();
        match nearest_labels.as_slice() {
            ["total"] => totals.push(quantity),
            ["active"] => actives.push(quantity),
            _ => {}
        }
    }

    ParameterCounts {
        total: unique(totals),
        active: unique(actives),
    }
}

fn unique(values: Vec<u64>) -> Option<u64> {
    let first = *values.first()?;
    values.iter().all(|value| *value == first).then_some(first)
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn ai__list_openrouter_models(
    api_key: String,
    base_url: String,
) -> Result<Vec<serde_json::Value>, String> {
    let url = format!(
        "{}/models?supported_parameters=tools",
        base_url.trim_end_matches('/')
    );
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .map_err(|e| format!("Failed to fetch models from {}: {}", url, e))?;

    if !response.status().is_success() {
        let status = response.status();
        return Err(format!(
            "OpenRouter models endpoint returned HTTP {}",
            status
        ));
    }

    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse models response: {}", e))?;

    let models = body["data"]
        .as_array()
        .ok_or_else(|| "Unexpected response format from /models endpoint".to_string())?;

    let result = models
        .iter()
        .map(|m| {
            let input_cost = m["pricing"]["prompt"]
                .as_str()
                .and_then(|s| s.parse::<f64>().ok());
            let output_cost = m["pricing"]["completion"]
                .as_str()
                .and_then(|s| s.parse::<f64>().ok());
            let description = m["description"].as_str().unwrap_or_default();
            let parameters = parse_parameter_counts(description);
            let input_modalities = m["architecture"]["input_modalities"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let output_modalities = m["architecture"]["output_modalities"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let supported_parameters = m["supported_parameters"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let publisher = m["id"]
                .as_str()
                .and_then(|id| id.split_once('/').map(|(publisher, _)| publisher));
            let supports_tools = supported_parameters
                .iter()
                .any(|parameter| parameter.as_str() == Some("tools"));

            serde_json::json!({
                "id": m["id"].as_str().unwrap_or(""),
                "name": m["name"].as_str().unwrap_or(""),
                "publisher": publisher,
                "description": description,
                "contextLength": m["context_length"].as_u64(),
                "inputCostPerToken": input_cost,
                "outputCostPerToken": output_cost,
                "inputModalities": input_modalities,
                "outputModalities": output_modalities,
                "supportedParameters": supported_parameters,
                "parameterCountTotal": parameters.total,
                "parameterCountActive": parameters.active,
                "supportsTools": supports_tools,
            })
        })
        .collect();

    Ok(result)
}

#[cfg(test)]
mod openrouter_metadata_tests {
    use super::{
        openrouter_context_length, openrouter_price_per_million, parse_parameter_counts,
        ParameterCounts,
    };

    #[test]
    fn preserves_missing_and_explicit_zero_context_lengths() {
        assert_eq!(openrouter_context_length(None).unwrap(), None);
        assert_eq!(openrouter_context_length(Some(0)).unwrap(), Some(0));
        assert_eq!(
            openrouter_context_length(Some(128_000)).unwrap(),
            Some(128_000)
        );
        assert!(openrouter_context_length(Some(u64::MAX)).is_err());
    }

    #[test]
    fn preserves_missing_and_explicit_zero_prices() {
        assert_eq!(openrouter_price_per_million(None), None);
        assert_eq!(openrouter_price_per_million(Some(0.0)), Some(0.0));
        assert_eq!(openrouter_price_per_million(Some(0.000_002)), Some(2.0));
        assert_eq!(openrouter_price_per_million(Some(-0.1)), None);
        assert_eq!(openrouter_price_per_million(Some(f64::INFINITY)), None);
    }

    #[test]
    fn parses_explicit_total_and_active_parameter_counts() {
        assert_eq!(
            parse_parameter_counts("70B total parameters, 13B active parameters"),
            ParameterCounts {
                total: Some(70_000_000_000),
                active: Some(13_000_000_000),
            }
        );
    }

    #[test]
    fn parses_explicit_word_units_and_reversed_labels() {
        assert_eq!(
            parse_parameter_counts("Total parameters: 1.5 billion; active parameters: 800 million"),
            ParameterCounts {
                total: Some(1_500_000_000),
                active: Some(800_000_000),
            }
        );
    }

    #[test]
    fn omits_unlabeled_or_conflicting_parameter_counts() {
        assert_eq!(
            parse_parameter_counts("A 70B parameter model with 13B active parameters"),
            ParameterCounts {
                total: None,
                active: Some(13_000_000_000),
            }
        );
        assert_eq!(
            parse_parameter_counts("70B total parameters; 80B total parameters"),
            ParameterCounts {
                total: None,
                active: None,
            }
        );
        assert_eq!(
            parse_parameter_counts("A GPT-4 model for coding"),
            ParameterCounts {
                total: None,
                active: None,
            }
        );
    }
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn ai__ensure_openrouter_model(
    service_iri: String,
    model_id: String,
    model_name: String,
    context_length: Option<u64>,
    input_cost_per_token: Option<f64>,
    output_cost_per_token: Option<f64>,
    supports_tools: bool,
    executor: State<'_, DbExecutor>,
) -> Result<String, String> {
    let context_length = openrouter_context_length(context_length)?;
    let input_price_per_million = openrouter_price_per_million(input_cost_per_token);
    let output_price_per_million = openrouter_price_per_million(output_cost_per_token);

    executor
        .write(move |conn| {
            // Return existing IRI if this model was already registered for this service.
            // Query by offeredBy (IRI property) then filter by modelIdentifier (literal).
            let existing =
                owl::find_entities_with_property(conn, "foundation:offeredBy", &service_iri)
                    .unwrap_or_default()
                    .into_iter()
                    .find(|iri| {
                        owl::get_literal_property(conn, iri, "foundation:modelIdentifier")
                            .ok()
                            .flatten()
                            .as_deref()
                            == Some(model_id.as_str())
                    });

            if let Some(iri) = existing {
                return Ok(iri);
            }

            let timestamp = chrono::Utc::now().timestamp_millis();
            let model_iri = format!("foundation:AIModel_{}", timestamp);
            let ind = Individual::new(&model_iri);

            ind.assert(conn, "foundation:AIModel", &model_name, "smart_toy", "ai")
                .map_err(|e| format!("Failed to create AIModel: {}", e))?;

            let mut props: Vec<Triple> = vec![
                Triple::new(
                    &model_iri,
                    "foundation:modelIdentifier",
                    Object::Literal {
                        value: model_id,
                        datatype: Some("xsd:string".to_string()),
                        language: None,
                    },
                ),
                Triple::new(&model_iri, "foundation:offeredBy", Object::Iri(service_iri)),
            ];
            if let Some(context_length) = context_length {
                props.push(Triple::new(
                    &model_iri,
                    "foundation:maxInputTokens",
                    Object::Integer(context_length),
                ));
            }

            // estimate_call_cost reads price per million tokens, so convert from OR's per-token values
            if let Some(input_price_per_million) = input_price_per_million {
                props.push(Triple::new(
                    &model_iri,
                    "foundation:inputPricePerMTok",
                    Object::Number(input_price_per_million),
                ));
            }
            if let Some(output_price_per_million) = output_price_per_million {
                props.push(Triple::new(
                    &model_iri,
                    "foundation:outputPricePerMTok",
                    Object::Number(output_price_per_million),
                ));
            }
            if supports_tools {
                props.push(Triple::new(
                    &model_iri,
                    "foundation:modelCapability",
                    Object::Literal {
                        value: "tool_calling".to_string(),
                        datatype: Some("xsd:string".to_string()),
                        language: None,
                    },
                ));
            }

            crate::owl::assert_raw_triples(conn, &props, "ai")
                .map_err(|e| format!("Failed to persist model properties: {}", e))?;

            Ok(model_iri)
        })
        .await
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn ai__list_api_calls(
    from_ms: Option<i64>,
    to_ms: Option<i64>,
    cursor: Option<ApiCallCursor>,
    limit: Option<i64>,
    snapshot_tx: Option<i64>,
    executor: State<'_, DbExecutor>,
) -> Result<serde_json::Value, String> {
    let page_size = limit.unwrap_or(100).clamp(1, 500) as usize;
    let (range_start, range_end) = api_call_range(from_ms, to_ms)?;
    let requested_snapshot = match cursor.as_ref() {
        Some(cursor) => {
            if cursor.from_ms != from_ms || cursor.to_ms != to_ms {
                return Err("A continuation cursor requires the original date interval".to_string());
            }
            if snapshot_tx.is_some_and(|snapshot| snapshot != cursor.snapshot_tx) {
                return Err("A continuation cursor requires its original snapshotTx".to_string());
            }
            Some(cursor.snapshot_tx)
        }
        None => match snapshot_tx {
            Some(snapshot) => Some(snapshot),
            None => None,
        },
    };
    executor
        .read(move |conn| {
            let pinned_tx = match requested_snapshot {
                Some(snapshot) => snapshot,
                None => crate::owl::current_snapshot_tx(conn)?,
            };
            let result = owl::page_relation_by_datetime_as_of(
                conn,
                "rdf:type",
                "foundation:AIAPICall",
                pinned_tx,
                "foundation:calledAt",
                range_start.as_deref(),
                range_end.as_deref(),
                false,
                cursor
                    .as_ref()
                    .map(|cursor| (cursor.order_key, cursor.iri.as_str())),
                page_size as i64 + 1,
                "foundation:hasStatus",
                "foundation:calledAt",
                "foundation:Status_1772596341042",
                "foundation:Status_1772993026091",
            )?;
            let has_more = result.items.len() > page_size;
            let page_items = result.items.into_iter().take(page_size).collect::<Vec<_>>();
            let next_cursor = if has_more {
                page_items.last().map(|item| ApiCallCursor {
                    order_key: item.key_julian_day,
                    iri: item.subject.clone(),
                    snapshot_tx: pinned_tx,
                    from_ms,
                    to_ms,
                })
            } else {
                None
            };
            let items: Vec<serde_json::Value> = page_items
                .into_iter()
                .filter_map(|row| {
                    let iri = row.subject;
                    let ind = Individual::get(conn, &iri).ok().flatten()?;

                    let called_at = ind
                        .properties
                        .iter()
                        .find(|(k, _)| k == "foundation:calledAt")
                        .and_then(|(_, v)| v.as_literal())
                        .unwrap_or_default();

                    let get_int = |prop: &str| {
                        ind.properties
                            .iter()
                            .find(|(k, _)| k == prop)
                            .and_then(|(_, v)| {
                                if let Object::Integer(n) = v {
                                    Some(*n as u64)
                                } else {
                                    None
                                }
                            })
                            .unwrap_or(0)
                    };

                    let get_num = |prop: &str| {
                        ind.properties
                            .iter()
                            .find(|(k, _)| k == prop)
                            .and_then(|(_, v)| match v {
                                Object::Number(n) => Some(*n),
                                Object::Literal { value, .. } => value.parse::<f64>().ok(),
                                _ => None,
                            })
                    };

                    let model = ind
                        .properties
                        .iter()
                        .find(|(k, _)| k == "foundation:model")
                        .and_then(|(_, v)| v.as_literal())
                        .unwrap_or_default();

                    let conv_iri = ind
                        .properties
                        .iter()
                        .find(|(k, _)| k == "foundation:generatedByConversation")
                        .and_then(|(_, v)| {
                            if let Object::Iri(s) = v {
                                Some(s.clone())
                            } else {
                                None
                            }
                        });

                    Some(serde_json::json!({
                        "iri": iri,
                        "model": model,
                        "calledAt": called_at,
                        "inputTokens": get_int("foundation:inputTokens"),
                        "outputTokens": get_int("foundation:outputTokens"),
                        "estimatedCost": get_num("foundation:estimatedCost"),
                        "conversationIri": conv_iri,
                    }))
                })
                .collect();

            Ok(serde_json::json!({
                "items": items,
                "next_cursor": next_cursor,
                "has_more": has_more,
                "snapshot_tx": pinned_tx,
                "total_count": result.stats.total_count,
            }))
        })
        .await
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiCallCursor {
    order_key: f64,
    iri: String,
    snapshot_tx: i64,
    from_ms: Option<i64>,
    to_ms: Option<i64>,
}

fn api_call_range(
    from_ms: Option<i64>,
    to_ms: Option<i64>,
) -> Result<(Option<String>, Option<String>), String> {
    let start = from_ms
        .map(|millis| {
            DateTime::<Utc>::from_timestamp_millis(millis)
                .map(|timestamp| timestamp.to_rfc3339_opts(SecondsFormat::Millis, true))
                .ok_or_else(|| "fromMs is outside the supported date range".to_string())
        })
        .transpose()?;
    let end = to_ms
        .map(|millis| {
            let exclusive_end = millis
                .checked_add(1)
                .ok_or_else(|| "toMs is outside the supported date range".to_string())?;
            DateTime::<Utc>::from_timestamp_millis(exclusive_end)
                .map(|timestamp| timestamp.to_rfc3339_opts(SecondsFormat::Millis, true))
                .ok_or_else(|| "toMs is outside the supported date range".to_string())
        })
        .transpose()?;
    Ok((start, end))
}

#[cfg(test)]
mod api_call_history_tests {
    use super::api_call_range;
    use crate::owl::{self, Connection};

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE transactions (id INTEGER PRIMARY KEY, origin TEXT, created_at INTEGER);
             CREATE TABLE origins (id INTEGER PRIMARY KEY, name TEXT UNIQUE);
             CREATE TABLE triples (
                id INTEGER PRIMARY KEY AUTOINCREMENT, subject TEXT NOT NULL, predicate TEXT NOT NULL,
                object TEXT, object_value TEXT, object_type TEXT NOT NULL, object_datatype TEXT,
                object_language TEXT, object_number REAL, object_integer INTEGER, object_datetime INTEGER,
                object_boolean INTEGER, tx INTEGER NOT NULL, origin_id INTEGER NOT NULL DEFAULT 1,
                created_at INTEGER NOT NULL DEFAULT 0, retracted INTEGER NOT NULL DEFAULT 0,
                is_current INTEGER NOT NULL DEFAULT 1
             );",
        )
        .unwrap();
        conn
    }

    fn add_api_call(conn: &Connection, iri: &str, called_at: &str, tx: i64) {
        conn.execute(
            "INSERT INTO triples (subject,predicate,object,object_type,tx) VALUES (?1,'rdf:type','foundation:AIAPICall','iri',?3)",
            rusqlite::params![iri, "foundation:AIAPICall", tx],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO triples (subject,predicate,object_value,object_type,object_datatype,tx) VALUES (?1,'foundation:calledAt',?2,'literal','xsd:dateTime',?3)",
            rusqlite::params![iri, called_at, tx],
        )
        .unwrap();
    }

    fn page(
        conn: &Connection,
        snapshot_tx: i64,
        range_start: Option<&str>,
        range_end: Option<&str>,
        after: Option<(f64, &str)>,
        limit: i64,
    ) -> owl::DateTimePage {
        owl::page_relation_by_datetime_as_of(
            conn,
            "rdf:type",
            "foundation:AIAPICall",
            snapshot_tx,
            "foundation:calledAt",
            range_start,
            range_end,
            false,
            after,
            limit,
            "foundation:hasStatus",
            "foundation:calledAt",
            "foundation:UnusedCompletedStatus",
            "foundation:UnusedFailedStatus",
        )
        .unwrap()
    }

    #[test]
    fn empty_interval_returns_no_items_and_zero_count() {
        let conn = setup_db();
        add_api_call(&conn, "foundation:Call_1", "2026-11-04T12:00:00.000Z", 1);
        let (start, end) =
            api_call_range(Some(1_793_802_000_000), Some(1_793_801_000_000)).unwrap();
        let result = page(&conn, 1, start.as_deref(), end.as_deref(), None, 10);
        assert!(result.items.is_empty());
        assert_eq!(result.stats.total_count, 0);
    }

    #[test]
    fn first_page_and_continuation_keep_date_order_and_snapshot() {
        let conn = setup_db();
        add_api_call(&conn, "foundation:Call_1", "2026-11-04T10:00:00.000Z", 1);
        add_api_call(&conn, "foundation:Call_2", "2026-11-04T11:00:00.000Z", 2);
        add_api_call(&conn, "foundation:Call_3", "2026-11-04T12:00:00.000Z", 3);
        let (start, end) = api_call_range(None, None).unwrap();
        let first = page(&conn, 3, start.as_deref(), end.as_deref(), None, 2);
        assert_eq!(first.stats.total_count, 3);
        assert_eq!(first.items.len(), 2);
        assert_eq!(first.items[0].subject, "foundation:Call_3");
        assert_eq!(first.items[1].subject, "foundation:Call_2");

        add_api_call(&conn, "foundation:Call_4", "2026-11-04T13:00:00.000Z", 4);
        let last = first.items.last().unwrap();
        let continuation = page(
            &conn,
            3,
            start.as_deref(),
            end.as_deref(),
            Some((last.key_julian_day, last.subject.as_str())),
            2,
        );
        assert_eq!(continuation.stats.total_count, 3);
        assert_eq!(continuation.items.len(), 1);
        assert_eq!(continuation.items[0].subject, "foundation:Call_1");
    }
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn ai__get_fallback_models(
    service_iri: String,
    executor: State<'_, DbExecutor>,
) -> Result<Vec<String>, String> {
    executor
        .read(move |conn| {
            let json = owl::get_literal_property(
                conn,
                &service_iri,
                "foundation:fallbackModelIdentifiers",
            )
            .unwrap_or_default()
            .unwrap_or_default();
            Ok(serde_json::from_str(&json).unwrap_or_default())
        })
        .await
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn ai__save_fallback_models(
    service_iri: String,
    model_ids: Vec<String>,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {
    let json = serde_json::to_string(&model_ids)
        .map_err(|e| format!("Failed to serialize fallback models: {}", e))?;
    executor
        .write(move |conn| {
            Individual::new(&service_iri)
                .add_property(
                    conn,
                    "foundation:fallbackModelIdentifiers",
                    vec![Object::Literal {
                        value: json,
                        datatype: Some("xsd:string".to_string()),
                        language: None,
                    }],
                    "ai",
                )
                .map_err(|e| format!("Failed to save fallback models: {}", e))?;
            Ok(String::new())
        })
        .await?;
    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn ai__validate_provider_key(api_key: String, base_url: String) -> Result<(), String> {
    let url = format!("{}/models", base_url.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .map_err(|e| format!("URL unreachable ({}): {}", url, e))?;

    match response.status().as_u16() {
        200..=299 => Ok(()),
        401 => Err("Invalid API key (401 Unauthorized).".to_string()),
        403 => Err("Access denied (403 Forbidden). Check the API key.".to_string()),
        code => Err(format!("Validation failed: HTTP {}.", code)),
    }
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn ai__get_service_base_url(
    service_iri: String,
    executor: State<'_, DbExecutor>,
) -> Result<String, String> {
    executor
        .read(move |conn| {
            Ok(
                owl::get_literal_property(conn, &service_iri, "foundation:apiBaseUrl")
                    .unwrap_or_default()
                    .unwrap_or_default(),
            )
        })
        .await
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn ai__save_service_base_url(
    service_iri: String,
    base_url: String,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {
    executor
        .write(move |conn| {
            Individual::new(&service_iri)
                .add_property(
                    conn,
                    "foundation:apiBaseUrl",
                    vec![Object::Literal {
                        value: base_url,
                        datatype: Some("xsd:anyURI".to_string()),
                        language: None,
                    }],
                    "ai",
                )
                .map_err(|e| format!("Failed to save base URL: {}", e))?;
            Ok(String::new())
        })
        .await?;
    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn ai__save_api_key(
    _app: AppHandle,
    api_key: String,
    service_iri: String,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {
    executor
        .write(move |conn| {
            // Retract previous API key if one is linked to the service
            if let Ok(Some(old_key_iri)) =
                owl::get_iri_property(conn, &service_iri, "foundation:apiKey")
            {
                Individual::retract(conn, &old_key_iri, "ai")
                    .map_err(|e| format!("Failed to retract old credential: {}", e))?;
            }

            let timestamp = chrono::Utc::now().timestamp_millis();
            let api_key_iri = format!("foundation:ClaudeAPIKey_{}", timestamp);
            let credential = Individual::new(&api_key_iri);

            credential
                .assert(
                    conn,
                    "foundation:APIKey",
                    "Claude AI API Key",
                    "vpn_key",
                    "ai",
                )
                .map_err(|e| format!("Failed to create APIKey: {}", e))?;

            credential
                .add_property(
                    conn,
                    "foundation:credentialValue",
                    vec![Object::Literal {
                        value: api_key,
                        datatype: Some("xsd:string".to_string()),
                        language: None,
                    }],
                    "ai",
                )
                .map_err(|e| format!("Failed to set credential value: {}", e))?;

            credential
                .add_property(
                    conn,
                    "foundation:credentialCreatedAt",
                    vec![Object::DateTime(
                        chrono::DateTime::from_timestamp_millis(timestamp)
                            .unwrap_or_default()
                            .to_rfc3339(),
                    )],
                    "ai",
                )
                .map_err(|e| format!("Failed to set created timestamp: {}", e))?;

            // Link the new key to the service
            let service = Individual::new(&service_iri);
            service
                .add_property(
                    conn,
                    "foundation:apiKey",
                    vec![Object::Iri(api_key_iri)],
                    "ai",
                )
                .map_err(|e| format!("Failed to link key to service: {}", e))?;

            Ok("saved".to_string())
        })
        .await?;

    super::log_backend("info", "API key saved successfully");

    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn ai__delete_api_key(
    service_iri: String,
    executor: State<'_, DbExecutor>,
) -> Result<(), String> {
    executor
        .write(move |conn| {
            if let Ok(Some(old_key_iri)) =
                owl::get_iri_property(conn, &service_iri, "foundation:apiKey")
            {
                Individual::retract(conn, &old_key_iri, "ai")
                    .map_err(|e| format!("Failed to retract credential: {}", e))?;
            }
            Individual::clear_property(conn, &service_iri, "foundation:apiKey", "ai")
                .map_err(|e| format!("Failed to clear apiKey link: {}", e))?;
            Ok(String::new())
        })
        .await?;

    super::log_backend("info", "API key deleted");
    Ok(())
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn ai__get_api_key(
    service_iri: String,
    executor: State<'_, DbExecutor>,
) -> Result<Option<String>, String> {
    executor
        .read(move |conn| {
            if let Ok(Some(api_key_iri)) =
                owl::get_iri_property(conn, &service_iri, "foundation:apiKey")
            {
                if let Ok(Some(value)) =
                    owl::get_literal_property(conn, &api_key_iri, "foundation:credentialValue")
                {
                    return Ok(Some(value));
                }
            }
            Ok(None)
        })
        .await
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn ai__initialize(
    app: AppHandle,
    _executor: State<'_, DbExecutor>,
) -> Result<(), String> {
    let app_for_recovery = app.clone();
    tauri::async_runtime::spawn(async move {
        let retention_gate = app_for_recovery
            .state::<super::setup::StartupRetentionGate>()
            .inner()
            .clone();
        retention_gate.wait().await;
        let executor_state = app_for_recovery.state::<DbExecutor>();
        let cancellation_state = app_for_recovery.state::<super::chat::AiCancellationState>();
        match super::chat::chat__recover_pending_tools(
            app_for_recovery.clone(),
            executor_state,
            cancellation_state,
        )
        .await
        {
            Ok(count) if count > 0 => {
                super::log_backend(
                    "info",
                    &format!("[RECOVERY] Recovered {} pending tool execution(s)", count),
                );
            }
            Ok(_) => {}
            Err(e) => {
                super::log_backend(
                    "warn",
                    &format!("[RECOVERY] Failed to check pending tools: {}", e),
                );
                app_for_recovery
                    .emit("ai-error", serde_json::json!({ "message": e }))
                    .ok();
            }
        }
    });

    Ok(())
}
