use super::{ToolTemplate, Parameter};

pub fn get_tool_definitions() -> Vec<crate::ai::providers::ToolDefinition> {
    get_available_tools()
        .into_iter()
        .map(|f| f.to_tool_definition())
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
                    name: "disjoint_with".to_string(),
                    param_type: "array".to_string(),
                    description: "Class IRIs declared pairwise disjoint with this class via owl:disjointWith. REPLACES all pairwise disjointness for this class. The reverse triple is asserted automatically. Provide [] to clear. Used to forbid a class from being combined with another in the same individual or super-class chain.".to_string(),
                    required: false,
                    schema: Some(serde_json::json!({ "type": "array", "items": { "type": "string" } })),
                },
                Parameter {
                    name: "all_disjoint_classes".to_string(),
                    param_type: "array".to_string(),
                    description: "Class IRIs (3+) declared mutually disjoint as a single owl:AllDisjointClasses set. Must include the class being defined. APPENDS a new set on every call (idempotent for the same exact members). For pairs, prefer disjoint_with. To remove an AllDisjointClasses set, retract its blank node directly.".to_string(),
                    required: false,
                    schema: Some(serde_json::json!({ "type": "array", "items": { "type": "string" }, "minItems": 3 })),
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
                Parameter {
                    name: "cascade_rules".to_string(),
                    param_type: "array".to_string(),
                    description: "Configure cascade-delete rules for this class. When an individual of this class is retracted, all related entities matching these rules are also retracted atomically — including their own cascade rules (recursive). Use this for any owned relationship where children should not outlive their parent.\n\nEach entry: { property_iri (string, required), direction ('range' | 'domain', required) }\n\n- direction 'range': retract all objects this individual references via property_iri. Use when the parent owns the children: (parent, property_iri, child) → child deleted. Example: Automation with hasSequenceFlow → retracting an Automation also retracts all its FlowNodes.\n- direction 'domain': retract all subjects that reference this individual via property_iri. Use when children point back to the parent: (child, property_iri, parent) → child deleted. Example: FlowNode with partOfAutomation → retracting an Automation also retracts all FlowNodes that reference it.\n\nProviding cascade_rules REPLACES all existing rules on the class. Omit to preserve existing rules. Call describe_class to inspect currently configured rules before modifying.".to_string(),
                    required: false,
                    schema: Some(serde_json::json!({
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "property_iri": { "type": "string" },
                                "direction": { "type": "string", "enum": ["range", "domain"] }
                            },
                            "required": ["property_iri", "direction"]
                        }
                    })),
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
                    description: "For 'object': target class IRI. For 'datatype': xsd type (e.g. 'xsd:string', 'xsd:integer', 'xsd:decimal', 'xsd:date', 'xsd:dateTime', 'xsd:boolean', 'xsd:anyURI') or custom type 'foundation:rrule' (iCalendar recurrence rule — renders inline recurrence editor in the Inspector). Omit to keep existing.".to_string(),
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
                    description: "Arithmetic formula using {{property_iri}} references. Operators: +, -, *, /, ** (power), % (modulo). DatatypeProperty only. References must be numeric properties (xsd:integer, xsd:decimal, xsd:float, xsd:double) of the same class. Circular dependencies are rejected. Cannot be used together with 'aggregation'.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "aggregation".to_string(),
                    param_type: "string".to_string(),
                    description: "Aggregation over related individuals. DatatypeProperty only. Cannot be used together with 'formula'. Syntax: FUNC({{source_prop_iri}}.sub_prop_iri) or CONTAR({{source_prop_iri}}). CRITICAL: ALL property IRIs MUST include the full namespace prefix (e.g. 'foundation:hasTasks', NEVER just 'hasTasks'). source_prop_iri is an ObjectProperty that links to related instances. sub_prop_iri is a numeric DatatypeProperty on those instances. Functions: SOMA/SUM (sum), MÉDIA/AVG (average), MÍNIMO/MIN (minimum), MÁXIMO/MAX (maximum) — all require sub_prop. CONTAR/COUNT (count related instances) — no sub_prop. Examples: 'SOMA({{foundation:hasTasks}}.foundation:estimatedHours)', 'CONTAR({{foundation:hasMembers}})'.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "query_config".to_string(),
                    param_type: "string".to_string(),
                    description: r#"JSON string defining a dynamic query property. ObjectProperty only. Cannot be combined with formula or aggregation. The resolved set is stored eagerly and kept up-to-date in real time by a background worker. Values are read-only and cannot be set directly.

