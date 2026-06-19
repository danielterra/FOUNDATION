use std::path::Path;
use std::sync::Mutex;

use rusqlite::Connection;
use tantivy::{
    DocId, Score, SegmentReader,
    Index, IndexWriter, IndexReader, ReloadPolicy, TantivyDocument,
    collector::TopDocs,
    merge_policy::LogMergePolicy,
    query::{AllQuery, BooleanQuery, Occur, Query, QueryParser, TermQuery},
    schema::{Field, IndexRecordOption, Schema, TextFieldIndexing, TextOptions, Value, STRING, STORED, TEXT, FAST},
    tokenizer::{LowerCaser, NgramTokenizer, TextAnalyzer},
    Term,
};

use crate::diagnostics::log_backend;

const WRITER_HEAP_BYTES: usize = 50_000_000;
const COMPLETED_PENALTY: f32 = 0.2;
const ACCESS_BOOST_WEIGHT: f32 = 0.3;

struct SearchIndex {
    index: Index,
    writer: IndexWriter,
    reader: IndexReader,
    f_iri: Field,
    f_label: Field,
    f_comment: Field,
    f_props: Field,
    f_content: Field,
    f_concept: Field,
    f_is_class: Field,
    f_boost: Field,
    f_completed: Field,
}

lazy_static::lazy_static! {
    static ref SEARCH_INDEX: Mutex<Option<SearchIndex>> = Mutex::new(None);
}

const NGRAM_TOKENIZER_NAME: &str = "ngram_label";

fn build_schema() -> (Schema, Field, Field, Field, Field, Field, Field, Field, Field, Field) {
    let mut b = Schema::builder();
    let f_iri       = b.add_text_field("iri",       STRING | STORED);
    let label_opts  = TextOptions::default()
        .set_stored()
        .set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer(NGRAM_TOKENIZER_NAME)
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        );
    let f_label     = b.add_text_field("label", label_opts);
    let f_comment   = b.add_text_field("comment",   TEXT);
    let f_props     = b.add_text_field("props",     TEXT);
    let f_content   = b.add_text_field("content",   TEXT);
    let f_concept   = b.add_text_field("concept",   STRING);
    let f_is_class  = b.add_text_field("is_class",  STRING);
    let f_boost     = b.add_u64_field("boost",      FAST   | STORED);
    let f_completed = b.add_u64_field("completed",  FAST);
    (b.build(), f_iri, f_label, f_comment, f_props, f_content, f_concept, f_is_class, f_boost, f_completed)
}

pub fn ensure_access_table(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS entity_access_count (
            iri   TEXT PRIMARY KEY,
            count INTEGER NOT NULL DEFAULT 0
        );"
    )
}

pub fn track_access(conn: &Connection, iri: &str) {
    let _ = conn.execute(
        "INSERT INTO entity_access_count (iri, count) VALUES (?1, 1)
         ON CONFLICT(iri) DO UPDATE SET count = count + 1",
        [iri],
    );
    reindex_subjects(conn, &[iri.to_string()]);
}

fn get_access_count(conn: &Connection, iri: &str) -> u64 {
    conn.query_row(
        "SELECT count FROM entity_access_count WHERE iri = ?1",
        [iri],
        |row| row.get::<_, i64>(0),
    )
    .map(|c| c as u64)
    .unwrap_or(0)
}

fn get_class_iris(conn: &Connection, subject: &str) -> Vec<String> {
    let direct: Option<String> = conn.query_row(
        "SELECT object FROM triples
         WHERE subject = ?1 AND retracted = 0 AND predicate = 'rdf:type'
           AND object NOT LIKE 'owl:%'
           AND object NOT LIKE 'rdf:%'
           AND object NOT LIKE 'rdfs:%'
         LIMIT 1",
        [subject],
        |row| row.get(0),
    ).ok();

    let Some(root) = direct else { return vec![] };

    conn.prepare(
        "WITH RECURSIVE ancestors(iri) AS (
             SELECT ?1
             UNION ALL
             SELECT t.object FROM triples t
             JOIN ancestors a ON t.subject = a.iri
             WHERE t.predicate = 'rdfs:subClassOf' AND t.retracted = 0
               AND t.object NOT LIKE 'owl:%'
               AND t.object NOT LIKE 'rdf:%'
               AND t.object NOT LIKE 'rdfs:%'
         )
         SELECT DISTINCT iri FROM ancestors",
    )
    .ok()
    .and_then(|mut stmt| {
        stmt.query_map([&root], |row| row.get(0))
            .ok()
            .map(|iter| iter.filter_map(|r| r.ok()).collect())
    })
    .unwrap_or_else(|| vec![root])
}

