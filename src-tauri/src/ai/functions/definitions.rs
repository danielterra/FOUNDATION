use super::{FunctionDefinition, Parameter};

pub fn get_claude_tools() -> Vec<crate::ai::providers::ClaudeTool> {
    get_available_functions()
        .into_iter()
        .map(|f| f.to_claude_tool())
        .collect()
}

pub fn get_available_functions() -> Vec<FunctionDefinition> {
    vec![
        FunctionDefinition {
            name: "remember_concepts".to_string(),
            description: "Search your memory for concepts you know about. Find what types of things you've learned. IMPORTANT: Always use English for concept searches and keep all concept labels in English.".to_string(),
            parameters: vec![
                Parameter {
                    name: "query".to_string(),
                    param_type: "string".to_string(),
                    description: "Search keywords separated by spaces in ENGLISH. ALL words must match (AND search). Use 1-3 most important words for best results. Example: 'driver license' instead of 'CNH habilitação carteira motorista'.".to_string(),
                    required: false,
                },
                Parameter {
                    name: "limit".to_string(),
                    param_type: "number".to_string(),
                    description: "How many results to return (default: 100)".to_string(),
                    required: false,
                },
                Parameter {
                    name: "offset".to_string(),
                    param_type: "number".to_string(),
                    description: "How many to skip for pagination (default: 0)".to_string(),
                    required: false,
                },
            ],
        },
        FunctionDefinition {
            name: "remember_concept".to_string(),
            description: "Remember everything you know about a specific concept - what it's related to, what things belong to it.".to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "ID of the concept (e.g., 'foundation:Person')".to_string(),
                    required: true,
                },
            ],
        },
        FunctionDefinition {
            name: "remember_things".to_string(),
            description: "Search your memory for specific things in a concept (e.g., which people, places, or organizations you remember).".to_string(),
            parameters: vec![
                Parameter {
                    name: "concept_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "Which concept to search in (optional - if not provided, searches across all concepts)".to_string(),
                    required: false,
                },
                Parameter {
                    name: "query".to_string(),
                    param_type: "string".to_string(),
                    description: "Search keywords separated by spaces. ALL words must match (AND search). Use 1-3 most important words for best results.".to_string(),
                    required: false,
                },
                Parameter {
                    name: "limit".to_string(),
                    param_type: "number".to_string(),
                    description: "How many results to return (default: 100)".to_string(),
                    required: false,
                },
                Parameter {
                    name: "offset".to_string(),
                    param_type: "number".to_string(),
                    description: "How many to skip for pagination (default: 0)".to_string(),
                    required: false,
                },
            ],
        },
        FunctionDefinition {
            name: "remember_thing".to_string(),
            description: "Remember everything you know about a specific thing - all its details and connections.".to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "ID of the thing you want to remember".to_string(),
                    required: true,
                },
            ],
        },
        FunctionDefinition {
            name: "learn_concept".to_string(),
            description: "Learn a new concept (e.g., when users mention a new type of thing you should remember). IMPORTANT: Always create concepts with English labels and descriptions, regardless of the user's language.".to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "Unique ID for this concept (e.g., 'foundation:Project')".to_string(),
                    required: true,
                },
                Parameter {
                    name: "label".to_string(),
                    param_type: "string".to_string(),
                    description: "Name for this concept in ENGLISH (e.g., 'Driver License' not 'CNH')".to_string(),
                    required: true,
                },
                Parameter {
                    name: "icon".to_string(),
                    param_type: "string".to_string(),
                    description: "Material icon name (e.g., 'category', 'label') OR image URL. For attachment images, get foundation:filePath from the File entity and use that URL (e.g., 'file:///path/to/image.jpg').".to_string(),
                    required: true,
                },
                Parameter {
                    name: "comment".to_string(),
                    param_type: "string".to_string(),
                    description: "Optional description".to_string(),
                    required: false,
                },
                Parameter {
                    name: "super_class".to_string(),
                    param_type: "string".to_string(),
                    description: "Optional parent concept ID".to_string(),
                    required: false,
                },
            ],
        },
        FunctionDefinition {
            name: "learn_thing".to_string(),
            description: "Learn about a new specific thing (person, place, organization, etc.). You'll get back an ID to reference it later. IMPORTANT: Before creating a new thing, ALWAYS search first using remember_things (WITHOUT specifying concept_iri) to check if it already exists and avoid duplicates.".to_string(),
            parameters: vec![
                Parameter {
                    name: "concept_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "What concept this thing belongs to".to_string(),
                    required: true,
                },
                Parameter {
                    name: "label".to_string(),
                    param_type: "string".to_string(),
                    description: "Name of this thing".to_string(),
                    required: true,
                },
                Parameter {
                    name: "icon".to_string(),
                    param_type: "string".to_string(),
                    description: "Material icon name (e.g., 'person', 'business') OR image URL. For attachment images, get foundation:filePath from the File entity and use that URL (e.g., 'file:///path/to/image.jpg').".to_string(),
                    required: true,
                },
                Parameter {
                    name: "comment".to_string(),
                    param_type: "string".to_string(),
                    description: "Optional description".to_string(),
                    required: false,
                },
            ],
        },
        FunctionDefinition {
            name: "learn_thing_detail".to_string(),
            description: "Learn a new detail or connection about a thing (e.g., learn that X works at Y, or that someone's birthday is May 15).".to_string(),
            parameters: vec![
                Parameter {
                    name: "instance_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "ID of the thing to add details to".to_string(),
                    required: true,
                },
                Parameter {
                    name: "property_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "What kind of connection or detail (e.g., 'foundation:worksAt')".to_string(),
                    required: true,
                },
                Parameter {
                    name: "value".to_string(),
                    param_type: "string".to_string(),
                    description: "The value (text, number, date, or ID of another thing)".to_string(),
                    required: true,
                },
                Parameter {
                    name: "value_type".to_string(),
                    param_type: "string".to_string(),
                    description: "'literal' for text/numbers/dates, 'iri' for connections to other things. Default: 'literal'".to_string(),
                    required: false,
                },
                Parameter {
                    name: "datatype".to_string(),
                    param_type: "string".to_string(),
                    description: "Data type: 'xsd:string', 'xsd:integer', 'xsd:decimal', 'xsd:boolean', 'xsd:date' (YYYY-MM-DD), 'xsd:dateTime' (ISO 8601), etc. Default: 'xsd:string'. Use xsd:date for dates without time, xsd:dateTime for timestamps.".to_string(),
                    required: false,
                },
            ],
        },
        FunctionDefinition {
            name: "learn_connection_type".to_string(),
            description: "Learn a new type of connection or detail you can remember about things (e.g., 'worksAt', 'bornOn', 'hasSkill').".to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "Unique ID for this connection type (e.g., 'foundation:hasAge')".to_string(),
                    required: true,
                },
                Parameter {
                    name: "label".to_string(),
                    param_type: "string".to_string(),
                    description: "Name for this connection type".to_string(),
                    required: true,
                },
                Parameter {
                    name: "property_type".to_string(),
                    param_type: "string".to_string(),
                    description: "'object' for connections to other things, 'datatype' for simple values (text, numbers, dates)".to_string(),
                    required: true,
                },
                Parameter {
                    name: "domain".to_string(),
                    param_type: "string".to_string(),
                    description: "Optional: which concept this applies to".to_string(),
                    required: false,
                },
                Parameter {
                    name: "range".to_string(),
                    param_type: "string".to_string(),
                    description: "Optional: for 'object' properties, what concept it connects to; for 'datatype', what type of value".to_string(),
                    required: false,
                },
                Parameter {
                    name: "comment".to_string(),
                    param_type: "string".to_string(),
                    description: "Optional description".to_string(),
                    required: false,
                },
            ],
        },
        FunctionDefinition {
            name: "remember_connection_type".to_string(),
            description: "Remember everything you know about a specific type of connection.".to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "ID of the connection type".to_string(),
                    required: true,
                },
            ],
        },
        FunctionDefinition {
            name: "forget_concept".to_string(),
            description: "Forget a concept (but not the specific things that belong to it).".to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "The IRI of the class to delete".to_string(),
                    required: true,
                },
            ],
        },
        FunctionDefinition {
            name: "forget_thing".to_string(),
            description: "Forget a specific thing completely (all its details and connections).".to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "ID of the thing to forget".to_string(),
                    required: true,
                },
            ],
        },
        FunctionDefinition {
            name: "forget_connection_type".to_string(),
            description: "Forget a type of connection.".to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "ID of the connection type to forget".to_string(),
                    required: true,
                },
            ],
        },
        FunctionDefinition {
            name: "update_concept".to_string(),
            description: "Update details about a concept (name, icon, description).".to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "ID of the concept".to_string(),
                    required: true,
                },
                Parameter {
                    name: "label".to_string(),
                    param_type: "string".to_string(),
                    description: "New name".to_string(),
                    required: false,
                },
                Parameter {
                    name: "icon".to_string(),
                    param_type: "string".to_string(),
                    description: "Material icon name OR image URL (e.g., 'file:///path/to/image.jpg')".to_string(),
                    required: false,
                },
                Parameter {
                    name: "comment".to_string(),
                    param_type: "string".to_string(),
                    description: "New description".to_string(),
                    required: false,
                },
                Parameter {
                    name: "super_class".to_string(),
                    param_type: "string".to_string(),
                    description: "New parent concept ID".to_string(),
                    required: false,
                },
            ],
        },
        FunctionDefinition {
            name: "update_thing".to_string(),
            description: "Update details about a thing (name, icon, description).".to_string(),
            parameters: vec![
                Parameter {
                    name: "iri".to_string(),
                    param_type: "string".to_string(),
                    description: "ID of the thing".to_string(),
                    required: true,
                },
                Parameter {
                    name: "label".to_string(),
                    param_type: "string".to_string(),
                    description: "New name".to_string(),
                    required: false,
                },
                Parameter {
                    name: "icon".to_string(),
                    param_type: "string".to_string(),
                    description: "Material icon name OR image URL (e.g., 'file:///path/to/image.jpg')".to_string(),
                    required: false,
                },
                Parameter {
                    name: "comment".to_string(),
                    param_type: "string".to_string(),
                    description: "New description".to_string(),
                    required: false,
                },
            ],
        },
        FunctionDefinition {
            name: "forget_thing_detail".to_string(),
            description: "Forget a specific detail or connection about a thing (e.g., forget that someone works at X).".to_string(),
            parameters: vec![
                Parameter {
                    name: "instance_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "ID of the thing".to_string(),
                    required: true,
                },
                Parameter {
                    name: "property_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "What kind of detail to forget".to_string(),
                    required: true,
                },
                Parameter {
                    name: "value".to_string(),
                    param_type: "string".to_string(),
                    description: "The specific value to forget".to_string(),
                    required: true,
                },
            ],
        },
        FunctionDefinition {
            name: "remember_things_by_details".to_string(),
            description: "Remember things that have specific details or connections (e.g., remember all people who work at X).".to_string(),
            parameters: vec![
                Parameter {
                    name: "concept_iri".to_string(),
                    param_type: "string".to_string(),
                    description: "Which concept to search in".to_string(),
                    required: true,
                },
                Parameter {
                    name: "properties".to_string(),
                    param_type: "array".to_string(),
                    description: "Details to match: array of {property: 'IRI', value: 'VALUE'}".to_string(),
                    required: true,
                },
            ],
        },
        FunctionDefinition {
            name: "blackboard_show".to_string(),
            description: "See what is currently being displayed on the blackboard/canvas. Returns a list of active widgets.".to_string(),
            parameters: vec![],
        },
        FunctionDefinition {
            name: "blackboard_add_widget".to_string(),
            description: "Add a widget to the blackboard to display information visually. The system will automatically position the widget. Use remember_concepts to search for available widget types (foundation:Widget subclasses).".to_string(),
            parameters: vec![
                Parameter {
                    name: "widget_type".to_string(),
                    param_type: "string".to_string(),
                    description: "The type of widget to add (e.g., 'Inspector'). Search for foundation:Widget subclasses to find available types.".to_string(),
                    required: true,
                },
                Parameter {
                    name: "params".to_string(),
                    param_type: "object".to_string(),
                    description: "Widget-specific parameters as a JSON object. For Inspector: {\"entity_id\": \"foundation:SomeEntity\"}".to_string(),
                    required: true,
                },
            ],
        },
        FunctionDefinition {
            name: "blackboard_remove".to_string(),
            description: "Remove a specific widget from the blackboard by its ID.".to_string(),
            parameters: vec![
                Parameter {
                    name: "widget_id".to_string(),
                    param_type: "string".to_string(),
                    description: "The ID of the widget to remove".to_string(),
                    required: true,
                },
            ],
        },
        FunctionDefinition {
            name: "blackboard_clear".to_string(),
            description: "Clear all widgets from the blackboard. Use this to start fresh.".to_string(),
            parameters: vec![],
        },
    ]
}
