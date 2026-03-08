use super::{ToolTemplate, Parameter};

pub fn get_claude_tools() -> Vec<crate::ai::providers::ClaudeTool> {
    get_available_tools()
        .into_iter()
        .map(|f| f.to_claude_tool())
        .collect()
}

pub fn get_available_tools() -> Vec<ToolTemplate> {
    vec![
        // ----------------------------------------------------------------
        // LEARN (create or update)
        // ----------------------------------------------------------------
        ToolTemplate {
            name: "learn_concepts".to_string(),
            array_mode: true,
            description: "Create or update a concept (a class of things). Without 'iri' creates; with existing 'iri' updates. Use English labels.".to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "IRI for this concept (e.g. 'foundation:Project'). Required.".to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "label".to_string(),
                    param_type: "string".to_string(),
                    description: "English name. Required when creating.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "icon".to_string(),
                    param_type: "string".to_string(),
                    description: "Material icon name or image URL. Required when creating.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "comment".to_string(),
                    param_type: "string".to_string(),
                    description: "Optional description.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "super_concept".to_string(),
                    param_type: "string".to_string(),
                    description: "Single parent concept IRI. Replaces existing parent.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "super_concepts".to_string(),
                    param_type: "array".to_string(),
                    description: "Parent concept IRIs (multiple inheritance). REPLACES all existing parents.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "allowed_statuses".to_string(),
                    param_type: "array".to_string(),
                    description: "Status IRIs allowed for this concept's things. REPLACES existing values.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "required_fields".to_string(),
                    param_type: "array".to_string(),
                    description: "Property IRIs required when creating a thing of this concept. REPLACES existing.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "upsert_details".to_string(),
                    param_type: "array".to_string(),
                    description: concat!(
                        "Upsert property definitions on this concept.",
                        " Each item: {iri, label, detail_type ('object'|'datatype'), range?, formula?, unit?, comment?}.",
                        " 'formula': {{property_iri}} expression — calculated, read-only. Circular deps rejected.",
                        " 'unit': QUDT IRI required for numeric types (e.g. 'unit:BRL').",
                        " Does NOT remove unlisted details — use remove_details.",
                    ).to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "remove_details".to_string(),
                    param_type: "array".to_string(),
                    description: "Property IRIs to permanently remove (also retracts all existing values on every thing).".to_string(),
                    required: false,
                    schema: None,
                },
            ],
        },
        ToolTemplate {
            name: "learn_things".to_string(),
            array_mode: true,
            description: "Create or update a thing (instance of a concept). Without 'iri' creates; with 'iri' updates. Search first with remember_things to avoid duplicates.".to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "IRI of an existing thing to update. Omit to create a new thing.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "concept_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "Concept this thing belongs to. Required when creating (no iri).".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "label".to_string(),
                    param_type: "string".to_string(),
                    description: "Name of this thing. Required when creating (no iri).".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "icon".to_string(),
                    param_type: "string".to_string(),
                    description: "Material icon name or image URL. Inherits from concept if omitted.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "comment".to_string(),
                    param_type: "string".to_string(),
                    description: "Optional description.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "upsert_properties".to_string(),
                    param_type: "array".to_string(),
                    description: concat!(
                        "Upsert property values on this thing.",
                        " Each item: {detail_iri, values: [...], value_type?: 'iri'|'literal', datatype?: 'xsd:string'|...}.",
                        " foundation:hasStatus is validated against the concept's allowedStatus list.",
                        " Does NOT remove unlisted properties — use remove_properties.",
                    ).to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "remove_properties".to_string(),
                    param_type: "array".to_string(),
                    description: "Property IRIs to clear all values of. Use forget_things to remove a single value.".to_string(),
                    required: false,
                    schema: None,
                },
            ],
        },

        // ----------------------------------------------------------------
        // REMEMBER (fetch by iri, or search without iri)
        // ----------------------------------------------------------------
        ToolTemplate {
            name: "remember_concepts".to_string(),
            array_mode: true,
            description: "Fetch or search concepts. With 'iri': full details (properties, connections, allowedStatuses, subclasses). Without 'iri': keyword search. Always use English.".to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "ID of the concept to fetch. Omit to search instead.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "query".to_string(),
                    param_type: "string".to_string(),
                    description: "Search keywords (used when no iri). ALL words must match (AND search). Use 1-3 important words in ENGLISH.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "limit".to_string(),
                    param_type: "integer".to_string(),
                    description: "Max results for search (default: 100).".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "offset".to_string(),
                    param_type: "integer".to_string(),
                    description: "Skip N results for pagination (default: 0).".to_string(),
                    required: false,
                    schema: None,
                },
            ],
        },
        ToolTemplate {
            name: "remember_things".to_string(),
            array_mode: true,
            description: "Fetch or search things. With 'iri': full details (all properties, connections, backlinks, allowedStatuses, requiredFields). Without 'iri': searches things by keyword, optionally filtered by concept. Each search result includes 'matchedProperties' showing which fields matched.".to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "IRI of the thing to fetch. Omit to search instead.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "concept_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "Filter search to this concept (optional, used when no iri).".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "query".to_string(),
                    param_type: "string".to_string(),
                    description: "Search keywords (used when no iri). ALL words must match.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "limit".to_string(),
                    param_type: "integer".to_string(),
                    description: "Max results for search (default: 15).".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "offset".to_string(),
                    param_type: "integer".to_string(),
                    description: "Skip N results for pagination (default: 0).".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "from_millis".to_string(),
                    param_type: "number".to_string(),
                    description: "Min creation timestamp in ms. Requires concept_iri.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "to_millis".to_string(),
                    param_type: "number".to_string(),
                    description: "Max creation timestamp in ms. Requires concept_iri.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "include_retracted".to_string(),
                    param_type: "boolean".to_string(),
                    description: "Include deleted things/facts. Default: false.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "properties".to_string(),
                    param_type: "array".to_string(),
                    description: "Filter by detail values. Requires concept_iri. Each item: {detail: 'IRI', value: 'VALUE', operator?: '='|'>='|'<='|'>'|'<'}. operator defaults to '='. For xsd:dateTime, operator applies to Unix ms timestamp.".to_string(),
                    required: false,
                    schema: None,
                },
            ],
        },

        // ----------------------------------------------------------------
        // FORGET (always with iri)
        // ----------------------------------------------------------------
        ToolTemplate {
            name: "forget_concepts".to_string(),
            array_mode: true,
            description: "Forget a concept definition (does not delete the things that belong to it).".to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "IRI of the concept to forget.".to_string(),
                    required: true,
                    schema: None,
                },
            ],
        },
        ToolTemplate {
            name: "forget_things".to_string(),
            array_mode: true,
            description: "Forget a thing or remove a detail value. iri only: forgets thing. + detail_iri: removes all values of that detail. + value: removes that specific value only.".to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "IRI of the thing.".to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "detail_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "Detail to remove. If omitted, the entire thing is forgotten.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "value".to_string(),
                    param_type: "string".to_string(),
                    description: "Specific value to remove. If omitted, all values of detail_iri are removed.".to_string(),
                    required: false,
                    schema: None,
                },
            ],
        },

        // ----------------------------------------------------------------
        // BLACKBOARD
        // ----------------------------------------------------------------
        ToolTemplate {
            name: "blackboard_widgets_list".to_string(),
            array_mode: false,
            description: "List available widget types and required params. Call before blackboard_update. Pass concept_iri to filter to widgets for a specific entity type.".to_string(),
            parameters: vec![
                Parameter {
                    name: "concept_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "Filter to widgets that can render this concept (e.g. 'foundation:Task').".to_string(),
                    required: false,
                    schema: None,
                },
            ],
        },

        ToolTemplate {
            name: "blackboard_update".to_string(),
            array_mode: true,
            description: concat!(
                "Update the blackboard.",
                " 'add': add a widget (needs widget_type + params).",
                " 'remove': remove a widget (params.widget_id required).",
                " 'replace': clear all widgets; optionally add new ones if widget_type+params provided.",
                " Widgets are live — no need to call after learn_things changes; entities update automatically.",
            ).to_string(),
            parameters: vec![
                Parameter {
                    name: "operation".to_string(),
                    param_type: "string".to_string(),
                    description: "'add', 'remove', or 'replace'.".to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "widget_type".to_string(),
                    param_type: "string".to_string(),
                    description: "Widget type for 'add'/'replace'. See blackboard_widgets_list.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "params".to_string(),
                    param_type: "object".to_string(),
                    description: "Widget-specific params for 'add'/'replace', or {widget_id} for 'remove'. See blackboard_widgets_list.".to_string(),
                    required: false,
                    schema: None,
                },
            ],
        },
    ]
}
