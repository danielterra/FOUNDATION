use crate::{AppState, SetupData};

#[tauri::command]
pub fn complete_initial_setup(state: tauri::State<AppState>, setup_data: String) -> Result<String, String> {
    let data: SetupData = serde_json::from_str(&setup_data)
        .map_err(|e| format!("Parse error: {}", e))?;

    let db = state.db.lock().map_err(|e| format!("Lock error: {}", e))?;

    if let Some(ref conn) = *db {
        // Origin IDs
        let user_origin_id: i64 = 2;  // foundation:CurrentUser
        let foundation_origin_id: i64 = 3;  // foundation:FOUNDATION

        // Get current timestamp
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        // Get next transaction ID
        let tx: i64 = conn.query_row(
            "SELECT COALESCE(MAX(tx), 0) + 1 FROM triples",
            [],
            |row| row.get(0)
        ).map_err(|e| format!("Query error: {}", e))?;

        // Create Person instance (CurrentUser)
        let person_id = "foundation:CurrentUser";

        // Person rdf:type
        conn.execute(
            "INSERT INTO triples (subject, predicate, object, object_type, tx, origin_id, created_at, retracted)
             VALUES (?, 'rdf:type', 'foundation:Person', 'iri', ?, ?, ?, 0)",
            (person_id, tx, user_origin_id, now)
        ).map_err(|e| format!("Insert error: {}", e))?;

        // Person name
        conn.execute(
            "INSERT INTO triples (subject, predicate, object_value, object_type, object_datatype, tx, origin_id, created_at, retracted)
             VALUES (?, 'foundation:name', ?, 'literal', 'xsd:string', ?, ?, ?, 0)",
            (person_id, &data.person_name, tx + 1, user_origin_id, now)
        ).map_err(|e| format!("Insert error: {}", e))?;

        // Person email (if provided)
        if let Some(ref email) = data.person_email {
            if !email.is_empty() {
                conn.execute(
                    "INSERT INTO triples (subject, predicate, object_value, object_type, object_datatype, tx, origin_id, created_at, retracted)
                     VALUES (?, 'foundation:email', ?, 'literal', 'xsd:string', ?, ?, ?, 0)",
                    (person_id, email, tx + 2, user_origin_id, now)
                ).map_err(|e| format!("Insert error: {}", e))?;
            }
        }

        // Create Computer instance
        let computer_id = "foundation:CurrentComputer";

        // Computer rdf:type
        conn.execute(
            "INSERT INTO triples (subject, predicate, object, object_type, tx, origin_id, created_at, retracted)
             VALUES (?, 'rdf:type', 'foundation:Computer', 'iri', ?, ?, ?, 0)",
            (computer_id, tx + 3, foundation_origin_id, now)
        ).map_err(|e| format!("Insert error: {}", e))?;

        // Computer name
        conn.execute(
            "INSERT INTO triples (subject, predicate, object_value, object_type, object_datatype, tx, origin_id, created_at, retracted)
             VALUES (?, 'foundation:name', ?, 'literal', 'xsd:string', ?, ?, ?, 0)",
            (computer_id, &data.computer_name, tx + 4, foundation_origin_id, now)
        ).map_err(|e| format!("Insert error: {}", e))?;

        // Computer processor (if provided)
        if let Some(ref processor) = data.computer_processor {
            if !processor.is_empty() {
                conn.execute(
                    "INSERT INTO triples (subject, predicate, object_value, object_type, object_datatype, tx, origin_id, created_at, retracted)
                     VALUES (?, 'foundation:processor', ?, 'literal', 'xsd:string', ?, ?, ?, 0)",
                    (computer_id, processor, tx + 5, foundation_origin_id, now)
                ).map_err(|e| format!("Insert error: {}", e))?;
            }
        }

        // Computer memory (if provided)
        if let Some(memory) = data.computer_memory {
            conn.execute(
                "INSERT INTO triples (subject, predicate, object_integer, object_type, object_datatype, tx, origin_id, created_at, retracted)
                 VALUES (?, 'foundation:memory', ?, 'literal', 'xsd:integer', ?, ?, ?, 0)",
                (computer_id, memory as i64, tx + 6, foundation_origin_id, now)
            ).map_err(|e| format!("Insert error: {}", e))?;
        }

        // Create SoftwareAgent instance (FOUNDATION app)
        let software_id = "foundation:FOUNDATIONApp";

        // Software rdf:type
        conn.execute(
            "INSERT INTO triples (subject, predicate, object, object_type, tx, origin_id, created_at, retracted)
             VALUES (?, 'rdf:type', 'foundation:SoftwareAgent', 'iri', ?, ?, ?, 0)",
            (software_id, tx + 7, foundation_origin_id, now)
        ).map_err(|e| format!("Insert error: {}", e))?;

        // Software name
        conn.execute(
            "INSERT INTO triples (subject, predicate, object_value, object_type, object_datatype, tx, origin_id, created_at, retracted)
             VALUES (?, 'foundation:name', 'FOUNDATION', 'literal', 'xsd:string', ?, ?, ?, 0)",
            (software_id, tx + 8, foundation_origin_id, now)
        ).map_err(|e| format!("Insert error: {}", e))?;

        // Software version
        conn.execute(
            "INSERT INTO triples (subject, predicate, object_value, object_type, object_datatype, tx, origin_id, created_at, retracted)
             VALUES (?, 'foundation:version', '0.1.0', 'literal', 'xsd:string', ?, ?, ?, 0)",
            (software_id, tx + 9, foundation_origin_id, now)
        ).map_err(|e| format!("Insert error: {}", e))?;

        Ok("Setup completed successfully".to_string())
    } else {
        Err("Database not initialized".to_string())
    }
}
