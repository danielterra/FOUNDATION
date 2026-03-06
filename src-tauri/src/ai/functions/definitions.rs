use super::{ToolTemplate, Parameter};

pub fn get_claude_tools() -> Vec<crate::ai::providers::ClaudeTool> {
    get_available_tools()
        .into_iter()
        .map(|f| f.to_claude_tool())
        .collect()
}

pub fn get_available_tools() -> Vec<ToolTemplate> {
    vec![
        ToolTemplate {
            name: "remember_concepts".to_string(),
            array_mode: true,
            description: concat!(
                "Search your memory for concepts you know about. Find what types of things",
                " you've learned. IMPORTANT: Always use English for concept searches and",
                " keep all concept labels in English.",
            ).to_string(),
            parameters: vec![
                Parameter {
                    name: "query".to_string(),
                    param_type: "string".to_string(),
                    description: concat!(
                        "Search keywords separated by spaces in ENGLISH. ALL words must match",
                        " (AND search). Use 1-3 most important words for best results.",
                        " Example: 'driver license' instead of",
                        " 'CNH habilitação carteira motorista'.",
                    ).to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "limit".to_string(),
                    param_type: "integer".to_string(),
                    description: "How many results to return (default: 100)".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "offset".to_string(),
                    param_type: "integer".to_string(),
                    description: "How many to skip for pagination (default: 0)".to_string(),
                    required: false,
                    schema: None,
                },
            ],
        },
        ToolTemplate {
            name: "remember_concept".to_string(),
            array_mode: true,
            description: concat!(
                "Remember everything you know about a specific concept.",
                " Returns properties (with source class), incomingProperties (object properties",
                " whose range is this concept), allowedStatuses (iri + label + icon + color),",
                " and requiredFields (properties with minCardinality >= 1).",
            ).to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "ID of the concept (e.g., 'foundation:Person')".to_string(),
                    required: true,
                    schema: None,
                },
            ],
        },
        ToolTemplate {
            name: "remember_things".to_string(),
            array_mode: true,
            description: concat!(
                "Search your memory for specific things in a concept",
                " (e.g., which people, places, or organizations you remember).",
                " Each result in the 'things' array includes a 'matchedProperties' field:",
                " an array of {detail_iri, value, datatype} objects representing only the",
                " properties that matched the query. When no query is provided,",
                " matchedProperties is an empty array.",
            ).to_string(),
            parameters: vec![
                Parameter {
                    name: "concept_iri".to_string(),
                    param_type: "string".to_string(),
                    description: concat!(
                        "Which concept to search in",
                        " (optional - if not provided, searches across all concepts)",
                    ).to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "query".to_string(),
                    param_type: "string".to_string(),
                    description: concat!(
                        "Search keywords separated by spaces. ALL words must match (AND search).",
                        " Use 1-3 most important words for best results.",
                    ).to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "limit".to_string(),
                    param_type: "integer".to_string(),
                    description: "How many results to return (default: 100)".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "offset".to_string(),
                    param_type: "integer".to_string(),
                    description: "How many to skip for pagination (default: 0)".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "from_millis".to_string(),
                    param_type: "number".to_string(),
                    description: concat!(
                        "Filter to entities created at or after this Unix timestamp in milliseconds (inclusive).",
                        " Timezone matters — always convert from local time.",
                        " Example: 2026-03-04T00:00:00-03:00 = 1772593200000.",
                        " Only applies when concept_iri is provided.",
                    ).to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "to_millis".to_string(),
                    param_type: "number".to_string(),
                    description: concat!(
                        "Filter to entities created at or before this Unix timestamp in milliseconds (inclusive).",
                        " Timezone matters — always convert from local time.",
                        " Example: 2026-03-04T23:59:59-03:00 = 1772679599000.",
                        " Only applies when concept_iri is provided.",
                    ).to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "include_retracted".to_string(),
                    param_type: "boolean".to_string(),
                    description: concat!(
                        "When true, include retracted (deleted) entities in results.",
                        " Default: false. Only applies when concept_iri is provided.",
                    ).to_string(),
                    required: false,
                    schema: None,
                },
            ],
        },
        ToolTemplate {
            name: "remember_thing".to_string(),
            array_mode: true,
            description: concat!(
                "Remember everything you know about a specific thing - all its details and",
                " connections. Returns backlinks grouped by concept as",
                " [{concept, conceptLabel, count}] sorted by count descending,",
                " so you can understand the shape of an entity's connections",
                " and decide whether to paginate through them.",
            ).to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "ID of the thing you want to remember".to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "include_retracted".to_string(),
                    param_type: "boolean".to_string(),
                    description: concat!(
                        "When true, include retracted (deleted) facts alongside active ones.",
                        " Each retracted fact will have retracted: true.",
                        " Default: false.",
                    ).to_string(),
                    required: false,
                    schema: None,
                },
            ],
        },
        ToolTemplate {
            name: "learn_concept".to_string(),
            array_mode: true,
            description: concat!(
                "Learn a new concept (e.g., when users mention a new type of thing you should",
                " remember). IMPORTANT: Always create concepts with English labels and",
                " descriptions, regardless of the user's language.",
            ).to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: concat!(
                        "Unique ID for this concept (e.g., 'foundation:Project')",
                    ).to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "label".to_string(),
                    param_type: "string".to_string(),
                    description: concat!(
                        "Name for this concept in ENGLISH (e.g., 'Driver License' not 'CNH')",
                    ).to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "icon".to_string(),
                    param_type: "string".to_string(),
                    description: concat!(
                        "Material icon name (e.g., 'category', 'label') OR image URL.",
                        " For attachment images, get foundation:filePath from the File entity",
                        " and use that URL (e.g., 'file:///path/to/image.jpg').",
                    ).to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "comment".to_string(),
                    param_type: "string".to_string(),
                    description: "Optional description".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "super_concept".to_string(),
                    param_type: "string".to_string(),
                    description: "Optional parent concept ID".to_string(),
                    required: false,
                    schema: None,
                },
            ],
        },
        ToolTemplate {
            name: "learn_thing".to_string(),
            array_mode: true,
            description: concat!(
                "Learn about a new specific thing (person, place, organization, etc.).",
                " You'll get back an ID to reference it later. IMPORTANT: Before creating a",
                " new thing, ALWAYS search first using remember_things (WITHOUT specifying",
                " concept_iri) to check if it already exists and avoid duplicates.",
            ).to_string(),
            parameters: vec![
                Parameter {
                    name: "concept_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "What concept this thing belongs to".to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "label".to_string(),
                    param_type: "string".to_string(),
                    description: "Name of this thing".to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "icon".to_string(),
                    param_type: "string".to_string(),
                    description: concat!(
                        "Material icon name (e.g., 'person', 'business') OR image URL.",
                        " For attachment images, get foundation:filePath from the File entity",
                        " and use that URL (e.g., 'file:///path/to/image.jpg').",
                        " Optional: if not provided, the concept's icon will be used.",
                    ).to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "comment".to_string(),
                    param_type: "string".to_string(),
                    description: "Optional description".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "properties".to_string(),
                    param_type: "array".to_string(),
                    description: concat!(
                        "Optional array of property values to set atomically on creation.",
                        " Each item: {detail_iri: 'foundation:hasStatus',",
                        " values: ['foundation:Active'], value_type?: 'iri'|'literal',",
                        " datatype?: 'xsd:string'}.",
                        " If any property fails, the entire operation is rolled back.",
                        " For foundation:hasStatus, the value is validated against the",
                        " concept's allowedStatus list.",
                    ).to_string(),
                    required: false,
                    schema: None,
                },
            ],
        },
        ToolTemplate {
            name: "learn_connection_type".to_string(),
            array_mode: true,
            description: concat!(
                "Learn a new type of connection or detail you can remember about things",
                " (e.g., 'worksAt', 'bornOn', 'hasSkill').",
                " If the IRI already exists, this performs an upsert — it updates the existing",
                " connection type with the provided values. You do NOT need to forget and recreate",
                " to update a connection type; simply call this with the same IRI.",
            ).to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: concat!(
                        "Unique ID for this connection type (e.g., 'foundation:hasAge')",
                    ).to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "label".to_string(),
                    param_type: "string".to_string(),
                    description: "Name for this connection type".to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "detail_type".to_string(),
                    param_type: "string".to_string(),
                    description: concat!(
                        "'object' for connections to other things,",
                        " 'datatype' for simple values (text, numbers, dates)",
                    ).to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "domain".to_string(),
                    param_type: "string".to_string(),
                    description: "Optional: which concept this applies to. Pass a single string or an array of strings to assign multiple domains.".to_string(),
                    required: false,
                    schema: Some(serde_json::json!({
                        "oneOf": [
                            {"type": "string"},
                            {"type": "array", "items": {"type": "string"}}
                        ]
                    })),
                },
                Parameter {
                    name: "range".to_string(),
                    param_type: "string".to_string(),
                    description: concat!(
                        "Optional: for 'object' properties, what concept it connects to;",
                        " for 'datatype', what type of value",
                    ).to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "comment".to_string(),
                    param_type: "string".to_string(),
                    description: "Optional description".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "unit".to_string(),
                    param_type: "string".to_string(),
                    description: concat!(
                        "QUDT unit IRI — REQUIRED when range is xsd:decimal, xsd:integer,",
                        " xsd:float, or xsd:double (e.g. 'unit:BRL', 'unit:M', 'unit:SEC').",
                        " MUST NOT be provided for non-numeric ranges; will error if supplied.",
                    ).to_string(),
                    required: false,
                    schema: None,
                },
            ],
        },
        ToolTemplate {
            name: "remember_connection_type".to_string(),
            array_mode: true,
            description: concat!(
                "Remember everything you know about a specific type of connection.",
            ).to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "ID of the connection type".to_string(),
                    required: true,
                    schema: None,
                },
            ],
        },
        ToolTemplate {
            name: "forget_concept".to_string(),
            array_mode: true,
            description: concat!(
                "Forget a concept (but not the specific things that belong to it).",
            ).to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "The IRI of the concept to forget".to_string(),
                    required: true,
                    schema: None,
                },
            ],
        },
        ToolTemplate {
            name: "forget_thing".to_string(),
            array_mode: true,
            description: concat!(
                "Forget a specific thing completely (all its details and connections).",
            ).to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "ID of the thing to forget".to_string(),
                    required: true,
                    schema: None,
                },
            ],
        },
        ToolTemplate {
            name: "forget_connection_type".to_string(),
            array_mode: true,
            description: "Forget a type of connection.".to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "ID of the connection type to forget".to_string(),
                    required: true,
                    schema: None,
                },
            ],
        },
        ToolTemplate {
            name: "update_concept".to_string(),
            array_mode: true,
            description: concat!(
                "Update details about a concept (name, icon, description, allowed statuses, required fields).",
            ).to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "ID of the concept".to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "label".to_string(),
                    param_type: "string".to_string(),
                    description: "New name".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "icon".to_string(),
                    param_type: "string".to_string(),
                    description: concat!(
                        "Material icon name OR image URL (e.g., 'file:///path/to/image.jpg')",
                    ).to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "comment".to_string(),
                    param_type: "string".to_string(),
                    description: "New description".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "super_concept".to_string(),
                    param_type: "string".to_string(),
                    description: concat!(
                        "New single parent concept ID.",
                        " Replaces all existing superclasses with this one.",
                    ).to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "super_concepts".to_string(),
                    param_type: "array".to_string(),
                    description: concat!(
                        "List of parent concept IRIs for multiple inheritance.",
                        " REPLACES all existing superclasses with this list.",
                        " Use this instead of super_concept when a concept needs multiple parents.",
                        " Example: ['foundation:ConcreteThing', 'foundation:AgentCapacity'].",
                        " Pass an empty array to remove all superclasses.",
                    ).to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "allowed_statuses".to_string(),
                    param_type: "array".to_string(),
                    description: concat!(
                        "Complete list of Status IRIs allowed for instances of this concept.",
                        " REPLACES any existing allowedStatus values.",
                        " Pass an empty array to remove all restrictions.",
                        " Example: ['foundation:Active', 'foundation:Archived'].",
                        " Once set, update_thing with foundation:hasStatus will reject",
                        " status values not in this list.",
                    ).to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "required_fields".to_string(),
                    param_type: "array".to_string(),
                    description: concat!(
                        "Complete list of property IRIs that are required (minCardinality >= 1) for instances of this concept.",
                        " REPLACES any existing OWL minCardinality restrictions.",
                        " Pass an empty array to remove all required field restrictions.",
                        " Example: ['foundation:name', 'foundation:hasStatus'].",
                        " Once set, instances cannot be created without providing these fields.",
                    ).to_string(),
                    required: false,
                    schema: None,
                },
            ],
        },
        ToolTemplate {
            name: "update_thing".to_string(),
            array_mode: true,
            description: "Update a thing's properties. Pass only the properties you want to change (partial update). Validates hasStatus values against the concept's allowedStatus list.".to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "ID of the thing".to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "label".to_string(),
                    param_type: "string".to_string(),
                    description: "New name".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "icon".to_string(),
                    param_type: "string".to_string(),
                    description: concat!(
                        "Material icon name OR image URL (e.g., 'file:///path/to/image.jpg')",
                    ).to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "comment".to_string(),
                    param_type: "string".to_string(),
                    description: "New description".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "properties".to_string(),
                    param_type: "array".to_string(),
                    description: concat!(
                        "Optional array of additional properties to update.",
                        " Each entry: {detail_iri, values: [...], value_type: 'literal'|'iri', datatype: 'xsd:string'|...}.",
                        " For foundation:hasStatus, the value is validated against the concept's allowedStatus list.",
                    ).to_string(),
                    required: false,
                    schema: None,
                },
            ],
        },
        ToolTemplate {
            name: "forget_thing_detail".to_string(),
            array_mode: true,
            description: concat!(
                "Forget a specific detail or connection about a thing",
                " (e.g., forget that someone works at X).",
            ).to_string(),
            parameters: vec![
                Parameter {
                    name: "thing_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "ID of the thing".to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "detail_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "What kind of detail to forget".to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "value".to_string(),
                    param_type: "string".to_string(),
                    description: "The specific value to forget".to_string(),
                    required: true,
                    schema: None,
                },
            ],
        },
        ToolTemplate {
            name: "remember_things_by_details".to_string(),
            array_mode: true,
            description: concat!(
                "Remember things that have specific details or connections",
                " (e.g., remember all people who work at X).",
            ).to_string(),
            parameters: vec![
                Parameter {
                    name: "concept_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "Which concept to search in".to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "properties".to_string(),
                    param_type: "array".to_string(),
                    description: concat!(
                        "Details to match: array of {detail: 'IRI', value: 'VALUE', operator: 'OP'}.",
                        " operator is optional and defaults to '='. Supported operators: '=', '>=', '<=', '>', '<'.",
                        " For xsd:dateTime values, the operator is applied to the Unix millisecond timestamp.",
                    ).to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "include_retracted".to_string(),
                    param_type: "boolean".to_string(),
                    description: concat!(
                        "When true, include retracted (deleted) things in results. Default: false.",
                    ).to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "limit".to_string(),
                    param_type: "integer".to_string(),
                    description: "Maximum number of results to return. Default: 100.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "offset".to_string(),
                    param_type: "integer".to_string(),
                    description: "Number of results to skip for pagination. Default: 0.".to_string(),
                    required: false,
                    schema: None,
                },
            ],
        },
        ToolTemplate {
            name: "blackboard_show".to_string(),
            array_mode: false,
            description: concat!(
                "See what is currently being displayed on the blackboard/canvas.",
                " Returns a list of active widgets.",
            ).to_string(),
            parameters: vec![],
        },
        ToolTemplate {
            name: "blackboard_add_widget".to_string(),
            array_mode: true,
            description: concat!(
                "Add a widget to the blackboard to display information visually.",
                " The system will automatically position the widget.",
                " Use remember_concepts to search for available widget types",
                " (foundation:Widget subclasses).",
            ).to_string(),
            parameters: vec![
                Parameter {
                    name: "widget_type".to_string(),
                    param_type: "string".to_string(),
                    description: concat!(
                        "The type of widget to add (e.g., 'Inspector').",
                        " Search for foundation:Widget subclasses to find available types.",
                    ).to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "params".to_string(),
                    param_type: "object".to_string(),
                    description: concat!(
                        "Widget-specific parameters as a JSON object.",
                        " For Inspector: {\"entity_id\": \"foundation:SomeEntity\"}",
                    ).to_string(),
                    required: true,
                    schema: None,
                },
            ],
        },
        ToolTemplate {
            name: "blackboard_remove".to_string(),
            array_mode: true,
            description: "Remove a specific widget from the blackboard by its ID.".to_string(),
            parameters: vec![
                Parameter {
                    name: "widget_id".to_string(),
                    param_type: "string".to_string(),
                    description: "The ID of the widget to remove".to_string(),
                    required: true,
                    schema: None,
                },
            ],
        },
    ]
}
