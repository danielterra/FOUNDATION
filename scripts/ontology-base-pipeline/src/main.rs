use oxrdf::{Literal, NamedOrBlankNode, Term};
use oxttl::TurtleParser;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

const APP_ROOT: &str = env!("CARGO_MANIFEST_DIR");
const MANIFEST_REL: &str = "../../src/lib/json-render/semantic/catalog-manifest.json";
const SCHEMA_REL: &str =
    "../../../foundation-core/ontology/base/semantic-widget-catalog-schema.ttl";
const CONTRACTS_REL: &str =
    "../../../foundation-core/ontology/base/semantic-widget-catalog-contracts.ttl";
const CORE_TTL_REL: &str = "../../../foundation-core/ontology/base/core.ttl";
const SQL_REL: &str = "../../../foundation-core/assets/ontology.sql";
const CORE_JSON_REL: &str = "../../../foundation-core/assets/semantic_widget_catalog.json";
const DB_SCHEMA_REL: &str = "../../../foundation-core/assets/schema.sql";
const SEED_ALLOWLIST_REL: &str = "../../../foundation-core/ontology/base/seed-allowlist.txt";

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Overlay {
    format_version: u32,
    manifest_version: u64,
    triples: Vec<OverlayTriple>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
struct OverlayTriple {
    subject: String,
    predicate: String,
    object: OverlayObject,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "lowercase")]
enum OverlayObject {
    Iri {
        value: String,
    },
    Literal {
        value: String,
        datatype: Option<String>,
        language: Option<String>,
    },
}

