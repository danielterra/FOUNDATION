use crate::eavto::Connection;
use crate::owl::{vocabulary, get_literal_property};

/// Returns `(class_group, individual_group, literal_group)` from the ontology.
/// Falls back to the compile-time defaults `(1, 6, 7)` if the ontology data is missing.
pub fn load_graph_node_groups(conn: &Connection) -> (u8, u8, u8) {
    let configs = get_graph_node_type_config(conn);
    let group_for = |label: &str| -> Option<u8> {
        configs.iter().find(|c| c.label == label).map(|c| c.group)
    };
    (
        group_for("Class Node").unwrap_or(1),
        group_for("Individual Node").unwrap_or(6),
        group_for("Literal Node").unwrap_or(7),
    )
}

/// Returns all `foundation:GraphNodeType` individuals with their configuration as a serializable structure.
pub fn get_graph_node_type_config(conn: &Connection) -> Vec<GraphNodeTypeConfig> {
    use crate::eavto::query;

    let Ok(types_result) = query::get_by_predicate_object(conn, vocabulary::rdf::TYPE, "foundation:GraphNodeType") else {
        return vec![];
    };

    let mut configs = Vec::new();
    for triple in &types_result.triples {
        let iri = &triple.subject;

        let label = get_literal_property(conn, iri, vocabulary::rdfs::LABEL)
            .ok()
            .flatten()
            .unwrap_or_default();

        let group_str = get_literal_property(conn, iri, "foundation:graphGroup")
            .ok()
            .flatten()
            .unwrap_or_default();

        let Ok(group) = group_str.parse::<u8>() else {
            continue;
        };

        configs.push(GraphNodeTypeConfig {
            iri: iri.clone(),
            label,
            group,
        });
    }

    configs.sort_by_key(|c| c.group);
    configs
}

#[derive(Debug, serde::Serialize)]
pub struct GraphNodeTypeConfig {
    pub iri: String,
    pub label: String,
    pub group: u8,
}
