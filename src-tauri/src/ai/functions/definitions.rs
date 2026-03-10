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
                    name: "super_concepts".to_string(),
                    param_type: "array".to_string(),
                    description: "Parent concept IRIs. REPLACES all existing parents. Use a single-element array for single inheritance.".to_string(),
                    required: false,
                    schema: Some(serde_json::json!({ "type": "array", "items": { "type": "string" } })),
                },
                Parameter {
                    name: "allowed_statuses".to_string(),
                    param_type: "array".to_string(),
                    description: "Status IRIs allowed for this concept's things. REPLACES existing values.".to_string(),
                    required: false,
                    schema: Some(serde_json::json!({ "type": "array", "items": { "type": "string" } })),
                },
                Parameter {
                    name: "required_fields".to_string(),
                    param_type: "array".to_string(),
                    description: "Property IRIs required when creating a thing of this concept. REPLACES existing.".to_string(),
                    required: false,
                    schema: Some(serde_json::json!({ "type": "array", "items": { "type": "string" } })),
                },
                Parameter {
                    name: "upsert_details".to_string(),
                    param_type: "array".to_string(),
                    description: concat!(
                        "Property IRIs to associate with this concept (adds concept as a domain).",
                        " The property must already exist — define it first with learn_properties.",
                        " Does NOT remove unlisted details — use remove_details.",
                    ).to_string(),
                    required: false,
                    schema: Some(serde_json::json!({ "type": "array", "items": { "type": "string" } })),
                },
                Parameter {
                    name: "remove_details".to_string(),
                    param_type: "array".to_string(),
                    description: "Property IRIs to permanently remove (also retracts all existing values on every thing).".to_string(),
                    required: false,
                    schema: Some(serde_json::json!({ "type": "array", "items": { "type": "string" } })),
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
                        " foundation:hasStatus is validated against the concept's allowedStatus list.",
                        " Does NOT remove unlisted properties — use remove_properties.",
                    ).to_string(),
                    required: false,
                    schema: Some(serde_json::json!({
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "detail_iri": { "type": "string", "description": "Property IRI." },
                                "values": { "type": "array", "items": { "type": "string" }, "description": "Values to set." }
                            },
                            "required": ["detail_iri", "values"]
                        }
                    })),
                },
                Parameter {
                    name: "remove_properties".to_string(),
                    param_type: "array".to_string(),
                    description: "Property IRIs to clear all values of. Use forget_things to remove a single value.".to_string(),
                    required: false,
                    schema: Some(serde_json::json!({ "type": "array", "items": { "type": "string" } })),
                },
            ],
        },

        ToolTemplate {
            name: "learn_properties".to_string(),
            array_mode: true,
            description: concat!(
                "Create or update an OWL property (reusable across multiple concepts).",
                " Domains are managed exclusively via learn_concepts upsert_details.",
            ).to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "IRI of the property (e.g. 'foundation:partOfProcess'). Required.".to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "label".to_string(),
                    param_type: "string".to_string(),
                    description: "Display name. Required when creating a new property.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "property_type".to_string(),
                    param_type: "string".to_string(),
                    description: "'object' (links to another thing) or 'datatype' (stores a literal value). Required when creating.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "range".to_string(),
                    param_type: "string".to_string(),
                    description: "For 'object': target class IRI. For 'datatype': xsd type (e.g. 'xsd:string', 'xsd:integer'). Omit to keep existing.".to_string(),
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
                    name: "unit".to_string(),
                    param_type: "string".to_string(),
                    description: "QUDT unit IRI. Required for numeric ranges (e.g. 'unit:Second', 'unit:Meter'). Omit to keep existing.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "formula".to_string(),
                    param_type: "string".to_string(),
                    description: "Calculated formula using {{property_iri}} syntax. Datatype properties only. Circular dependencies are rejected.".to_string(),
                    required: false,
                    schema: None,
                },
            ],
        },

        // ----------------------------------------------------------------
        // REMEMBER / GET
        // ----------------------------------------------------------------
        ToolTemplate {
            name: "remember".to_string(),
            array_mode: false,
            description: concat!(
                "Search across classes and individuals. ALL query tokens must match (AND semantics).",
                " Without query: lists entities. With concept_iri: scoped to that class.",
                " With filters: filters by property values (requires concept_iri).",
                " Each result includes type ('class'|'individual'), matchedProperties, conceptType, and status.",
            ).to_string(),
            parameters: vec![
                Parameter {
                    name: "query".to_string(),
                    param_type: "string".to_string(),
                    description: "Search keywords. ALL tokens must match (AND). Split by whitespace. Use 1-3 important words in English.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "type".to_string(),
                    param_type: "string".to_string(),
                    description: "Filter to 'class' or 'individual' only. Omit to return both.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "concept_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "Filter to instances of this class (e.g. 'foundation:Task').".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "filters".to_string(),
                    param_type: "array".to_string(),
                    description: "Filter by property values. Requires concept_iri. Each item: {detail: 'property_iri', value: 'VALUE', operator?: '='|'>='|'<='|'>'|'<'}. operator defaults to '='. For xsd:date use ISO 'YYYY-MM-DD'. For xsd:dateTime use RFC3339.".to_string(),
                    required: false,
                    schema: Some(serde_json::json!({
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "detail": { "type": "string", "description": "Property IRI to filter on." },
                                "value": { "type": "string", "description": "Value to match." },
                                "operator": { "type": "string", "description": "Comparison operator: '=', '>=', '<=', '>', '<'. Defaults to '='." }
                            },
                            "required": ["detail", "value"]
                        }
                    })),
                },
                Parameter {
                    name: "include_retracted".to_string(),
                    param_type: "boolean".to_string(),
                    description: "Include deleted entities/facts. Default: false.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "limit".to_string(),
                    param_type: "integer".to_string(),
                    description: "Max results (default: 20).".to_string(),
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
            name: "get_concepts".to_string(),
            array_mode: false,
            description: "Batch-fetch full concept (class) details by IRI array. Returns properties, connections, allowedStatuses, subclasses, required fields, and incoming properties for each.".to_string(),
            parameters: vec![
                Parameter {
                    name: "iris".to_string(),
                    param_type: "array".to_string(),
                    description: "Array of concept IRIs to fetch (e.g. ['foundation:Task', 'foundation:Project']).".to_string(),
                    required: true,
                    schema: Some(serde_json::json!({
                        "type": "array",
                        "items": { "type": "string" }
                    })),
                },
            ],
        },
        ToolTemplate {
            name: "get_things".to_string(),
            array_mode: false,
            description: "Batch-fetch full individual (thing) details by IRI array. Returns all properties, backlinks, allowedStatuses, and requiredFields for each.".to_string(),
            parameters: vec![
                Parameter {
                    name: "iris".to_string(),
                    param_type: "array".to_string(),
                    description: "Array of thing IRIs to fetch (e.g. ['foundation:Task_123', 'foundation:Task_456']).".to_string(),
                    required: true,
                    schema: Some(serde_json::json!({
                        "type": "array",
                        "items": { "type": "string" }
                    })),
                },
            ],
        },
        ToolTemplate {
            name: "remember_properties".to_string(),
            array_mode: true,
            description: "Fetch or search OWL properties. With 'iri': full details (type, domains, ranges, unit, formula). Without 'iri': keyword search across all properties.".to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "IRI of the property to fetch. Omit to search instead.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "query".to_string(),
                    param_type: "string".to_string(),
                    description: "Search keywords matched against label, comment, and IRI.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "limit".to_string(),
                    param_type: "integer".to_string(),
                    description: "Max results for search (default: 50).".to_string(),
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
            name: "forget_properties".to_string(),
            array_mode: true,
            description: "Permanently remove an OWL property definition and all its triples.".to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "IRI of the property to forget.".to_string(),
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
        // PROCESS AUTOMATION
        // ----------------------------------------------------------------
        ToolTemplate {
            name: "run_process".to_string(),
            array_mode: false,
            description: "Trigger a BPMN process by IRI. The process runs asynchronously in the background. Returns immediately with confirmation.".to_string(),
            parameters: vec![
                Parameter {
                    name: "process_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "IRI of the bpmn_Process to execute (e.g. 'foundation:bpmn_Process_123').".to_string(),
                    required: true,
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
