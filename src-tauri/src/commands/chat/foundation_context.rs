use crate::eavto::Connection;
use crate::owl::{Class, Individual};

const AUTOMATION_TASK_ROOT: &str = "foundation:automation_Task";
const AUTOMATION_EVENT_ROOT: &str = "foundation:automation_Event";
const AUTOMATION_GATEWAY_ROOT: &str = "foundation:automation_Gateway";

pub fn foundation_architecture_context(conn: &Connection) -> String {
    let task_types = list_subclasses_of(conn, AUTOMATION_TASK_ROOT);
    let event_types = list_subclasses_of(conn, AUTOMATION_EVENT_ROOT);
    let gateway_types = list_subclasses_of(conn, AUTOMATION_GATEWAY_ROOT);

    let task_list = render_list(&task_types, "no task types registered");
    let event_list = render_list(&event_types, "no event types registered");
    let gateway_list = render_list(&gateway_types, "no gateway types registered");

    format!(
        "# Foundation — system architecture\n\
         \n\
         Foundation is the user's personal knowledge base. Everything — people, tasks, \
         notes, widgets, agents, automations, settings — are individuals of an \
         RDF/OWL ontology stored in the graph. You operate on this graph by calling the \
         MCP tools (`search`, `describe_class`, `describe_individual`, \
         `assert_individual`, `add_property_values`, `replace_property_values`, \
         `remove_property_values`, `retract_individual`, `define_class`, `define_property`, \
         `class_graph`, among others).\n\
         \n\
         ## Ontology governance (CRITICAL)\n\
         \n\
         **Core principle**: always reuse what already exists. NEVER create redundancy. \
         Every new class, property or individual is semantic debt — duplicates \
         fragment the graph and make queries and calculations infeasible. When in doubt: do not create, \
         search more.\n\
         \n\
         ### Before creating a CLASS or PROPERTY (mandatory)\n\
         1. Call `class_graph` on the related semantic region to understand the existing \
         structure. E.g.: `class_graph(class_iri: \"foundation:Person\", max_depth: 3)`.\n\
         2. Examine the returned nodes (classes) and edges (relationships). Look for \
         synonyms, name variations, concepts that cover the same territory.\n\
         3. Use `search(query: \"...\", type: \"class\")` with alternative terms (in the user's \
         language and English) before concluding that it does not exist.\n\
         4. If it is a class, identify the appropriate superclass — prefer extending by \
         subclassing over reinventing. Only create a root when there is no suitable semantic parent.\n\
         5. If it is a property equivalent/inverse of an existing one, do NOT create it: use the \
         existing one, possibly with `inverse_label`.\n\
         6. Only call `define_class`/`define_property` after the steps above confirm \
         there is no equivalent.\n\
         \n\
         ### Before creating an INDIVIDUAL (mandatory)\n\
         1. Identify the target class. Call `describe_class` to see its required \
         fields, applicable properties, subclasses and restrictions.\n\
         2. Use `class_graph` when you need context (how this class connects to the \
         rest of the graph) — especially to decide which relationship properties \
         to fill.\n\
         3. **Look for duplicates using a natural key**: before creating a Person, search by \
         email/name; before a Company, search by tax ID/name; before a Task, search \
         by label+context. Use `search` filtered by `class_iri` and by key properties.\n\
         4. If it exists — use the existing IRI. If you need to add new information, use \
         `add_property_values` on the existing individual.\n\
         5. Choose the most specific class that applies (prefer `foundation:CSVFile` to \
         `foundation:File` when applicable).\n\
         \n\
         ### Modeling conventions\n\
         - **Property domain**: `rdfs:domain` goes on the class that **owns** the \
         property, never on the range class. E.g.: `foundation:hasStatus` has domain \
         `owl:Thing` (not `foundation:Status`); `foundation:userRole` has domain \
         `foundation:UserStory` (not `foundation:Persona`).\n\
         - **Relationships**: always declare the property on the \"many\" side pointing \
         to the \"one\" side (child → parent). To navigate in the inverse direction use `inverse_label` \
         instead of duplicating the property.\n\
         - **Primitive types vs references**: prefer `owl:ObjectProperty` pointing to \
         an existing class (`foundation:City`, `foundation:EmailAddress`, \
         `foundation:Person`) over a bare `xsd:string`. Use primitives for \
         scalar/boolean/date values, identifiers/codes, free text and monetary values.\n\
         - **Numeric properties**: always declare `qudt:unit` (`unit:Count`, \
         `unit:Meter`, `currency:BRL`, etc.).\n\
         - **Data correction**: when you find a stored value that is wrong/dirty, fix the \
         data in the graph. Do not add normalization/sanitization logic to compensate — the source \
         of truth is the graph.\n\
         \n\
         ## Ontology\n\
         - **Class** (`owl:Class`): defines a type. E.g.: `foundation:Task`, `foundation:Person`.\n\
         - **Subclass / polymorphism** (`rdfs:subClassOf`): a class inherits properties and \
         behaviors from its superclass. E.g.: `foundation:CSVFile` is a subclass of \
         `foundation:File`. When searching for instances of a class, also consider \
         instances of subclasses — and when creating an instance, choose the most specific class \
         that applies.\n\
         - **Individual** (instance): each concrete entity. IRIs follow the pattern \
         `foundation:ClassName_{{timestamp}}`. Use `assert_individual` to create one.\n\
         - **Property**: a typed link between individuals (object property) or between \
         an individual and a literal (datatype property). Declared via `define_property`.\n\
         \n\
         ## Property types\n\
         Every property behaves as one of the four modes below. Before creating a \
         new one, check via `describe_class`/`describe_property` whether something \
         equivalent already exists.\n\
         \n\
         1. **Value** — stores a typed literal (`xsd:string`, `xsd:integer`, `xsd:decimal`, \
         `xsd:date`, `xsd:dateTime`, `xsd:boolean`, `xsd:anyURI`) or an IRI reference (object \
         property with `rdfs:range` pointing to another class). Numeric properties \
         require `qudt:unit` (e.g.: `unit:Count`, `unit:Meter`, `currency:BRL`).\n\
         2. **Calculation** — the property has a `foundation:formula` with an expression that references \
         other properties via `{{foundation:propName}}`. The value is recomputed \
         automatically when its dependencies change. Aggregations use \
         `foundation:aggregation` with functions such as \
         `SUM({{foundation:hasItems}}.foundation:amount)` — it traverses a collection and aggregates.\n\
         3. **Query** — the property has a `foundation:queryDefinition` with JSON describing \
         the target class and filters. The property's value is the list of individuals that match \
         the filter, recomputed dynamically.\n\
         4. **Reference** — an object property that points to another individual via an IRI. The `rdfs:range` \
         defines the target class. It can have an inverse (`owl:inverseOf`) to navigate in both \
         directions without duplicating the triple.\n\
         \n\
         ## Automations\n\
         A `foundation:automation_Process` is a BPMN-like flow. Each node is a \
         `foundation:automation_FlowNode` connected to the process via `foundation:partOfProcess` \
         and to its neighbors via `foundation:flowTo`. The executor dispatches each node to the \
         corresponding handler based on its `rdf:type`.\n\
         \n\
         ### Registered events\n\
         {event_list}\n\
         \n\
         ### Registered task types\n\
         {task_list}\n\
         \n\
         ### Gateways\n\
         {gateway_list}\n\
         \n\
         For a task/process to run, it needs `foundation:scheduledAt` in the future or \
         `foundation:hasStatus = foundation:InProgress`. Without that it stays pending.\n\
         \n\
         ## Blackboard\n\
         The blackboard is the visual widget panel. There is one blackboard per conversation, and \
         a default blackboard (`foundation:DefaultBlackboard`) used when the internal chat \
         is not active. Each widget renders a graph entity using a \
         `foundation:WidgetType`. You add widgets via \
         `add_widget_to_blackboard(entity_iri, widget_type, blackboard_iri?)` — `blackboard_iri` \
         is optional: if omitted, it uses the active conversation's blackboard. The list of available \
         widgets appears later in this prompt in the \"AI-creatable widgets\" section.",
        task_list = task_list,
        event_list = event_list,
        gateway_list = gateway_list,
    )
}

fn list_subclasses_of(conn: &Connection, root_iri: &str) -> Vec<(String, String, Option<String>)> {
    let Ok(iris) = Class::get_descendant_iris(conn, root_iri) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for iri in iris {
        if iri == root_iri {
            continue;
        }
        if let Ok(Some(ind)) = Individual::get(conn, &iri) {
            let label = ind.label.clone().unwrap_or_else(|| iri.clone());
            let comment = ind.comment.clone().filter(|c| !c.trim().is_empty());
            out.push((iri, label, comment));
        }
    }
    out.sort_by(|a, b| a.1.cmp(&b.1));
    out
}

fn render_list(items: &[(String, String, Option<String>)], empty_msg: &str) -> String {
    if items.is_empty() {
        return format!("_{}_", empty_msg);
    }
    items.iter()
        .map(|(iri, label, comment)| match comment {
            Some(c) => format!("- `{}` ({}) — {}", iri, label, c),
            None => format!("- `{}` ({})", iri, label),
        })
        .collect::<Vec<_>>()
        .join("\n")
}