Format: {"targetClass":"<class_iri>","filters":[...]}

Each filter: {"propertyIri":"<prop_iri>","operator":"<op>","value":"<literal>"}
Operators: eq, neq, gt, lt, gte, lte, between, exists, not_exists
- eq/neq/gt/lt/gte/lte: compare property value to "value" (literal or {{self.prop_iri}} to read from the owner instance)
- between: requires "valueFrom" and "valueTo" instead of "value"
- exists / not_exists: no "value" needed — checks presence/absence of the property

Self-reference: use "{{self.foundation:myProp}}" in "value"/"valueFrom"/"valueTo" to resolve the owner's property at query time. If the owner lacks that property, the query returns an empty set.

Example — find all active Tasks whose due date is before this Project's deadline:
{"targetClass":"foundation:Task","filters":[{"propertyIri":"foundation:hasStatus","operator":"eq","value":"foundation:StatusActive"},{"propertyIri":"foundation:dueDate","operator":"lt","value":"{{self.foundation:deadline}}"}]}"#.to_string(),
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

        ToolTemplate {
            name: "restore_individual".to_string(),
            array_mode: true,
            description: "Restore a retracted individual by re-asserting its triples as new rows. \
                Only restores triples from the most recent retraction event. Superseded values \
                from prior updates are not restored. Use search(include_retracted: true) to \
                discover restorable IRIs.".to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "IRI of the individual to restore".to_string(),
                    required: true,
                    schema: None,
                },
            ],
        },

        ToolTemplate {
            name: "restore_class".to_string(),
            array_mode: true,
            description: "Restore a retracted class and all instances that were cascade-deleted \
                with it. Instances retracted independently before the class retraction are NOT \
                restored. Use search(include_retracted: true) to discover restorable IRIs.".to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "IRI of the class to restore".to_string(),
                    required: true,
                    schema: None,
                },
            ],
        },

        ToolTemplate {
            name: "restore_property".to_string(),
            array_mode: true,
            description: "Restore a retracted property definition and all facts retracted in \
                the same operation. Facts superseded by earlier updates are NOT restored. \
                Use search(include_retracted: true) to discover restorable IRIs.".to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "IRI of the property to restore".to_string(),
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
                    description: "Property filters. Each: {detail, value, operator?}. operator: '='|'!='|'>='|'<='|'>'|'<'|'exists'|'not_exists' (default '='). 'exists'/'not_exists' ignore the value field. ISO date for xsd:date, RFC3339 for xsd:dateTime.".to_string(),
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
            description: "Fetch full schema for one or more classes: properties, subclasses, restrictions, allowed statuses. Each entry in the `properties` array includes a `propertyType` field: `value` (plain literal), `calculation` (formula/aggregation), `reference` (ObjectProperty pointing to an individual), or `query` (ObjectProperty resolved via a stored query).".to_string(),
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
            description: "Fetch full details for one or more individuals: all property values, backlinks, and allowed statuses. Each entry in the `properties` array includes a `propertyType` field: `value` (plain literal), `calculation` (formula/aggregation), `reference` (ObjectProperty or built-in IRI link), or `query` (ObjectProperty resolved via a stored query).".to_string(),
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

        ToolTemplate {
            name: "read_binary_file".to_string(),
            array_mode: false,
            description: "Read a binary file stored in FOUNDATION and return its bytes as base64 with the detected MIME type. Supports PDF (application/pdf), JPEG, PNG, WebP, GIF — detected from magic bytes. Unknown formats return application/octet-stream. The result can be passed directly to Claude as a document or image content block.".to_string(),
            parameters: vec![
                Parameter {
                    name: "file_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "IRI of the foundation:File individual (e.g. 'foundation:File_1234567890')".to_string(),
                    required: true,
                    schema: None,
                },
            ],
        },

        ToolTemplate {
            name: "read_pdf_page".to_string(),
            array_mode: false,
            description: "Read a single page of a PDF foundation:File and persist it as a 1-page PDF in the OS temp dir. Returns page_path (filesystem path), page, total_pages, and media_type=application/pdf — NOT inline base64. The chat layer loads the file lazily and presents Claude a native document content block. Prefer this over read_binary_file for PDFs to avoid loading the whole document at once.".to_string(),
            parameters: vec![
                Parameter {
                    name: "file_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "IRI of the foundation:File individual containing a PDF (e.g. 'foundation:File_1234567890')".to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "page".to_string(),
                    param_type: "integer".to_string(),
                    description: "Page number to extract (1-based). Must be between 1 and total_pages.".to_string(),
                    required: true,
                    schema: None,
                },
            ],
        },

        ToolTemplate {
            name: "head_text_file".to_string(),
            array_mode: false,
            description: "Return the first N lines of a text-based foundation:File individual. Also returns the total line count so the agent knows the file's full size. Ideal for inspecting CSV headers and structure before choosing a processing strategy.".to_string(),
            parameters: vec![
                Parameter {
                    name: "file_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "IRI of the foundation:File individual (e.g. 'foundation:File_1234567890')".to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "n".to_string(),
                    param_type: "integer".to_string(),
                    description: "Number of lines to return. Default: 10.".to_string(),
                    required: false,
                    schema: None,
                },
            ],
        },

        ToolTemplate {
            name: "read_text_lines".to_string(),
            array_mode: false,
            description: "Read a specific line range from a text-based foundation:File individual. Returns lines with 1-based numbers, total line count, and character metadata. Lines exceeding the character window are truncated — use start_char/end_char to iterate over long lines.".to_string(),
            parameters: vec![
                Parameter {
                    name: "file_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "IRI of the foundation:File individual (e.g. 'foundation:File_1234567890')".to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "start_line".to_string(),
                    param_type: "integer".to_string(),
                    description: "First line to return (1-based, inclusive).".to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "end_line".to_string(),
                    param_type: "integer".to_string(),
                    description: "Last line to return (1-based, inclusive). Must be >= start_line.".to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "start_char".to_string(),
                    param_type: "integer".to_string(),
                    description: "First character position within each line (1-based). Default: 1.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "end_char".to_string(),
                    param_type: "integer".to_string(),
                    description: "Last character position within each line (1-based, inclusive). Default: start_char + 4095.".to_string(),
                    required: false,
                    schema: None,
                },
            ],
        },

        ToolTemplate {
            name: "read_property_page".to_string(),
            array_mode: false,
            description: "Read a paginated slice of a large literal property value from an individual. Returns the requested page of text, total character count, total pages, and whether more pages follow. Use this after describe_individual shows a truncated value (marked with '…[truncated: N chars total]') to read the full content in 2 000-character chunks.".to_string(),
            parameters: vec![
                Parameter {
                    name: "individual_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "IRI of the individual whose property you want to read (e.g. 'foundation:UserStory_123').".to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "property_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "IRI of the literal property to paginate (e.g. 'foundation:implementationPlan').".to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "page".to_string(),
                    param_type: "integer".to_string(),
                    description: "1-based page number to retrieve. Defaults to 1.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "page_size".to_string(),
                    param_type: "integer".to_string(),
                    description: "Characters per page. Defaults to 2000.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "value_index".to_string(),
                    param_type: "integer".to_string(),
                    description: "0-based index into multi-valued properties. Defaults to 0.".to_string(),
                    required: false,
                    schema: None,
                },
            ],
        },

        ToolTemplate {
            name: "attach_file_to_individual".to_string(),
            array_mode: false,
            description: format!(
                "Attach a local file to an existing individual as a foundation:File and link it via a property (default foundation:hasFile). PREFERRED WORKFLOW: drop the file at the absolute path '{inbox}' first (this inbox directory is created on startup; expose it via the Filesystem MCP server or any other tool that can write there), then call this tool with file_path set to '{inbox}/<filename>'. The tool copies the bytes into '{attachments}/' and only deletes the source after the file entity is fully asserted and linked, so a mid-flight failure leaves the original intact. ALTERNATIVE: file_data_base64 + file_name — bytes are decoded server-side, useful when neither side can share a filesystem (slower because of base64 inflation and JSON-RPC overhead). RESULT: a foundation:File is asserted with fileName/filePath/fileSize/fileHash/hasFileType/uploadDate and the triple {{target_iri, link_property, file_iri}} is appended. LINK PROPERTY: defaults to foundation:hasFile — the ontology validates that target_iri's class accepts the property; pick a different link_property if it doesn't (e.g. foundation:hasAttachment, foundation:attachedFile, foundation:hasDocument, foundation:attachments).",
                inbox = crate::paths::inbox_dir().display(),
                attachments = crate::paths::attachments_dir().display(),
            ),
            parameters: vec![
                Parameter {
                    name: "target_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "IRI of the existing individual that should receive the file (e.g. 'foundation:FinancialObligation_123').".to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "file_path".to_string(),
                    param_type: "string".to_string(),
                    description: format!(
                        "Absolute path to the file on the MCP server's filesystem. PREFERRED: place the file at '{inbox}/<filename>' first; the tool copies it into attachments/ and deletes the source after the link succeeds.",
                        inbox = crate::paths::inbox_dir().display(),
                    ),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "file_data_base64".to_string(),
                    param_type: "string".to_string(),
                    description: "Base64-encoded file bytes. Slower than file_path (base64 inflation + JSON-RPC overhead) — only use when neither side can share a filesystem. No size cap, but prefer file_path via the inbox for any non-trivial file.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "file_name".to_string(),
                    param_type: "string".to_string(),
                    description: "File name including extension (e.g. 'darf.pdf'). Required with file_data_base64; optional with file_path (defaults to the basename).".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "link_property".to_string(),
                    param_type: "string".to_string(),
                    description: "Object property used to link target_iri → file_iri. Default: 'foundation:hasFile'.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "mime_type".to_string(),
                    param_type: "string".to_string(),
                    description: "Override MIME type (e.g. 'application/pdf'). Default: inferred from file_name extension.".to_string(),
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
                Parameter {
                    name: "input_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "IRI of the input individual required by the automation's start event. Omit only if the automation has no inputClass.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "dry_run".to_string(),
                    param_type: "boolean".to_string(),
                    description: "If true, executes the automation in read-only mode: all reads and searches run normally, but write operations (assert_individual, replace_property_values, etc.) are intercepted and return mock success responses without touching the database. Use this to test scripts safely before committing data.".to_string(),
                    required: false,
                    schema: None,
                },
            ],
        },

        // ----------------------------------------------------------------
        // BLACKBOARD TOOLS
        // ----------------------------------------------------------------
        ToolTemplate {
            name: "list_blackboard_widgets".to_string(),
            array_mode: false,
            description: "List widgets currently open on a blackboard. Returns widget_iri, widget_type, and entity_iri for each open widget. Returns empty list if the blackboard is empty.".to_string(),
            parameters: vec![
                Parameter {
                    name: "blackboard_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "IRI of the Blackboard to inspect. Optional — if omitted, the blackboard of the active conversation is used (or the default blackboard when no conversation is active).".to_string(),
                    required: false,
                    schema: None,
                },
            ],
        },

        ToolTemplate {
            name: "add_widget_to_blackboard".to_string(),
            array_mode: false,
            description: "Open a widget on a blackboard to display an entity. If a widget for the same entity already exists on this blackboard, returns the existing widget instead of creating a duplicate. widget_type defaults to 'inspector' (universal). Use list_blackboard_widgets first to avoid duplicates.".to_string(),
            parameters: vec![
                Parameter {
                    name: "entity_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "IRI of the entity to display (e.g. 'foundation:Task_123')".to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "widget_type".to_string(),
                    param_type: "string".to_string(),
                    description: "Widget type ID (e.g. 'inspector', 'mermaid', 'graph'). Omit to use the class default or fall back to 'inspector'.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "blackboard_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "IRI of the Blackboard to add the widget to. Optional — if omitted, the blackboard of the active conversation is used (or the default blackboard when no conversation is active).".to_string(),
                    required: false,
                    schema: None,
                },
            ],
        },

        ToolTemplate {
            name: "remove_widget_from_blackboard".to_string(),
            array_mode: false,
            description: "Remove a widget from its blackboard. Only closes the widget — does not modify or delete the entity it displays. Returns an error if the widget IRI does not exist or is already closed.".to_string(),
            parameters: vec![
                Parameter {
                    name: "widget_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "IRI of the Widget to remove (e.g. 'foundation:Widget_inspector_Task_123'). Use list_blackboard_widgets to find widget IRIs.".to_string(),
                    required: true,
                    schema: None,
                },
            ],
        },

        // ----------------------------------------------------------------
        // DATA SYNC TOOLS
        // ----------------------------------------------------------------

        ToolTemplate {
            name: "datasync_create_source".to_string(),
            array_mode: false,
            description: "Create a new DataSource that pulls data from an external API via HTTP. Validates credentials before activating. Returns the DataSource IRI and its initial status (Active or Error).".to_string(),
            parameters: vec![
                Parameter {
                    name: "connector_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "IRI of the ExternalServiceConnector (must have baseUrl and hasCredential configured).".to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "sync_namespace".to_string(),
                    param_type: "string".to_string(),
                    description: "Namespace prefix for deterministic IRIs of synced entities (e.g. 'github', 'jira'). Must be a valid IRI prefix.".to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "sync_schedule".to_string(),
                    param_type: "string".to_string(),
                    description: "Cron expression for the extract schedule (e.g. '0 */6 * * *' for every 6 hours).".to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "label".to_string(),
                    param_type: "string".to_string(),
                    description: "Human-readable label for this DataSource (e.g. 'GitHub Issues — acme/repo').".to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "target_endpoint".to_string(),
                    param_type: "string".to_string(),
                    description: "URL path appended to the connector baseUrl for the extract request (e.g. '/repos/owner/repo/issues'). Can be a full URL.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "item_path".to_string(),
                    param_type: "string".to_string(),
                    description: "JSON pointer selecting the items array in the response (e.g. '/items'). Leave empty if the root is the array.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "transform_script".to_string(),
                    param_type: "string".to_string(),
                    description: "Optional Rhai script for the transform automation's CodeTask. When provided, a transform Automation (StartEvent → CodeTask → EndEvent) is created and linked via foundation:transformAutomation on the DataSource. The script runs inside the Rhai engine with the mcp(), parse_json(), to_json(), log() and other helpers available.".to_string(),
                    required: false,
                    schema: None,
                },
            ],
        },

        ToolTemplate {
            name: "datasync_upsert_item".to_string(),
            array_mode: false,
            description: "Upsert a domain entity from a RawDataRecord. Without target_iri, builds a deterministic IRI (namespace:ClassName_externalId). With target_iri, adopts an existing individual in-place preserving its relationships. Also calls upsert_sync_record and marks the raw as Transformed. All writes go through DbExecutor::write so the notify path fires normally.".to_string(),
            parameters: vec![
                Parameter {
                    name: "data_source_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "IRI of the DataSource this record belongs to.".to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "raw_record_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "IRI of the RawDataRecord being transformed.".to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "class_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "OWL class IRI of the entity to create/update (e.g. 'foundation:AIModel').".to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "external_id".to_string(),
                    param_type: "string".to_string(),
                    description: "Stable external identifier (e.g. model slug 'openai/gpt-4o'). Used to compute the deterministic IRI and the SyncRecord key.".to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "properties".to_string(),
                    param_type: "object".to_string(),
                    description: "Key-value map of property IRIs → values to set on the entity. String values starting with a known vocabulary prefix (foundation:, anthropic:, owl:, rdf:, rdfs:, xsd:, qudt:) and containing no spaces are stored as IRI object references; all other strings are stored as xsd:string literals. Booleans and numbers are typed accordingly. The 'label' key is special: it sets rdfs:label.".to_string(),
                    required: false,
                    schema: Some(serde_json::json!({ "type": "object", "additionalProperties": true })),
                },
                Parameter {
                    name: "target_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "When provided, adopt this existing individual instead of creating a new deterministic IRI. Useful for migrating legacy manually-created entities.".to_string(),
                    required: false,
                    schema: None,
                },
            ],
        },

        ToolTemplate {
            name: "datasync_run".to_string(),
            array_mode: false,
            description: "Trigger an immediate extract cycle for a DataSource (bypasses the cron schedule). Fetches the endpoint, stages RawDataRecords, and optionally triggers the transform automation.".to_string(),
            parameters: vec![
                Parameter {
                    name: "data_source_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "IRI of the DataSource to run (e.g. 'foundation:DataSource_123').".to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "run_transform".to_string(),
                    param_type: "boolean".to_string(),
                    description: "If true, also triggers the transform automation after staging. Default: false.".to_string(),
                    required: false,
                    schema: None,
                },
            ],
        },

        ToolTemplate {
            name: "datasync_retry_transform".to_string(),
            array_mode: false,
            description: "Retry transformation for RawDataRecords in Error status for a given DataSource. Resets them to Pending and triggers the transform automation.".to_string(),
            parameters: vec![
                Parameter {
                    name: "data_source_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "IRI of the DataSource whose failed records should be retried.".to_string(),
                    required: true,
                    schema: None,
                },
            ],
        },

        ToolTemplate {
            name: "datasync_status".to_string(),
            array_mode: false,
            description: "Returns sync statistics for all DataSources or a specific one: count of RawDataRecords in Pending, Transformed, Error, Skipped states, plus DataSource connection status.".to_string(),
            parameters: vec![
                Parameter {
                    name: "data_source_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "IRI of a specific DataSource to inspect. Omit to get summary for all.".to_string(),
                    required: false,
                    schema: None,
                },
            ],
        },

        ToolTemplate {
            name: "datasync_list_raw".to_string(),
            array_mode: false,
            description: "Lists RawDataRecords for a DataSource, optionally filtered by transformStatus. Returns IRI, externalId, receivedAt, transformStatus, retryCount.".to_string(),
            parameters: vec![
                Parameter {
                    name: "data_source_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "IRI of the DataSource to query.".to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "transform_status".to_string(),
                    param_type: "string".to_string(),
                    description: "Filter by transformStatus IRI (e.g. 'foundation:Pending'). Omit for all records.".to_string(),
                    required: false,
                    schema: None,
                },
                Parameter {
                    name: "limit".to_string(),
                    param_type: "integer".to_string(),
                    description: "Maximum number of records to return. Default: 50.".to_string(),
                    required: false,
                    schema: None,
                },
            ],
        },

        ToolTemplate {
            name: "datasync_inspect_raw".to_string(),
            array_mode: false,
            description: "Returns the full details of a single RawDataRecord including its rawPayload (or file path for large payloads), transformError, and linked domain entities.".to_string(),
            parameters: vec![
                Parameter {
                    name: "raw_record_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "IRI of the RawDataRecord to inspect.".to_string(),
                    required: true,
                    schema: None,
                },
            ],
        },

        ToolTemplate {
            name: "datasync_set_transform".to_string(),
            array_mode: false,
            description: "Set the Rhai transform script for a DataSource. Always validates the script first — invalid scripts are rejected and the existing script is preserved. With dry_run=true, executes the candidate script against real pending records with all writes mocked, returning what would have been created/updated without persisting anything. Without dry_run, persists foundation:script on the DataSource's transform CodeTask and triggers revalidation via the script_validator.".to_string(),
            parameters: vec![
                Parameter {
                    name: "data_source_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "IRI of the DataSource whose transform script to set (e.g. 'foundation:DataSource_123').".to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "script".to_string(),
                    param_type: "string".to_string(),
                    description: "Rhai script to set. Must compile successfully before being accepted. The script has access to mcp(), parse_json(), to_json(), log(), ctx_get(), owl_get_property(), owl_get_one(), owl_set_property(), owl_assert(), today(), today_plus() helpers.".to_string(),
                    required: true,
                    schema: None,
                },
                Parameter {
                    name: "dry_run".to_string(),
                    param_type: "boolean".to_string(),
                    description: "When true, executes the candidate script against real pending records with writes mocked — returns script output and intercepted write calls. Nothing is persisted. Default: false.".to_string(),
                    required: false,
                    schema: None,
                },
            ],
        },

    ]
}
