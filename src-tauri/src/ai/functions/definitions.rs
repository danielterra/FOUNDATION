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
            description: concat!(
                "Create or update a concept (a type of thing, e.g. 'Person', 'Project', 'Invoice').",
                " Without an existing IRI this creates a new concept; with an existing IRI it updates it.",
                " IMPORTANT: Always use English labels and descriptions.",
                " Define the concept's value fields (details) via 'calculated_fields'",
                " and its relationships to other concepts (connections) via 'connections'.",
                " Calculated fields are read-only — their value is auto-computed from a formula",
                " using {{property_iri}} syntax (e.g. \"{{foundation:width}} * {{foundation:height}}\").",
                " Circular formula dependencies are rejected at creation time.",
            ).to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "Unique ID for this concept (e.g. 'foundation:Project'). Required.".to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "label".to_string(),
                    param_type: "string".to_string(),
                    description: concat!(
                        "Name in ENGLISH (e.g. 'Driver License' not 'CNH').",
                        " Required when creating a new concept.",
                    ).to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "icon".to_string(),
                    param_type: "string".to_string(),
                    description: concat!(
                        "Material icon name (e.g. 'category', 'label') OR image URL.",
                        " Required when creating a new concept.",
                    ).to_string(),
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
                    description: concat!(
                        "List of parent concept IRIs for multiple inheritance.",
                        " REPLACES all existing parents. Pass [] to remove all.",
                    ).to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "allowed_statuses".to_string(),
                    param_type: "array".to_string(),
                    description: concat!(
                        "Complete list of Status IRIs allowed for things of this concept.",
                        " REPLACES existing values. Pass [] to remove all restrictions.",
                    ).to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "required_fields".to_string(),
                    param_type: "array".to_string(),
                    description: concat!(
                        "Property IRIs that must be provided when creating a thing of this concept.",
                        " REPLACES existing restrictions. Pass [] to remove all.",
                    ).to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "add_details".to_string(),
                    param_type: "array".to_string(),
                    description: concat!(
                        "ADDS or updates property definitions for this concept.",
                        " Each item: {iri, label, detail_type, range?, formula?, unit?, comment?}.",
                        " 'detail_type': 'object' for connections to other concepts,",
                        " 'datatype' for value fields (editable or calculated).",
                        " 'range': target concept IRI (object) or xsd: type (datatype, default xsd:string).",
                        " 'formula': {{property_iri}} expression — makes the field calculated and read-only.",
                        " 'unit': QUDT IRI (e.g. 'unit:M2', 'unit:BRL') — required for numeric types.",
                        " Does NOT remove details not listed — use remove_details for that.",
                    ).to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "remove_details".to_string(),
                    param_type: "array".to_string(),
                    description: concat!(
                        "List of property IRIs to permanently remove from this concept.",
                        " Each item is a string IRI (e.g. 'foundation:myField').",
                        " Works for both connections and calculated/value fields.",
                        " Also retracts all existing values of that property from every thing.",
                    ).to_string(),
                    required: false,
                    schema: None,
                },
            ],
        },
        ToolTemplate {
            name: "learn_things".to_string(),
            array_mode: true,
            description: concat!(
                "Create or update a specific thing (an instance of a concept).",
                " Without 'iri' this creates a new thing and returns its generated IRI.",
                " With 'iri' of an existing thing this updates it.",
                " IMPORTANT: Before creating, ALWAYS search first with remember_things",
                " (without iri) to avoid duplicates.",
            ).to_string(),
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
                    description: concat!(
                        "Material icon name or image URL.",
                        " Optional — inherits from concept when not provided.",
                    ).to_string(),
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
                    name: "add_properties".to_string(),
                    param_type: "array".to_string(),
                    description: concat!(
                        "ADDS or updates detail and connection values on this thing.",
                        " Each item: {detail_iri, values: [...], value_type?: 'iri'|'literal',",
                        " datatype?: 'xsd:string'|...}.",
                        " For foundation:hasStatus, value is validated against the concept's",
                        " allowedStatus list.",
                        " Does NOT remove properties not listed — use remove_properties for that.",
                    ).to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "remove_properties".to_string(),
                    param_type: "array".to_string(),
                    description: concat!(
                        "List of detail IRIs whose values should be removed from this thing.",
                        " Each item is a string IRI (e.g. 'foundation:myDetail').",
                        " Removes ALL values of that property from this thing.",
                        " To remove a single value, use forget_things instead.",
                    ).to_string(),
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
            description: concat!(
                "Fetch or search concepts.",
                " With 'iri': returns full details of that concept (properties, connections,",
                " allowedStatuses, requiredFields, subclasses, etc.).",
                " Without 'iri': searches all concepts by keyword.",
                " IMPORTANT: Always use English for searches.",
            ).to_string(),
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
                    description: concat!(
                        "Search keywords (used when no iri). ALL words must match (AND search).",
                        " Use 1-3 important words in ENGLISH.",
                    ).to_string(),
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
            description: concat!(
                "Fetch or search things.",
                " With 'iri': returns full details of that thing (all properties, connections,",
                " backlinks, allowedStatuses, requiredFields).",
                " Without 'iri': searches things by keyword, optionally filtered by concept.",
                " Each search result includes 'matchedProperties' showing which fields matched.",
            ).to_string(),
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
                    description: concat!(
                        "Filter to things created at or after this Unix timestamp in ms.",
                        " Only applies when concept_iri is provided.",
                    ).to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "to_millis".to_string(),
                    param_type: "number".to_string(),
                    description: concat!(
                        "Filter to things created at or before this Unix timestamp in ms.",
                        " Only applies when concept_iri is provided.",
                    ).to_string(),
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
                    description: concat!(
                        "Filter by specific detail values. Requires concept_iri.",
                        " Each item: {detail: 'IRI', value: 'VALUE', operator?: '='|'>='|'<='|'>'|'<'}.",
                        " operator defaults to '='. For xsd:dateTime, operator applies to Unix ms timestamp.",
                    ).to_string(),
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
            description: concat!(
                "Forget a thing or remove a specific detail value from it.",
                " With only 'iri': forgets the thing completely.",
                " With 'iri' + 'detail_iri': removes all values of that detail from the thing.",
                " With 'iri' + 'detail_iri' + 'value': removes only that specific value.",
            ).to_string(),
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
                    description: "IRI of the detail to remove. If omitted, the entire thing is forgotten.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "value".to_string(),
                    param_type: "string".to_string(),
                    description: "Specific value to remove. If omitted and detail_iri is set, all values of that detail are removed.".to_string(),
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
            description: "List available widget types and their required params. Call this before using blackboard_update. Pass concept_iri to filter to widgets that can render a specific entity.".to_string(),
            parameters: vec![
                Parameter {
                    name: "concept_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "Optional. IRI of the entity concept (e.g. 'foundation:Task'). When provided, only entity-bound widgets are returned.".to_string(),
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
                " 'add': add a widget (requires widget_type and params with widget-specific data).",
                " 'remove': remove a widget (params.widget_id required).",
                " 'replace': remove all existing widgets;",
                " if widget_type and params are also provided, adds those widgets after clearing.",
                " IMPORTANT: Widgets are live — any update to the underlying entity's properties",
                " (e.g. diagramSource on a MermaidDiagram) is reflected on screen in real time.",
                " You do NOT need to call blackboard_update to refresh a widget after editing an entity;",
                " just update the entity with learn_things and the widget updates automatically.",
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
                    description: "Widget type for 'add'/'replace'. Call blackboard_widgets_list to see available types and required params.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "params".to_string(),
                    param_type: "object".to_string(),
                    description: "Widget-specific params for 'add'/'replace', or {\"widget_id\": \"...\"} for 'remove'. Call blackboard_widgets_list to see required params per widget type.".to_string(),
                    required: false,
                    schema: None,
                },
            ],
        },
    ]
}
