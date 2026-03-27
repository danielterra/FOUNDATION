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
        // WRITE TOOLS (create / modify)
        // ----------------------------------------------------------------
        ToolTemplate {
            name: "define_class".to_string(),
            array_mode: true,
            description: "Define or update an OWL class. Creates if new, updates if IRI exists.".to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "Class IRI (e.g. 'foundation:Project')".to_string(),
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
                    description: "Icon for this entity. Accepts: (1) Material Symbols name (e.g. 'person', 'home', 'star'); (2) local file URL (e.g. 'file:///Users/alice/Pictures/logo.png'); (3) web URL ('https://example.com/icon.svg'); (4) data URI ('data:image/png;base64,...'). Required when creating.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "new_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "Rename this class to a new IRI. Migrates all references.".to_string(),
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
                    name: "super_classes".to_string(),
                    param_type: "array".to_string(),
                    description: "Parent class IRIs. REPLACES all existing. Required when creating.".to_string(),
                    required: false,
                    schema: Some(serde_json::json!({ "type": "array", "items": { "type": "string" } })),
                },
                Parameter {
                    name: "allowed_statuses".to_string(),
                    param_type: "array".to_string(),
                    description: "Status IRIs allowed for this class's individuals. REPLACES existing.".to_string(),
                    required: false,
                    schema: Some(serde_json::json!({ "type": "array", "items": { "type": "string" } })),
                },
                Parameter {
                    name: "property_restrictions".to_string(),
                    param_type: "array".to_string(),
                    description: "Cardinality constraints. Each: {property_iri, cardinality_min?, cardinality_max?}. REPLACES existing restrictions.".to_string(),
                    required: false,
                    schema: Some(serde_json::json!({
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "property_iri": { "type": "string" },
                                "cardinality_min": { "type": "integer" },
                                "cardinality_max": { "type": "integer" }
                            },
                            "required": ["property_iri"]
                        }
                    })),
                },
                Parameter {
                    name: "add_properties".to_string(),
                    param_type: "array".to_string(),
                    description: "Property IRIs to associate with this class (adds class as domain). Property must already exist via define_property. Does NOT remove unlisted.".to_string(),
                    required: false,
                    schema: Some(serde_json::json!({ "type": "array", "items": { "type": "string" } })),
                },
                Parameter {
                    name: "remove_properties".to_string(),
                    param_type: "array".to_string(),
                    description: "Property IRIs to permanently dissociate from this class.".to_string(),
                    required: false,
                    schema: Some(serde_json::json!({ "type": "array", "items": { "type": "string" } })),
                },
            ],
        },

        ToolTemplate {
            name: "define_property".to_string(),
            array_mode: true,
            description: "Define or update an OWL property (ObjectProperty or DatatypeProperty).".to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "Property IRI (e.g. 'foundation:hasStatus')".to_string(),
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
                    name: "property_type".to_string(),
                    param_type: "string".to_string(),
                    description: "'object' (links to another individual) or 'datatype' (literal value). Required when creating.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "range".to_string(),
                    param_type: "string".to_string(),
                    description: "For 'object': target class IRI. For 'datatype': xsd type. Omit to keep existing.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "domains".to_string(),
                    param_type: "array".to_string(),
                    description: "Class IRIs where this property appears. REPLACES existing domains if provided.".to_string(),
                    required: false,
                    schema: Some(serde_json::json!({ "type": "array", "items": { "type": "string" } })),
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
                    description: "QUDT unit IRI for numeric properties (e.g. 'unit:Second').".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "domain_labels".to_string(),
                    param_type: "array".to_string(),
                    description: "Domain-specific labels for this property. Each entry: {domain (class IRI), forward_label (label when traversing forward from that domain), inverse_label (optional, label for backlinks into that domain)}.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "formula".to_string(),
                    param_type: "string".to_string(),
                    description: "Arithmetic formula using {{property_iri}} references. Operators: +, -, *, /, ** (power), % (modulo). DatatypeProperty only. References must be numeric properties (xsd:integer, xsd:decimal, xsd:float, xsd:double) of the same class. Circular dependencies are rejected.".to_string(),
                    required: false,
                    schema: None,
                },
            ],
        },

        ToolTemplate {
            name: "assert_individual".to_string(),
            array_mode: true,
            description: "Create a new individual (instance) of a class.".to_string(),
            parameters: vec![
                Parameter {
                    name: "class_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "Class this individual belongs to (e.g. 'foundation:Task')".to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "label".to_string(),
                    param_type: "string".to_string(),
                    description: "Name of this individual.".to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "icon".to_string(),
                    param_type: "string".to_string(),
                    description: "Icon for this entity. Accepts: (1) Material Symbols name (e.g. 'person', 'home', 'star'); (2) local file URL (e.g. 'file:///Users/alice/Pictures/logo.png'); (3) web URL ('https://example.com/icon.svg'); (4) data URI ('data:image/png;base64,...'). Inherits from class if omitted.".to_string(),
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
                    name: "properties".to_string(),
                    param_type: "array".to_string(),
                    description: "Initial property values. Each: {property_iri, values[]}. foundation:hasStatus is validated.".to_string(),
                    required: false,
                    schema: Some(serde_json::json!({
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "property_iri": { "type": "string" },
                                "values": { "type": "array", "items": { "type": "string" } }
                            },
                            "required": ["property_iri", "values"]
                        }
                    })),
                },
            ],
        },

        ToolTemplate {
            name: "add_property_values".to_string(),
            array_mode: true,
            description: "Append values to an individual's property (does NOT replace existing values).".to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "Individual IRI".to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "property_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "Property to add values to".to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "values".to_string(),
                    param_type: "array".to_string(),
                    description: "Values to append".to_string(),
                    required: true,
                    schema: Some(serde_json::json!({ "type": "array", "items": { "type": "string" } })),
                },
            ],
        },

        ToolTemplate {
            name: "replace_property_values".to_string(),
            array_mode: true,
            description: "Replace ALL values of an individual's property with the given values.".to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "Individual IRI".to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "property_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "Property to replace values of".to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "values".to_string(),
                    param_type: "array".to_string(),
                    description: "New values (replaces all existing)".to_string(),
                    required: true,
                    schema: Some(serde_json::json!({ "type": "array", "items": { "type": "string" } })),
                },
            ],
        },

        ToolTemplate {
            name: "remove_property_values".to_string(),
            array_mode: true,
            description: "Remove specific values from an individual's property.".to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "Individual IRI".to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "property_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "Property to remove values from".to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "values".to_string(),
                    param_type: "array".to_string(),
                    description: "Specific values to remove".to_string(),
                    required: true,
                    schema: Some(serde_json::json!({ "type": "array", "items": { "type": "string" } })),
                },
            ],
        },

        ToolTemplate {
            name: "clear_property".to_string(),
            array_mode: true,
            description: "Remove ALL values of a property from an individual.".to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "Individual IRI".to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "property_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "Property to clear all values of".to_string(),
                    required: true,
                    schema: None,
                },
            ],
        },

        ToolTemplate {
            name: "retract_individual".to_string(),
            array_mode: true,
            description: "Permanently delete an individual and all its property values.".to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "IRI of the individual to delete".to_string(),
                    required: true,
                    schema: None,
                },
            ],
        },

        ToolTemplate {
            name: "retract_class".to_string(),
            array_mode: true,
            description: "Permanently remove a class definition and all its instances.".to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "IRI of the class to retract".to_string(),
                    required: true,
                    schema: None,
                },
            ],
        },

        ToolTemplate {
            name: "retract_property".to_string(),
            array_mode: true,
            description: "Permanently remove a property definition and all its asserted values.".to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "IRI of the property to retract".to_string(),
                    required: true,
                    schema: None,
                },
            ],
        },

        // ----------------------------------------------------------------
        // READ TOOLS
        // ----------------------------------------------------------------
        ToolTemplate {
            name: "search".to_string(),
            array_mode: false,
            description: "Search for classes or individuals by label, type, or property value. ALL query tokens must match (AND). Use space-separated words — CamelCase is auto-expanded (e.g. 'ErrorHandler' → 'Error Handler'). To look up a known class by IRI use describe_class instead.".to_string(),
            parameters: vec![
                Parameter {
                    name: "query".to_string(),
                    param_type: "string".to_string(),
                    description: "Search keywords. ALL tokens must match. Use 1-3 important English words. CamelCase is auto-expanded, e.g. 'ErrorHandler' → 'Error Handler'. Prefer space-separated words when possible.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "type".to_string(),
                    param_type: "string".to_string(),
                    description: "Filter results to 'class' (owl:Class) or 'individual'. Omit to return both. Note: to fetch a known class by IRI, use describe_class — search is for discovery when the IRI is unknown.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "class_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "Filter to individuals of this class (e.g. 'foundation:Task'). Returns all instances when query is empty.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "filters".to_string(),
                    param_type: "array".to_string(),
                    description: "Property filters. Each: {detail, value, operator?}. operator: '='|'>='|'<='|'>'|'<' (default '='). ISO date for xsd:date, RFC3339 for xsd:dateTime.".to_string(),
                    required: false,
                    schema: Some(serde_json::json!({
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "detail": { "type": "string" },
                                "value": { "type": "string" },
                                "operator": { "type": "string" }
                            },
                            "required": ["detail", "value"]
                        }
                    })),
                },
                Parameter {
                    name: "include_retracted".to_string(),
                    param_type: "boolean".to_string(),
                    description: "Include deleted entities. Default: false.".to_string(),
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
                    description: "Skip N results (default: 0).".to_string(),
                    required: false,
                    schema: None,
                },
            ],
        },

        ToolTemplate {
            name: "describe_class".to_string(),
            array_mode: false,
            description: "Fetch full schema for one or more classes: properties, subclasses, restrictions, allowed statuses.".to_string(),
            parameters: vec![
                Parameter {
                    name: "iris".to_string(),
                    param_type: "array".to_string(),
                    description: "Class IRIs to fetch (e.g. ['foundation:Task', 'foundation:Project'])".to_string(),
                    required: true,
                    schema: Some(serde_json::json!({ "type": "array", "items": { "type": "string" } })),
                },
            ],
        },

        ToolTemplate {
            name: "describe_individual".to_string(),
            array_mode: false,
            description: "Fetch full details for one or more individuals: all property values, backlinks, and allowed statuses.".to_string(),
            parameters: vec![
                Parameter {
                    name: "iris".to_string(),
                    param_type: "array".to_string(),
                    description: "Individual IRIs to fetch".to_string(),
                    required: true,
                    schema: Some(serde_json::json!({ "type": "array", "items": { "type": "string" } })),
                },
            ],
        },

        ToolTemplate {
            name: "describe_property".to_string(),
            array_mode: true,
            description: "Fetch full details for a property by IRI, or search by keywords.".to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "Property IRI. Omit to search instead.".to_string(),
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
                    description: "Max search results (default: 50).".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "offset".to_string(),
                    param_type: "integer".to_string(),
                    description: "Skip N results (default: 0).".to_string(),
                    required: false,
                    schema: None,
                },
            ],
        },

        // ----------------------------------------------------------------
        // GRAPH / PROCESS TOOLS
        // ----------------------------------------------------------------
        ToolTemplate {
            name: "class_graph".to_string(),
            array_mode: false,
            description: "Explore how a class connects to others via object properties. Prefer over describe_class when you need the relationship graph.".to_string(),
            parameters: vec![
                Parameter {
                    name: "class_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "IRI of the root OWL class (e.g. 'foundation:Task')".to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "max_depth".to_string(),
                    param_type: "integer".to_string(),
                    description: "Max traversal depth. Default: 2, max: 5.".to_string(),
                    required: false,
                    schema: None,
                },
            ],
        },

        ToolTemplate {
            name: "get_automation".to_string(),
            array_mode: false,
            description: "Return the full structure of an automation: all flow nodes (with type, label, assigned agent/role/user, tools, input/output concepts) and all sequence flows (with condition expressions). Use this to understand or reason about an automation before running or modifying it.".to_string(),
            parameters: vec![
                Parameter {
                    name: "automation_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "IRI of the Automation to inspect".to_string(),
                    required: true,
                    schema: None,
                },
            ],
        },

        ToolTemplate {
            name: "run_automation".to_string(),
            array_mode: false,
            description: "Start an automation (BPMN process) asynchronously. Returns immediately; execution happens in the background.".to_string(),
            parameters: vec![
                Parameter {
                    name: "automation_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "IRI of the Automation to execute".to_string(),
                    required: true,
                    schema: None,
                },
            ],
        },

        // ----------------------------------------------------------------
        // BLACKBOARD
        // ----------------------------------------------------------------
        ToolTemplate {
            name: "blackboard_update".to_string(),
            array_mode: true,
            description: concat!(
                "Use to add, remove, or replace widgets on the user's blackboard. Current blackboard state is provided automatically in the conversation context.",
                " Widgets are live — no need to refresh after assert_individual changes.",
                " Widget type IDs: 'inspector' (any entity), 'meta_process' (MetaProcess), 'mermaid' (MermaidDiagram), 'process_status' (MetaProcess), 'connector_credential' (ExternalServiceConnector), 'connector_manager' (ExternalServiceConnector).",
            ).to_string(),
            parameters: vec![
                Parameter {
                    name: "operation".to_string(),
                    param_type: "string".to_string(),
                    description: "'add', 'remove', or 'replace'".to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "widget_type".to_string(),
                    param_type: "string".to_string(),
                    description: "Widget type ID".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "params".to_string(),
                    param_type: "object".to_string(),
                    description: "Widget params".to_string(),
                    required: false,
                    schema: None,
                },
            ],
        },
    ]
}
