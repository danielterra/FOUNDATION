use std::path::Path;
use std::sync::Mutex;

use rusqlite::Connection;
use tantivy::{
    DocId, Score, SegmentReader,
    Index, IndexWriter, IndexReader, ReloadPolicy, TantivyDocument,
    collector::TopDocs,
    query::{BooleanQuery, Occur, Query, QueryParser, TermQuery},
    schema::{Field, IndexRecordOption, Schema, Value, STRING, STORED, TEXT, FAST},
    Term,
};

use crate::commands::log_backend;

const WRITER_HEAP_BYTES: usize = 50_000_000;

struct SearchIndex {
    index: Index,
    writer: IndexWriter,
    reader: IndexReader,
    f_iri: Field,
    f_label: Field,
    f_comment: Field,
    f_props: Field,
    f_concept: Field,
    f_boost: Field,
}

lazy_static::lazy_static! {
    static ref SEARCH_INDEX: Mutex<Option<SearchIndex>> = Mutex::new(None);
}

fn build_schema() -> (Schema, Field, Field, Field, Field, Field, Field) {
    let mut b = Schema::builder();
    let f_iri     = b.add_text_field("iri",     STRING | STORED);
    let f_label   = b.add_text_field("label",   TEXT   | STORED);
    let f_comment = b.add_text_field("comment", TEXT);
    let f_props   = b.add_text_field("props",   TEXT);
    let f_concept = b.add_text_field("concept", STRING);
    let f_boost   = b.add_u64_field("boost",    FAST   | STORED);
    (b.build(), f_iri, f_label, f_comment, f_props, f_concept, f_boost)
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

fn get_concept_iri(conn: &Connection, subject: &str) -> Option<String> {
    conn.query_row(
        "SELECT object FROM triples
         WHERE subject = ?1 AND retracted = 0 AND predicate = 'rdf:type'
           AND object NOT LIKE 'owl:%'
           AND object NOT LIKE 'rdf:%'
           AND object NOT LIKE 'rdfs:%'
         LIMIT 1",
        [subject],
        |row| row.get(0),
    )
    .ok()
}

pub fn init(index_dir: &Path, conn: &Connection) {
    let mut guard = match SEARCH_INDEX.lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    if guard.is_some() {
        return;
    }
    match do_init(index_dir, conn) {
        Ok(idx) => {
            *guard = Some(idx);
            log_backend("info", "Search index ready");
        }
        Err(e) => {
            log_backend("error", &format!("Search index init failed: {}", e));
        }
    }
}

fn do_init(index_dir: &Path, conn: &Connection) -> Result<SearchIndex, Box<dyn std::error::Error>> {
    let (schema, f_iri, f_label, f_comment, f_props, f_concept, f_boost) = build_schema();

    let (index, needs_rebuild) = if index_dir.exists() {
        let stale = Index::open_in_dir(index_dir)
            .map(|existing| existing.schema() != schema)
            .unwrap_or(true);
        if stale {
            std::fs::remove_dir_all(index_dir)?;
            std::fs::create_dir_all(index_dir)?;
            (Index::create_in_dir(index_dir, schema)?, true)
        } else {
            (Index::open_in_dir(index_dir)?, false)
        }
    } else {
        std::fs::create_dir_all(index_dir)?;
        (Index::create_in_dir(index_dir, schema)?, true)
    };

    let writer: IndexWriter = index.writer(WRITER_HEAP_BYTES)?;
    let reader = index
        .reader_builder()
        .reload_policy(ReloadPolicy::OnCommitWithDelay)
        .try_into()?;

    let mut idx = SearchIndex {
        index, writer, reader, f_iri, f_label, f_comment, f_props, f_concept, f_boost,
    };

    if needs_rebuild {
        do_full_rebuild(&mut idx, conn)?;
    }

    Ok(idx)
}

fn do_full_rebuild(
    idx: &mut SearchIndex,
    conn: &Connection,
) -> Result<(), Box<dyn std::error::Error>> {
    log_backend("info", "Building search index from scratch...");

    let mut stmt = conn.prepare(
        "SELECT DISTINCT subject FROM triples WHERE retracted = 0 AND predicate = 'rdfs:label'",
    )?;
    let subjects: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    log_backend("info", &format!("Indexing {} labeled subjects", subjects.len()));

    idx.writer.delete_all_documents()?;

    for subject in &subjects {
        if let Some(doc) = build_document(idx, conn, subject) {
            idx.writer.add_document(doc)?;
        }
    }

    idx.writer.commit()?;
    log_backend("info", "Search index build complete");
    Ok(())
}

fn build_document(idx: &SearchIndex, conn: &Connection, subject: &str) -> Option<TantivyDocument> {
    let mut stmt = conn.prepare(
        "SELECT predicate, object_value
         FROM triples
         WHERE subject = ? AND retracted = 0
           AND object_type = 'literal'
           AND predicate NOT IN (
               'foundation:content',
               'foundation:icon',
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

    for (predicate, value) in &rows {
        match predicate.as_str() {
            "rdfs:label"   => label   = value.clone(),
            "rdfs:comment" => comment = value.clone(),
            _              => props.push(value.clone()),
        }
    }

    if label.is_empty() {
        return None;
    }

    let access_count = get_access_count(conn, subject);
    let concept = get_concept_iri(conn, subject);

    let mut doc = TantivyDocument::default();
    doc.add_text(idx.f_iri, subject);
    doc.add_text(idx.f_label, &label);
    if !comment.is_empty() {
        doc.add_text(idx.f_comment, &comment);
    }
    if !props.is_empty() {
        doc.add_text(idx.f_props, &props.join(" "));
    }
    if let Some(c) = concept {
        doc.add_text(idx.f_concept, &c);
    }
    doc.add_u64(idx.f_boost, access_count);
    Some(doc)
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

        if let Some(doc) = build_document(idx, conn, subject) {
            if let Err(e) = idx.writer.add_document(doc) {
                log_backend("warn", &format!("Search index: failed to add {}: {}", subject, e));
            }
        }
    }

    if let Err(e) = idx.writer.commit() {
        log_backend("warn", &format!("Search index commit failed: {}", e));
    }
}

pub fn search(query: &str, concept_iri: Option<&str>, limit: usize) -> Vec<String> {
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
        vec![idx.f_label, idx.f_comment, idx.f_props],
    );
    parser.set_field_boost(idx.f_label, 3.0);
    parser.set_field_boost(idx.f_comment, 1.5);

    let safe_query = sanitize_query(query);

    let text_query: Box<dyn Query> = match parser.parse_query(&safe_query) {
        Ok(q) => q,
        Err(_) => match parser.parse_query(&format!("\"{}\"", query.replace('"', ""))) {
            Ok(q) => q,
            Err(_) => return vec![],
        },
    };

    let final_query: Box<dyn Query> = match concept_iri {
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

    let top_docs_collector = TopDocs::with_limit(limit).tweak_score(
        move |seg_reader: &SegmentReader| {
            let boost_col = seg_reader.fast_fields().u64("boost").ok();
            move |doc: DocId, score: Score| {
                let count = boost_col.as_ref()
                    .and_then(|col| col.first(doc))
                    .unwrap_or(0);
                score * (1.0 + (count as f32).ln_1p() * 0.3)
            }
        }
    );

    let top_docs = match searcher.search(final_query.as_ref(), &top_docs_collector) {
        Ok(docs) => docs,
        Err(_) => return vec![],
    };

    top_docs
        .into_iter()
        .filter_map(|(_score, addr)| {
            let doc: TantivyDocument = searcher.doc(addr).ok()?;
            doc.get_first(idx.f_iri)
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .collect()
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
