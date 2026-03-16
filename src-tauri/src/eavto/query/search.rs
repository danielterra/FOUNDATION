use rusqlite::Connection;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Debug, Clone)]
pub struct EntitySearchRow {
    pub subject: String,
    pub label: String,
    pub type_iri: Option<String>,
    pub has_icon_iri: Option<String>,
    pub icon_literal: Option<String>,
    pub props_raw: Option<String>,
}

pub fn search_entities(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> Result<Vec<EntitySearchRow>> {
    if query.is_empty() {
        return search_entities_all(conn, limit);
    }

    let q_lower = query.to_lowercase();
    let q_like = format!("%{}%", q_lower);
    let q_start = format!("{}%", q_lower);

    let sql = "
    WITH
    iri_match AS (
        SELECT DISTINCT subject, 100 AS score, NULL AS match_pred, NULL AS match_val
        FROM triples
        WHERE retracted = 0
          AND (subject = :q_exact
               OR LOWER(subject) = :q_lower
               OR LOWER(SUBSTR(subject, INSTR(subject, ':') + 1)) = :q_lower)
    ),
    label_match AS (
        SELECT DISTINCT subject,
            CASE
                WHEN LOWER(object_value) = :q_lower THEN 50
                WHEN LOWER(object_value) LIKE :q_start THEN 40
                ELSE 30
            END AS score,
            NULL AS match_pred, NULL AS match_val
        FROM triples
        WHERE retracted = 0
          AND predicate = 'rdfs:label'
          AND object_type = 'literal'
          AND LOWER(object_value) LIKE :q_like
    ),
    comment_match AS (
        SELECT DISTINCT subject, 20 AS score,
            predicate AS match_pred, object_value AS match_val
        FROM triples
        WHERE retracted = 0
          AND predicate = 'rdfs:comment'
          AND object_type = 'literal'
          AND LOWER(object_value) LIKE :q_like
    ),
    prop_match AS (
        SELECT DISTINCT subject, 10 AS score,
            predicate AS match_pred, SUBSTR(object_value, 1, 200) AS match_val
        FROM triples
        WHERE retracted = 0
          AND object_type = 'literal'
          AND predicate NOT IN ('rdfs:label', 'rdfs:comment', 'foundation:icon', 'foundation:content')
          AND LENGTH(object_value) <= 500
          AND LOWER(object_value) LIKE :q_like
    ),
    all_matches AS (
        SELECT subject, score, match_pred, match_val FROM iri_match
        UNION ALL
        SELECT subject, score, match_pred, match_val FROM label_match
        UNION ALL
        SELECT subject, score, match_pred, match_val FROM comment_match
        UNION ALL
        SELECT subject, score, match_pred, match_val FROM prop_match
    ),
    best AS (
        SELECT subject, MAX(score) AS best_score
        FROM all_matches
        GROUP BY subject
    ),
    matched_props AS (
        SELECT subject,
            GROUP_CONCAT(match_pred || char(31) || match_val, char(30)) AS props_raw
        FROM (SELECT DISTINCT subject, match_pred, match_val
              FROM all_matches
              WHERE match_pred IS NOT NULL AND score <= 20)
        GROUP BY subject
    ),
    labels AS (
        SELECT subject, MIN(object_value) AS label
        FROM triples
        WHERE retracted = 0 AND predicate = 'rdfs:label' AND object_type = 'literal'
        GROUP BY subject
    ),
    types AS (
        SELECT subject,
            COALESCE(
                MIN(CASE WHEN object NOT LIKE 'owl:%' AND object NOT LIKE 'rdf:%' AND object NOT LIKE 'rdfs:%' THEN object END),
                MIN(object)
            ) AS type_iri
        FROM triples
        WHERE retracted = 0 AND predicate = 'rdf:type' AND object_type = 'iri'
        GROUP BY subject
    ),
    has_icon AS (
        SELECT subject, MIN(object) AS icon_iri
        FROM triples
        WHERE retracted = 0 AND predicate = 'foundation:hasIcon' AND object_type = 'iri'
        GROUP BY subject
    ),
    icon_lit AS (
        SELECT subject, MIN(object_value) AS icon_val
        FROM triples
        WHERE retracted = 0 AND predicate = 'foundation:icon' AND object_type = 'literal'
        GROUP BY subject
    )
    SELECT
        b.subject,
        COALESCE(l.label, b.subject) AS label,
        t.type_iri,
        hi.icon_iri,
        il.icon_val,
        mp.props_raw
    FROM best b
    LEFT JOIN labels l ON l.subject = b.subject
    LEFT JOIN types t ON t.subject = b.subject
    LEFT JOIN has_icon hi ON hi.subject = b.subject
    LEFT JOIN icon_lit il ON il.subject = b.subject
    LEFT JOIN matched_props mp ON mp.subject = b.subject
    ORDER BY b.best_score DESC, length(COALESCE(l.label, b.subject)) ASC
    LIMIT :limit
    ";

    let limit_i64 = limit as i64;
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(
        rusqlite::named_params! {
            ":q_exact": query,
            ":q_lower": q_lower,
            ":q_like": q_like,
            ":q_start": q_start,
            ":limit": limit_i64,
        },
        |row| Ok(EntitySearchRow {
            subject: row.get(0)?,
            label: row.get(1)?,
            type_iri: row.get(2)?,
            has_icon_iri: row.get(3)?,
            icon_literal: row.get(4)?,
            props_raw: row.get(5)?,
        }),
    )?.collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}


pub(super) fn search_entities_all(conn: &Connection, limit: usize) -> Result<Vec<EntitySearchRow>> {
    let sql = "
    WITH
    labels AS (
        SELECT subject, MIN(object_value) AS label
        FROM triples
        WHERE retracted = 0 AND predicate = 'rdfs:label' AND object_type = 'literal'
        GROUP BY subject
    ),
    types AS (
        SELECT subject,
            COALESCE(
                MIN(CASE WHEN object NOT LIKE 'owl:%' AND object NOT LIKE 'rdf:%' AND object NOT LIKE 'rdfs:%' THEN object END),
                MIN(object)
            ) AS type_iri
        FROM triples
        WHERE retracted = 0 AND predicate = 'rdf:type' AND object_type = 'iri'
        GROUP BY subject
    ),
    has_icon AS (
        SELECT subject, MIN(object) AS icon_iri
        FROM triples
        WHERE retracted = 0 AND predicate = 'foundation:hasIcon' AND object_type = 'iri'
        GROUP BY subject
    ),
    icon_lit AS (
        SELECT subject, MIN(object_value) AS icon_val
        FROM triples
        WHERE retracted = 0 AND predicate = 'foundation:icon' AND object_type = 'literal'
        GROUP BY subject
    )
    SELECT
        l.subject, l.label,
        t.type_iri, hi.icon_iri, il.icon_val, NULL AS props_raw
    FROM labels l
    LEFT JOIN types t ON t.subject = l.subject
    LEFT JOIN has_icon hi ON hi.subject = l.subject
    LEFT JOIN icon_lit il ON il.subject = l.subject
    ORDER BY length(l.label) ASC
    LIMIT :limit
    ";

    let limit_i64 = limit as i64;
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(
        rusqlite::named_params! { ":limit": limit_i64 },
        |row| Ok(EntitySearchRow {
            subject: row.get(0)?,
            label: row.get(1)?,
            type_iri: row.get(2)?,
            has_icon_iri: row.get(3)?,
            icon_literal: row.get(4)?,
            props_raw: row.get(5)?,
        }),
    )?.collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}