#[derive(Debug, Deserialize)]
struct Manifest {
    version: u64,
    components: Vec<Component>,
    actions: Vec<Value>,
    examples: BTreeMap<String, Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Component {
    id: String,
    contract_version: u64,
    description: String,
    props_schema: Value,
    slots: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum RdfObject {
    Iri(String),
    Blank(String),
    Literal {
        value: String,
        datatype: String,
        language: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Fact {
    subject: String,
    predicate: String,
    object: RdfObject,
}

fn project_path(relative: &str) -> PathBuf {
    Path::new(APP_ROOT).join(relative)
}

fn qname(value: &str) -> String {
    const PREFIXES: &[(&str, &str)] = &[
        ("foundation", "http://foundation.local/ontology/"),
        ("rdf", "http://www.w3.org/1999/02/22-rdf-syntax-ns#"),
        ("rdfs", "http://www.w3.org/2000/01/rdf-schema#"),
        ("owl", "http://www.w3.org/2002/07/owl#"),
        ("xsd", "http://www.w3.org/2001/XMLSchema#"),
        ("skos", "http://www.w3.org/2004/02/skos/core#"),
        ("qudt", "http://qudt.org/schema/qudt/"),
        ("unit", "http://qudt.org/vocab/unit/"),
    ];
    PREFIXES
        .iter()
        .find_map(|(prefix, namespace)| {
            value
                .strip_prefix(namespace)
                .map(|tail| format!("{prefix}:{tail}"))
        })
        .unwrap_or_else(|| value.to_string())
}

fn turtle_files(paths: &[PathBuf]) -> Result<Vec<Fact>, Box<dyn Error>> {
    let mut facts = BTreeSet::new();
    for path in paths {
        let input = fs::read(path)?;
        for parsed in TurtleParser::new().for_reader(input.as_slice()) {
            let triple = parsed?;
            let subject = match triple.subject {
                NamedOrBlankNode::NamedNode(node) => qname(node.as_str()),
                NamedOrBlankNode::BlankNode(node) => format!("_:{}", node.as_str()),
            };
            let predicate = qname(triple.predicate.as_str());
            let object = match triple.object {
                Term::NamedNode(node) => RdfObject::Iri(qname(node.as_str())),
                Term::BlankNode(node) => RdfObject::Blank(format!("_:{}", node.as_str())),
                Term::Literal(literal) => literal_object(literal),
            };
            facts.insert(Fact {
                subject,
                predicate,
                object,
            });
        }
    }
    Ok(facts.into_iter().collect())
}

fn turtle_occurrence_count(paths: &[PathBuf]) -> Result<usize, Box<dyn Error>> {
    let mut count = 0;
    for path in paths {
        let input = fs::read(path)?;
        for parsed in TurtleParser::new().for_reader(input.as_slice()) {
            parsed?;
            count += 1;
        }
    }
    Ok(count)
}

fn literal_object(value: Literal) -> RdfObject {
    let lang = value.language().map(str::to_owned);
    let datatype = qname(value.datatype().as_str());
    RdfObject::Literal {
        value: value.value().to_owned(),
        datatype,
        language: lang,
    }
}

fn canonical_json(value: &Value) -> Result<String, Box<dyn Error>> {
    Ok(serde_json::to_string(value)?)
}

fn manifest() -> Result<Manifest, Box<dyn Error>> {
    let content = fs::read(project_path(MANIFEST_REL))?;
    Ok(serde_json::from_slice(&content)?)
}

fn contract_slug(id: &str) -> String {
    id.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

fn current_contracts(
    previous: &Overlay,
) -> Result<BTreeMap<String, (u64, Map<String, Value>)>, Box<dyn Error>> {
    let mut records = BTreeMap::<String, Map<String, Value>>::new();
    for fact in &previous.triples {
        let OverlayObject::Literal { value, .. } = &fact.object else {
            continue;
        };
        records
            .entry(fact.subject.clone())
            .or_default()
            .insert(fact.predicate.clone(), Value::String(value.clone()));
    }
    let mut output = BTreeMap::new();
    for (iri, fields) in records {
        let Some(version) = fields
            .get("foundation:componentContractVersion")
            .and_then(Value::as_str)
            .and_then(|v| v.parse::<u64>().ok())
        else {
            continue;
        };
        output.insert(iri, (version, fields));
    }
    Ok(output)
}

fn make_triple(subject: &str, predicate: &str, object: OverlayObject) -> OverlayTriple {
    OverlayTriple {
        subject: subject.to_string(),
        predicate: predicate.to_string(),
        object,
    }
}

fn literal(value: String) -> OverlayObject {
    OverlayObject::Literal {
        value,
        datatype: Some("xsd:string".into()),
        language: None,
    }
}

fn validate_contract_version(component_id: &str, contract_version: u64) -> Result<(), String> {
    if contract_version == 0 && component_id != "Information" {
        return Err(format!("{component_id} needs a positive contractVersion"));
    }
    Ok(())
}

fn contract_hash(
    component: &Component,
    actions: &[Value],
    example: &Value,
) -> Result<String, Box<dyn Error>> {
    let mut props = component.props_schema.clone();
    if !component.slots.is_empty() {
        props
            .as_object_mut()
            .ok_or("Component props schema must be object")?
            .insert(
                "x-foundation-slots".into(),
                Value::Array(component.slots.clone()),
            );
    }
    let payload = json!({
        "componentId": component.id,
        "intent": component.description,
        "propsSchema": props,
        "actionsSchema": actions,
        "examples": example,
    });
    Ok(format!(
        "sha256:{:x}",
        Sha256::digest(canonical_json(&payload)?.as_bytes())
    ))
}

fn component_triples(
    manifest: &Manifest,
    previous: &Overlay,
) -> Result<Vec<OverlayTriple>, Box<dyn Error>> {
    let historical = current_contracts(previous)?;
    let mut by_id = BTreeMap::<String, Vec<(u64, String, Map<String, Value>)>>::new();
    for (iri, (version, fields)) in historical {
        let id = fields
            .get("foundation:componentId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        by_id.entry(id).or_default().push((version, iri, fields));
    }
    let current_versions = manifest
        .components
        .iter()
        .map(|component| (component.id.as_str(), component.contract_version))
        .collect::<BTreeMap<_, _>>();
    let mut preserved_iris = BTreeSet::new();
    for (component_id, versions) in &by_id {
        for (version, iri, _) in versions {
            match current_versions.get(component_id.as_str()) {
                Some(current) if version > current => {
                    return Err(format!(
                        "{} renderer version regressed below published v{}",
                        component_id, version
                    )
                    .into());
                }
                Some(current) if version == current => {}
                _ => {
                    preserved_iris.insert(iri.clone());
                }
            }
        }
    }
    let mut triples = previous.triples.iter().filter(|triple| {
        preserved_iris.contains(&triple.subject)
            || (triple.subject == "foundation:CoreOntologyRegistry"
                && triple.predicate == "foundation:includesIndividual"
                && matches!(&triple.object, OverlayObject::Iri { value } if preserved_iris.contains(value)))
    }).cloned().collect::<Vec<_>>();
    for iri in &preserved_iris {
        triples.push(make_triple(
            iri,
            "rdf:type",
            OverlayObject::Iri {
                value: "foundation:SemanticComponentDefinition".into(),
            },
        ));
    }
    for component in &manifest.components {
        validate_contract_version(&component.id, component.contract_version)?;
        let example = manifest
            .examples
            .get(&component.id)
            .ok_or_else(|| format!("Missing example for {}", component.id))?;
        let mut props = component.props_schema.clone();
        if !component.slots.is_empty() {
            props
                .as_object_mut()
                .ok_or("Component props schema must be object")?
                .insert(
                    "x-foundation-slots".into(),
                    Value::Array(component.slots.clone()),
                );
        }
        let props = serde_json::to_value(props)?;
        let actions = serde_json::to_value(&manifest.actions)?;
        let props_json = canonical_json(&props)?;
        let actions_json = canonical_json(&actions)?;
        let example_json = canonical_json(example)?;
        let hash = contract_hash(component, &manifest.actions, example)?;
        let iri = format!(
            "foundation:SemanticComponentDefinition_{}_v{}",
            contract_slug(&component.id),
            component.contract_version
        );
        let mut values = BTreeMap::<String, String>::new();
        values.insert("foundation:componentId".into(), component.id.clone());
        values.insert(
            "foundation:componentContractVersion".into(),
            component.contract_version.to_string(),
        );
        values.insert(
            "foundation:componentIntent".into(),
            component.description.clone(),
        );
        values.insert("foundation:componentPropsSchema".into(), props_json);
        values.insert("foundation:componentActionsSchema".into(), actions_json);
        values.insert("foundation:componentExamples".into(), example_json);
        values.insert("foundation:componentContractHash".into(), hash);
        values.insert(
            "foundation:componentManifestVersion".into(),
            manifest.version.to_string(),
        );

        if let Some(history) = by_id.get(&component.id) {
            let max = history.iter().map(|(v, _, _)| *v).max().unwrap_or(0);
            if component.contract_version < max {
                return Err(format!(
                    "{} current contract version regressed below history",
                    component.id
                )
                .into());
            }
            if let Some((_, _, previous_fields)) = history
                .iter()
                .find(|(v, _, _)| *v == component.contract_version)
            {
                for (predicate, expected) in &values {
                    let old = previous_fields
                        .get(predicate)
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    if predicate == "foundation:componentManifestVersion" {
                        continue;
                    }
                    if predicate == "foundation:componentPropsSchema"
                        || predicate == "foundation:componentActionsSchema"
                        || predicate == "foundation:componentExamples"
                    {
                        if canonical_json(&serde_json::from_str::<Value>(old)?)? != *expected {
                            return Err(format!(
                                "{} v{} changed without incrementing contractVersion",
                                component.id, component.contract_version
                            )
                            .into());
                        }
                    } else if old != expected {
                        return Err(format!("{} v{} changed immutable field {} without incrementing contractVersion", component.id, component.contract_version, predicate).into());
                    }
                }
            } else if component.contract_version <= max {
                return Err(format!(
                    "{} needs a higher contractVersion than its history",
                    component.id
                )
                .into());
            }
        }

        let label = format!("{} v{}", component.id, component.contract_version);
        let mut current = vec![
            make_triple(&iri, "rdfs:label", literal(label)),
            make_triple(
                &iri,
                "rdf:type",
                OverlayObject::Iri {
                    value: "foundation:SemanticComponentDefinition".into(),
                },
            ),
            make_triple(
                &iri,
                "foundation:hasStatus",
                OverlayObject::Iri {
                    value: "foundation:Status_1781300928499".into(),
                },
            ),
            make_triple(
                "foundation:CoreOntologyRegistry",
                "foundation:includesIndividual",
                OverlayObject::Iri { value: iri.clone() },
            ),
        ];
        for (predicate, value) in values {
            current.push(make_triple(&iri, &predicate, literal(value)));
        }
        triples.extend(current);
    }
    triples.sort_by(|a, b| {
        (&a.subject, &a.predicate, object_key(&a.object)).cmp(&(
            &b.subject,
            &b.predicate,
            object_key(&b.object),
        ))
    });
    triples.dedup();
    Ok(triples)
}

fn object_key(object: &OverlayObject) -> String {
    match object {
        OverlayObject::Iri { value } => format!("i:{value}"),
        OverlayObject::Literal {
            value,
            datatype,
            language,
        } => format!("l:{datatype:?}:{language:?}:{value}"),
    }
}

fn ttl_string(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
    )
}

fn ttl_term(value: &str, iri: bool) -> String {
    if iri {
        if value.starts_with("http://")
            || value.starts_with("https://")
            || value.starts_with("urn:")
        {
            format!("<{value}>")
        } else {
            value.into()
        }
    } else {
        ttl_string(value)
    }
}

fn write_contract_sources(manifest: &Manifest) -> Result<(), Box<dyn Error>> {
    let path = project_path(CONTRACTS_REL);
    let previous = if project_path(CORE_JSON_REL).exists() {
        serde_json::from_slice::<Overlay>(&fs::read(project_path(CORE_JSON_REL))?)?
    } else {
        Overlay {
            format_version: 1,
            manifest_version: manifest.version,
            triples: vec![],
        }
    };
    let triples = component_triples(manifest, &previous)?;
    let mut ttl = String::from(
        "@prefix foundation: <http://foundation.local/ontology/> .\n@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\n# Generated from catalog-manifest.json; component contracts are immutable and independently versioned.\n",
    );
    for triple in &triples {
        let object = match &triple.object {
            OverlayObject::Iri { value } => ttl_term(value, true),
            OverlayObject::Literal {
                value,
                datatype,
                language,
            } => {
                let mut result = ttl_string(value);
                if let Some(language) = language {
                    result.push_str(&format!("@{language}"));
                } else if let Some(datatype) = datatype {
                    if datatype != "xsd:string" {
                        result.push_str(&format!("^^{}", ttl_term(datatype, true)));
                    }
                }
                result
            }
        };
        ttl.push_str(&format!(
            "{} {} {} .\n",
            ttl_term(&triple.subject, true),
            ttl_term(&triple.predicate, true),
            object
        ));
    }
    fs::write(path, ttl)?;
    let overlay = Overlay {
        format_version: 1,
        manifest_version: manifest.version,
        triples,
    };
    fs::write(
        project_path(CORE_JSON_REL),
        format!("{}\n", serde_json::to_string_pretty(&overlay)?),
    )?;
    Ok(())
}

fn literal_has_personal_data(text: &str) -> bool {
    let email = regex::Regex::new(r"(?i)[a-z0-9._%+-]+@[a-z0-9.-]+\.[a-z]{2,}")
        .expect("email scan pattern is valid");
    let secret = regex::Regex::new(r"(?i)(?:sk-[a-z0-9_-]{12,}|bearer\s+[a-z0-9._-]{12,})")
        .expect("secret scan pattern is valid");
    secret.is_match(text) || email.is_match(text)
}

fn verify_seed_safety(facts: &[Fact]) -> Result<(), Box<dyn Error>> {
    let personal_iri = regex::Regex::new(r"(?i)foundation:(?:Email|Person|ThisUser|ThisComputer|ThisFoundationInstance|AIConversation|AIConversationMessage|Conversation|Message|MonthlyBudget|BankAccount|Account|BankStatement|WorkflowExecution|StepExecution|ImportRequest)_?\d+")
        .expect("personal IRI scan pattern is valid");
    let timestamp_iri = regex::Regex::new(r"(?:^|:)[A-Za-z][A-Za-z0-9]*_[0-9]{8,}$")
        .expect("timestamp IRI scan pattern is valid");
    // Exact allowlist of timestamped IDs that are part of the shipped core vocabulary.
    // User-created examples, QA fixtures, automations, and generated local entities must not ship.
    let approved_core_iris = [
        "foundation:Company_1777471062604",
        "foundation:FileType_1793272906262",
        "foundation:MCPTool_1793272238439",
        "foundation:Persona_1772476248172",
        "foundation:Persona_1773180459062",
        "foundation:Persona_1773783644387",
        "foundation:SoftwareRelease_1793272253227",
        "foundation:Status_1772570972065",
        "foundation:Status_1772570972069",
        "foundation:Status_1772600993751",
        "foundation:Status_1772940750706",
        "foundation:Status_1773183515321",
        "foundation:Status_1773672143644",
        "foundation:Status_1781300928499",
        "foundation:Status_1781300928524",
        "foundation:Status_1781300928559",
        "foundation:Status_1781300928585",
        "foundation:Status_1781300928612",
        "foundation:Status_1781300928637",
        "foundation:Status_1781300928665",
        "foundation:WidgetType_1793923655031",
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    let subjects = facts
        .iter()
        .map(|fact| fact.subject.as_str())
        .collect::<BTreeSet<_>>();
    let sensitive = [
        "foundation:credentialValue",
        "foundation:accessToken",
        "foundation:refreshToken",
        "foundation:apiKey",
        "foundation:secretValue",
    ];
    for fact in facts {
        if [fact.subject.as_str(), fact.predicate.as_str()]
            .into_iter()
            .any(|iri| timestamp_iri.is_match(iri) && !approved_core_iris.contains(iri))
            || matches!(&fact.object, RdfObject::Iri(iri) if timestamp_iri.is_match(iri) && !approved_core_iris.contains(iri.as_str()))
        {
            return Err("seed contains an unapproved timestamped/local IRI".into());
        }
        if sensitive.contains(&fact.predicate.as_str()) {
            return Err(format!("seed contains sensitive predicate {}", fact.predicate).into());
        }
        if personal_iri.is_match(&fact.subject) {
            return Err("seed contains a user-specific entity IRI".into());
        }
        if let RdfObject::Literal { value, .. } = &fact.object {
            if literal_has_personal_data(value) {
                return Err("seed contains a secret or email-like literal".into());
            }
        }
        if let RdfObject::Iri(iri) = &fact.object {
            if personal_iri.is_match(iri) {
                return Err("seed contains a user-specific reference IRI".into());
            }
            if iri.starts_with("foundation:")
                && !subjects.contains(iri.as_str())
                && !approved_core_iris.contains(iri.as_str())
                && !iri.starts_with("foundation:icon-material-symbols-name-")
            {
                return Err(
                    "seed contains a Foundation reference outside its curated subject set".into(),
                );
            }
        }
    }

    let registry_members = facts
        .iter()
        .filter_map(
            |fact| match (&*fact.subject, &*fact.predicate, &fact.object) {
                (
                    "foundation:CoreOntologyRegistry",
                    "foundation:includesIndividual",
                    RdfObject::Iri(iri),
                ) => Some(iri.as_str()),
                _ => None,
            },
        )
        .collect::<BTreeSet<_>>();
    let global_types = [
        "foundation:AIAPIService",
        "foundation:AIModel",
        "foundation:BloodType",
        "foundation:Country",
        "foundation:FileType",
        "foundation:Gender",
        "foundation:MCPTool",
        "foundation:MaritalStatus",
        "foundation:SemanticComponentDefinition",
        "foundation:SexualOrientation",
        "foundation:SkinColor",
        "foundation:SoftwareProduct",
        "foundation:SoftwareRelease",
        "foundation:Status",
        "foundation:TaskPriority",
        "foundation:WidgetType",
        "foundation:JsonRenderSpec",
        "rdfs:Datatype",
        "qudt:AspectClass",
        "qudt:BitEncodingType",
        "qudt:BooleanEncodingType",
        "qudt:ByteEncodingType",
        "qudt:CardinalityType",
        "qudt:CharEncodingType",
        "qudt:CountingUnit",
        "qudt:CurrencyUnit",
        "qudt:DateTimeStringEncodingType",
        "qudt:DerivedUnit",
        "qudt:DimensionlessUnit",
        "qudt:EndianType",
        "qudt:FloatingPointEncodingType",
        "qudt:IntegerEncodingType",
        "qudt:LogarithmicUnit",
        "qudt:OrderedType",
        "qudt:QuantityKind",
        "qudt:Unit",
    ];
    for member in registry_members {
        if !subjects.contains(member) {
            return Err("core registry references an absent curated individual".into());
        }
        if !facts.iter().any(|fact| {
            fact.subject == member
                && fact.predicate == "rdf:type"
                && matches!(&fact.object, RdfObject::Iri(iri) if global_types.contains(&iri.as_str()))
        }) {
            return Err("core registry references an individual outside the global type allowlist".into());
        }
    }
    Ok(())
}

#[derive(Default)]
struct SeedAllowlist {
    subjects: BTreeSet<String>,
    predicates: BTreeSet<String>,
    vocabulary_iris: BTreeSet<String>,
}

fn seed_allowlist() -> Result<SeedAllowlist, Box<dyn Error>> {
    let content = fs::read_to_string(project_path(SEED_ALLOWLIST_REL))?;
    let mut allowlist = SeedAllowlist::default();
    for (line_number, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.split_whitespace();
        let (Some(kind), Some(value), None) = (fields.next(), fields.next(), fields.next()) else {
            return Err(format!(
                "seed allowlist line {} must contain a kind and one IRI",
                line_number + 1
            )
            .into());
        };
        let target = match kind {
            "subject" => &mut allowlist.subjects,
            "predicate" => &mut allowlist.predicates,
            "vocabulary-iri" => &mut allowlist.vocabulary_iris,
            _ => {
                return Err(format!(
                    "seed allowlist line {} has an unknown kind",
                    line_number + 1
                )
                .into());
            }
        };
        if !target.insert(value.to_string()) {
            return Err(format!(
                "seed allowlist line {} duplicates an entry",
                line_number + 1
            )
            .into());
        }
    }
    if allowlist.subjects.is_empty() || allowlist.predicates.is_empty() {
        return Err("seed allowlist must define subjects and predicates".into());
    }
    Ok(allowlist)
}

fn verify_seed_allowlist(facts: &[Fact], allowlist: &SeedAllowlist) -> Result<(), Box<dyn Error>> {
    let source_subjects = facts
        .iter()
        .map(|fact| fact.subject.clone())
        .collect::<BTreeSet<_>>();
    let source_predicates = facts
        .iter()
        .map(|fact| fact.predicate.clone())
        .collect::<BTreeSet<_>>();
    let source_vocabulary_iris = facts
        .iter()
        .filter_map(|fact| match &fact.object {
            RdfObject::Iri(iri) if !source_subjects.contains(iri) => Some(iri.clone()),
            _ => None,
        })
        .collect::<BTreeSet<_>>();

    if source_subjects != allowlist.subjects {
        return Err(
            "source subject set differs from the explicitly reviewed seed allowlist".into(),
        );
    }
    if source_predicates != allowlist.predicates {
        return Err(
            "source predicate set differs from the explicitly reviewed seed allowlist".into(),
        );
    }
    if source_vocabulary_iris != allowlist.vocabulary_iris {
        return Err(
            "source vocabulary IRI set differs from the explicitly reviewed seed allowlist".into(),
        );
    }

    for fact in facts {
        if !allowlist.subjects.contains(&fact.subject) {
            return Err("seed contains a subject outside the explicit allowlist".into());
        }
        if !allowlist.predicates.contains(&fact.predicate) {
            return Err("seed contains a predicate outside the explicit allowlist".into());
        }
        if let RdfObject::Iri(iri) = &fact.object {
            if !allowlist.subjects.contains(iri) && !allowlist.vocabulary_iris.contains(iri) {
                return Err("seed contains an IRI object outside the explicit allowlists".into());
            }
        }
    }
    Ok(())
}

fn verify_seed(facts: &[Fact]) -> Result<(), Box<dyn Error>> {
    let allowlist = seed_allowlist()?;
    verify_seed_allowlist(facts, &allowlist)?;
    verify_seed_safety(facts)
}

fn sql_quote(value: &str) -> String {
    if value.trim_end() != value || value.contains('\n') || value.contains('\r') {
        let hex = value
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02X}"))
            .collect::<String>();
        return format!("CAST(X'{hex}' AS TEXT)");
    }
    format!("'{}'", value.replace('\'', "''"))
}

fn typed_columns(value: &str, datatype: &str) -> (Option<String>, Option<String>, Option<i64>) {
    let local = datatype.rsplit(['#', ':']).next().unwrap_or(datatype);
    if ["integer", "int", "long"].contains(&local) {
        return (None, value.parse::<i64>().ok().map(|v| v.to_string()), None);
    }
    if ["decimal", "double", "float"].contains(&local) {
        return (
            value
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite())
                .map(|v| v.to_string()),
            None,
            None,
        );
    }
    if local == "boolean" {
        return (
            None,
            None,
            Some(if value == "true" || value == "1" {
                1
            } else {
                0
            }),
        );
    }
    (None, None, None)
}

fn sql_for_facts(facts: &[Fact]) -> Result<String, Box<dyn Error>> {
    let mut lines = vec!["-- FOUNDATION curated core ontology seed".to_string(), "-- Generated from tracked ontology/base Turtle sources by ontology-base-pipeline.".to_string(), "BEGIN;".into(), "INSERT OR IGNORE INTO origins (id, name, description) VALUES (101, 'foundation-core-ontology-base', 'Curated immutable ontology seed');".into(), "INSERT OR IGNORE INTO transactions (tx, origin, created_at) VALUES (1, 'ontology-import', 0);".into(), "-- Curated schema, global vocabulary, and semantic component catalog.".into()];
    for batch in facts.chunks(500) {
        let mut rows = Vec::new();
        for fact in batch {
            let (object, value, datatype, lang, kind, number, integer, boolean) = match &fact.object
            {
                RdfObject::Iri(iri) => (
                    Some(iri.as_str()),
                    None,
                    None,
                    None,
                    "iri",
                    None,
                    None,
                    None,
                ),
                RdfObject::Blank(id) => (
                    Some(id.as_str()),
                    None,
                    None,
                    None,
                    "blank",
                    None,
                    None,
                    None,
                ),
                RdfObject::Literal {
                    value,
                    datatype,
                    language,
                } => {
                    let (number, integer, boolean) = typed_columns(value, datatype);
                    (
                        None,
                        Some(value.as_str()),
                        Some(datatype.as_str()),
                        language.as_deref(),
                        "literal",
                        number,
                        integer,
                        boolean,
                    )
                }
            };
            let optional = |v: Option<&str>| v.map(sql_quote).unwrap_or_else(|| "NULL".into());
            let bool_value = boolean
                .map(|v| v.to_string())
                .unwrap_or_else(|| "NULL".into());
            rows.push(format!(
                "({},{},{},{},{},{},{},{},{},{},1,101,0,0)",
                sql_quote(&fact.subject),
                sql_quote(&fact.predicate),
                optional(object),
                optional(value),
                optional(datatype),
                optional(lang),
                sql_quote(kind),
                number.unwrap_or_else(|| "NULL".into()),
                integer.unwrap_or_else(|| "NULL".into()),
                bool_value
            ));
        }
        lines.push("INSERT OR IGNORE INTO triples (subject,predicate,object,object_value,object_datatype,object_language,object_type,object_number,object_integer,object_boolean,tx,origin_id,retracted,created_at) VALUES".into());
        lines.push(format!("{};", rows.join(",\n  ")));
    }
    lines.push("COMMIT;".into());
    Ok(lines.join("\n"))
}

fn verify_import(sql: &str, expected: &[Fact]) -> Result<usize, Box<dyn Error>> {
    let conn = rusqlite::Connection::open_in_memory()?;
    conn.execute_batch(&fs::read_to_string(project_path(DB_SCHEMA_REL))?)?;
    conn.execute_batch(sql)?;
    let count: usize = conn.query_row(
        "SELECT COUNT(*) FROM triples WHERE retracted = 0",
        [],
        |r| r.get(0),
    )?;
    let person: i64 = conn.query_row("SELECT COUNT(*) FROM triples WHERE subject='foundation:Person' AND predicate='rdf:type' AND object='owl:Class' AND retracted=0", [], |r| r.get(0))?;
    if person == 0 {
        return Err("seed import did not include foundation:Person".into());
    }
    let mut statement = conn.prepare(
        "SELECT subject,predicate,object,object_value,object_datatype,object_language,object_type FROM triples WHERE retracted=0",
    )?;
    let rows = statement.query_map([], |row| {
        let subject: String = row.get(0)?;
        let predicate: String = row.get(1)?;
        let object: Option<String> = row.get(2)?;
        let value: Option<String> = row.get(3)?;
        let datatype: Option<String> = row.get(4)?;
        let language: Option<String> = row.get(5)?;
        let kind: String = row.get(6)?;
        let object = match kind.as_str() {
            "iri" => RdfObject::Iri(object.ok_or_else(|| rusqlite::Error::InvalidQuery)?),
            "blank" => RdfObject::Blank(object.ok_or_else(|| rusqlite::Error::InvalidQuery)?),
            "literal" => RdfObject::Literal {
                value: value.ok_or_else(|| rusqlite::Error::InvalidQuery)?,
                datatype: datatype.ok_or_else(|| rusqlite::Error::InvalidQuery)?,
                language,
            },
            _ => return Err(rusqlite::Error::InvalidQuery),
        };
        Ok(Fact {
            subject,
            predicate,
            object,
        })
    })?;
    let actual = rows.collect::<Result<BTreeSet<_>, _>>()?;
    let expected = expected.iter().cloned().collect::<BTreeSet<_>>();
    if actual != expected {
        let missing_facts = expected.difference(&actual).cloned().collect::<Vec<_>>();
        let missing = missing_facts.len();
        let extra = actual.difference(&expected).count();
        let mut datatype_counts = BTreeMap::<String, usize>::new();
        for fact in &missing_facts {
            if let RdfObject::Literal { datatype, .. } = &fact.object {
                *datatype_counts.entry(datatype.clone()).or_default() += 1;
            }
        }
        return Err(format!(
            "seed import changed the curated RDF fact set (missing={missing}, extra={extra}, missing_literal_datatypes={datatype_counts:?})"
        )
        .into());
    }
    Ok(count)
}

fn export_sql() -> Result<(), Box<dyn Error>> {
    let core = project_path(CORE_TTL_REL);
    let schema = project_path(SCHEMA_REL);
    let contracts = project_path(CONTRACTS_REL);
    let mut facts = turtle_files(&[core, schema, contracts])?;
    verify_seed(&facts)?;
    facts.sort();
    let sql = sql_for_facts(&facts)?;
    let count = verify_import(&sql, &facts)?;
    fs::write(project_path(SQL_REL), &sql)?;
    println!("generated_seed_triples={count}");
    Ok(())
}

fn verify_catalog(manifest: &Manifest, facts: &[Fact]) -> Result<(), Box<dyn Error>> {
    let facts_set = facts.iter().cloned().collect::<BTreeSet<_>>();
    let component_ids = manifest
        .components
        .iter()
        .map(|component| component.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut history = BTreeMap::<String, BTreeMap<String, BTreeMap<String, String>>>::new();
    for fact in facts {
        if !fact
            .subject
            .starts_with("foundation:SemanticComponentDefinition_")
        {
            continue;
        }
        if let RdfObject::Literal { value, .. } = &fact.object {
            history
                .entry(fact.subject.clone())
                .or_default()
                .entry(fact.predicate.clone())
                .or_default()
                .insert(value.clone(), value.clone());
        }
    }
    for iri in history.keys() {
        if !facts_set.contains(&Fact {
            subject: iri.clone(),
            predicate: "rdf:type".into(),
            object: RdfObject::Iri("foundation:SemanticComponentDefinition".into()),
        }) {
            return Err(format!(
                "{iri} is missing rdf:type foundation:SemanticComponentDefinition"
            )
            .into());
        }
    }
    let mut seen_versions = BTreeSet::new();
    let mut current_by_id = BTreeMap::<String, (u64, String)>::new();
    for (iri, fields) in &history {
        let get_single = |predicate: &str| -> Result<&str, Box<dyn Error>> {
            let values = fields
                .get(predicate)
                .ok_or_else(|| format!("{iri} is missing {predicate}"))?;
            if values.len() != 1 {
                return Err(format!("{iri} has duplicate values for {predicate}").into());
            }
            Ok(values.keys().next().expect("one value exists").as_str())
        };
        let id = get_single("foundation:componentId")?;
        let version = get_single("foundation:componentContractVersion")?
            .parse::<u64>()
            .map_err(|_| format!("{iri} has an invalid contract version"))?;
        if !component_ids.contains(id) {
            return Err(format!("historical contract {iri} has unknown component ID {id}").into());
        }
        let expected_iri = format!(
            "foundation:SemanticComponentDefinition_{}_v{version}",
            contract_slug(id)
        );
        if *iri != expected_iri || !seen_versions.insert((id.to_string(), version)) {
            return Err(
                format!("historical contract IRI/version is invalid or duplicated: {iri}").into(),
            );
        }
        let props: Value = serde_json::from_str(get_single("foundation:componentPropsSchema")?)?;
        let actions: Value =
            serde_json::from_str(get_single("foundation:componentActionsSchema")?)?;
        let examples: Value = serde_json::from_str(get_single("foundation:componentExamples")?)?;
        let payload = json!({
            "componentId": id,
            "intent": get_single("foundation:componentIntent")?,
            "propsSchema": props,
            "actionsSchema": actions,
            "examples": examples,
        });
        let hash = format!(
            "sha256:{:x}",
            Sha256::digest(canonical_json(&payload)?.as_bytes())
        );
        if get_single("foundation:componentContractHash")? != hash {
            return Err(format!("contract hash verification failed for {iri}").into());
        }
        match current_by_id.get_mut(id) {
            Some(current) if version > current.0 => *current = (version, iri.clone()),
            Some(_) => {}
            None => {
                current_by_id.insert(id.to_string(), (version, iri.clone()));
            }
        }
    }

    for component in &manifest.components {
        let example = manifest
            .examples
            .get(&component.id)
            .ok_or_else(|| format!("missing example for {}", component.id))?;
        let mut props = component.props_schema.clone();
        if !component.slots.is_empty() {
            props
                .as_object_mut()
                .ok_or("propsSchema must be object")?
                .insert(
                    "x-foundation-slots".into(),
                    Value::Array(component.slots.clone()),
                );
        }
        let actions = Value::Array(manifest.actions.clone());
        let iri = format!(
            "foundation:SemanticComponentDefinition_{}_v{}",
            contract_slug(&component.id),
            component.contract_version
        );
        if current_by_id.get(&component.id) != Some(&(component.contract_version, iri.clone())) {
            return Err(format!(
                "current contract version for {} does not match the renderer manifest",
                component.id
            )
            .into());
        }
        let checks = [
            (
                "rdf:type",
                RdfObject::Iri("foundation:SemanticComponentDefinition".into()),
            ),
            (
                "foundation:componentId",
                RdfObject::Literal {
                    value: component.id.clone(),
                    datatype: "xsd:string".into(),
                    language: None,
                },
            ),
            (
                "foundation:componentIntent",
                RdfObject::Literal {
                    value: component.description.clone(),
                    datatype: "xsd:string".into(),
                    language: None,
                },
            ),
            (
                "foundation:componentPropsSchema",
                RdfObject::Literal {
                    value: canonical_json(&props)?,
                    datatype: "xsd:string".into(),
                    language: None,
                },
            ),
            (
                "foundation:componentActionsSchema",
                RdfObject::Literal {
                    value: canonical_json(&actions)?,
                    datatype: "xsd:string".into(),
                    language: None,
                },
            ),
            (
                "foundation:componentExamples",
                RdfObject::Literal {
                    value: canonical_json(example)?,
                    datatype: "xsd:string".into(),
                    language: None,
                },
            ),
            (
                "foundation:componentContractHash",
                RdfObject::Literal {
                    value: contract_hash(component, &manifest.actions, example)?,
                    datatype: "xsd:string".into(),
                    language: None,
                },
            ),
        ];
        for (predicate, object) in checks {
            if !facts_set.contains(&Fact {
                subject: iri.clone(),
                predicate: predicate.into(),
                object,
            }) {
                return Err(format!(
                    "curated ontology contract does not match manifest for {}",
                    component.id
                )
                .into());
            }
        }
        let manifest_version = RdfObject::Literal {
            value: manifest.version.to_string(),
            datatype: "xsd:string".into(),
            language: None,
        };
        if !facts_set.contains(&Fact {
            subject: iri.clone(),
            predicate: "foundation:componentManifestVersion".into(),
            object: manifest_version,
        }) {
            return Err(format!(
                "manifest version for {} differs from the current renderer manifest",
                component.id
            )
            .into());
        }
        if !facts_set.contains(&Fact {
            subject: "foundation:CoreOntologyRegistry".into(),
            predicate: "foundation:includesIndividual".into(),
            object: RdfObject::Iri(iri.clone()),
        }) {
            return Err(format!("core registry does not include {iri}").into());
        }
    }
    let required_schema_facts = [
        (
            "foundation:defaultWidgetSpec",
            "rdf:type",
            RdfObject::Iri("owl:ObjectProperty".into()),
        ),
        (
            "foundation:defaultWidgetSpec",
            "rdfs:domain",
            RdfObject::Iri("owl:Class".into()),
        ),
        (
            "foundation:defaultWidgetSpec",
            "rdfs:range",
            RdfObject::Iri("foundation:JsonRenderSpec".into()),
        ),
    ];
    for (subject, predicate, object) in required_schema_facts {
        if !facts_set.contains(&Fact {
            subject: subject.into(),
            predicate: predicate.into(),
            object,
        }) {
            return Err(
                "foundation:defaultWidgetSpec base schema is missing its type, domain, or range"
                    .into(),
            );
        }
    }
    Ok(())
}

fn verify() -> Result<(), Box<dyn Error>> {
    let manifest = manifest()?;
    if manifest.version != 2 {
        return Err("manifest version must be 2".into());
    }
    let source_paths = [
        project_path(CORE_TTL_REL),
        project_path(SCHEMA_REL),
        project_path(CONTRACTS_REL),
    ];
    let raw_occurrences = turtle_occurrence_count(&source_paths)?;
    let facts = turtle_files(&source_paths)?;
    let mut required_typed_datatypes = BTreeMap::<String, usize>::new();
    for fact in &facts {
        if let RdfObject::Literal { datatype, .. } = &fact.object {
            if [
                "xsd:integer",
                "xsd:int",
                "xsd:long",
                "xsd:decimal",
                "xsd:double",
                "xsd:float",
                "xsd:boolean",
            ]
            .contains(&datatype.as_str())
            {
                *required_typed_datatypes
                    .entry(datatype.clone())
                    .or_default() += 1;
            }
        }
    }
    verify_seed(&facts)?;
    verify_catalog(&manifest, &facts)?;
    let expected = sql_for_facts(&facts)?;
    let actual = fs::read_to_string(project_path(SQL_REL))?;
    if expected != actual {
        return Err("ontology.sql differs from the deterministic Turtle export".into());
    }
    let imported = verify_import(&actual, &facts)?;
    println!("turtle_facts={}", facts.len());
    println!("source_turtle_occurrences={raw_occurrences}");
    println!(
        "source_duplicate_occurrences={}",
        raw_occurrences - facts.len()
    );
    println!("typed_literal_facts_by_datatype={required_typed_datatypes:?}");
    println!("semantic_components={}", manifest.components.len());
    println!(
        "semantic_contract_versions={}",
        manifest
            .components
            .iter()
            .map(|c| c.contract_version)
            .sum::<u64>()
    );
    println!("seed_current_facts={imported}");
    println!("intentional_fact_exclusions=0");
    println!("source_to_seed_missing_facts=0");
    println!("source_to_seed_extra_facts=0");
    println!("seed_personal_secret_scan=PASS");
    println!("clean_database_seed_import=PASS");
    Ok(())
}

fn generate_overlay() -> Result<(), Box<dyn Error>> {
    let manifest = manifest()?;
    if manifest.version != 2 {
        return Err("manifest version must be 2".into());
    }
    write_contract_sources(&manifest)?;
    println!("catalog_components={}", manifest.components.len());
    println!(
        "catalog_contract_versions={}",
        manifest
            .components
            .iter()
            .map(|c| c.contract_version)
            .sum::<u64>()
    );
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let command = std::env::args().nth(1).unwrap_or_else(|| "verify".into());
    match command.as_str() {
        "generate-overlay" => generate_overlay(),
        "export-sql" => export_sql(),
        "verify" => verify(),
        other => Err(format!(
            "unknown command {other}; use generate-overlay, export-sql, or verify"
        )
        .into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use foundation_core::eavto::{
        Object, Triple, query::get_by_entity_predicate, store::append_triples,
    };
    use std::sync::OnceLock;

    fn curated_facts() -> &'static Vec<Fact> {
        static FACTS: OnceLock<Vec<Fact>> = OnceLock::new();
        FACTS.get_or_init(|| {
            turtle_files(&[
                project_path(CORE_TTL_REL),
                project_path(SCHEMA_REL),
                project_path(CONTRACTS_REL),
            ])
            .expect("curated ontology Turtle sources must parse")
        })
    }

    #[test]
    fn information_contract_may_start_at_version_zero() {
        assert_eq!(validate_contract_version("Information", 0), Ok(()));
    }

    #[test]
    fn other_component_contracts_must_start_at_a_positive_version() {
        let error = validate_contract_version("Comparison", 0).unwrap_err();
        assert!(error.contains("positive contractVersion"));
        assert_eq!(validate_contract_version("Comparison", 1), Ok(()));
    }

    #[test]
    fn committed_manifest_matches_the_versioned_component_and_action_catalog() {
        let manifest = manifest().unwrap();
        let expected_versions = BTreeMap::from([
            ("Action", 1_u64),
            ("Comparison", 2),
            ("Confirmation", 1),
            ("Decision", 1),
            ("Details", 2),
            ("Distribution", 1),
            ("Information", 0),
            ("ValueEntry", 2),
        ]);
        let component_ids = manifest
            .components
            .iter()
            .map(|component| component.id.as_str())
            .collect::<BTreeSet<_>>();
        let action_ids = manifest
            .actions
            .iter()
            .filter_map(|action| action.get("id").and_then(Value::as_str))
            .collect::<BTreeSet<_>>();
        let example_ids = manifest
            .examples
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();

        assert_eq!(manifest.version, 2);
        assert_eq!(component_ids, expected_versions.keys().copied().collect());
        assert_eq!(
            action_ids,
            [
                "announce",
                "perform",
                "pushState",
                "removeState",
                "setState",
                "validateForm",
            ]
            .into_iter()
            .collect()
        );
        assert_eq!(example_ids, component_ids);
        for component in manifest.components {
            assert_eq!(
                Some(&component.contract_version),
                expected_versions.get(component.id.as_str()),
                "unexpected contract version for {}",
                component.id
            );
            assert_eq!(
                component.props_schema.get("type").and_then(Value::as_str),
                Some("object")
            );
            assert!(
                component
                    .props_schema
                    .get("properties")
                    .and_then(Value::as_object)
                    .is_some()
            );
            assert!(
                component
                    .props_schema
                    .get("required")
                    .and_then(Value::as_array)
                    .is_some()
            );
            let example = manifest.examples.get(&component.id).unwrap();
            let elements = example.get("elements").and_then(Value::as_object).unwrap();
            let root = example.get("root").and_then(Value::as_str).unwrap();
            assert_eq!(
                elements
                    .get(root)
                    .and_then(|element| element.get("type"))
                    .and_then(Value::as_str),
                Some(component.id.as_str()),
                "example root must instantiate component {}",
                component.id
            );
        }
    }

    #[test]
    fn generated_seed_is_deterministic_and_cleanly_importable() {
        let facts = curated_facts();
        verify_seed(facts).unwrap();
        let first = sql_for_facts(facts).unwrap();
        let second = sql_for_facts(facts).unwrap();
        assert_eq!(first, second);
        assert_eq!(verify_import(&first, facts).unwrap(), facts.len());
    }

    #[test]
    fn canonical_seed_matches_explicit_subject_predicate_and_vocabulary_allowlists() {
        verify_seed(curated_facts()).unwrap();
    }

    #[test]
    fn explicit_allowlist_rejects_untimestamped_and_timestamped_local_subjects() {
        let allowlist = seed_allowlist().unwrap();
        for local in [
            "foundation:DanielTerra",
            "foundation:PrivateRecord_20261112",
        ] {
            let fact = Fact {
                subject: local.into(),
                predicate: "rdf:type".into(),
                object: RdfObject::Iri("owl:NamedIndividual".into()),
            };
            let error = verify_seed_allowlist(&[fact], &allowlist)
                .unwrap_err()
                .to_string();
            assert!(error.contains("explicitly reviewed seed allowlist"));
            assert!(!error.contains(local));
        }
    }

    #[test]
    fn explicit_allowlist_rejects_unapproved_predicates_and_iri_objects() {
        let allowlist = seed_allowlist().unwrap();
        let mut predicate_facts = curated_facts().clone();
        predicate_facts.push(Fact {
            subject: "foundation:Person".into(),
            predicate: "foundation:privateNote".into(),
            object: RdfObject::Literal {
                value: "local data".into(),
                datatype: "xsd:string".into(),
                language: None,
            },
        });
        assert!(
            verify_seed_allowlist(&predicate_facts, &allowlist)
                .unwrap_err()
                .to_string()
                .contains("predicate set differs")
        );

        let mut object_facts = curated_facts().clone();
        object_facts.push(Fact {
            subject: "foundation:Person".into(),
            predicate: "rdf:type".into(),
            object: RdfObject::Iri("foundation:PrivatePerson".into()),
        });
        let error = verify_seed_allowlist(&object_facts, &allowlist)
            .unwrap_err()
            .to_string();
        assert!(error.contains("vocabulary IRI set differs"));
        assert!(!error.contains("PrivatePerson"));
    }

    #[test]
    fn allowlist_does_not_expand_from_new_source_facts() {
        let mut facts = curated_facts().clone();
        let count_before = seed_allowlist().unwrap().subjects.len();
        facts.push(Fact {
            subject: "foundation:NewPersonalRecord".into(),
            predicate: "rdf:type".into(),
            object: RdfObject::Iri("owl:NamedIndividual".into()),
        });
        let allowlist = seed_allowlist().unwrap();
        assert_eq!(allowlist.subjects.len(), count_before);
        assert!(verify_seed_allowlist(&facts, &allowlist).is_err());
    }

    #[test]
    fn unsupported_integer_datatypes_keep_lexical_value_without_typed_column() {
        for datatype in [
            "xsd:short",
            "xsd:byte",
            "xsd:nonNegativeInteger",
            "xsd:positiveInteger",
        ] {
            assert_eq!(
                typed_columns("7", datatype),
                (None, None, None),
                "{datatype}"
            );
        }
        for datatype in ["xsd:integer", "xsd:int", "xsd:long"] {
            assert_eq!(typed_columns("7", datatype), (None, Some("7".into()), None));
        }
    }

    #[test]
    fn sql_serialization_preserves_trailing_whitespace_without_sql_trailing_space() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        for value in ["curated literal  \n", "first line \nsecond line"] {
            let expression = sql_quote(value);
            assert!(!expression.chars().last().unwrap().is_whitespace());
            assert!(!expression.contains('\n'));
            let round_trip: String = conn
                .query_row(&format!("SELECT {expression}"), [], |row| row.get(0))
                .unwrap();
            assert_eq!(round_trip, value);
        }
    }

    #[test]
    fn contract_hashes_and_latest_versioned_contracts_match_source() {
        let manifest = manifest().unwrap();
        let facts = curated_facts();
        verify_catalog(&manifest, facts).unwrap();
    }

    #[test]
    fn catalog_verifier_rejects_untyped_historical_contracts() {
        let manifest = manifest().unwrap();
        let current_iris = manifest
            .components
            .iter()
            .map(|component| {
                format!(
                    "foundation:SemanticComponentDefinition_{}_v{}",
                    contract_slug(&component.id),
                    component.contract_version
                )
            })
            .collect::<BTreeSet<_>>();
        let historical_iri = curated_facts()
            .iter()
            .map(|fact| fact.subject.as_str())
            .find(|iri| {
                iri.starts_with("foundation:SemanticComponentDefinition_")
                    && !current_iris.contains(*iri)
            })
            .expect("curated ontology must include a historical component contract")
            .to_string();
        let missing_type = curated_facts()
            .iter()
            .filter(|fact| {
                !(fact.subject == historical_iri
                    && fact.predicate == "rdf:type"
                    && matches!(
                        &fact.object,
                        RdfObject::Iri(iri)
                            if iri == "foundation:SemanticComponentDefinition"
                    ))
            })
            .cloned()
            .collect::<Vec<_>>();

        let error = verify_catalog(&manifest, &missing_type)
            .unwrap_err()
            .to_string();
        assert!(error.contains("missing rdf:type"));
    }

    #[test]
    fn generated_overlay_types_preserved_historical_contracts() {
        let manifest = manifest().unwrap();
        let component = manifest
            .components
            .iter()
            .find(|component| component.contract_version > 1)
            .expect("manifest must include a component with historical contracts");
        let historical_version = component.contract_version - 1;
        let historical_iri = format!(
            "foundation:SemanticComponentDefinition_{}_v{historical_version}",
            contract_slug(&component.id)
        );
        let previous = Overlay {
            format_version: 1,
            manifest_version: manifest.version,
            triples: vec![
                make_triple(
                    &historical_iri,
                    "foundation:componentId",
                    literal(component.id.clone()),
                ),
                make_triple(
                    &historical_iri,
                    "foundation:componentContractVersion",
                    literal(historical_version.to_string()),
                ),
                make_triple(
                    "foundation:CoreOntologyRegistry",
                    "foundation:includesIndividual",
                    OverlayObject::Iri {
                        value: historical_iri.clone(),
                    },
                ),
            ],
        };

        let generated = component_triples(&manifest, &previous).unwrap();
        assert!(generated.iter().any(|triple| {
            triple.subject == historical_iri
                && triple.predicate == "rdf:type"
                && matches!(
                    &triple.object,
                    OverlayObject::Iri { value }
                        if value == "foundation:SemanticComponentDefinition"
                )
        }));
    }

    #[test]
    fn canonical_contract_hashes_match_shared_golden_vectors() {
        let manifest = manifest().unwrap();
        let fixture: Value =
            serde_json::from_slice(&fs::read(project_path("contract-hash-golden.json")).unwrap())
                .unwrap();
        let expected = fixture["hashes"].as_object().unwrap();
        for component in &manifest.components {
            let example = &manifest.examples[&component.id];
            assert_eq!(
                contract_hash(component, &manifest.actions, example).unwrap(),
                expected[&component.id]["hash"].as_str().unwrap(),
                "{} golden hash",
                component.id
            );
        }
    }

    #[test]
    fn stale_intent_or_example_is_rejected_by_hash_and_manifest_verification() {
        let source_manifest = manifest().unwrap();
        let facts = curated_facts();

        let mut changed_intent = manifest().unwrap();
        changed_intent.components[0].description.push_str(" stale");
        assert!(verify_catalog(&changed_intent, facts).is_err());

        let mut changed_example = source_manifest;
        changed_example.examples.get_mut("Action").unwrap()["state"] = json!({"stale": true});
        assert!(verify_catalog(&changed_example, facts).is_err());
    }

    #[test]
    fn timestamped_local_iris_are_rejected_in_subject_predicate_and_object() {
        let local = "foundation:LocalFixture_20261112";
        let fixtures = [
            Fact {
                subject: local.into(),
                predicate: "rdf:type".into(),
                object: RdfObject::Iri("owl:Class".into()),
            },
            Fact {
                subject: "foundation:Safe".into(),
                predicate: local.into(),
                object: RdfObject::Literal {
                    value: "safe".into(),
                    datatype: "xsd:string".into(),
                    language: None,
                },
            },
            Fact {
                subject: "foundation:Safe".into(),
                predicate: "rdf:type".into(),
                object: RdfObject::Iri(local.into()),
            },
        ];
        for fact in fixtures {
            let error = verify_seed_safety(&[fact]).unwrap_err().to_string();
            assert!(error.contains("unapproved timestamped/local IRI"));
            assert!(!error.contains(local));
        }
    }

    #[test]
    fn approved_timestamped_core_iris_are_allowed() {
        let facts = vec![Fact {
            subject: "foundation:Status_1772600993751".into(),
            predicate: "rdf:type".into(),
            object: RdfObject::Iri("owl:NamedIndividual".into()),
        }];
        verify_seed_safety(&facts).unwrap();
    }

    #[test]
    fn removed_local_property_automation_and_qa_fixtures_stay_out_of_core() {
        let removed = [
            "Property_1780936859653",
            "Automation_1776641130838",
            "_20261104",
        ];
        for fact in curated_facts() {
            let text = match &fact.object {
                RdfObject::Iri(value) | RdfObject::Blank(value) => value.as_str(),
                RdfObject::Literal { value, .. } => value.as_str(),
            };
            assert!(
                !removed.iter().any(|token| {
                    fact.subject.contains(token)
                        || fact.predicate.contains(token)
                        || text.contains(token)
                }),
                "curated core must not reintroduce a removed local fixture"
            );
        }
    }

    #[test]
    fn seed_guard_rejects_personal_and_secret_material() {
        let facts = vec![Fact {
            subject: "foundation:Safe".into(),
            predicate: "rdfs:comment".into(),
            object: RdfObject::Literal {
                value: "person@example.com".into(),
                datatype: "xsd:string".into(),
                language: None,
            },
        }];
        assert!(verify_seed_safety(&facts).is_err());
    }

    #[test]
    fn packaged_seed_and_overlay_sync_preserve_local_specs_idempotently() {
        let mut conn = foundation_core::eavto::Connection::open_in_memory().unwrap();
        conn.execute_batch(&fs::read_to_string(project_path(DB_SCHEMA_REL)).unwrap())
            .unwrap();
        conn.execute_batch(&fs::read_to_string(project_path(SQL_REL)).unwrap())
            .unwrap();
        let initial_sync = foundation_core::semantic_catalog_overlay::sync(&mut conn).unwrap();
        assert!(
            !initial_sync.changed,
            "base seed already contains the packaged catalog"
        );

        let contract_iri = "foundation:SemanticComponentDefinition_Information_v0";
        let contract_before =
            get_by_entity_predicate(&conn, contract_iri, "foundation:componentPropsSchema")
                .unwrap();
        assert_eq!(contract_before.count, 1);
        let contract_tx_before = contract_before.triples[0].tx;

        let local_spec_iri = "foundation:JsonRenderSpec_VerificationSeedUpgrade";
        let spec_source = r#"{"root":"root","elements":{"root":{"type":"Information","children":[],"props":{"title":"Local fixture"}}}}"#;
        append_triples(
            &mut conn,
            &[
                Triple::new(
                    local_spec_iri,
                    "rdf:type",
                    Object::Iri("foundation:JsonRenderSpec".into()),
                ),
                Triple::new(
                    local_spec_iri,
                    "foundation:specSource",
                    Object::Literal {
                        value: spec_source.into(),
                        datatype: Some("xsd:string".into()),
                        language: None,
                    },
                ),
            ],
            "verification-fixture",
        )
        .unwrap();

        let repeated_sync = foundation_core::semantic_catalog_overlay::sync(&mut conn).unwrap();
        assert!(
            !repeated_sync.changed,
            "identical catalog sync must be a no-op"
        );
        let contract_after =
            get_by_entity_predicate(&conn, contract_iri, "foundation:componentPropsSchema")
                .unwrap();
        assert_eq!(
            contract_after.count, 1,
            "catalog sync must not duplicate contract facts"
        );
        assert_eq!(
            contract_after.triples[0].tx, contract_tx_before,
            "no-op sync must not write a new contract revision"
        );
        let local_spec =
            get_by_entity_predicate(&conn, local_spec_iri, "foundation:specSource").unwrap();
        assert_eq!(local_spec.triples.len(), 1);
        assert_eq!(
            local_spec.triples[0].object.as_literal().as_deref(),
            Some(spec_source)
        );
    }
}