pub fn init(index_dir: &Path, conn: &Connection) {
    let mut guard = match SEARCH_INDEX.lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    if guard.is_some() {
        return;
    }
    let t0 = std::time::Instant::now();
    log_backend("info", &format!("[SEARCH] Initializing index at {:?}", index_dir));
    match do_init(index_dir, conn) {
        Ok(idx) => {
            *guard = Some(idx);
            log_backend("info", &format!("[SEARCH] Index ready in {}ms", t0.elapsed().as_millis()));
        }
        Err(e) => {
            log_backend("error", &format!("[SEARCH] Init failed after {}ms: {}", t0.elapsed().as_millis(), e));
        }
    }
}

fn do_init(index_dir: &Path, conn: &Connection) -> Result<SearchIndex, Box<dyn std::error::Error>> {
    for attempt in 0u8..2 {
        if attempt > 0 {
            log_backend("warn", "[SEARCH] Corrupt index — wiping and rebuilding from scratch");
            std::fs::remove_dir_all(index_dir).ok();
        }
        let t = std::time::Instant::now();
        log_backend("info", &format!("[SEARCH] attempt {} — try_open_index", attempt + 1));
        match try_open_index(index_dir, conn) {
            Ok(idx) => {
                log_backend("info", &format!("[SEARCH] attempt {} succeeded in {}ms", attempt + 1, t.elapsed().as_millis()));
                return Ok(idx);
            }
            Err(e) if attempt == 0 => {
                log_backend("warn", &format!("[SEARCH] attempt 1 failed after {}ms: {} — retrying", t.elapsed().as_millis(), e));
            }
            Err(e) => return Err(e),
        }
    }
    unreachable!()
}

fn try_open_index(index_dir: &Path, conn: &Connection) -> Result<SearchIndex, Box<dyn std::error::Error>> {
    let (schema, f_iri, f_label, f_comment, f_props, f_content, f_concept, f_is_class, f_boost, f_completed) = build_schema();

    let t = std::time::Instant::now();
    let meta_json = index_dir.join("meta.json");
    let (index, needs_rebuild) = if meta_json.exists() {
        // meta.json present: directory holds a previously written index — safe to open.
        log_backend("info", &format!("[SEARCH] meta.json found, opening index at {:?}", index_dir));
        let existing = Index::open_in_dir(index_dir)?;
        if existing.schema() == schema {
            log_backend("info", &format!("[SEARCH] open_in_dir OK, schema matches ({}ms)", t.elapsed().as_millis()));
            (existing, false)
        } else {
            log_backend("warn", &format!("[SEARCH] Schema mismatch — recreating ({}ms)", t.elapsed().as_millis()));
            // Drop the Index to release MmapDirectory handles before removing on Windows.
            drop(existing);
            std::fs::remove_dir_all(index_dir)?;
            std::fs::create_dir_all(index_dir)?;
            (Index::create_in_dir(index_dir, schema)?, true)
        }
    } else {
        // No meta.json: directory absent or empty/fresh — create directly without opening.
        // Calling open_in_dir on a directory without meta.json would instantiate a
        // MmapDirectory watcher on Windows, holding a directory handle that prevents the
        // subsequent remove_dir_all (os error 32). Skipping open_in_dir avoids that entirely.
        log_backend("info", "[SEARCH] meta.json absent — creating fresh index");
        std::fs::create_dir_all(index_dir)?;
        (Index::create_in_dir(index_dir, schema)?, true)
    };

    index.tokenizers().register(
        NGRAM_TOKENIZER_NAME,
        TextAnalyzer::builder(NgramTokenizer::new(2, 10, false)?)
            .filter(LowerCaser)
            .build(),
    );

    log_backend("info", &format!("[SEARCH] Acquiring writer ({}ms)", t.elapsed().as_millis()));
    let writer: IndexWriter = index.writer(WRITER_HEAP_BYTES)?;
    writer.set_merge_policy(Box::new(LogMergePolicy::default()));
    log_backend("info", &format!("[SEARCH] Writer ready ({}ms)", t.elapsed().as_millis()));

    let seg_count = index.searchable_segment_ids()?.len();
    log_backend("info", &format!("[SEARCH] Segment count: {} ({}ms)", seg_count, t.elapsed().as_millis()));

    log_backend("info", &format!("[SEARCH] Building reader ({}ms)", t.elapsed().as_millis()));
    let reader = index
        .reader_builder()
        .reload_policy(ReloadPolicy::Manual)
        .try_into()?;
    log_backend("info", &format!("[SEARCH] Reader ready, needs_rebuild={} ({}ms)", needs_rebuild, t.elapsed().as_millis()));

    let mut idx = SearchIndex {
        index, writer, reader, f_iri, f_label, f_comment, f_props, f_content, f_concept, f_is_class, f_boost, f_completed,
    };

    if needs_rebuild {
        do_full_rebuild(&mut idx, conn)?;
    }

    Ok(idx)
}

