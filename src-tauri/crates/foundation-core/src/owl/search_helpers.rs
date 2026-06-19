pub(super) fn score_entity_against_tokens(
    iri: &str,
    triples: &[crate::eavto::Triple],
    tokens: &[String],
    matched_properties: &mut Vec<serde_json::Value>,
) -> Option<i32> {
    let label = triples.iter()
        .find(|t| t.predicate == "rdfs:label")
        .and_then(|t| t.object.as_literal())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();

    let local_part = iri.split(':').last().unwrap_or("").to_lowercase();

    let mut total_score: i32 = 0;

    for token in tokens {
        let token_lower = token.to_lowercase();
        let mut token_score: i32 = 0;
        let mut matched_prop: Option<serde_json::Value> = None;

        if iri.to_lowercase() == token_lower || local_part == token_lower {
            token_score = 100;
        } else if label == token_lower {
            token_score = 50;
        } else if label.starts_with(&token_lower) {
            token_score = 40;
        } else if label.contains(&token_lower) {
            token_score = 30;
        } else {
            let comment_match = triples.iter()
                .find(|t| t.predicate == "rdfs:comment" && t.object.as_literal().map(|v| v.to_lowercase().contains(&token_lower)).unwrap_or(false));
            if comment_match.is_some() {
                token_score = 20;
                matched_prop = Some(serde_json::json!({ "detail_iri": "rdfs:comment" }));
            } else {
                let prop_match = triples.iter().find(|t| {
                    t.predicate != "rdfs:label"
                        && t.predicate != "rdfs:comment"
                        && t.predicate != "foundation:hasIcon"
                        && t.object.as_literal().map(|v| v.to_lowercase().contains(&token_lower)).unwrap_or(false)
                });
                if let Some(pm) = prop_match {
                    token_score = 10;
                    matched_prop = Some(serde_json::json!({ "detail_iri": pm.predicate }));
                }
            }
        }

        if token_score == 0 {
            return None;
        }
        total_score += token_score;
        if let Some(mp) = matched_prop {
            matched_properties.push(mp);
        }
    }

    Some(total_score)
}

pub(super) fn matched_properties_for_tokens(
    iri: &str,
    triples: &[crate::eavto::Triple],
    tokens: &[String],
) -> Vec<serde_json::Value> {
    let local_part = iri.split(':').last().unwrap_or("").to_lowercase();
    let label = triples.iter()
        .find(|t| t.predicate == "rdfs:label")
        .and_then(|t| t.object.as_literal())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();

    let mut matched: Vec<serde_json::Value> = Vec::new();

    for token in tokens {
        let tok = token.to_lowercase();

        if iri.to_lowercase().contains(&tok) || local_part.contains(&tok) || label.contains(&tok) {
            continue;
        }

        let prop_match = triples.iter().find(|t| {
            t.predicate != "rdfs:label"
                && t.predicate != "foundation:hasIcon"
                && t.object.as_literal()
                    .map(|v| v.to_lowercase().contains(&tok))
                    .unwrap_or(false)
        });

        if let Some(pm) = prop_match {
            let entry = serde_json::json!({ "detail_iri": pm.predicate });
            if !matched.iter().any(|e| e == &entry) {
                matched.push(entry);
            }
        }
    }

    matched
}

pub(super) fn entity_type_matches(type_iri: Option<&str>, filter: Option<&str>) -> bool {
    match filter {
        None => true,
        Some(f) => {
            let is_class = type_iri == Some("owl:Class");
            let et = if is_class { "class" } else { "individual" };
            et == f
        }
    }
}