fn fetch_class_iris(conn: &Connection) -> std::collections::HashSet<String> {
    conn.prepare(
        "SELECT DISTINCT t1.subject FROM triples t1
         WHERE t1.predicate = 'rdf:type' AND t1.retracted = 0
           AND t1.object IN ('owl:Class', 'owl:ObjectProperty', 'owl:DatatypeProperty', 'owl:AnnotationProperty')
           AND NOT EXISTS (
               SELECT 1 FROM triples t2
               WHERE t2.subject = t1.subject AND t2.predicate = 'rdf:type' AND t2.retracted = 0
                 AND t2.object NOT LIKE 'owl:%'
                 AND t2.object NOT LIKE 'rdf:%'
                 AND t2.object NOT LIKE 'rdfs:%'
           )",
    )
    .ok()
    .and_then(|mut stmt| {
        stmt.query_map([], |row| row.get(0))
            .ok()
            .map(|iter| iter.filter_map(|r| r.ok()).collect())
    })
    .unwrap_or_default()
}

fn do_full_rebuild(
    idx: &mut SearchIndex,
    conn: &Connection,
) -> Result<(), Box<dyn std::error::Error>> {
    let t = std::time::Instant::now();
    log_backend("info", "[SEARCH] Full rebuild starting...");

    let mut stmt = conn.prepare(
        "SELECT DISTINCT subject FROM triples WHERE retracted = 0 AND predicate = 'rdfs:label'",
    )?;
    let labeled: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    log_backend("info", &format!("[SEARCH] Labeled subjects: {} ({}ms)", labeled.len(), t.elapsed().as_millis()));

    let mut msg_stmt = conn.prepare(
        "SELECT DISTINCT subject FROM triples WHERE retracted = 0
         AND predicate = 'rdf:type' AND object = 'foundation:AIConversationMessage'",
    )?;
    let messages: Vec<String> = msg_stmt
        .query_map([], |row| row.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    log_backend("info", &format!("[SEARCH] Message subjects: {} ({}ms)", messages.len(), t.elapsed().as_millis()));

    let mut seen = std::collections::HashSet::new();
    let subjects: Vec<String> = labeled.into_iter()
        .chain(messages)
        .filter(|s| seen.insert(s.clone()))
        .collect();

    log_backend("info", &format!("[SEARCH] Indexing {} total subjects ({}ms)", subjects.len(), t.elapsed().as_millis()));

    let class_iris = fetch_class_iris(conn);
    log_backend("info", &format!("[SEARCH] Class IRI fetch done ({}ms)", t.elapsed().as_millis()));

    idx.writer.delete_all_documents()?;

    for (i, subject) in subjects.iter().enumerate() {
        let is_class = class_iris.contains(subject.as_str());
        if let Some(doc) = build_document(idx, conn, subject, is_class) {
            idx.writer.add_document(doc)?;
        }
        if i > 0 && i % 1000 == 0 {
            log_backend("info", &format!("[SEARCH] Indexed {}/{} subjects ({}ms)", i, subjects.len(), t.elapsed().as_millis()));
        }
    }
    log_backend("info", &format!("[SEARCH] All documents added, committing ({}ms)", t.elapsed().as_millis()));

    idx.writer.commit()?;
    idx.reader.reload()?;
    log_backend("info", &format!("[SEARCH] Full rebuild complete in {}ms", t.elapsed().as_millis()));
    Ok(())
}

fn is_completed_status(conn: &Connection, status_iri: &str) -> bool {
    let mut visited = std::collections::HashSet::new();
    let mut queue = vec![status_iri.to_string()];
    while let Some(iri) = queue.pop() {
        if !visited.insert(iri.clone()) {
            continue;
        }
        if iri == "foundation:Completed" {
            return true;
        }
        let parents: Vec<String> = conn
            .prepare(
                "SELECT object FROM triples
                 WHERE subject = ?1 AND predicate = 'rdfs:subClassOf' AND retracted = 0",
            )
            .ok()
            .and_then(|mut stmt| {
                stmt.query_map([&iri], |row| row.get(0))
                    .ok()
                    .map(|iter| iter.filter_map(|r| r.ok()).collect())
            })
            .unwrap_or_default();
        queue.extend(parents);
    }
    false
}

fn subject_is_class(conn: &Connection, subject: &str) -> bool {
    let has_instance_type: bool = conn.query_row(
        "SELECT COUNT(*) FROM triples
         WHERE subject = ?1 AND retracted = 0 AND predicate = 'rdf:type'
           AND object NOT LIKE 'owl:%'
           AND object NOT LIKE 'rdf:%'
           AND object NOT LIKE 'rdfs:%'",
        [subject],
        |row| row.get::<_, i64>(0),
    ).map(|c| c > 0).unwrap_or(false);

    if has_instance_type {
        return false;
    }

    conn.query_row(
        "SELECT COUNT(*) FROM triples
         WHERE subject = ?1 AND retracted = 0 AND predicate = 'rdf:type'
           AND object IN ('owl:Class', 'owl:ObjectProperty', 'owl:DatatypeProperty', 'owl:AnnotationProperty')",
        [subject],
        |row| row.get::<_, i64>(0),
    ).map(|c| c > 0).unwrap_or(false)
}

fn subject_is_completed(conn: &Connection, subject: &str) -> bool {
    let status_iri = conn.query_row(
        "SELECT object FROM triples
         WHERE subject = ?1 AND predicate = 'foundation:hasStatus' AND retracted = 0 LIMIT 1",
        [subject],
        |row| row.get::<_, String>(0),
    ).ok();
    match status_iri {
        Some(iri) => is_completed_status(conn, &iri),
        None => false,
    }
}

fn build_document(idx: &SearchIndex, conn: &Connection, subject: &str, is_class: bool) -> Option<TantivyDocument> {
    let mut stmt = conn.prepare(
        "SELECT predicate, object_value
         FROM triples
         WHERE subject = ? AND retracted = 0
           AND object_type = 'literal'
           AND predicate NOT IN (
               'foundation:hasIcon',
               'foundation:partOfConversation',
               'foundation:sender',
               'foundation:receiver',
               'foundation:sentAt'
           )",
    ).ok()?;

    let rows: Vec<(String, String)> = stmt
        .query_map([subject], |row| Ok((row.get(0)?, row.get(1)?)))
        .ok()?
        .filter_map(|r| r.ok())
        .filter(|(_, v): &(String, String)| !v.is_empty())
        .collect();

    let mut label = String::new();
    let mut comment = String::new();
    let mut props: Vec<String> = Vec::new();
    let mut content_raw = String::new();

    for (predicate, value) in &rows {
        match predicate.as_str() {
            "rdfs:label"         => label       = value.clone(),
            "rdfs:comment"       => comment     = value.clone(),
            "foundation:content" => content_raw = value.clone(),
            _                    => props.push(value.clone()),
        }
    }

    // Index IRI-valued properties: predicate label + object label.
    // e.g. foundation:mother → Person makes "Mãe Andrea Terra Borlino" searchable.
    if let Ok(mut iri_stmt) = conn.prepare(
        "SELECT t.predicate, t.object FROM triples t
         WHERE t.subject = ?1 AND t.retracted = 0 AND t.object_type = 'iri'
           AND t.predicate NOT IN (
               'rdf:type', 'rdfs:subClassOf', 'owl:inverseOf',
               'rdfs:domain', 'rdfs:range', 'owl:inverseOf',
               'foundation:hasIcon', 'foundation:partOfConversation',
               'foundation:sender', 'foundation:receiver'
           )",
    ) {
        let iri_rows: Vec<(String, String)> = iri_stmt
            .query_map([subject], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default();

        for (predicate, obj_iri) in &iri_rows {
            let pred_label: Option<String> = conn.query_row(
                "SELECT object_value FROM triples \
                 WHERE subject = ?1 AND predicate = 'rdfs:label' AND retracted = 0 LIMIT 1",
                [predicate.as_str()],
                |row| row.get(0),
            ).ok();
            let obj_label: Option<String> = conn.query_row(
                "SELECT object_value FROM triples \
                 WHERE subject = ?1 AND predicate = 'rdfs:label' AND retracted = 0 LIMIT 1",
                [obj_iri.as_str()],
                |row| row.get(0),
            ).ok();
            if let Some(l) = pred_label { props.push(l); }
            if let Some(l) = obj_label  { props.push(l); }
        }
    }

    let content_text = if content_raw.is_empty() {
        String::new()
    } else {
        extract_content_text(&content_raw)
    };

    if label.is_empty() {
        if content_text.is_empty() {
            return None;
        }
        label = content_text.chars().take(80).collect();
    }

    let access_count = get_access_count(conn, subject);
    let classes = get_class_iris(conn, subject);
    let completed = u64::from(subject_is_completed(conn, subject));

    let mut doc = TantivyDocument::default();
    doc.add_text(idx.f_iri, subject);
    doc.add_text(idx.f_label, &label);
    if !comment.is_empty() {
        doc.add_text(idx.f_comment, &comment);
    }
    if !props.is_empty() {
        doc.add_text(idx.f_props, &props.join(" "));
    }
    if !content_text.is_empty() {
        doc.add_text(idx.f_content, &content_text);
    }
    for c in &classes {
        doc.add_text(idx.f_concept, c);
    }
    doc.add_text(idx.f_is_class, if is_class { "1" } else { "0" });
    doc.add_u64(idx.f_boost, access_count);
    doc.add_u64(idx.f_completed, completed);
    Some(doc)
}

fn extract_content_text(raw: &str) -> String {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) {
        if let Some(arr) = value.as_array() {
            let text: String = arr.iter()
                .filter_map(|b| b.get("text")?.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            if !text.is_empty() {
                return text;
            }
        }
        if let Some(s) = value.as_str() {
            return s.to_string();
        }
    }
    raw.to_string()
}

pub fn remove_from_index(subjects: &[String]) {
    if subjects.is_empty() {
        return;
    }
    let mut guard = match SEARCH_INDEX.lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    let idx = match guard.as_mut() {
        Some(idx) => idx,
        None => return,
    };
    for subject in subjects {
        let term = Term::from_field_text(idx.f_iri, subject);
        idx.writer.delete_term(term);
    }
    if let Err(e) = idx.writer.commit() {
        log_backend("warn", &format!("Search index commit failed: {}", e));
        return;
    }
    if let Err(e) = idx.reader.reload() {
        log_backend("warn", &format!("Search index reader reload failed: {}", e));
    }
}

pub fn reindex_subjects(conn: &Connection, subjects: &[String]) {
    if subjects.is_empty() {
        return;
    }

    let mut guard = match SEARCH_INDEX.lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    let idx = match guard.as_mut() {
        Some(idx) => idx,
        None => return,
    };

    let unique: std::collections::HashSet<&String> = subjects.iter().collect();

    for subject in unique {
        let term = Term::from_field_text(idx.f_iri, subject);
        idx.writer.delete_term(term);

        let is_class = subject_is_class(conn, subject);
        if let Some(doc) = build_document(idx, conn, subject, is_class) {
            if let Err(e) = idx.writer.add_document(doc) {
                log_backend("warn", &format!("Search index: failed to add {}: {}", subject, e));
            }
        }
    }

    if let Err(e) = idx.writer.commit() {
        log_backend("warn", &format!("Search index commit failed: {}", e));
        return;
    }
    if let Err(e) = idx.reader.reload() {
        log_backend("warn", &format!("Search index reader reload failed: {}", e));
    }
}

pub fn search(query: &str, class_iri: Option<&str>, limit: usize) -> Vec<String> {
    search_with_scores(query, class_iri, limit)
        .into_iter()
        .map(|(iri, _)| iri)
        .collect()
}

pub fn search_with_scores(query: &str, class_iri: Option<&str>, limit: usize) -> Vec<(String, f32)> {
    if query.trim().is_empty() {
        return vec![];
    }

    let guard = match SEARCH_INDEX.lock() {
        Ok(g) => g,
        Err(_) => return vec![],
    };
    let idx = match guard.as_ref() {
        Some(idx) => idx,
        None => return vec![],
    };

    let searcher = idx.reader.searcher();

    let mut parser = QueryParser::for_index(
        &idx.index,
        vec![idx.f_label, idx.f_comment, idx.f_props, idx.f_content],
    );
    parser.set_field_boost(idx.f_label, 3.0);
    parser.set_field_boost(idx.f_comment, 1.5);
    parser.set_field_boost(idx.f_content, 0.8);

    let safe_query = sanitize_query(&expand_camel_case(query));

    let text_query: Box<dyn Query> = match parser.parse_query(&safe_query) {
        Ok(q) => q,
        Err(_) => match parser.parse_query(&format!("\"{}\"", query.replace('"', ""))) {
            Ok(q) => q,
            Err(_) => return vec![],
        },
    };

    let final_query: Box<dyn Query> = match class_iri {
        Some(concept) => {
            let term = Term::from_field_text(idx.f_concept, concept);
            let concept_filter = TermQuery::new(term, IndexRecordOption::Basic);
            Box::new(BooleanQuery::new(vec![
                (Occur::Must, text_query),
                (Occur::Must, Box::new(concept_filter)),
            ]))
        }
        None => text_query,
    };

    let fetch_limit = (limit * 3).max(50);
    let top_docs_collector = TopDocs::with_limit(fetch_limit).tweak_score(
        move |seg_reader: &SegmentReader| {
            let boost_col     = seg_reader.fast_fields().u64("boost").ok();
            let completed_col = seg_reader.fast_fields().u64("completed").ok();
            move |doc: DocId, score: Score| {
                let count = boost_col.as_ref()
                    .and_then(|col| col.first(doc))
                    .unwrap_or(0);
                let is_completed = completed_col.as_ref()
                    .and_then(|col| col.first(doc))
                    .unwrap_or(0);
                let boosted = score * (1.0 + (count as f32).ln_1p() * ACCESS_BOOST_WEIGHT);
                if is_completed > 0 { boosted * COMPLETED_PENALTY } else { boosted }
            }
        }
    );

    let top_docs = match searcher.search(final_query.as_ref(), &top_docs_collector) {
        Ok(docs) => docs,
        Err(_) => return vec![],
    };

    let query_lower = query.trim().to_lowercase();

    let mut results: Vec<(String, f32)> = top_docs
        .into_iter()
        .filter_map(|(score, addr)| {
            let doc: TantivyDocument = searcher.doc(addr).ok()?;
            let iri = doc.get_first(idx.f_iri)
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())?;
            let label = doc.get_first(idx.f_label)
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let final_score = if label.trim().to_lowercase() == query_lower {
                score * 10.0
            } else {
                score
            };
            Some((iri, final_score))
        })
        .collect();

    results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(limit);
    results
}

pub fn is_initialized() -> bool {
    SEARCH_INDEX.lock().ok().map_or(false, |g| g.is_some())
}

pub fn search_all(class_iri: Option<&str>, limit: usize) -> Vec<String> {
    let guard = match SEARCH_INDEX.lock() {
        Ok(g) => g,
        Err(_) => return vec![],
    };
    let idx = match guard.as_ref() {
        Some(idx) => idx,
        None => return vec![],
    };

    let searcher = idx.reader.searcher();

    let final_query: Box<dyn Query> = match class_iri {
        Some(concept) => {
            let term = Term::from_field_text(idx.f_concept, concept);
            let concept_filter = TermQuery::new(term, IndexRecordOption::Basic);
            Box::new(BooleanQuery::new(vec![
                (Occur::Must, Box::new(AllQuery) as Box<dyn Query>),
                (Occur::Must, Box::new(concept_filter)),
            ]))
        }
        None => Box::new(AllQuery),
    };

    let top_docs_collector = TopDocs::with_limit(limit).tweak_score(
        move |seg_reader: &SegmentReader| {
            let boost_col = seg_reader.fast_fields().u64("boost").ok();
            move |doc: DocId, score: Score| {
                let count = boost_col.as_ref()
                    .and_then(|col| col.first(doc))
                    .unwrap_or(0);
                score * (1.0 + (count as f32).ln_1p() * ACCESS_BOOST_WEIGHT)
            }
        }
    );

    match searcher.search(final_query.as_ref(), &top_docs_collector) {
        Ok(top_docs) => top_docs.into_iter()
            .filter_map(|(_, addr)| {
                let doc: TantivyDocument = searcher.doc(addr).ok()?;
                doc.get_first(idx.f_iri)
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
            .collect(),
        Err(_) => vec![],
    }
}

pub fn search_concepts_with_scores(query: &str, limit: usize) -> Vec<(String, f32)> {
    if query.trim().is_empty() {
        return vec![];
    }

    let guard = match SEARCH_INDEX.lock() {
        Ok(g) => g,
        Err(_) => return vec![],
    };
    let idx = match guard.as_ref() {
        Some(idx) => idx,
        None => return vec![],
    };

    let searcher = idx.reader.searcher();

    let mut parser = QueryParser::for_index(
        &idx.index,
        vec![idx.f_label, idx.f_comment],
    );
    parser.set_field_boost(idx.f_label, 3.0);
    parser.set_field_boost(idx.f_comment, 1.5);

    let safe_query = sanitize_query(&expand_camel_case(query));

    let text_query: Box<dyn Query> = match parser.parse_query(&safe_query) {
        Ok(q) => q,
        Err(_) => match parser.parse_query(&format!("\"{}\"", query.replace('"', ""))) {
            Ok(q) => q,
            Err(_) => return vec![],
        },
    };

    let class_term = Term::from_field_text(idx.f_is_class, "1");
    let class_filter = TermQuery::new(class_term, IndexRecordOption::Basic);
    let final_query: Box<dyn Query> = Box::new(BooleanQuery::new(vec![
        (Occur::Must, text_query),
        (Occur::Must, Box::new(class_filter)),
    ]));

    let top_docs_collector = TopDocs::with_limit(limit);

    let top_docs = match searcher.search(final_query.as_ref(), &top_docs_collector) {
        Ok(docs) => docs,
        Err(_) => return vec![],
    };

    top_docs
        .into_iter()
        .filter_map(|(score, addr)| {
            let doc: TantivyDocument = searcher.doc(addr).ok()?;
            let iri = doc.get_first(idx.f_iri)
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())?;
            Some((iri, score))
        })
        .collect()
}

/// Inserts spaces before uppercase letters in CamelCase runs so that
/// e.g. "ErrorHandler" becomes "Error Handler" before Tantivy tokenisation.
fn expand_camel_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    let chars: Vec<char> = s.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if i > 0 && c.is_uppercase() {
            let prev = chars[i - 1];
            if prev.is_lowercase() || prev.is_ascii_digit() {
                out.push(' ');
            }
        }
        out.push(c);
    }
    out
}

fn sanitize_query(query: &str) -> String {
    let special = [
        ':', '/', '\\', '(', ')', '[', ']', '{', '}', '!', '^', '"', '~', '*', '?', '+', '-',
    ];
    let has_special = query.chars().any(|c| special.contains(&c));
    if has_special {
        query
            .split_whitespace()
            .map(|t| {
                let clean = t.replace('"', "");
                if clean.is_empty() { String::new() } else { format!("\"{}\"", clean) }
            })
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        query.to_string()
    }
}
