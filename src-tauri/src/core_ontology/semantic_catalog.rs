use crate::owl::{
    self, Class, ClassType, Connection, Individual, Object, Property, PropertyType, Triple,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};

const COMPONENT_CLASS: &str = "foundation:SemanticComponentDefinition";
const CORE_REGISTRY: &str = "foundation:CoreOntologyRegistry";
const CORE_INDIVIDUAL: &str = "foundation:includesIndividual";
const COMPONENT_ID: &str = "foundation:componentId";
const COMPONENT_INTENT: &str = "foundation:componentIntent";
const COMPONENT_PROPS_SCHEMA: &str = "foundation:componentPropsSchema";
const COMPONENT_ACTIONS_SCHEMA: &str = "foundation:componentActionsSchema";
const COMPONENT_EXAMPLES: &str = "foundation:componentExamples";
const COMPONENT_MANIFEST_VERSION: &str = "foundation:componentManifestVersion";
const CONTRACT_VERSION: &str = "foundation:componentContractVersion";
const CONTRACT_HASH: &str = "foundation:componentContractHash";
const SPEC_CLASS: &str = "foundation:JsonRenderSpec";
const CLASS_DEFAULT_WIDGET_SPEC: &str = "foundation:defaultWidgetSpec";
const SPEC_SOURCE: &str = "foundation:specSource";
const SPEC_CONTRACT: &str = "foundation:usesComponentContract";
const SPEC_RENDERS_ENTITY: &str = "foundation:rendersEntity";
const SYNC_ORIGIN: &str = "semantic_catalog_sync";
const SCHEMA_SYNC_ORIGIN: &str = "core_ontology_sync";
const MANIFEST: &str = include_str!(concat!(env!("OUT_DIR"), "/semantic_catalog_manifest.json"));

#[derive(Debug, Clone, Deserialize)]
struct SemanticCatalogManifest {
    version: u64,
    components: Vec<SemanticComponent>,
    actions: Vec<Value>,
    examples: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Deserialize)]
struct SemanticComponent {
    id: String,
    #[serde(rename = "contractVersion")]
    contract_version: u64,
    description: String,
    #[serde(rename = "propsSchema")]
    props_schema: Value,
    slots: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ComponentContract {
    id: String,
    intent: String,
    props_schema: String,
    actions_schema: String,
    examples: String,
    manifest_version: String,
    hash: String,
}

#[derive(Debug, Clone)]
struct StoredContract {
    iri: String,
    version: Option<u64>,
    contract: ComponentContract,
    legacy_action_wrapper: bool,
    integrity_valid: bool,
    stored_hash: Option<String>,
    integrity_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncSummary {
    pub contract_versions_created: usize,
    pub registry_links_added: usize,
    pub registry_contracts_verified: usize,
    pub specs_requiring_update: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticSpecCompatibility {
    pub spec_iri: String,
    pub spec_source: String,
    pub renders_entity: String,
    pub snapshot_tx: i64,
    pub component_compatibility: Vec<SemanticComponentCompatibility>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticComponentCompatibility {
    pub component_id: String,
    pub contract_iri: Option<String>,
    pub pinned_version: Option<u64>,
    pub target_version: Option<u64>,
    pub status: String,
    pub reason: String,
}

#[derive(Debug, Default)]
struct SpecRepinPlan {
    repins: Vec<(String, Vec<String>)>,
    issues: Vec<String>,
}

#[derive(Debug)]
struct InformationResetPlan {
    contract: ComponentContract,
    obsolete_contract_iris: Vec<String>,
    obsolete_registry_iris: Vec<String>,
    obsolete_spec_iris: Vec<String>,
    managed_spec_iris: Vec<String>,
    spec_repins: Vec<InformationSpecMigration>,
}

#[derive(Debug)]
struct InformationSpecMigration {
    iri: String,
    obsolete_pins: Vec<String>,
    source_update: Option<String>,
}

/// Installs the app-owned semantic catalog vocabulary after foundation-core's base seed.
/// This is the canonical schema sync for both new and existing local databases.
pub fn sync_schema(conn: &mut Connection) -> Result<(), String> {
    if !Class::exists(conn, COMPONENT_CLASS) {
        Class::new(COMPONENT_CLASS)
            .assert(
                conn,
                ClassType::OwlClass,
                "Semantic Component Definition",
                "widgets",
                Some("foundation:AbstractContinuant"),
                SCHEMA_SYNC_ORIGIN,
            )
            .map_err(|error| error.to_string())?;
    }
    if !Class::exists(conn, SPEC_CLASS) {
        return Err(format!(
            "Ontology class {SPEC_CLASS} is missing from the foundation-core base ontology"
        ));
    }

    for (iri, label, property_type, range, domains) in [
        (
            COMPONENT_ID,
            "Component ID",
            PropertyType::DatatypeProperty,
            "xsd:string",
            vec![COMPONENT_CLASS],
        ),
        (
            COMPONENT_INTENT,
            "Component Intent",
            PropertyType::DatatypeProperty,
            "xsd:string",
            vec![COMPONENT_CLASS],
        ),
        (
            COMPONENT_PROPS_SCHEMA,
            "Component Props Schema",
            PropertyType::DatatypeProperty,
            "xsd:string",
            vec![COMPONENT_CLASS],
        ),
        (
            COMPONENT_ACTIONS_SCHEMA,
            "Component Actions Schema",
            PropertyType::DatatypeProperty,
            "xsd:string",
            vec![COMPONENT_CLASS],
        ),
        (
            COMPONENT_EXAMPLES,
            "Component Examples",
            PropertyType::DatatypeProperty,
            "xsd:string",
            vec![COMPONENT_CLASS],
        ),
        (
            COMPONENT_MANIFEST_VERSION,
            "Component Manifest Version",
            PropertyType::DatatypeProperty,
            "xsd:string",
            vec![COMPONENT_CLASS],
        ),
        (
            CONTRACT_VERSION,
            "Component Contract Version",
            PropertyType::DatatypeProperty,
            "xsd:string",
            vec![COMPONENT_CLASS],
        ),
        (
            CONTRACT_HASH,
            "Component Contract Hash",
            PropertyType::DatatypeProperty,
            "xsd:string",
            vec![COMPONENT_CLASS],
        ),
        (
            SPEC_CONTRACT,
            "Uses Component Contract",
            PropertyType::ObjectProperty,
            COMPONENT_CLASS,
            vec![SPEC_CLASS],
        ),
        (
            CLASS_DEFAULT_WIDGET_SPEC,
            "Default Widget Specification",
            PropertyType::ObjectProperty,
            SPEC_CLASS,
            vec!["owl:Class"],
        ),
    ] {
        ensure_property_schema(conn, iri, label, property_type, range, &domains)?;
    }

    ensure_registry_schema(conn)?;
    Ok(())
}

fn ensure_property_schema(
    conn: &mut Connection,
    iri: &str,
    label: &str,
    property_type: PropertyType,
    range: &str,
    domains: &[&str],
) -> Result<(), String> {
    let Some(property) = Property::get(conn, iri).map_err(|error| error.to_string())? else {
        Property::new(iri)
            .assert(
                conn,
                property_type,
                label,
                Some("Semantic json-render component catalog schema"),
                domains,
                Some(range),
                None,
                SCHEMA_SYNC_ORIGIN,
            )
            .map_err(|error| error.to_string())?;
        return Ok(());
    };

    if property.property_type != property_type {
        return Err(format!(
            "Ontology property {iri} has type {:?}; expected {:?}",
            property.property_type, property_type
        ));
    }
    if property.ranges.iter().any(|existing| existing != range) {
        return Err(format!(
            "Ontology property {iri} has incompatible ranges {:?}; expected only {range}",
            property.ranges
        ));
    }
    if property
        .domains
        .iter()
        .any(|existing| !domains.contains(&existing.as_str()))
    {
        return Err(format!(
            "Ontology property {iri} has incompatible domains {:?}; expected only {:?}",
            property.domains, domains
        ));
    }

    let mut missing = Vec::new();
    for domain in domains {
        if !property.domains.iter().any(|existing| existing == domain) {
            missing.push(Triple::new(
                iri,
                "rdfs:domain",
                Object::Iri((*domain).to_string()),
            ));
        }
    }
    if property.ranges.is_empty() {
        missing.push(Triple::new(
            iri,
            "rdfs:range",
            Object::Iri(range.to_string()),
        ));
    }
    if !missing.is_empty() {
        owl::assert_raw_triples_with_tx(conn, &missing, SCHEMA_SYNC_ORIGIN)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn ensure_registry_schema(conn: &mut Connection) -> Result<(), String> {
    if Individual::get(conn, CORE_REGISTRY)
        .map_err(|error| error.to_string())?
        .is_none()
    {
        return Err(format!(
            "Ontology individual {CORE_REGISTRY} is missing from the foundation-core base ontology"
        ));
    }
    if Property::get(conn, CORE_INDIVIDUAL)
        .map_err(|error| error.to_string())?
        .is_none()
    {
        return Err(format!(
            "Ontology property {CORE_INDIVIDUAL} is missing from the foundation-core base ontology"
        ));
    }
    Ok(())
}

pub fn sync(conn: &mut Connection) -> Result<SyncSummary, String> {
    let manifest: SemanticCatalogManifest = serde_json::from_str(MANIFEST)
        .map_err(|error| format!("Generated semantic catalog manifest is invalid: {error}"))?;
    sync_manifest(conn, manifest)
}

fn sync_manifest(
    conn: &mut Connection,
    manifest: SemanticCatalogManifest,
) -> Result<SyncSummary, String> {
    owl::with_transaction(conn, |conn| sync_manifest_in_transaction(conn, manifest))
}

fn sync_manifest_in_transaction(
    conn: &mut Connection,
    manifest: SemanticCatalogManifest,
) -> Result<SyncSummary, String> {
    for property in [
        COMPONENT_ID,
        COMPONENT_INTENT,
        COMPONENT_PROPS_SCHEMA,
        COMPONENT_ACTIONS_SCHEMA,
        COMPONENT_EXAMPLES,
        COMPONENT_MANIFEST_VERSION,
        CONTRACT_VERSION,
        CONTRACT_HASH,
    ] {
        if owl::Property::get(conn, property)
            .map_err(|error| error.to_string())?
            .is_none()
        {
            return Err(format!(
                "Ontology property {property} is missing after semantic catalog schema sync"
            ));
        }
    }
    if !owl::Class::exists(conn, COMPONENT_CLASS) {
        return Err(format!(
            "Ontology class {COMPONENT_CLASS} is missing after semantic catalog schema sync"
        ));
    }

    let actions_schema = canonical_json(&Value::Array(manifest.actions.clone()))?;
    let mut sanitized_ids = HashSet::new();
    let mut planned_components = Vec::with_capacity(manifest.components.len());

    for component in manifest.components {
        let sanitized = sanitize_component_id(&component.id);
        if !sanitized_ids.insert(sanitized) {
            return Err(format!(
                "Semantic component IDs collide after IRI normalization: {}",
                component.id
            ));
        }
        let examples = manifest
            .examples
            .get(&component.id)
            .ok_or_else(|| format!("Missing semantic example for component {}", component.id))?;
        let declared_version = component.contract_version;
        let current = make_contract(
            component.id.clone(),
            component.description,
            component.props_schema,
            component.slots,
            actions_schema.clone(),
            examples.clone(),
            manifest.version.to_string(),
        )?;
        planned_components.push((current, declared_version));
    }

    let information_reset = planned_components
        .iter()
        .find(|(contract, version)| contract.id == "Information" && *version == 0)
        .map(|(contract, _)| plan_information_reset(conn, contract))
        .transpose()?;

    let mut component_issues = Vec::new();
    for (current, declared_version) in &planned_components {
        if current.id == "Information" && *declared_version == 0 {
            continue;
        }
        if let Err(error) = validate_component_sync(conn, current, *declared_version) {
            component_issues.push(component_sync_diagnostic(
                conn,
                current,
                *declared_version,
                &error,
            )?);
        }
    }
    if !component_issues.is_empty() {
        return Err(format!(
            "Semantic catalog sync found {} component contract issue(s); no catalog writes were made:\n- {}",
            component_issues.len(),
            component_issues.join("\n- ")
        ));
    }
    let legacy_contract_repins = plan_legacy_contract_repins(conn, &planned_components)?;
    let spec_repin_plan = plan_compatible_spec_repins(
        conn,
        &planned_components,
        &legacy_contract_repins,
        information_reset.is_some(),
    )?;

    let mut changed = 0;
    let reset_spec_iris = information_reset
        .as_ref()
        .map(|reset| {
            reset
                .managed_spec_iris
                .iter()
                .map(String::as_str)
                .collect::<HashSet<_>>()
        })
        .unwrap_or_default();
    if let Some(reset) = &information_reset {
        retract_iri_links(
            conn,
            CORE_REGISTRY,
            CORE_INDIVIDUAL,
            &reset.obsolete_registry_iris,
        )?;
        for spec_iri in &reset.obsolete_spec_iris {
            Individual::retract(conn, spec_iri, SYNC_ORIGIN).map_err(|error| error.to_string())?;
        }
        for iri in &reset.obsolete_contract_iris {
            Individual::retract(conn, iri, SYNC_ORIGIN).map_err(|error| error.to_string())?;
        }
        let version_zero_iri = component_iri("Information", 0);
        if !Individual::get(conn, &version_zero_iri)
            .map_err(|error| error.to_string())?
            .is_some()
        {
            create_contract(conn, &reset.contract, 0)?;
            changed += 1;
        }
        for spec_migration in &reset.spec_repins {
            if let Some(source) = &spec_migration.source_update {
                replace_literal_property(conn, &spec_migration.iri, SPEC_SOURCE, source)?;
            }
            retract_iri_links(
                conn,
                &spec_migration.iri,
                SPEC_CONTRACT,
                &spec_migration.obsolete_pins,
            )?;
            let version_zero_iri = component_iri("Information", 0);
            if !owl::get_all_iri_properties(conn, &spec_migration.iri, SPEC_CONTRACT)
                .map_err(|error| error.to_string())?
                .contains(&version_zero_iri)
            {
                Individual::new(&spec_migration.iri)
                    .append_property(
                        conn,
                        SPEC_CONTRACT,
                        vec![Object::Iri(version_zero_iri)],
                        SYNC_ORIGIN,
                    )
                    .map_err(|error| error.to_string())?;
            }
        }
    }
    for (current, declared_version) in planned_components {
        if current.id == "Information" && declared_version == 0 {
            continue;
        }
        changed += sync_component(conn, current, declared_version)?;
    }
    for (spec_iri, contracts) in &spec_repin_plan.repins {
        if reset_spec_iris.contains(spec_iri.as_str()) {
            continue;
        }
        set_spec_contracts(conn, spec_iri, contracts)?;
    }

    let (registry_links_added, registry_contracts_verified) = sync_core_registry(conn)?;

    Ok(SyncSummary {
        contract_versions_created: changed,
        registry_links_added,
        registry_contracts_verified,
        specs_requiring_update: spec_repin_plan.issues,
    })
}

fn component_sync_diagnostic(
    conn: &Connection,
    current: &ComponentContract,
    declared_version: u64,
    reason: &str,
) -> Result<String, String> {
    let existing = Individual::find_by_class_and_properties(
        conn,
        COMPONENT_CLASS,
        &[(COMPONENT_ID, &current.id)],
    )
    .map_err(|error| error.to_string())?
    .into_iter()
    .map(|iri| match read_contract_diagnostic(conn, &iri) {
        Ok((record, _)) => format!(
            "{{iri:{},version:{:?},storedHash:{},differingFields:{:?}}}",
            record.iri,
            record.version,
            record.contract.hash,
            definition_differences(&record.contract, current)
        ),
        Err(error) => format!("{{iri:{iri},diagnosticError:{error}}}"),
    })
    .collect::<Vec<_>>();
    Ok(format!(
        "componentId={}, declaredVersion={}, currentManifestHash={}, existingContracts=[{}]; {reason}",
        current.id,
        declared_version,
        current.hash,
        existing.join(",")
    ))
}

fn sync_core_registry(conn: &mut Connection) -> Result<(usize, usize), String> {
    if Individual::get(conn, CORE_REGISTRY)
        .map_err(|error| error.to_string())?
        .is_none()
    {
        return Err(format!(
            "Ontology individual {CORE_REGISTRY} is missing; run the ontology bootstrap before syncing semantic contracts"
        ));
    }
    if owl::Property::get(conn, CORE_INDIVIDUAL)
        .map_err(|error| error.to_string())?
        .is_none()
    {
        return Err(format!(
            "Ontology property {CORE_INDIVIDUAL} is missing; update the core ontology before syncing semantic contracts"
        ));
    }

    let contracts =
        Class::get_instances(conn, COMPONENT_CLASS).map_err(|error| error.to_string())?;
    let versioned_contracts = contracts
        .into_iter()
        .filter_map(|iri| match read_contract_diagnostic(conn, &iri) {
            Ok((contract, _)) if contract.version.is_some() => Some(Ok(iri)),
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let included = owl::get_all_iri_properties(conn, CORE_REGISTRY, CORE_INDIVIDUAL)
        .map_err(|error| error.to_string())?;
    let missing = missing_contract_iris(&versioned_contracts, &included);
    let missing_count = missing.len();
    if !missing.is_empty() {
        Individual::new(CORE_REGISTRY)
            .append_property(
                conn,
                CORE_INDIVIDUAL,
                missing.into_iter().map(Object::Iri).collect(),
                SYNC_ORIGIN,
            )
            .map_err(|error| error.to_string())?;
    }

    let included_after_sync = owl::get_all_iri_properties(conn, CORE_REGISTRY, CORE_INDIVIDUAL)
        .map_err(|error| error.to_string())?;
    ensure_registry_complete(&versioned_contracts, &included_after_sync)?;

    Ok((missing_count, versioned_contracts.len()))
}

fn missing_contract_iris(versioned_contracts: &[String], included: &[String]) -> Vec<String> {
    versioned_contracts
        .iter()
        .filter(|iri| !included.contains(*iri))
        .cloned()
        .collect()
}

fn ensure_registry_complete(
    versioned_contracts: &[String],
    included: &[String],
) -> Result<(), String> {
    let missing = missing_contract_iris(versioned_contracts, included);
    if missing.is_empty() {
        return Ok(());
    }
    Err(format!(
        "Core ontology registry sync did not persist all versioned component contracts: {}",
        missing.join(", ")
    ))
}

fn sync_component(
    conn: &mut Connection,
    current: ComponentContract,
    declared_version: u64,
) -> Result<usize, String> {
    validate_component_sync(conn, &current, declared_version)?;
    repair_semantically_identical_contract_hashes(conn, &current, declared_version)?;
    let records = list_component_contracts_diagnostic(conn, &current.id)?;
    migrate_legacy_action_wrappers(conn, &records, declared_version)?;
    let records = list_component_contracts_diagnostic(conn, &current.id)?;
    let versioned: Vec<&StoredContract> = records
        .iter()
        .filter(|record| record.version.is_some())
        .collect();
    let highest_version = versioned.iter().filter_map(|record| record.version).max();
    if let Some(existing) = versioned
        .iter()
        .find(|record| record.version == Some(declared_version))
    {
        if !same_definition(&existing.contract, &current) {
            return Err(format!(
                "Semantic component {} contractVersion {} is immutable and differs from the generated manifest; increment contractVersion explicitly",
                current.id, declared_version
            ));
        }
        if highest_version.is_some_and(|version| version > declared_version) {
            return Err(format!(
                "Semantic component {} manifest declares stale contractVersion {}; ontology already contains version {}",
                current.id,
                declared_version,
                highest_version.unwrap_or_default()
            ));
        }
        return Ok(0);
    }
    create_contract(conn, &current, declared_version)?;
    Ok(1)
}

fn validate_component_sync(
    conn: &Connection,
    current: &ComponentContract,
    declared_version: u64,
) -> Result<(), String> {
    if declared_version == 0 && current.id != "Information" {
        return Err(format!(
            "Only the Information semantic contract may use reset baseline contractVersion 0; {} is invalid",
            current.id
        ));
    }
    let iris = Individual::find_by_class_and_properties(
        conn,
        COMPONENT_CLASS,
        &[(COMPONENT_ID, &current.id)],
    )
    .map_err(|error| error.to_string())?;
    let mut records = Vec::with_capacity(iris.len());
    for iri in iris {
        let (record, integrity_error) = read_contract_diagnostic(conn, &iri)?;
        if let Some(error) = integrity_error.filter(|_| record.version == Some(declared_version)) {
            let differing_fields = definition_differences(&record.contract, current);
            if record.version != Some(declared_version)
                || !differing_fields.is_empty()
                || !same_definition(&record.contract, current)
                || record.contract.hash != current.hash
            {
                return Err(format!(
                    "{error}, currentManifestHash={}, currentManifestDifferingFields={differing_fields:?}; contract content is not proven equivalent, so no hash migration was applied",
                    current.hash
                ));
            }
        }
        records.push(record);
    }
    let versioned: Vec<&StoredContract> = records
        .iter()
        .filter(|record| record.version.is_some())
        .collect();
    let highest_version = versioned.iter().filter_map(|record| record.version).max();
    if let Some(existing) = versioned
        .iter()
        .find(|record| record.version == Some(declared_version))
    {
        if !same_definition(&existing.contract, current) {
            return Err(format!(
                "Semantic component {} contractVersion {} is immutable and differs from the generated manifest: differingFields={:?}, storedHash={}, manifestHash={}; increment contractVersion explicitly",
                current.id,
                declared_version,
                definition_differences(&existing.contract, current),
                existing.contract.hash,
                current.hash
            ));
        }
        if highest_version.is_some_and(|version| version > declared_version) {
            return Err(format!(
                "Semantic component {} manifest declares stale contractVersion {}; ontology already contains version {}",
                current.id,
                declared_version,
                highest_version.unwrap_or_default()
            ));
        }
        return Ok(());
    }
    if let Some(highest_version) = highest_version {
        if declared_version <= highest_version {
            return Err(format!(
                "Semantic component {} manifest declares contractVersion {}, but ontology already contains version {}; choose a new explicit version",
                current.id, declared_version, highest_version
            ));
        }
    }

    let legacy: Vec<&StoredContract> = records
        .iter()
        .filter(|record| record.version.is_none())
        .collect();
    if legacy.len() > 1 {
        return Err(format!(
            "More than one unversioned semantic definition exists for component {}",
            current.id
        ));
    }
    if let Some(baseline) = legacy.first() {
        match matching_versioned_legacy_contract(&records, baseline) {
            Ok(Some(_)) => {}
            Ok(None) => {
                let differing_fields = definition_differences(&baseline.contract, current);
                return Err(format!(
                    "Unversioned semantic contract {} cannot migrate to {} v{}: canonical content differs in {:?}; legacyCanonicalHash={}, manifestHash={}, contractDiagnostics=[{}]. The legacy definition is preserved; an exact versioned contract match is required",
                    baseline.iri,
                    current.id,
                    declared_version,
                    differing_fields,
                    baseline.contract.hash,
                    current.hash,
                    contract_diagnostics(&records, baseline).join(",")
                ));
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn repair_semantically_identical_contract_hashes(
    conn: &mut Connection,
    current: &ComponentContract,
    declared_version: u64,
) -> Result<(), String> {
    let iris = Individual::find_by_class_and_properties(
        conn,
        COMPONENT_CLASS,
        &[(COMPONENT_ID, &current.id)],
    )
    .map_err(|error| error.to_string())?;
    for iri in iris {
        let (record, integrity_error) = read_contract_diagnostic(conn, &iri)?;
        if integrity_error.is_none() || record.version != Some(declared_version) {
            continue;
        }
        let differing_fields = definition_differences(&record.contract, current);
        if record.version != Some(declared_version)
            || !differing_fields.is_empty()
            || !same_definition(&record.contract, current)
            || record.contract.hash != current.hash
        {
            return Err(format!(
                "Cannot rehash semantic contract {}: version {:?} must match manifest version {}, canonical differences are {:?}; stored definition is preserved",
                record.iri, record.version, declared_version, differing_fields
            ));
        }
        if record.legacy_action_wrapper {
            replace_literal_property(
                conn,
                &record.iri,
                COMPONENT_ACTIONS_SCHEMA,
                &record.contract.actions_schema,
            )?;
        }
        replace_literal_property(conn, &record.iri, CONTRACT_HASH, &current.hash)?;
    }
    Ok(())
}

fn definition_differences(
    left: &ComponentContract,
    right: &ComponentContract,
) -> Vec<&'static str> {
    let mut fields = Vec::new();
    if left.id != right.id {
        fields.push("componentId");
    }
    if left.intent != right.intent {
        fields.push("intent");
    }
    if left.props_schema != right.props_schema {
        fields.push("propsSchema");
    }
    if left.actions_schema != right.actions_schema {
        fields.push("actionsSchema");
    }
    if left.examples != right.examples {
        fields.push("examples");
    }
    fields
}

fn migrate_legacy_action_wrappers(
    conn: &mut Connection,
    records: &[StoredContract],
    declared_version: u64,
) -> Result<usize, String> {
    let mut legacy: Vec<&StoredContract> = records
        .iter()
        .filter(|record| record.version == Some(declared_version) && record.legacy_action_wrapper)
        .collect();
    legacy.sort_by_key(|record| record.version);
    let mut migrated = 0;

    for old in legacy {
        let canonical_actions = old.contract.actions_schema.clone();
        let canonical_hash = contract_hash(
            &old.contract.id,
            &old.contract.intent,
            &old.contract.props_schema,
            &canonical_actions,
            &old.contract.examples,
        )?;
        replace_literal_property(conn, &old.iri, COMPONENT_ACTIONS_SCHEMA, &canonical_actions)?;
        replace_literal_property(conn, &old.iri, CONTRACT_HASH, &canonical_hash)?;
        migrated += 1;
    }

    Ok(migrated)
}

fn replace_literal_property(
    conn: &mut Connection,
    subject: &str,
    predicate: &str,
    value: &str,
) -> Result<(), String> {
    Individual::clear_property(conn, subject, predicate, SYNC_ORIGIN)
        .map_err(|error| error.to_string())?;
    Individual::new(subject)
        .add_property(
            conn,
            predicate,
            vec![Object::Literal {
                value: value.to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }],
            SYNC_ORIGIN,
        )
        .map_err(|error| error.to_string())
}

fn list_component_contracts_diagnostic(
    conn: &Connection,
    component_id: &str,
) -> Result<Vec<StoredContract>, String> {
    let iris = Individual::find_by_class_and_properties(
        conn,
        COMPONENT_CLASS,
        &[(COMPONENT_ID, component_id)],
    )
    .map_err(|error| error.to_string())?;
    iris.iter()
        .map(|iri| read_contract_diagnostic(conn, iri).map(|(contract, _)| contract))
        .collect()
}

fn read_contract(conn: &Connection, iri: &str) -> Result<StoredContract, String> {
    let (contract, integrity_error) = read_contract_diagnostic(conn, iri)?;
    if let Some(error) = integrity_error {
        return Err(error);
    }
    Ok(contract)
}

fn read_contract_diagnostic(
    conn: &Connection,
    iri: &str,
) -> Result<(StoredContract, Option<String>), String> {
    let individual = Individual::get(conn, iri)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("Semantic component definition {iri} was not found"))?;
    let id = literal_value(&individual, COMPONENT_ID)?;
    let intent = literal_value(&individual, COMPONENT_INTENT)?;
    let stored_props_schema = literal_value(&individual, COMPONENT_PROPS_SCHEMA)?;
    let props_schema = canonical_json_text(&stored_props_schema)?;
    let stored_actions_schema = literal_value(&individual, COMPONENT_ACTIONS_SCHEMA)?;
    let actions_schema = canonical_actions_schema(&stored_actions_schema)?;
    let legacy_action_wrapper = legacy_actions_wrapper(&stored_actions_schema)?;
    let stored_examples = literal_value(&individual, COMPONENT_EXAMPLES)?;
    let examples = canonical_json_text(&stored_examples)?;
    let manifest_version = literal_value(&individual, COMPONENT_MANIFEST_VERSION)?;
    let version = literal_values(&individual, CONTRACT_VERSION)
        .first()
        .map(|value| {
            value
                .parse::<u64>()
                .map_err(|error| format!("Invalid contract version {value:?} on {iri}: {error}"))
        })
        .transpose()?;
    if let Some(version) = version {
        let expected_iri = component_iri(&id, version);
        if iri != expected_iri {
            return Err(format!(
                "Semantic component definition {iri} does not match its declared identity {expected_iri}"
            ));
        }
    }
    let canonical_hash = contract_hash(&id, &intent, &props_schema, &actions_schema, &examples)?;
    let raw_payload_hash = contract_hash(
        &id,
        &intent,
        &stored_props_schema,
        &stored_actions_schema,
        &stored_examples,
    )?;
    let persisted_hash = literal_values(&individual, CONTRACT_HASH).first().cloned();
    let expected_persisted_hash = if legacy_action_wrapper {
        raw_payload_hash.clone()
    } else {
        canonical_hash.clone()
    };
    let (hash, integrity_error) = match persisted_hash.clone() {
        Some(hash) if hash == expected_persisted_hash => (canonical_hash, None),
        Some(stored_hash) => {
            let version_label = version
                .map(|version| version.to_string())
                .unwrap_or_else(|| "unversioned".to_string());
            let error = format!(
                "Semantic component contract hash mismatch: iri={iri}, contractVersion={version_label}, storedHash={stored_hash}, rawPayloadHash={raw_payload_hash}, canonicalPayloadHash={canonical_hash}, storedMatchesRaw={}, storedMatchesCanonical={}, expectedRepresentation={}",
                stored_hash == raw_payload_hash,
                stored_hash == canonical_hash,
                if legacy_action_wrapper {
                    "raw legacy wrapper"
                } else {
                    "canonical"
                },
            );
            (canonical_hash, Some(error))
        }
        None if version.is_none() => (canonical_hash, None),
        None => {
            let version_label = version
                .map(|version| version.to_string())
                .unwrap_or_else(|| "unversioned".to_string());
            let error = format!(
                "Semantic component contract hash missing: iri={iri}, contractVersion={version_label}, rawPayloadHash={raw_payload_hash}, canonicalPayloadHash={canonical_hash}"
            );
            (canonical_hash, Some(error))
        }
    };
    Ok((
        StoredContract {
            iri: iri.to_string(),
            version,
            contract: ComponentContract {
                id,
                intent,
                props_schema,
                actions_schema,
                examples,
                manifest_version,
                hash,
            },
            legacy_action_wrapper,
            integrity_valid: integrity_error.is_none(),
            stored_hash: persisted_hash,
            integrity_error: integrity_error.clone(),
        },
        integrity_error,
    ))
}

fn make_contract(
    id: String,
    intent: String,
    mut props_schema: Value,
    slots: Vec<Value>,
    actions_schema: String,
    examples: Value,
    manifest_version: String,
) -> Result<ComponentContract, String> {
    let slots_are_present = !slots.is_empty();
    if slots_are_present {
        props_schema
            .as_object_mut()
            .ok_or_else(|| format!("The props schema for component {id} must be a JSON object"))?
            .insert("x-foundation-slots".to_string(), Value::Array(slots));
    }
    let props_schema = canonical_json(&props_schema)?;
    let examples = canonical_json(&examples)?;
    let hash = contract_hash(&id, &intent, &props_schema, &actions_schema, &examples)?;
    Ok(ComponentContract {
        id,
        intent,
        props_schema,
        actions_schema,
        examples,
        manifest_version,
        hash,
    })
}

fn canonical_actions_schema(source: &str) -> Result<String, String> {
    let value: Value = serde_json::from_str(source)
        .map_err(|error| format!("Invalid component actions schema JSON: {error}"))?;
    let actions = value
        .as_array()
        .or_else(|| value.get("actions").and_then(Value::as_array))
        .ok_or_else(|| {
            "Component actions schema must be an array or an object with actions".to_string()
        })?;
    canonical_json(&Value::Array(actions.clone()))
}

fn legacy_actions_wrapper(source: &str) -> Result<bool, String> {
    let value: Value = serde_json::from_str(source)
        .map_err(|error| format!("Invalid component actions schema JSON: {error}"))?;
    Ok(value
        .as_object()
        .and_then(|object| object.get("actions"))
        .and_then(Value::as_array)
        .is_some())
}

fn contract_hash(
    id: &str,
    intent: &str,
    props_schema: &str,
    actions_schema: &str,
    examples: &str,
) -> Result<String, String> {
    let props: Value = serde_json::from_str(props_schema).map_err(|error| error.to_string())?;
    let actions: Value = serde_json::from_str(actions_schema).map_err(|error| error.to_string())?;
    let examples: Value = serde_json::from_str(examples).map_err(|error| error.to_string())?;
    let canonical = canonical_json(&serde_json::json!({
        "componentId": id,
        "intent": intent,
        "propsSchema": props,
        "actionsSchema": actions,
        "examples": examples,
    }))?;
    Ok(format!("sha256:{:x}", Sha256::digest(canonical.as_bytes())))
}

fn create_contract(
    conn: &mut Connection,
    contract: &ComponentContract,
    version: u64,
) -> Result<(), String> {
    let iri = component_iri(&contract.id, version);
    if Individual::get(conn, &iri)
        .map_err(|error| error.to_string())?
        .is_some()
    {
        let existing = read_contract(conn, &iri)?;
        if existing.version == Some(version)
            && same_definition(&existing.contract, contract)
            && existing.contract.hash == contract.hash
        {
            return Ok(());
        }
        return Err(format!(
            "Immutable semantic contract IRI collision at {iri}; refusing to overwrite it"
        ));
    }

    let individual = Individual::new(&iri);
    individual
        .assert_without_icon(
            conn,
            COMPONENT_CLASS,
            &format!("{} v{}", contract.id, version),
            SYNC_ORIGIN,
        )
        .map_err(|error| error.to_string())?;

    for (predicate, value) in [
        (COMPONENT_ID, contract.id.clone()),
        (COMPONENT_INTENT, contract.intent.clone()),
        (COMPONENT_PROPS_SCHEMA, contract.props_schema.clone()),
        (COMPONENT_ACTIONS_SCHEMA, contract.actions_schema.clone()),
        (COMPONENT_EXAMPLES, contract.examples.clone()),
        (
            COMPONENT_MANIFEST_VERSION,
            contract.manifest_version.clone(),
        ),
        (CONTRACT_VERSION, version.to_string()),
        (CONTRACT_HASH, contract.hash.clone()),
    ] {
        individual
            .add_property(
                conn,
                predicate,
                vec![Object::Literal {
                    value,
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                }],
                SYNC_ORIGIN,
            )
            .map_err(|error| error.to_string())?;
    }
    let active_status_iri = crate::core_ontology::status::resolve_active_status(conn)?;
    individual
        .add_property(
            conn,
            "foundation:hasStatus",
            vec![Object::Iri(active_status_iri)],
            SYNC_ORIGIN,
        )
        .map_err(|error| error.to_string())
}

fn same_definition(left: &ComponentContract, right: &ComponentContract) -> bool {
    left.id == right.id
        && left.intent == right.intent
        && left.props_schema == right.props_schema
        && left.actions_schema == right.actions_schema
        && left.examples == right.examples
}

fn literal_value(individual: &Individual, predicate: &str) -> Result<String, String> {
    literal_values(individual, predicate)
        .into_iter()
        .next()
        .ok_or_else(|| {
            format!(
                "{} is missing required property {predicate}",
                individual.iri
            )
        })
}

fn literal_values(individual: &Individual, predicate: &str) -> Vec<String> {
    individual
        .properties
        .iter()
        .filter(|(current_predicate, _)| current_predicate == predicate)
        .filter_map(|(_, value)| value.as_literal())
        .collect()
}

fn canonical_json_text(source: &str) -> Result<String, String> {
    let value: Value = serde_json::from_str(source).map_err(|error| {
        format!("Invalid JSON stored in semantic component definition: {error}")
    })?;
    canonical_json(&value)
}

fn canonical_json(value: &Value) -> Result<String, String> {
    serde_json::to_string(value).map_err(|error| error.to_string())
}

fn sanitize_component_id(component_id: &str) -> String {
    component_id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn component_iri(component_id: &str, version: u64) -> String {
    format!(
        "foundation:SemanticComponentDefinition_{}_v{version}",
        sanitize_component_id(component_id)
    )
}

fn matching_versioned_legacy_contract<'a>(
    records: &'a [StoredContract],
    legacy: &StoredContract,
) -> Result<Option<&'a StoredContract>, String> {
    let matches: Vec<&StoredContract> = records
        .iter()
        .filter(|record| record.version.is_some())
        .filter(|record| {
            record.integrity_valid
                && record.contract.hash == legacy.contract.hash
                && same_definition(&record.contract, &legacy.contract)
        })
        .collect();
    match matches.as_slice() {
        [] => Ok(None),
        [contract] => Ok(Some(*contract)),
        _ => Err(format!(
            "Unversioned semantic contract {} has more than one versioned contract with the same canonical content: {}",
            legacy.iri,
            matches
                .iter()
                .map(|contract| contract.iri.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

fn plan_information_reset(
    conn: &Connection,
    target: &ComponentContract,
) -> Result<InformationResetPlan, String> {
    if target.id != "Information" {
        return Err("The v0 reset is reserved for the Information component".to_string());
    }
    let records = list_component_contracts_diagnostic(conn, "Information")?;
    let version_zero_iri = component_iri("Information", 0);
    let existing_v0 = records.iter().find(|record| record.iri == version_zero_iri);
    let keeps_v0 = existing_v0
        .is_some_and(|record| record.integrity_valid && same_definition(&record.contract, target));
    let obsolete_contract_iris = records
        .iter()
        .filter(|record| !(record.iri == version_zero_iri && keeps_v0))
        .map(|record| record.iri.clone())
        .collect::<Vec<_>>();
    let obsolete_contract_set = obsolete_contract_iris
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let information_iri_prefix = component_iri("Information", 0)
        .trim_end_matches("_v0")
        .to_string();
    let registry_links = owl::get_all_iri_properties(conn, CORE_REGISTRY, CORE_INDIVIDUAL)
        .map_err(|error| error.to_string())?;
    let obsolete_registry_iris = registry_links
        .into_iter()
        .filter(|iri| {
            obsolete_contract_set.contains(iri.as_str())
                || (iri.starts_with(&information_iri_prefix)
                    && iri != &component_iri("Information", 0))
        })
        .collect();

    let information_contract_iris = records
        .iter()
        .map(|record| record.iri.as_str())
        .collect::<HashSet<_>>();
    let spec_iris = Class::get_instances(conn, SPEC_CLASS).map_err(|error| error.to_string())?;
    let mut obsolete_spec_iris = Vec::new();
    let mut managed_spec_iris = Vec::new();
    let mut spec_repins = Vec::new();
    for spec_iri in spec_iris {
        let pins = owl::get_all_iri_properties(conn, &spec_iri, SPEC_CONTRACT)
            .map_err(|error| error.to_string())?;
        let has_information_pin = pins.iter().any(|pin| {
            information_contract_iris.contains(pin.as_str())
                || pin.starts_with(&information_iri_prefix)
        });
        let is_default_widget =
            !owl::query_by_predicate_object(conn, CLASS_DEFAULT_WIDGET_SPEC, &spec_iri)
                .map_err(|error| error.to_string())?
                .triples
                .is_empty();
        let Some(source) = owl::get_literal_property(conn, &spec_iri, SPEC_SOURCE)
            .map_err(|error| error.to_string())?
        else {
            if has_information_pin {
                if !is_default_widget {
                    obsolete_spec_iris.push(spec_iri);
                    continue;
                }
                return Err(format!(
                    "Cannot reset Information contracts: JsonRenderSpec {spec_iri} references an old Information contract but has no specSource"
                ));
            }
            continue;
        };
        let component_ids = match spec_component_ids(&source) {
            Ok(component_ids) => component_ids,
            Err(error) if has_information_pin => {
                if !is_default_widget {
                    obsolete_spec_iris.push(spec_iri);
                    continue;
                }
                return Err(format!(
                    "Cannot reset Information contracts: JsonRenderSpec {spec_iri} has invalid source: {}",
                    error.diagnostic()
                ));
            }
            Err(_) => continue,
        };
        let uses_information = component_ids.iter().any(|id| id == "Information");
        if !uses_information && !has_information_pin {
            continue;
        }
        if !uses_information {
            if !is_default_widget {
                obsolete_spec_iris.push(spec_iri);
                continue;
            }
            return Err(format!(
                "Cannot reset Information contracts: JsonRenderSpec {spec_iri} pins Information but does not render it"
            ));
        }
        managed_spec_iris.push(spec_iri.clone());
        let mut spec: Value = match serde_json::from_str(&source) {
            Ok(spec) => spec,
            Err(_error) if !is_default_widget => {
                obsolete_spec_iris.push(spec_iri);
                continue;
            }
            Err(error) => {
                return Err(format!(
                    "Cannot reset Information contracts: invalid source for {spec_iri}: {error}"
                ));
            }
        };
        let source_changed = match ensure_information_layout(&mut spec, &spec_iri) {
            Ok(changed) => changed,
            Err(_error) if !is_default_widget => {
                obsolete_spec_iris.push(spec_iri);
                continue;
            }
            Err(error) => return Err(error),
        };
        if let Err(error) =
            validate_spec_component_occurrences(&spec, "Information", &target.props_schema)
        {
            if !is_default_widget {
                obsolete_spec_iris.push(spec_iri);
                continue;
            }
            return Err(format!(
                "Cannot reset Information contracts without a compatible minimal migration for {spec_iri}: {error}"
            ));
        }

        let obsolete_pins = pins
            .into_iter()
            .filter(|pin| {
                (information_contract_iris.contains(pin.as_str())
                    || pin.starts_with(&information_iri_prefix))
                    && pin != &version_zero_iri
            })
            .collect::<Vec<_>>();
        let has_version_zero = owl::get_all_iri_properties(conn, &spec_iri, SPEC_CONTRACT)
            .map_err(|error| error.to_string())?
            .contains(&version_zero_iri);
        if !obsolete_pins.is_empty() || !has_version_zero || source_changed {
            spec_repins.push(InformationSpecMigration {
                iri: spec_iri,
                obsolete_pins,
                source_update: source_changed.then(|| spec.to_string()),
            });
        }
    }

    Ok(InformationResetPlan {
        contract: target.clone(),
        obsolete_contract_iris,
        obsolete_registry_iris,
        obsolete_spec_iris,
        managed_spec_iris,
        spec_repins,
    })
}

fn ensure_information_layout(spec: &mut Value, spec_iri: &str) -> Result<bool, String> {
    let elements = spec
        .get_mut("elements")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| format!("JsonRenderSpec {spec_iri} has no elements object"))?;
    let mut changed = false;
    for (element_id, element) in elements {
        if element.get("type").and_then(Value::as_str) != Some("Information") {
            continue;
        }
        let element = element.as_object_mut().ok_or_else(|| {
            format!("JsonRenderSpec {spec_iri} element {element_id} is not an object")
        })?;
        let props = element
            .entry("props")
            .or_insert_with(|| Value::Object(serde_json::Map::new()))
            .as_object_mut()
            .ok_or_else(|| {
                format!("JsonRenderSpec {spec_iri} Information props must be an object")
            })?;
        match props.get_mut("layout") {
            None => {
                props.insert("layout".to_string(), serde_json::json!({"columns": 2}));
                changed = true;
            }
            Some(Value::Object(layout)) if !layout.contains_key("columns") => {
                layout.insert("columns".to_string(), Value::from(2));
                changed = true;
            }
            Some(Value::Object(_)) => {}
            Some(_) => {
                return Err(format!(
                    "Cannot reset Information contracts: JsonRenderSpec {spec_iri} element {element_id} has a non-object layout"
                ));
            }
        }
    }
    Ok(changed)
}

fn retract_iri_links(
    conn: &mut Connection,
    subject: &str,
    predicate: &str,
    iris: &[String],
) -> Result<(), String> {
    if iris.is_empty() {
        return Ok(());
    }
    let triples = iris
        .iter()
        .map(|iri| Triple::new(subject, predicate, Object::Iri(iri.clone())))
        .collect::<Vec<_>>();
    owl::retract_raw_triples(conn, &triples, SYNC_ORIGIN).map_err(|error| error.to_string())
}

fn contract_diagnostics(records: &[StoredContract], legacy: &StoredContract) -> Vec<String> {
    records
        .iter()
        .map(|record| {
            let differences = definition_differences(&record.contract, &legacy.contract);
            format!(
                "{{iri:{},version:{:?},storedHash:{:?},canonicalHash:{},integrityValid:{},integrityError:{:?},sameDefinition:{},differingFields:{:?},legacyActionWrapper:{}}}",
                record.iri,
                record.version,
                record.stored_hash,
                record.contract.hash,
                record.integrity_valid,
                record.integrity_error,
                same_definition(&record.contract, &legacy.contract),
                differences,
                record.legacy_action_wrapper
            )
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SpecComponentIdsError {
    InvalidJson(String),
    MissingRoot,
    MissingElements,
    InvalidElements,
    MissingComponentType,
    InvalidComponentType,
}

impl SpecComponentIdsError {
    fn diagnostic(&self) -> String {
        match self {
            Self::InvalidJson(error) => format!("specSource is not valid JSON: {error}"),
            Self::MissingRoot => "specSource has no root element".to_string(),
            Self::MissingElements => "specSource has no elements object".to_string(),
            Self::InvalidElements => "specSource elements is not an object".to_string(),
            Self::MissingComponentType => "an element has no component type".to_string(),
            Self::InvalidComponentType => "an element component type is not a string".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticSpecReadError {
    InvalidStructure(String),
    Internal(String),
}

impl SemanticSpecReadError {
    pub fn diagnostic(&self) -> &str {
        match self {
            Self::InvalidStructure(diagnostic) | Self::Internal(diagnostic) => diagnostic,
        }
    }

    pub fn user_error_code(&self) -> Option<&'static str> {
        match self {
            Self::InvalidStructure(_) => Some("invalid_spec_structure"),
            Self::Internal(_) => None,
        }
    }
}

impl From<String> for SemanticSpecReadError {
    fn from(error: String) -> Self {
        Self::Internal(error)
    }
}

impl std::fmt::Display for SemanticSpecReadError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.diagnostic())
    }
}

fn spec_component_ids(spec_source: &str) -> Result<Vec<String>, SpecComponentIdsError> {
    let spec: Value = serde_json::from_str(spec_source)
        .map_err(|error| SpecComponentIdsError::InvalidJson(error.to_string()))?;
    if spec.get("root").is_none() {
        return Err(SpecComponentIdsError::MissingRoot);
    }
    let elements = spec
        .get("elements")
        .ok_or(SpecComponentIdsError::MissingElements)?
        .as_object()
        .ok_or(SpecComponentIdsError::InvalidElements)?;
    let mut component_ids = HashSet::new();
    for element in elements.values() {
        let component_id = element
            .get("type")
            .ok_or(SpecComponentIdsError::MissingComponentType)?
            .as_str()
            .ok_or(SpecComponentIdsError::InvalidComponentType)?;
        component_ids.insert(component_id.to_string());
    }
    let mut component_ids: Vec<String> = component_ids.into_iter().collect();
    component_ids.sort();
    Ok(component_ids)
}

fn contract_iri_for_component(
    conn: &Connection,
    component_id: &str,
    choose_latest: bool,
) -> Result<Option<String>, String> {
    let records = list_component_contracts_diagnostic(conn, component_id)?;
    let versioned: Vec<&StoredContract> = records
        .iter()
        .filter(|record| record.version.is_some())
        .collect();
    let selected = if choose_latest {
        versioned
            .into_iter()
            .max_by_key(|record| record.version.unwrap_or_default())
    } else {
        versioned
            .into_iter()
            .min_by_key(|record| record.version.unwrap_or_default())
    };
    Ok(selected.map(|record| record.iri.clone()))
}

pub fn link_spec_to_latest_contracts(
    conn: &mut Connection,
    spec_iri: &str,
    spec_source: &str,
) -> Result<bool, String> {
    if spec_source.trim().is_empty() {
        return set_spec_contracts(conn, spec_iri, &[]);
    }
    let component_ids = spec_component_ids(spec_source).map_err(|error| {
        format!(
            "Invalid semantic widget spec {spec_iri}: {}",
            error.diagnostic()
        )
    })?;
    let resolved_contract_iris = component_ids
        .iter()
        .map(|component_id| contract_iri_for_component(conn, component_id, true))
        .collect::<Result<Vec<_>, _>>()?;
    let Some(contract_iris) = resolved_contract_iris
        .into_iter()
        .collect::<Option<Vec<_>>>()
    else {
        return Ok(false);
    };
    set_spec_contracts(conn, spec_iri, &contract_iris)
}

fn link_legacy_specs(conn: &mut Connection) -> Result<usize, String> {
    let spec_iris = Class::get_instances(conn, SPEC_CLASS).map_err(|error| error.to_string())?;
    let mut linked = 0;
    for spec_iri in spec_iris {
        if !owl::get_all_iri_properties(conn, &spec_iri, SPEC_CONTRACT)
            .map_err(|error| error.to_string())?
            .is_empty()
        {
            continue;
        }
        let Some(source) = owl::get_literal_property(conn, &spec_iri, SPEC_SOURCE)
            .map_err(|error| error.to_string())?
        else {
            continue;
        };
        let spec: Value = serde_json::from_str(&source).map_err(|error| {
            format!("Cannot link legacy semantic spec {spec_iri}: invalid JSON: {error}")
        })?;
        let component_ids = spec_component_ids(&source).map_err(|error| {
            format!(
                "Cannot link legacy semantic spec {spec_iri}: {}",
                error.diagnostic()
            )
        })?;
        let resolved_contract_iris = component_ids
            .iter()
            .map(|component_id| contract_iri_for_component(conn, component_id, true))
            .collect::<Result<Vec<_>, _>>()?;
        let Some(contract_iris) = resolved_contract_iris
            .into_iter()
            .collect::<Option<Vec<_>>>()
        else {
            continue;
        };
        let compatible = component_ids
            .iter()
            .zip(&contract_iris)
            .all(|(component_id, iri)| {
                read_contract_diagnostic(conn, iri)
                    .ok()
                    .is_some_and(|(contract, _)| {
                        validate_spec_component_occurrences(
                            &spec,
                            component_id,
                            &contract.contract.props_schema,
                        )
                        .is_ok()
                    })
            });
        if !compatible {
            continue;
        }
        if set_spec_contracts(conn, &spec_iri, &contract_iris)? {
            linked += 1;
        }
    }
    Ok(linked)
}

fn set_spec_contracts(
    conn: &mut Connection,
    spec_iri: &str,
    contract_iris: &[String],
) -> Result<bool, String> {
    let mut current = owl::get_all_iri_properties(conn, spec_iri, SPEC_CONTRACT)
        .map_err(|error| error.to_string())?;
    current.sort();
    let mut desired = contract_iris.to_vec();
    desired.sort();
    if current == desired {
        return Ok(false);
    }
    if desired.is_empty() {
        if !current.is_empty() {
            Individual::clear_property(conn, spec_iri, SPEC_CONTRACT, SYNC_ORIGIN)
                .map_err(|error| error.to_string())?;
        }
        return Ok(!current.is_empty());
    }
    if !current.is_empty() {
        Individual::clear_property(conn, spec_iri, SPEC_CONTRACT, SYNC_ORIGIN)
            .map_err(|error| error.to_string())?;
    }
    Individual::new(spec_iri)
        .add_property(
            conn,
            SPEC_CONTRACT,
            desired.into_iter().map(Object::Iri).collect(),
            SYNC_ORIGIN,
        )
        .map_err(|error| error.to_string())?;
    Ok(true)
}

pub fn sync_legacy_specs(conn: &mut Connection) -> Result<usize, String> {
    link_legacy_specs(conn)
}

fn plan_legacy_contract_repins(
    conn: &Connection,
    planned_components: &[(ComponentContract, u64)],
) -> Result<BTreeMap<String, String>, String> {
    let mut repins = BTreeMap::new();
    for (component, version) in planned_components {
        if component.id == "Information" && *version == 0 {
            continue;
        }
        let records = list_component_contracts_diagnostic(conn, &component.id)?;
        let legacy: Vec<&StoredContract> = records
            .iter()
            .filter(|record| record.version.is_none())
            .collect();
        if legacy.len() > 1 {
            return Err(format!(
                "More than one unversioned semantic definition exists for component {}",
                component.id
            ));
        }
        let Some(legacy) = legacy.first() else {
            continue;
        };
        if let Some(versioned) = matching_versioned_legacy_contract(&records, legacy)? {
            repins.insert(legacy.iri.clone(), versioned.iri.clone());
        }
    }
    Ok(repins)
}

fn plan_compatible_spec_repins(
    conn: &Connection,
    planned_components: &[(ComponentContract, u64)],
    legacy_contract_repins: &BTreeMap<String, String>,
    reset_information: bool,
) -> Result<SpecRepinPlan, String> {
    let targets: BTreeMap<&str, (&ComponentContract, u64)> = planned_components
        .iter()
        .map(|(contract, version)| (contract.id.as_str(), (contract, *version)))
        .collect();
    let spec_iris = Class::get_instances(conn, SPEC_CLASS).map_err(|error| error.to_string())?;
    let mut plan = SpecRepinPlan::default();

    for spec_iri in spec_iris {
        let pins = owl::get_all_iri_properties(conn, &spec_iri, SPEC_CONTRACT)
            .map_err(|error| error.to_string())?;
        let has_legacy_pin = pins
            .iter()
            .any(|pin| legacy_contract_repins.contains_key(pin));
        let Some(source) = owl::get_literal_property(conn, &spec_iri, SPEC_SOURCE)
            .map_err(|error| error.to_string())?
        else {
            if has_legacy_pin {
                return Err(format!(
                    "Cannot migrate legacy semantic contract pin for {spec_iri}: specSource is missing"
                ));
            }
            if !pins.is_empty() {
                plan.issues.push(spec_migration_issue(
                    conn,
                    &spec_iri,
                    "pinned spec has no specSource to prove compatibility",
                ));
            }
            continue;
        };
        let Ok(spec) = serde_json::from_str::<Value>(&source) else {
            if has_legacy_pin {
                return Err(format!(
                    "Cannot migrate legacy semantic contract pin for {spec_iri}: specSource is invalid JSON"
                ));
            }
            plan.issues.push(spec_migration_issue(
                conn,
                &spec_iri,
                "specSource is invalid JSON",
            ));
            continue;
        };
        let mut component_ids = match spec_component_ids(&source) {
            Ok(component_ids) => component_ids,
            Err(error) => {
                if has_legacy_pin {
                    return Err(format!(
                        "Cannot migrate legacy semantic contract pin for {spec_iri}: specSource is structurally invalid: {}",
                        error.diagnostic()
                    ));
                }
                plan.issues.push(spec_migration_issue(
                    conn,
                    &spec_iri,
                    &format!("specSource is structurally invalid: {}", error.diagnostic()),
                ));
                continue;
            }
        };
        if reset_information {
            component_ids.retain(|component_id| component_id != "Information");
        }
        let mut desired = Vec::new();
        let mut changed = false;
        let mut spec_errors = Vec::new();
        let mut old_by_component: BTreeMap<String, StoredContract> = BTreeMap::new();
        let mut migrated_legacy_components = HashSet::new();
        for pin in pins {
            let (resolved_pin, migrated_legacy) = match legacy_contract_repins.get(&pin) {
                Some(versioned_iri) => (versioned_iri.as_str(), true),
                None => (pin.as_str(), false),
            };
            let old = match read_contract_diagnostic(conn, resolved_pin) {
                Ok((record, _)) => record,
                Err(error) => {
                    spec_errors.push(format!("pinned contract {pin}: {error}"));
                    continue;
                }
            };
            if reset_information && old.contract.id == "Information" {
                continue;
            }
            if old.version.is_none() {
                spec_errors.push(format!("pinned contract {pin} is unversioned"));
                continue;
            }
            if migrated_legacy {
                migrated_legacy_components.insert(old.contract.id.clone());
                changed = true;
            }
            if old_by_component
                .insert(old.contract.id.clone(), old)
                .is_some()
            {
                spec_errors.push(format!("multiple pins exist for component in {spec_iri}"));
            }
        }
        for component_id in &component_ids {
            let pinned = old_by_component.remove(component_id);
            let target = targets.get(component_id.as_str());
            let target_version = target.map(|(_, version)| *version);
            let target_contract = target.map(|(contract, _)| *contract);
            if let Some(old) = pinned {
                let old_version = old.version.unwrap_or_default();
                if let Err(error) = validate_spec_component_occurrences(
                    &spec,
                    component_id,
                    &old.contract.props_schema,
                ) {
                    spec_errors.push(format!(
                        "{component_id} pinned v{old_version}: incompatible with its pinned contract: {error}"
                    ));
                    continue;
                }
                if let (Some(target_contract), Some(target_version)) =
                    (target_contract, target_version)
                {
                    if old_version < target_version {
                        match validate_spec_component_occurrences(
                            &spec,
                            component_id,
                            &target_contract.props_schema,
                        ) {
                            Ok(()) => {
                                desired.push(component_iri(component_id, target_version));
                                changed = true;
                            }
                            Err(error) => {
                                if migrated_legacy_components.contains(component_id) {
                                    desired.push(old.iri);
                                } else {
                                    spec_errors.push(format!(
                                        "{component_id} v{old_version} → v{target_version}: {error}"
                                    ));
                                }
                            }
                        }
                    } else if old_version == target_version {
                        desired.push(old.iri);
                    } else {
                        spec_errors.push(format!(
                            "{component_id} pin v{old_version} is newer than app catalog v{target_version}"
                        ));
                    }
                } else {
                    desired.push(old.iri);
                }
            } else if let (Some(target_contract), Some(target_version)) =
                (target_contract, target_version)
            {
                match validate_spec_component_occurrences(
                    &spec,
                    component_id,
                    &target_contract.props_schema,
                ) {
                    Ok(()) => {
                        desired.push(component_iri(component_id, target_version));
                        changed = true;
                    }
                    Err(error) => spec_errors.push(format!(
                        "{component_id} has no contract pin and is incompatible with v{target_version}: {error}"
                    )),
                }
            } else {
                spec_errors.push(format!(
                    "{component_id} has no pin or current contract in the app catalog"
                ));
            }
        }
        if !old_by_component.is_empty() {
            spec_errors.push(format!(
                "contract pins contain component(s) absent from specSource: {}",
                old_by_component
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if !spec_errors.is_empty() {
            if has_legacy_pin {
                return Err(format!(
                    "Cannot migrate legacy semantic contract pin for {spec_iri}: {}",
                    spec_errors.join("; ")
                ));
            }
            plan.issues.push(spec_migration_issue(
                conn,
                &spec_iri,
                &spec_errors.join("; "),
            ));
        } else if changed {
            desired.sort();
            desired.dedup();
            plan.repins.push((spec_iri, desired));
        }
    }
    Ok(plan)
}

pub fn inspect_semantic_spec_compatibility(
    conn: &Connection,
    spec_iri: &str,
) -> Result<SemanticSpecCompatibility, SemanticSpecReadError> {
    let spec_source = owl::get_literal_property(conn, spec_iri, SPEC_SOURCE)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("JSON render spec {spec_iri} has no specSource"))?;
    let spec: Value = serde_json::from_str(&spec_source).map_err(|error| {
        SemanticSpecReadError::InvalidStructure(format!(
            "JSON render spec {spec_iri} has malformed specSource JSON: {error}"
        ))
    })?;
    let component_ids = spec_component_ids(&spec_source).map_err(|error| {
        SemanticSpecReadError::InvalidStructure(format!(
            "JSON render spec {spec_iri} has an invalid structure: {}",
            error.diagnostic()
        ))
    })?;
    let pins = owl::get_all_iri_properties(conn, spec_iri, SPEC_CONTRACT)
        .map_err(|error| error.to_string())?;
    let mut pinned_by_component = BTreeMap::new();
    for pin in pins {
        let (contract, integrity_error) = read_contract_diagnostic(conn, &pin)?;
        if pinned_by_component
            .insert(contract.contract.id.clone(), (contract, integrity_error))
            .is_some()
        {
            return Err(SemanticSpecReadError::Internal(format!(
                "JSON render spec {spec_iri} has multiple contract pins for one component"
            )));
        }
    }
    let manifest: SemanticCatalogManifest = serde_json::from_str(MANIFEST)
        .map_err(|error| format!("Generated semantic catalog manifest is invalid: {error}"))?;
    let actions_schema = canonical_json(&Value::Array(manifest.actions.clone()))?;
    let mut current_contracts = BTreeMap::new();
    for component in manifest.components {
        let examples = manifest
            .examples
            .get(&component.id)
            .ok_or_else(|| format!("Missing semantic example for component {}", component.id))?;
        let contract = make_contract(
            component.id.clone(),
            component.description,
            component.props_schema,
            component.slots,
            actions_schema.clone(),
            examples.clone(),
            manifest.version.to_string(),
        )?;
        current_contracts.insert(component.id, (contract, component.contract_version));
    }

    let mut component_compatibility = Vec::with_capacity(component_ids.len());
    for component_id in component_ids {
        let target = current_contracts.get(&component_id);
        let pinned = pinned_by_component.remove(&component_id);
        let (target_version, target_contract) = match target {
            Some((contract, version)) => (Some(*version), Some(contract)),
            None => (None, None),
        };
        let compatibility = match (pinned, target_contract) {
            (pinned, None) => SemanticComponentCompatibility {
                component_id: component_id.clone(),
                contract_iri: pinned.as_ref().map(|(contract, _)| contract.iri.clone()),
                pinned_version: pinned.as_ref().and_then(|(contract, _)| contract.version),
                target_version: None,
                status: "missing_contract".to_string(),
                reason: "This component is not present in the current app catalog; ask the agent to update the widget.".to_string(),
            },
            (None, Some(target_contract)) => {
                let reason = match validate_spec_component_occurrences(
                    &spec,
                    &component_id,
                    &target_contract.props_schema,
                ) {
                    Ok(()) => "No version pin exists; the widget can be updated to the current contract.".to_string(),
                    Err(error) => format!("The widget spec must be updated for the current contract: {error}"),
                };
                SemanticComponentCompatibility {
                    component_id: component_id.clone(),
                    contract_iri: None,
                    pinned_version: None,
                    target_version,
                    status: if reason.starts_with("No version pin") { "update_available" } else { "update_required" }.to_string(),
                    reason,
                }
            }
            (Some((pinned, integrity_error)), Some(target_contract)) => {
                let pinned_version = pinned.version;
                let pinned_validation = validate_spec_component_occurrences(
                    &spec,
                    &component_id,
                    &pinned.contract.props_schema,
                );
                let status_and_reason = if pinned_version == target_version
                    && integrity_error.is_some()
                {
                    let integrity_error = integrity_error.unwrap_or_default();
                    ("update_required", format!("Pinned contract metadata is invalid: {integrity_error}. Ask the agent to update this widget."))
                } else if let Err(error) = pinned_validation {
                    ("update_required", format!("The spec does not satisfy its pinned contract: {error}. Ask the agent to update this widget."))
                } else if pinned_version == target_version {
                    match validate_spec_component_occurrences(&spec, &component_id, &target_contract.props_schema) {
                        Ok(()) => ("current", String::new()),
                        Err(error) => ("update_required", format!("The spec must be updated for the current contract: {error}. Ask the agent to update this widget.")),
                    }
                } else if pinned_version.is_some_and(|version| {
                    target_version.is_some_and(|target| version < target)
                }) {
                    match validate_spec_component_occurrences(&spec, &component_id, &target_contract.props_schema) {
                        Ok(()) => ("update_available", "The widget is compatible with the current contract and can be updated.".to_string()),
                        Err(error) => ("update_required", format!("The widget must be updated for the current contract: {error}. Ask the agent to update this widget.")),
                    }
                } else {
                    ("update_required", "The pinned version is newer than the current app catalog; update the app catalog before rendering this widget.".to_string())
                };
                SemanticComponentCompatibility {
                    component_id,
                    contract_iri: Some(pinned.iri),
                    pinned_version,
                    target_version,
                    status: status_and_reason.0.to_string(),
                    reason: status_and_reason.1,
                }
            }
        };
        component_compatibility.push(compatibility);
    }
    for (component_id, (pinned, _)) in pinned_by_component {
        component_compatibility.push(SemanticComponentCompatibility {
            component_id,
            contract_iri: Some(pinned.iri),
            pinned_version: pinned.version,
            target_version: None,
            status: "missing_contract".to_string(),
            reason: "A pinned component is absent from the widget spec or current app catalog; ask the agent to update the widget.".to_string(),
        });
    }
    component_compatibility.sort_by(|left, right| left.component_id.cmp(&right.component_id));

    Ok(SemanticSpecCompatibility {
        spec_iri: spec_iri.to_string(),
        spec_source,
        renders_entity: owl::get_iri_property(conn, spec_iri, SPEC_RENDERS_ENTITY)
            .map_err(|error| error.to_string())?
            .unwrap_or_default(),
        snapshot_tx: owl::current_snapshot_tx(conn)?,
        component_compatibility,
    })
}

fn spec_migration_issue(conn: &Connection, spec_iri: &str, reason: &str) -> String {
    let label = Individual::get(conn, spec_iri)
        .ok()
        .flatten()
        .and_then(|individual| literal_values(&individual, "rdfs:label").into_iter().next());
    match label {
        Some(label) => format!("{spec_iri} ({label}): {reason}"),
        None => format!("{spec_iri}: {reason}"),
    }
}

fn validate_spec_component_occurrences(
    spec: &Value,
    component_id: &str,
    props_schema_json: &str,
) -> Result<(), String> {
    let schema: Value = serde_json::from_str(props_schema_json)
        .map_err(|error| format!("component props schema is invalid: {error}"))?;
    let elements = spec
        .get("elements")
        .and_then(Value::as_object)
        .ok_or_else(|| "spec elements must be an object".to_string())?;
    for (element_id, element) in elements {
        if element.get("type").and_then(Value::as_str) != Some(component_id) {
            continue;
        }
        let empty_props = Value::Object(serde_json::Map::new());
        let props = element.get("props").unwrap_or(&empty_props);
        validate_json_schema_value(&schema, props, &format!("elements.{element_id}.props"))?;
        let bindings = element.get("bindings").unwrap_or(&Value::Null);
        validate_element_bindings(
            &schema,
            bindings,
            &format!("elements.{element_id}.bindings"),
        )?;
    }
    Ok(())
}

fn validate_element_bindings(schema: &Value, bindings: &Value, path: &str) -> Result<(), String> {
    if bindings.is_null() {
        return Ok(());
    }
    let bindings = bindings
        .as_object()
        .ok_or_else(|| format!("{path} must be an object"))?;
    let targets = schema
        .get("x-foundation-element-bindings")
        .and_then(|contract| contract.get("properties"))
        .and_then(Value::as_object)
        .ok_or_else(|| format!("{path} are not declared by this component contract"))?;
    for (target, binding) in bindings {
        let target_schema = targets
            .get(target)
            .ok_or_else(|| format!("{path}.{target} is not an allowed binding target"))?;
        validate_json_schema_value(target_schema, binding, &format!("{path}.{target}"))?;
    }
    Ok(())
}

fn validate_json_schema_value(schema: &Value, value: &Value, path: &str) -> Result<(), String> {
    if let Some(constraint) = schema.get("const") {
        if constraint != value {
            return Err(format!("{path} does not match its constant value"));
        }
    }
    if let Some(values) = schema.get("enum").and_then(Value::as_array) {
        if !values.contains(value) {
            return Err(format!("{path} is not an allowed value"));
        }
    }
    if let Some(types) = schema.get("type") {
        let valid = if let Some(kind) = types.as_str() {
            json_value_has_type(value, kind)
        } else if let Some(kinds) = types.as_array() {
            kinds
                .iter()
                .filter_map(Value::as_str)
                .any(|kind| json_value_has_type(value, kind))
        } else {
            false
        };
        if !valid {
            return Err(format!("{path} has a value type outside its contract"));
        }
    }
    for keyword in ["minimum", "maximum"] {
        if let Some(limit) = schema.get(keyword) {
            let limit = limit
                .as_number()
                .ok_or_else(|| format!("{path} contract has a non-numeric {keyword}"))?;
            if let Some(actual) = value.as_number() {
                let ordering = compare_json_numbers(actual, limit)
                    .ok_or_else(|| format!("{path} cannot be compared with its {keyword} bound"))?;
                let violates_bound = match (keyword, ordering) {
                    ("minimum", std::cmp::Ordering::Less)
                    | ("maximum", std::cmp::Ordering::Greater) => true,
                    _ => false,
                };
                if violates_bound {
                    return Err(format!(
                        "{path} is outside the {keyword} bound in its contract"
                    ));
                }
            }
        }
    }
    for keyword in ["anyOf", "oneOf"] {
        if let Some(choices) = schema.get(keyword).and_then(Value::as_array) {
            let matches = choices
                .iter()
                .filter(|choice| validate_json_schema_value(choice, value, path).is_ok())
                .count();
            let valid = if keyword == "oneOf" {
                matches == 1
            } else {
                matches > 0
            };
            if !valid {
                return Err(format!("{path} does not match {keyword} in its contract"));
            }
        }
    }
    if let Some(required) = schema.get("required").and_then(Value::as_array) {
        let object = value
            .as_object()
            .ok_or_else(|| format!("{path} must be an object"))?;
        for key in required.iter().filter_map(Value::as_str) {
            if !object.contains_key(key) {
                return Err(format!("{path}.{key} is required by the new contract"));
            }
        }
    }
    if let Some(property_names_schema) = schema.get("propertyNames") {
        if let Some(object) = value.as_object() {
            for key in object.keys() {
                validate_json_schema_value(
                    property_names_schema,
                    &Value::String(key.clone()),
                    &format!("{path} property name '{key}'"),
                )?;
            }
        }
    }
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        let object = value
            .as_object()
            .ok_or_else(|| format!("{path} must be an object"))?;
        for (key, nested_value) in object {
            if let Some(nested_schema) = properties.get(key) {
                validate_json_schema_value(nested_schema, nested_value, &format!("{path}.{key}"))?;
            } else if schema.get("additionalProperties") == Some(&Value::Bool(false)) {
                return Err(format!("{path}.{key} is not accepted by the new contract"));
            } else if let Some(additional_schema) = schema
                .get("additionalProperties")
                .filter(|value| value.is_object())
            {
                validate_json_schema_value(
                    additional_schema,
                    nested_value,
                    &format!("{path}.{key}"),
                )?;
            }
        }
    }
    if let Some(items) = schema.get("items") {
        let array = value
            .as_array()
            .ok_or_else(|| format!("{path} must be an array"))?;
        if schema
            .get("minItems")
            .and_then(Value::as_u64)
            .is_some_and(|minimum| array.len() < minimum as usize)
        {
            return Err(format!(
                "{path} has fewer items than the new contract requires"
            ));
        }
        for (index, item) in array.iter().enumerate() {
            validate_json_schema_value(items, item, &format!("{path}[{index}]"))?;
        }
    }
    if let Some(minimum) = schema.get("minLength").and_then(Value::as_u64) {
        if value
            .as_str()
            .is_some_and(|text| text.chars().count() < minimum as usize)
        {
            return Err(format!("{path} is shorter than the new contract allows"));
        }
    }
    let known_annotations = [
        "$schema",
        "description",
        "title",
        "default",
        "examples",
        "deprecated",
        "x-foundation-element-bindings",
        "x-foundation-slots",
    ];
    if let Some(object) = schema.as_object() {
        let known_keywords = [
            "type",
            "const",
            "enum",
            "anyOf",
            "oneOf",
            "required",
            "properties",
            "propertyNames",
            "additionalProperties",
            "items",
            "minItems",
            "minLength",
            "minimum",
            "maximum",
        ];
        if let Some(unknown) = object.keys().find(|key| {
            !known_keywords.contains(&key.as_str()) && !known_annotations.contains(&key.as_str())
        }) {
            return Err(format!(
                "{path} cannot be proven compatible because schema keyword {unknown} is unsupported"
            ));
        }
    }
    Ok(())
}

fn compare_json_numbers(
    left: &serde_json::Number,
    right: &serde_json::Number,
) -> Option<std::cmp::Ordering> {
    use std::cmp::Ordering;

    if let (Some(left), Some(right)) = (left.as_i64(), right.as_i64()) {
        return Some(left.cmp(&right));
    }
    if let (Some(left), Some(right)) = (left.as_u64(), right.as_u64()) {
        return Some(left.cmp(&right));
    }
    if let (Some(left), Some(right)) = (left.as_i64(), right.as_u64()) {
        return Some(if left < 0 {
            Ordering::Less
        } else {
            (left as u64).cmp(&right)
        });
    }
    if let (Some(left), Some(right)) = (left.as_u64(), right.as_i64()) {
        return Some(if right < 0 {
            Ordering::Greater
        } else {
            left.cmp(&(right as u64))
        });
    }
    left.as_f64()?.partial_cmp(&right.as_f64()?)
}

fn json_value_has_type(value: &Value, expected: &str) -> bool {
    match expected {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "boolean" => value.is_boolean(),
        "number" => value.is_number(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "null" => value.is_null(),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn semantic_component(id: &str, version: u64, description: &str) -> SemanticComponent {
        SemanticComponent {
            id: id.to_string(),
            contract_version: version,
            description: description.to_string(),
            props_schema: serde_json::json!({
                "type": "object",
                "properties": {"label": {"type": "string"}}
            }),
            slots: Vec::new(),
        }
    }

    fn test_manifest(components: Vec<SemanticComponent>) -> SemanticCatalogManifest {
        let examples = components
            .iter()
            .map(|component| {
                (
                    component.id.clone(),
                    serde_json::json!({
                        "root": "root",
                        "elements": {"root": {"type": component.id}}
                    }),
                )
            })
            .collect();
        SemanticCatalogManifest {
            version: 2,
            components,
            actions: Vec::new(),
            examples,
        }
    }

    fn create_unversioned_contract(conn: &mut Connection, contract: &ComponentContract) {
        let iri = format!("foundation:SemanticComponentDefinition_{}", contract.id);
        let individual = Individual::new(&iri);
        individual
            .assert_without_icon(
                conn,
                COMPONENT_CLASS,
                &format!("{} legacy", contract.id),
                "test",
            )
            .unwrap();
        for (predicate, value) in [
            (COMPONENT_ID, contract.id.clone()),
            (COMPONENT_INTENT, contract.intent.clone()),
            (COMPONENT_PROPS_SCHEMA, contract.props_schema.clone()),
            (COMPONENT_ACTIONS_SCHEMA, contract.actions_schema.clone()),
            (COMPONENT_EXAMPLES, contract.examples.clone()),
            (
                COMPONENT_MANIFEST_VERSION,
                contract.manifest_version.clone(),
            ),
        ] {
            individual
                .add_property(
                    conn,
                    predicate,
                    vec![Object::Literal {
                        value,
                        datatype: Some("xsd:string".to_string()),
                        language: None,
                    }],
                    "test",
                )
                .unwrap();
        }
        let active_status_iri = crate::core_ontology::status::resolve_active_status(conn).unwrap();
        individual
            .add_property(
                conn,
                "foundation:hasStatus",
                vec![Object::Iri(active_status_iri)],
                "test",
            )
            .unwrap();
    }

    fn seed_catalog_test_ontology(conn: &mut Connection) {
        for (iri, label, parent) in [
            ("owl:NamedIndividual", "Named Individual", None),
            ("foundation:Continuant", "Continuant", None),
            (
                "foundation:AbstractContinuant",
                "Abstract Continuant",
                Some("foundation:Continuant"),
            ),
            (
                SPEC_CLASS,
                "JSON Render Spec",
                Some("foundation:AbstractContinuant"),
            ),
            ("foundation:Status", "Status", None),
        ] {
            if !Class::exists(conn, iri) {
                Class::new(iri)
                    .assert(conn, ClassType::OwlClass, label, "category", parent, "test")
                    .unwrap();
            }
        }
        if Property::get(conn, "foundation:hasStatus")
            .unwrap()
            .is_none()
        {
            Property::new("foundation:hasStatus")
                .assert(
                    conn,
                    PropertyType::ObjectProperty,
                    "Has Status",
                    None,
                    &["foundation:Continuant"],
                    Some("foundation:Status"),
                    None,
                    "test",
                )
                .unwrap();
        }
        if Property::get(conn, SPEC_SOURCE).unwrap().is_none() {
            Property::new(SPEC_SOURCE)
                .assert(
                    conn,
                    PropertyType::DatatypeProperty,
                    "Spec Source",
                    None,
                    &[SPEC_CLASS],
                    Some("xsd:string"),
                    None,
                    "test",
                )
                .unwrap();
        }
        if Property::get(conn, SPEC_RENDERS_ENTITY).unwrap().is_none() {
            Property::new(SPEC_RENDERS_ENTITY)
                .assert(
                    conn,
                    PropertyType::ObjectProperty,
                    "Renders Entity",
                    None,
                    &[SPEC_CLASS],
                    Some("owl:Thing"),
                    None,
                    "test",
                )
                .unwrap();
        }
        if Property::get(conn, CORE_INDIVIDUAL).unwrap().is_none() {
            Property::new(CORE_INDIVIDUAL)
                .assert(
                    conn,
                    PropertyType::ObjectProperty,
                    "Includes Individual",
                    None,
                    &["owl:NamedIndividual"],
                    None,
                    None,
                    "test",
                )
                .unwrap();
        }
        if Individual::get(conn, CORE_REGISTRY).unwrap().is_none() {
            Individual::new(CORE_REGISTRY)
                .assert_without_icon(
                    conn,
                    "owl:NamedIndividual",
                    "Core Ontology Registry",
                    "test",
                )
                .unwrap();
        }
        if crate::core_ontology::status::resolve_active_status(conn).is_err() {
            Individual::new("test:ActiveStatus")
                .assert_without_icon(conn, "foundation:Status", "Ativo", "test")
                .unwrap();
        }
    }

    #[test]
    fn component_iri_is_stable_and_includes_contract_version() {
        assert_eq!(
            component_iri("ValueEntry", 3),
            "foundation:SemanticComponentDefinition_ValueEntry_v3"
        );
    }

    #[test]
    fn compatibility_proof_enforces_inclusive_numeric_minimum_and_maximum() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "columns": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 6
                }
            },
            "required": ["columns"],
            "additionalProperties": false
        });

        for columns in [1, 3, 6] {
            assert!(validate_json_schema_value(
                &schema,
                &serde_json::json!({"columns": columns}),
                "props"
            )
            .is_ok());
        }
        for columns in [0, 7] {
            assert!(validate_json_schema_value(
                &schema,
                &serde_json::json!({"columns": columns}),
                "props"
            )
            .is_err());
        }
    }

    #[test]
    fn compatibility_proof_validates_object_property_names() {
        let schema = serde_json::json!({
            "type": "object",
            "propertyNames": { "minLength": 2 },
            "additionalProperties": { "type": "string" }
        });

        assert!(validate_json_schema_value(
            &schema,
            &serde_json::json!({ "ab": "value" }),
            "props"
        )
        .is_ok());
        assert!(
            validate_json_schema_value(&schema, &serde_json::json!({ "a": "value" }), "props")
                .is_err()
        );
    }

    #[test]
    fn compatibility_hash_ignores_json_object_key_order() {
        let first = contract_hash(
            "ValueEntry",
            "Displays a value",
            r#"{"properties":{"label":{"type":"string"}},"required":["label"]}"#,
            r#"[{"id":"announce","paramsSchema":{"required":["message"],"type":"object"}}]"#,
            r#"{"root":"value","elements":{"value":{"type":"ValueEntry"}}}"#,
        )
        .unwrap();
        let reordered = contract_hash(
            "ValueEntry",
            "Displays a value",
            r#"{"required":["label"],"properties":{"label":{"type":"string"}}}"#,
            r#"[{"paramsSchema":{"type":"object","required":["message"]},"id":"announce"}]"#,
            r#"{"elements":{"value":{"type":"ValueEntry"}},"root":"value"}"#,
        )
        .unwrap();
        assert_eq!(first, reordered);
    }

    #[test]
    fn compatibility_hash_changes_when_a_required_property_changes() {
        let first = contract_hash(
            "ValueEntry",
            "Displays a value",
            r#"{"properties":{"label":{"type":"string"}},"required":["label"]}"#,
            "[]",
            "{}",
        )
        .unwrap();
        let changed = contract_hash(
            "ValueEntry",
            "Displays a value",
            r#"{"properties":{"label":{"type":"string"},"valueType":{"type":"string"}},"required":["label","valueType"]}"#,
            "[]",
            "{}",
        )
        .unwrap();
        assert_ne!(first, changed);
    }

    #[test]
    fn compatibility_hash_includes_intent_and_examples() {
        let baseline = contract_hash(
            "ValueEntry",
            "Displays a value",
            r#"{"type":"object","properties":{"label":{"type":"string"}}}"#,
            "[]",
            r#"{"root":"value","elements":{"value":{"type":"ValueEntry"}}}"#,
        )
        .unwrap();
        let changed_intent = contract_hash(
            "ValueEntry",
            "Displays a value with its source",
            r#"{"type":"object","properties":{"label":{"type":"string"}}}"#,
            "[]",
            r#"{"root":"value","elements":{"value":{"type":"ValueEntry"}}}"#,
        )
        .unwrap();
        let changed_example = contract_hash(
            "ValueEntry",
            "Displays a value",
            r#"{"type":"object","properties":{"label":{"type":"string"}}}"#,
            "[]",
            r#"{"root":"value","elements":{"value":{"type":"ValueEntry"},"source":{"type":"ValueEntry"}}}"#,
        )
        .unwrap();

        assert_ne!(baseline, changed_intent);
        assert_ne!(baseline, changed_example);
    }

    #[test]
    fn spec_component_ids_are_unique_and_sorted() {
        let source = r#"{"root":"a","elements":{"a":{"type":"ValueEntry"},"b":{"type":"Decision"},"c":{"type":"ValueEntry"}}}"#;
        assert_eq!(
            spec_component_ids(source).unwrap(),
            vec!["Decision".to_string(), "ValueEntry".to_string()]
        );
    }

    #[test]
    fn empty_spec_has_no_component_contracts() {
        assert_eq!(
            spec_component_ids(r#"{"root":"","elements":{}}"#),
            Ok(Vec::new())
        );
    }

    #[test]
    fn spec_component_ids_distinguishes_invalid_structure_from_json_errors() {
        assert_eq!(
            spec_component_ids(r#"{"invalid":true}"#),
            Err(SpecComponentIdsError::MissingRoot)
        );
        assert_eq!(
            spec_component_ids(r#"{"root":"root"}"#),
            Err(SpecComponentIdsError::MissingElements)
        );
        assert!(matches!(
            spec_component_ids(r#"{"root":"root","elements":[]}"#),
            Err(SpecComponentIdsError::InvalidElements)
        ));
        assert!(matches!(
            spec_component_ids("{"),
            Err(SpecComponentIdsError::InvalidJson(_))
        ));
    }

    #[test]
    fn malformed_spec_source_error_names_spec_and_recommends_regeneration() {
        let mut conn = crate::eavto::test_helpers::setup_test_db();
        seed_catalog_test_ontology(&mut conn);
        let spec_iri = "foundation:JsonRenderSpec_MalformedSource";
        Individual::new(spec_iri)
            .assert_without_icon(&mut conn, SPEC_CLASS, "Malformed spec", "test")
            .unwrap();
        Individual::new(spec_iri)
            .add_property(
                &mut conn,
                SPEC_SOURCE,
                vec![Object::Literal {
                    value: r#"{"root":"root","elements":{"root":{"type":"Information"}}"#
                        .to_string(),
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                }],
                "test",
            )
            .unwrap();

        let error = inspect_semantic_spec_compatibility(&conn, spec_iri).unwrap_err();
        assert_eq!(error.user_error_code(), Some("invalid_spec_structure"));
        assert!(error.diagnostic().contains(spec_iri));
        assert!(error.diagnostic().contains("malformed specSource JSON"));
        assert!(error.diagnostic().contains("EOF"));
    }

    #[test]
    fn structurally_invalid_spec_returns_a_stable_error_code_with_internal_diagnostic() {
        let mut conn = crate::eavto::test_helpers::setup_test_db();
        seed_catalog_test_ontology(&mut conn);
        let spec_iri = "foundation:JsonRenderSpec_MissingElements";
        Individual::new(spec_iri)
            .assert_without_icon(&mut conn, SPEC_CLASS, "Invalid spec", "test")
            .unwrap();
        Individual::new(spec_iri)
            .add_property(
                &mut conn,
                SPEC_SOURCE,
                vec![Object::Literal {
                    value: r#"{"root":"root","invalid":true}"#.to_string(),
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                }],
                "test",
            )
            .unwrap();

        let error = inspect_semantic_spec_compatibility(&conn, spec_iri).unwrap_err();
        assert_eq!(error.user_error_code(), Some("invalid_spec_structure"));
        assert!(error.diagnostic().contains(spec_iri));
        assert!(error.diagnostic().contains("no elements object"));
    }

    #[test]
    fn generated_manifest_contains_the_versioned_catalog_contract() {
        let manifest: SemanticCatalogManifest = serde_json::from_str(MANIFEST).unwrap();
        let expected_versions = BTreeMap::from([
            ("Action", 1_u64),
            ("Comparison", 2),
            ("Confirmation", 1),
            ("Decision", 1),
            ("Details", 2),
            ("Distribution", 2),
            ("Information", 0),
            ("ValueEntry", 2),
        ]);
        let component_ids = manifest
            .components
            .iter()
            .map(|component| component.id.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        let action_ids = manifest
            .actions
            .iter()
            .filter_map(|action| action.get("id").and_then(Value::as_str))
            .collect::<std::collections::BTreeSet<_>>();
        let example_ids = manifest
            .examples
            .keys()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>();

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
            assert!(component
                .props_schema
                .get("properties")
                .and_then(Value::as_object)
                .is_some());
            assert!(component
                .props_schema
                .get("required")
                .and_then(Value::as_array)
                .is_some());
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
    fn semantic_spec_read_reports_current_compatible_and_required_update_states() {
        let mut conn = crate::eavto::test_helpers::setup_test_db();
        seed_catalog_test_ontology(&mut conn);
        sync_schema(&mut conn).unwrap();
        sync_schema(&mut conn).unwrap();
        sync(&mut conn).unwrap();
        let spec_iri = "foundation:JsonRenderSpec_CompatibilityRead";
        let current_source = r#"{"root":"amount","rendersEntity":"foundation:MonthlyBudget_Example","elements":{"amount":{"type":"ValueEntry","props":{"label":"Total","valueType":"currency","editable":true}}}}"#;
        Individual::new("foundation:MonthlyBudget_Example")
            .assert_without_icon(
                &mut conn,
                "owl:NamedIndividual",
                "Example monthly budget",
                "test",
            )
            .unwrap();
        Individual::new(spec_iri)
            .assert_without_icon(&mut conn, SPEC_CLASS, "Compatibility fixture", "test")
            .unwrap();
        Individual::new(spec_iri)
            .add_property(
                &mut conn,
                SPEC_SOURCE,
                vec![Object::Literal {
                    value: current_source.to_string(),
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                }],
                "test",
            )
            .unwrap();
        Individual::new(spec_iri)
            .add_property(
                &mut conn,
                SPEC_RENDERS_ENTITY,
                vec![Object::Iri("foundation:MonthlyBudget_Example".to_string())],
                "test",
            )
            .unwrap();
        link_spec_to_latest_contracts(&mut conn, spec_iri, current_source).unwrap();

        let current = inspect_semantic_spec_compatibility(&conn, spec_iri).unwrap();
        assert_eq!(current.renders_entity, "foundation:MonthlyBudget_Example");
        assert_eq!(current.component_compatibility[0].status, "current");
        assert_eq!(current.component_compatibility[0].pinned_version, Some(2));
        assert_eq!(current.component_compatibility[0].target_version, Some(2));

        let current_contract = read_contract(&conn, &component_iri("ValueEntry", 2)).unwrap();
        create_contract(&mut conn, &current_contract.contract, 1).unwrap();
        replace_literal_property(
            &mut conn,
            &component_iri("ValueEntry", 1),
            CONTRACT_HASH,
            "sha256:historical-algorithm",
        )
        .unwrap();
        set_spec_contracts(&mut conn, spec_iri, &[component_iri("ValueEntry", 1)]).unwrap();
        let compatible_update = inspect_semantic_spec_compatibility(&conn, spec_iri).unwrap();
        assert_eq!(
            compatible_update.component_compatibility[0].status,
            "update_available"
        );
        assert_eq!(
            compatible_update.component_compatibility[0].pinned_version,
            Some(1)
        );
        assert_eq!(
            compatible_update.component_compatibility[0].target_version,
            Some(2)
        );

        let incompatible_source = current_source.replace("currency", "not-a-value-type");
        replace_literal_property(&mut conn, spec_iri, SPEC_SOURCE, &incompatible_source).unwrap();
        let incompatible = inspect_semantic_spec_compatibility(&conn, spec_iri).unwrap();
        assert_eq!(
            incompatible.component_compatibility[0].status,
            "update_required"
        );
        assert!(incompatible.component_compatibility[0]
            .reason
            .contains("agent to update"));
    }

    #[test]
    fn registry_sync_only_appends_missing_contracts_and_is_idempotent() {
        let versioned = vec![
            "foundation:SemanticComponentDefinition_Action_v1".to_string(),
            "foundation:SemanticComponentDefinition_Decision_v1".to_string(),
        ];
        let existing = vec!["foundation:SemanticComponentDefinition_Action_v1".to_string()];

        let missing = missing_contract_iris(&versioned, &existing);
        assert_eq!(
            missing,
            vec!["foundation:SemanticComponentDefinition_Decision_v1".to_string()]
        );

        let complete = [
            existing[0].clone(),
            "foundation:SemanticComponentDefinition_Decision_v1".to_string(),
        ];
        assert!(missing_contract_iris(&versioned, &complete).is_empty());
        assert!(ensure_registry_complete(&versioned, &complete).is_ok());
    }

    #[test]
    fn registry_sync_fails_when_any_versioned_contract_is_not_persisted() {
        let versioned = vec!["foundation:SemanticComponentDefinition_Decision_v1".to_string()];
        let error = ensure_registry_complete(&versioned, &[]).unwrap_err();
        assert!(error.contains("SemanticComponentDefinition_Decision_v1"));
    }

    #[test]
    fn schema_sync_installs_app_catalog_and_is_idempotent() {
        let mut conn = crate::eavto::test_helpers::setup_test_db();
        seed_catalog_test_ontology(&mut conn);

        sync_schema(&mut conn).unwrap();
        sync_schema(&mut conn).unwrap();

        assert!(Class::exists(&conn, COMPONENT_CLASS));
        let contract_property = Property::get(&conn, SPEC_CONTRACT).unwrap().unwrap();
        assert_eq!(
            contract_property.property_type,
            PropertyType::ObjectProperty
        );
        assert_eq!(contract_property.domains, vec![SPEC_CLASS.to_string()]);
        assert_eq!(contract_property.ranges, vec![COMPONENT_CLASS.to_string()]);
        let default_spec_property = Property::get(&conn, CLASS_DEFAULT_WIDGET_SPEC)
            .unwrap()
            .unwrap();
        assert_eq!(
            default_spec_property.property_type,
            PropertyType::ObjectProperty
        );
        assert_eq!(default_spec_property.domains, vec!["owl:Class".to_string()]);
        assert_eq!(default_spec_property.ranges, vec![SPEC_CLASS.to_string()]);
        for property in [
            COMPONENT_ID,
            COMPONENT_INTENT,
            COMPONENT_PROPS_SCHEMA,
            COMPONENT_ACTIONS_SCHEMA,
            COMPONENT_EXAMPLES,
            COMPONENT_MANIFEST_VERSION,
            CONTRACT_VERSION,
            CONTRACT_HASH,
        ] {
            assert!(Property::get(&conn, property).unwrap().is_some());
        }
    }

    #[test]
    fn schema_sync_rejects_default_spec_property_with_non_class_domain() {
        let mut conn = crate::eavto::test_helpers::setup_test_db();
        seed_catalog_test_ontology(&mut conn);
        Property::new(CLASS_DEFAULT_WIDGET_SPEC)
            .assert(
                &mut conn,
                PropertyType::ObjectProperty,
                "Default Widget Specification",
                None,
                &["owl:Class", "foundation:MonthlyBudget"],
                Some(SPEC_CLASS),
                None,
                "test",
            )
            .unwrap();

        let error = sync_schema(&mut conn).unwrap_err();

        assert!(error.contains("incompatible domains"));
        assert!(error.contains("foundation:MonthlyBudget"));
    }

    #[test]
    fn startup_sync_creates_contracts_and_registry_memberships_once() {
        let mut conn = crate::eavto::test_helpers::setup_test_db();
        seed_catalog_test_ontology(&mut conn);
        sync_schema(&mut conn).unwrap();
        let expected_contract_count = serde_json::from_str::<SemanticCatalogManifest>(MANIFEST)
            .unwrap()
            .components
            .len();

        let first = sync(&mut conn).unwrap();
        let second = sync(&mut conn).unwrap();

        assert_eq!(first.contract_versions_created, expected_contract_count);
        assert_eq!(first.registry_contracts_verified, expected_contract_count);
        assert_eq!(second.contract_versions_created, 0);
        assert_eq!(second.registry_links_added, 0);
        assert_eq!(second.registry_contracts_verified, expected_contract_count);
        let contracts = Class::get_instances(&conn, COMPONENT_CLASS).unwrap();
        assert_eq!(contracts.len(), expected_contract_count);
        let registered =
            owl::get_all_iri_properties(&conn, CORE_REGISTRY, CORE_INDIVIDUAL).unwrap();
        assert_eq!(registered.len(), expected_contract_count);
        assert!(contracts.iter().all(|iri| registered.contains(iri)));
    }

    #[test]
    fn startup_sync_preserves_existing_details_v1_and_registers_manifest_v2_idempotently() {
        let mut conn = crate::eavto::test_helpers::setup_test_db();
        seed_catalog_test_ontology(&mut conn);
        sync_schema(&mut conn).unwrap();

        let manifest: SemanticCatalogManifest = serde_json::from_str(MANIFEST).unwrap();
        let actions_schema = canonical_json(&Value::Array(manifest.actions.clone())).unwrap();
        let details = manifest
            .components
            .iter()
            .find(|component| component.id == "Details")
            .unwrap();
        assert_eq!(details.contract_version, 2);
        let legacy = make_contract(
            details.id.clone(),
            "Historical Details v1 contract".to_string(),
            serde_json::json!({
                "type": "object",
                "properties": {"legacySummary": {"type": "string"}}
            }),
            Vec::new(),
            actions_schema,
            serde_json::json!({"root":"legacy","elements":{"legacy":{"type":"Details"}}}),
            "1".to_string(),
        )
        .unwrap();
        create_contract(&mut conn, &legacy, 1).unwrap();
        let preserved_v1 = read_contract(&conn, &component_iri("Details", 1)).unwrap();

        let first = sync(&mut conn).unwrap();

        assert!(Individual::get(&conn, &component_iri("Details", 2))
            .unwrap()
            .is_some());
        let stored_v1 = read_contract(&conn, &component_iri("Details", 1)).unwrap();
        assert_eq!(stored_v1.iri, preserved_v1.iri);
        assert_eq!(stored_v1.version, preserved_v1.version);
        assert_eq!(
            stored_v1.contract, preserved_v1.contract,
            "startup must leave the existing v1 definition and hash untouched"
        );
        assert_eq!(first.contract_versions_created, manifest.components.len());
        let registered =
            owl::get_all_iri_properties(&conn, CORE_REGISTRY, CORE_INDIVIDUAL).unwrap();
        assert!(registered.contains(&component_iri("Details", 1)));
        assert!(registered.contains(&component_iri("Details", 2)));

        let tx_after_first_sync = owl::current_snapshot_tx(&conn).unwrap();
        let second = sync(&mut conn).unwrap();

        assert_eq!(second.contract_versions_created, 0);
        assert_eq!(second.registry_links_added, 0);
        assert_eq!(
            owl::current_snapshot_tx(&conn).unwrap(),
            tx_after_first_sync,
            "repeated startup sync must not append ontology writes"
        );
        let stored_v1 = read_contract(&conn, &component_iri("Details", 1)).unwrap();
        assert_eq!(stored_v1.iri, preserved_v1.iri);
        assert_eq!(stored_v1.version, preserved_v1.version);
        assert_eq!(stored_v1.contract, preserved_v1.contract);
    }

    #[test]
    fn startup_sync_repins_unversioned_specs_to_an_exact_version_and_creates_v6_idempotently() {
        let mut conn = crate::eavto::test_helpers::setup_test_db();
        seed_catalog_test_ontology(&mut conn);
        sync_schema(&mut conn).unwrap();

        let legacy_manifest = test_manifest(vec![semantic_component(
            "Information",
            1,
            "Historical information",
        )]);
        let legacy_component = &legacy_manifest.components[0];
        let legacy_contract = make_contract(
            legacy_component.id.clone(),
            legacy_component.description.clone(),
            legacy_component.props_schema.clone(),
            legacy_component.slots.clone(),
            "[]".to_string(),
            legacy_manifest.examples["Information"].clone(),
            legacy_manifest.version.to_string(),
        )
        .unwrap();
        create_contract(&mut conn, &legacy_contract, 1).unwrap();
        create_unversioned_contract(&mut conn, &legacy_contract);
        let legacy_iri = format!(
            "foundation:SemanticComponentDefinition_{}",
            legacy_contract.id
        );
        replace_literal_property(&mut conn, &legacy_iri, CONTRACT_HASH, &legacy_contract.hash)
            .unwrap();

        let current_component = semantic_component("Information", 6, "Current information");
        let current_manifest = test_manifest(vec![current_component.clone()]);
        let current_contract = make_contract(
            current_component.id.clone(),
            current_component.description.clone(),
            current_component.props_schema.clone(),
            current_component.slots.clone(),
            "[]".to_string(),
            current_manifest.examples["Information"].clone(),
            current_manifest.version.to_string(),
        )
        .unwrap();
        create_contract(&mut conn, &current_contract, 5).unwrap();

        let spec_iri = "foundation:JsonRenderSpec_LegacyInformationPin";
        let spec_source = r#"{"root":"info","elements":{"info":{"type":"Information","props":{"label":"Resumo"}}}}"#;
        Individual::new(spec_iri)
            .assert_without_icon(&mut conn, SPEC_CLASS, "Legacy information pin", "test")
            .unwrap();
        Individual::new(spec_iri)
            .add_property(
                &mut conn,
                SPEC_SOURCE,
                vec![Object::Literal {
                    value: spec_source.to_string(),
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                }],
                "test",
            )
            .unwrap();
        set_spec_contracts(&mut conn, spec_iri, &[legacy_iri.clone()]).unwrap();
        let preserved_legacy = read_contract(&conn, &legacy_iri).unwrap();

        let first = sync_manifest(&mut conn, current_manifest.clone()).unwrap();

        assert_eq!(first.contract_versions_created, 1);
        assert!(Individual::get(&conn, &component_iri("Information", 6))
            .unwrap()
            .is_some());
        assert_eq!(
            owl::get_all_iri_properties(&conn, spec_iri, SPEC_CONTRACT).unwrap(),
            vec![component_iri("Information", 6)]
        );
        assert_eq!(
            read_contract(&conn, &legacy_iri).unwrap().contract,
            preserved_legacy.contract,
            "migration must preserve the unversioned definition"
        );

        let tx_after_first_sync = owl::current_snapshot_tx(&conn).unwrap();
        let second = sync_manifest(&mut conn, current_manifest).unwrap();
        assert_eq!(second.contract_versions_created, 0);
        assert_eq!(second.registry_links_added, 0);
        assert_eq!(
            owl::current_snapshot_tx(&conn).unwrap(),
            tx_after_first_sync,
            "the legacy pin migration and v6 creation must be idempotent"
        );
    }

    #[test]
    fn startup_sync_rejects_unversioned_contract_without_equivalent_version_or_reset() {
        let mut conn = crate::eavto::test_helpers::setup_test_db();
        seed_catalog_test_ontology(&mut conn);
        sync_schema(&mut conn).unwrap();

        let legacy_manifest = test_manifest(vec![semantic_component(
            "Information",
            1,
            "Unknown legacy information",
        )]);
        let legacy_component = &legacy_manifest.components[0];
        let legacy_contract = make_contract(
            legacy_component.id.clone(),
            legacy_component.description.clone(),
            legacy_component.props_schema.clone(),
            legacy_component.slots.clone(),
            "[]".to_string(),
            legacy_manifest.examples["Information"].clone(),
            legacy_manifest.version.to_string(),
        )
        .unwrap();
        create_unversioned_contract(&mut conn, &legacy_contract);

        let current_component = semantic_component("Information", 6, "Current information");
        let current_manifest = test_manifest(vec![current_component.clone()]);
        let current_contract = make_contract(
            current_component.id.clone(),
            current_component.description.clone(),
            current_component.props_schema.clone(),
            current_component.slots.clone(),
            "[]".to_string(),
            current_manifest.examples["Information"].clone(),
            current_manifest.version.to_string(),
        )
        .unwrap();
        create_contract(&mut conn, &current_contract, 5).unwrap();

        let tx_before_sync = owl::current_snapshot_tx(&conn).unwrap();
        let error = sync_manifest(&mut conn, current_manifest).unwrap_err();

        assert!(error.contains("exact versioned contract match is required"));
        assert!(Individual::get(&conn, &component_iri("Information", 6))
            .unwrap()
            .is_none());
        assert_eq!(
            owl::current_snapshot_tx(&conn).unwrap(),
            tx_before_sync,
            "a normal version bump cannot silently reset an unmatched legacy contract"
        );
    }

    #[test]
    fn startup_sync_resets_information_to_v0_and_migrates_legacy_spec_pin() {
        let mut conn = crate::eavto::test_helpers::setup_test_db();
        seed_catalog_test_ontology(&mut conn);
        sync_schema(&mut conn).unwrap();

        let legacy_manifest = test_manifest(vec![semantic_component(
            "Information",
            1,
            "Historical information",
        )]);
        let legacy_component = &legacy_manifest.components[0];
        let legacy_contract = make_contract(
            legacy_component.id.clone(),
            legacy_component.description.clone(),
            legacy_component.props_schema.clone(),
            legacy_component.slots.clone(),
            "[]".to_string(),
            legacy_manifest.examples["Information"].clone(),
            legacy_manifest.version.to_string(),
        )
        .unwrap();
        let legacy_iri = format!(
            "foundation:SemanticComponentDefinition_{}",
            legacy_contract.id
        );
        create_unversioned_contract(&mut conn, &legacy_contract);
        replace_literal_property(&mut conn, &legacy_iri, CONTRACT_HASH, &legacy_contract.hash)
            .unwrap();

        let divergent_v1 = make_contract(
            "Information".to_string(),
            "Divergent v1 intent".to_string(),
            serde_json::from_str(&legacy_contract.props_schema).unwrap(),
            Vec::new(),
            "[]".to_string(),
            serde_json::from_str(&legacy_contract.examples).unwrap(),
            legacy_contract.manifest_version.clone(),
        )
        .unwrap();
        create_contract(&mut conn, &divergent_v1, 1).unwrap();
        replace_literal_property(
            &mut conn,
            &component_iri("Information", 1),
            CONTRACT_HASH,
            &legacy_contract.hash,
        )
        .unwrap();
        let preserved_v1 = read_contract_diagnostic(&conn, &component_iri("Information", 1))
            .unwrap()
            .0;
        assert!(!preserved_v1.integrity_valid);

        let info_v4 = make_contract(
            "Information".to_string(),
            "Historical v4 information".to_string(),
            serde_json::json!({"type":"object","properties":{"label":{"type":"string"}}}),
            Vec::new(),
            "[]".to_string(),
            serde_json::json!({"version": 4}),
            "2".to_string(),
        )
        .unwrap();
        let info_v6 = make_contract(
            "Information".to_string(),
            "Historical v6 information".to_string(),
            serde_json::json!({"type":"object","properties":{"label":{"type":"string"}}}),
            Vec::new(),
            "[]".to_string(),
            serde_json::json!({"version": 6}),
            "2".to_string(),
        )
        .unwrap();
        create_contract(&mut conn, &info_v4, 4).unwrap();
        create_contract(&mut conn, &info_v6, 6).unwrap();
        let details = make_contract(
            "Details".to_string(),
            "Shows supporting details".to_string(),
            serde_json::json!({"type":"object","properties":{"label":{"type":"string"}}}),
            Vec::new(),
            "[]".to_string(),
            serde_json::json!({"component": "Details"}),
            "2".to_string(),
        )
        .unwrap();
        create_contract(&mut conn, &details, 2).unwrap();
        let old_information_iris = vec![
            legacy_iri.clone(),
            component_iri("Information", 1),
            component_iri("Information", 4),
            component_iri("Information", 6),
        ];
        Individual::new(CORE_REGISTRY)
            .append_property(
                &mut conn,
                CORE_INDIVIDUAL,
                old_information_iris
                    .iter()
                    .cloned()
                    .chain(std::iter::once(component_iri("Details", 2)))
                    .map(Object::Iri)
                    .collect(),
                "test",
            )
            .unwrap();

        let spec_iris = (0..4)
            .map(|index| format!("foundation:JsonRenderSpec_InfoReset_{index}"))
            .collect::<Vec<_>>();
        let mut spec_sources = Vec::new();
        for (index, spec_iri) in spec_iris.iter().enumerate() {
            let spec_source = serde_json::json!({
                "root": "info",
                "elements": {
                    "info": {
                        "type": "Information",
                        "props": {"title": format!("Resumo {index}")},
                        "bindings": {
                            "content": {
                                "source": "entity",
                                "propertyIri": "foundation:summary"
                            }
                        }
                    },
                    "details": {"type": "Details", "props": {"label": "Detalhes"}}
                }
            })
            .to_string();
            Individual::new(spec_iri)
                .assert_without_icon(&mut conn, SPEC_CLASS, "Information reset test", "test")
                .unwrap();
            Individual::new(spec_iri)
                .add_property(
                    &mut conn,
                    SPEC_SOURCE,
                    vec![Object::Literal {
                        value: spec_source.clone(),
                        datatype: Some("xsd:string".to_string()),
                        language: None,
                    }],
                    "test",
                )
                .unwrap();
            set_spec_contracts(
                &mut conn,
                spec_iri,
                &[
                    component_iri("Information", if index % 2 == 0 { 4 } else { 6 }),
                    component_iri("Details", 2),
                ],
            )
            .unwrap();
            spec_sources.push(spec_source);
        }
        let fixture_iri = "foundation:JsonRenderSpec_TestFixtureBlockingInformationReset";
        Individual::new(fixture_iri)
            .assert_without_icon(&mut conn, SPEC_CLASS, "Obsolete test fixture", "test")
            .unwrap();
        Individual::new(fixture_iri)
            .add_property(
                &mut conn,
                SPEC_SOURCE,
                vec![Object::Literal {
                    value: r#"{"root":"info","elements":{"info":{"type":"Information","props":{"label":"fixture"}}}}"#.to_string(),
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                }],
                "test",
            )
            .unwrap();
        set_spec_contracts(&mut conn, fixture_iri, &[component_iri("Information", 4)]).unwrap();

        let mut current_component = semantic_component("Information", 0, "Current information");
        current_component.props_schema = serde_json::json!({
            "type":"object",
            "additionalProperties":false,
            "required":["title","layout"],
            "properties":{
                "title":{"type":"string"},
                "layout":{
                    "type":"object",
                    "additionalProperties":false,
                    "required":["columns"],
                    "properties":{"columns":{"type":"integer","minimum":1,"maximum":6}}
                }
            },
            "x-foundation-element-bindings": {
                "type":"object",
                "additionalProperties":false,
                "properties": {
                    "content": {
                        "type":"object",
                        "additionalProperties":false,
                        "required":["source","propertyIri"],
                        "properties": {
                            "source":{"type":"string","const":"entity"},
                            "propertyIri":{"type":"string","minLength":1}
                        }
                    }
                }
            }
        });
        let current_manifest = test_manifest(vec![current_component]);
        let result = sync_manifest(&mut conn, current_manifest.clone()).unwrap();

        assert_eq!(result.contract_versions_created, 1);
        assert!(Individual::get(&conn, &component_iri("Information", 0))
            .unwrap()
            .is_some());
        for old_iri in &old_information_iris {
            assert!(Individual::get(&conn, old_iri).unwrap().is_none());
        }
        assert!(Individual::get(&conn, &legacy_iri).unwrap().is_none());
        assert!(Individual::get(&conn, &component_iri("Details", 2))
            .unwrap()
            .is_some());
        assert!(Individual::get(&conn, fixture_iri).unwrap().is_none());
        let registry = owl::get_all_iri_properties(&conn, CORE_REGISTRY, CORE_INDIVIDUAL).unwrap();
        assert!(registry.contains(&component_iri("Information", 0)));
        assert!(registry.contains(&component_iri("Details", 2)));
        for old_iri in &old_information_iris {
            assert!(!registry.contains(old_iri));
        }
        for (spec_iri, original_source) in spec_iris.iter().zip(&spec_sources) {
            let mut pins = owl::get_all_iri_properties(&conn, spec_iri, SPEC_CONTRACT).unwrap();
            pins.sort();
            let mut expected_pins =
                vec![component_iri("Information", 0), component_iri("Details", 2)];
            expected_pins.sort();
            assert_eq!(pins, expected_pins);
            let migrated_source: Value = serde_json::from_str(
                owl::get_literal_property(&conn, spec_iri, SPEC_SOURCE)
                    .unwrap()
                    .as_deref()
                    .unwrap(),
            )
            .unwrap();
            let original_value: Value = serde_json::from_str(original_source).unwrap();
            assert_eq!(
                migrated_source["elements"]["info"]["bindings"],
                original_value["elements"]["info"]["bindings"],
                "the reset must preserve all entity bindings"
            );
            assert_eq!(
                migrated_source["elements"]["info"]["props"]["layout"]["columns"],
                2
            );
            let mut migrated_without_layout = migrated_source;
            migrated_without_layout["elements"]["info"]["props"]
                .as_object_mut()
                .unwrap()
                .remove("layout");
            assert_eq!(migrated_without_layout, original_value);
        }

        let tx_after_first_sync = owl::current_snapshot_tx(&conn).unwrap();
        let second = sync_manifest(&mut conn, current_manifest).unwrap();
        assert_eq!(second.contract_versions_created, 0);
        assert_eq!(
            owl::current_snapshot_tx(&conn).unwrap(),
            tx_after_first_sync,
            "Information reset, spec migration, and v0 sync must be idempotent"
        );
    }

    #[test]
    fn information_v0_reset_aborts_before_writes_when_a_spec_cannot_be_preserved() {
        let mut conn = crate::eavto::test_helpers::setup_test_db();
        seed_catalog_test_ontology(&mut conn);
        sync_schema(&mut conn).unwrap();
        let historical = make_contract(
            "Information".to_string(),
            "Historical information".to_string(),
            serde_json::json!({"type":"object","properties":{"label":{"type":"string"}}}),
            Vec::new(),
            "[]".to_string(),
            serde_json::json!({"version": 4}),
            "2".to_string(),
        )
        .unwrap();
        create_contract(&mut conn, &historical, 4).unwrap();
        Individual::new(CORE_REGISTRY)
            .append_property(
                &mut conn,
                CORE_INDIVIDUAL,
                vec![Object::Iri(component_iri("Information", 4))],
                "test",
            )
            .unwrap();

        let spec_iri = "foundation:JsonRenderSpec_IncompatibleInformationReset";
        let spec_source = r#"{"root":"info","elements":{"info":{"type":"Information","props":{"label":"Resumo","unknown":"perdido"}}}}"#;
        Individual::new(spec_iri)
            .assert_without_icon(&mut conn, SPEC_CLASS, "Incompatible reset", "test")
            .unwrap();
        Individual::new(spec_iri)
            .add_property(
                &mut conn,
                SPEC_SOURCE,
                vec![Object::Literal {
                    value: spec_source.to_string(),
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                }],
                "test",
            )
            .unwrap();
        set_spec_contracts(&mut conn, spec_iri, &[component_iri("Information", 4)]).unwrap();
        Class::new("foundation:TestWidgetOwner")
            .assert(
                &mut conn,
                ClassType::OwlClass,
                "Test Widget Owner",
                "category",
                Some("foundation:AbstractContinuant"),
                "test",
            )
            .unwrap();
        Individual::new("foundation:TestWidgetOwner")
            .add_property(
                &mut conn,
                CLASS_DEFAULT_WIDGET_SPEC,
                vec![Object::Iri(spec_iri.to_string())],
                "test",
            )
            .unwrap();

        let mut current = semantic_component("Information", 0, "Current information");
        current.props_schema = serde_json::json!({
            "type":"object",
            "additionalProperties":false,
            "properties":{"label":{"type":"string"}}
        });
        let current_manifest = test_manifest(vec![current]);
        let tx_before_sync = owl::current_snapshot_tx(&conn).unwrap();
        let error = sync_manifest(&mut conn, current_manifest).unwrap_err();

        assert!(
            error.contains(
                "Cannot reset Information contracts without a compatible minimal migration"
            ),
            "{error}"
        );
        assert!(Individual::get(&conn, &component_iri("Information", 0))
            .unwrap()
            .is_none());
        assert!(Individual::get(&conn, &component_iri("Information", 4))
            .unwrap()
            .is_some());
        assert_eq!(
            owl::get_all_iri_properties(&conn, spec_iri, SPEC_CONTRACT).unwrap(),
            vec![component_iri("Information", 4)]
        );
        assert_eq!(
            owl::get_literal_property(&conn, spec_iri, SPEC_SOURCE)
                .unwrap()
                .as_deref(),
            Some(spec_source)
        );
        assert_eq!(
            owl::get_all_iri_properties(&conn, CORE_REGISTRY, CORE_INDIVIDUAL).unwrap(),
            vec![component_iri("Information", 4)]
        );
        assert_eq!(
            owl::current_snapshot_tx(&conn).unwrap(),
            tx_before_sync,
            "an incompatible spec must abort before any reset writes"
        );
    }

    #[test]
    fn startup_sync_rejects_equal_stored_hashes_when_canonical_content_or_integrity_differs() {
        let mut conn = crate::eavto::test_helpers::setup_test_db();
        seed_catalog_test_ontology(&mut conn);
        sync_schema(&mut conn).unwrap();

        let legacy_manifest = test_manifest(vec![semantic_component(
            "Information",
            1,
            "Historical information",
        )]);
        let legacy_component = &legacy_manifest.components[0];
        let legacy_contract = make_contract(
            legacy_component.id.clone(),
            legacy_component.description.clone(),
            legacy_component.props_schema.clone(),
            legacy_component.slots.clone(),
            "[]".to_string(),
            legacy_manifest.examples["Information"].clone(),
            legacy_manifest.version.to_string(),
        )
        .unwrap();
        let legacy_iri = format!(
            "foundation:SemanticComponentDefinition_{}",
            legacy_contract.id
        );
        create_unversioned_contract(&mut conn, &legacy_contract);
        let shared_corrupt_hash = "sha256:shared-corrupt-hash";
        replace_literal_property(&mut conn, &legacy_iri, CONTRACT_HASH, shared_corrupt_hash)
            .unwrap();

        let divergent_contract = make_contract(
            "Information".to_string(),
            "Divergent versioned information".to_string(),
            serde_json::from_str(&legacy_contract.props_schema).unwrap(),
            Vec::new(),
            "[]".to_string(),
            serde_json::json!({"example": "different"}),
            legacy_contract.manifest_version.clone(),
        )
        .unwrap();
        create_contract(&mut conn, &divergent_contract, 1).unwrap();
        replace_literal_property(
            &mut conn,
            &component_iri("Information", 1),
            CONTRACT_HASH,
            shared_corrupt_hash,
        )
        .unwrap();

        let current_component = semantic_component("Information", 7, "Current information");
        let current_manifest = test_manifest(vec![current_component]);
        let tx_before_sync = owl::current_snapshot_tx(&conn).unwrap();
        let error = sync_manifest(&mut conn, current_manifest).unwrap_err();

        assert!(error.contains("exact versioned contract match is required"));
        assert!(error.contains(&format!("storedHash:Some(\"{shared_corrupt_hash}\")")));
        assert!(error.contains("integrityValid:false"));
        assert!(error.contains("sameDefinition:false"));
        assert!(error.contains("differingFields:[\"intent\", \"examples\"]"));
        assert!(Individual::get(&conn, &component_iri("Information", 7))
            .unwrap()
            .is_none());
        assert_eq!(
            owl::current_snapshot_tx(&conn).unwrap(),
            tx_before_sync,
            "a mismatched legacy pin must abort before any sync writes"
        );
    }

    #[test]
    fn per_component_versions_are_persisted_and_existing_specs_repin_idempotently() {
        let mut conn = crate::eavto::test_helpers::setup_test_db();
        seed_catalog_test_ontology(&mut conn);
        sync_schema(&mut conn).unwrap();

        let initial = test_manifest(vec![
            semantic_component("Decision", 1, "Selects a candidate"),
            semantic_component("ValueEntry", 2, "Presents a value"),
        ]);
        assert_eq!(
            sync_manifest(&mut conn, initial)
                .unwrap()
                .contract_versions_created,
            2
        );
        assert!(Individual::get(&conn, &component_iri("Decision", 1))
            .unwrap()
            .is_some());
        assert!(Individual::get(&conn, &component_iri("ValueEntry", 2))
            .unwrap()
            .is_some());

        let spec_iri = "foundation:JsonRenderSpec_PinnedDecision";
        let spec_source = r#"{"root":"decision","elements":{"decision":{"type":"Decision"}}}"#;
        Individual::new(spec_iri)
            .assert_without_icon(&mut conn, SPEC_CLASS, "Pinned decision", "test")
            .unwrap();
        Individual::new(spec_iri)
            .add_property(
                &mut conn,
                SPEC_SOURCE,
                vec![Object::Literal {
                    value: spec_source.to_string(),
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                }],
                "test",
            )
            .unwrap();
        assert!(link_spec_to_latest_contracts(&mut conn, spec_iri, spec_source).unwrap());

        let updated = test_manifest(vec![
            semantic_component("Decision", 2, "Selects and compares candidates"),
            semantic_component("ValueEntry", 2, "Presents a value"),
        ]);
        assert_eq!(
            sync_manifest(&mut conn, updated.clone())
                .unwrap()
                .contract_versions_created,
            1
        );
        assert_eq!(
            sync_manifest(&mut conn, updated)
                .unwrap()
                .contract_versions_created,
            0
        );
        assert_eq!(
            owl::get_all_iri_properties(&conn, spec_iri, SPEC_CONTRACT).unwrap(),
            vec![component_iri("Decision", 2)],
            "a compatible widget is repinned to the explicitly declared new contract"
        );
        assert!(!link_spec_to_latest_contracts(&mut conn, spec_iri, spec_source).unwrap());
        assert!(!link_spec_to_latest_contracts(&mut conn, spec_iri, spec_source).unwrap());
        assert_eq!(
            owl::get_all_iri_properties(&conn, spec_iri, SPEC_CONTRACT).unwrap(),
            vec![component_iri("Decision", 2)]
        );
        assert_eq!(
            owl::get_literal_property(&conn, spec_iri, SPEC_SOURCE)
                .unwrap()
                .as_deref(),
            Some(spec_source)
        );
    }

    #[test]
    fn declared_version_cannot_rewrite_an_existing_component_contract() {
        let mut conn = crate::eavto::test_helpers::setup_test_db();
        seed_catalog_test_ontology(&mut conn);
        sync_schema(&mut conn).unwrap();
        let first = test_manifest(vec![semantic_component(
            "Decision",
            1,
            "Selects a candidate",
        )]);
        sync_manifest(&mut conn, first).unwrap();

        let changed_without_bump = test_manifest(vec![semantic_component(
            "Decision",
            1,
            "Selects and compares",
        )]);
        let error = sync_manifest(&mut conn, changed_without_bump).unwrap_err();
        assert!(error.contains("contractVersion 1 is immutable"));
        assert!(Individual::get(&conn, &component_iri("Decision", 2))
            .unwrap()
            .is_none());
    }

    #[test]
    fn catalog_sync_aggregates_all_immutable_contract_divergences_before_writes() {
        let mut conn = crate::eavto::test_helpers::setup_test_db();
        seed_catalog_test_ontology(&mut conn);
        sync_schema(&mut conn).unwrap();
        let initial = test_manifest(vec![
            semantic_component("Decision", 1, "Selects a candidate"),
            semantic_component("ValueEntry", 1, "Presents a value"),
        ]);
        sync_manifest(&mut conn, initial).unwrap();

        let changed = test_manifest(vec![
            semantic_component("Decision", 1, "Selects and compares candidates"),
            semantic_component("ValueEntry", 1, "Presents a value with provenance"),
        ]);
        let error = sync_manifest(&mut conn, changed).unwrap_err();
        assert!(error.contains("2 component contract issue(s)"));
        assert!(error.contains("componentId=Decision"));
        assert!(error.contains("componentId=ValueEntry"));
        assert!(error.contains("differingFields:[\"intent\"]"));
        assert!(Individual::get(&conn, &component_iri("Decision", 2))
            .unwrap()
            .is_none());
        assert!(Individual::get(&conn, &component_iri("ValueEntry", 2))
            .unwrap()
            .is_none());
    }

    #[test]
    fn compatible_spec_is_repinned_to_new_contract_without_changing_source() {
        let mut conn = crate::eavto::test_helpers::setup_test_db();
        seed_catalog_test_ontology(&mut conn);
        sync_schema(&mut conn).unwrap();
        let v1 = make_contract(
            "ValueEntry".to_string(),
            "Presents a value".to_string(),
            serde_json::json!({
                "type":"object",
                "properties":{"label":{"type":"string"},"valueType":{"type":"string"}},
                "required":["label","valueType"],
                "x-foundation-element-bindings": {
                    "type":"object",
                    "additionalProperties":false,
                    "properties":{"value":{"type":"object","additionalProperties":false,"required":["source","propertyIri"],"properties":{"source":{"type":"string","const":"entity"},"propertyIri":{"type":"string","minLength":1}}}}
                }
            }),
            Vec::new(),
            "[]".to_string(),
            serde_json::json!({"root":"value","elements":{"value":{"type":"ValueEntry"}}}),
            "2".to_string(),
        )
        .unwrap();
        create_contract(&mut conn, &v1, 1).unwrap();
        let spec_iri = "foundation:JsonRenderSpec_ValueBinding";
        let source = r#"{"root":"value","rendersEntity":"foundation:MonthlyBudget_Example","elements":{"value":{"type":"ValueEntry","props":{"label":"Total","valueType":"currency","cardinality":"single"},"bindings":{"value":{"source":"entity","propertyIri":"foundation:monthlyAmount"}}}}}"#;
        Individual::new(spec_iri)
            .assert_without_icon(&mut conn, SPEC_CLASS, "Budget total", "test")
            .unwrap();
        Individual::new(spec_iri)
            .add_property(
                &mut conn,
                SPEC_SOURCE,
                vec![Object::Literal {
                    value: source.to_string(),
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                }],
                "test",
            )
            .unwrap();
        set_spec_contracts(&mut conn, spec_iri, &[component_iri("ValueEntry", 1)]).unwrap();

        let mut component =
            semantic_component("ValueEntry", 2, "Presents a value and its data source");
        component.props_schema = serde_json::json!({
            "type": "object",
            "required": ["label", "valueType"],
            "properties": {
                "label": {"type":"string"},
                "valueType": {"type":"string"},
                "cardinality": {"enum":["single","multiple"]}
            },
            "x-foundation-element-bindings": {
                "type":"object",
                "additionalProperties":false,
                "properties": {
                    "value": {
                        "type":"object",
                        "additionalProperties":false,
                        "required":["source","propertyIri"],
                        "properties": {
                            "source":{"type":"string","const":"entity"},
                            "propertyIri":{"type":"string","minLength":1}
                        }
                    }
                }
            }
        });
        let manifest = test_manifest(vec![component]);

        sync_manifest(&mut conn, manifest.clone()).unwrap();
        assert_eq!(
            owl::get_all_iri_properties(&conn, spec_iri, SPEC_CONTRACT).unwrap(),
            vec![component_iri("ValueEntry", 2)]
        );
        assert_eq!(
            owl::get_literal_property(&conn, spec_iri, SPEC_SOURCE)
                .unwrap()
                .as_deref(),
            Some(source),
            "the binding's entity, property, cardinality and other spec intent remain byte-for-byte unchanged"
        );
        let second = sync_manifest(&mut conn, manifest).unwrap();
        assert_eq!(second.contract_versions_created, 0);
        assert_eq!(second.registry_links_added, 0);
    }

    #[test]
    fn incompatible_spec_is_reported_without_blocking_startup_or_partial_repins() {
        let mut conn = crate::eavto::test_helpers::setup_test_db();
        seed_catalog_test_ontology(&mut conn);
        sync_schema(&mut conn).unwrap();
        sync_schema(&mut conn).unwrap();
        let sentinel_iri = "foundation:LocalIndividual_CatalogSyncSentinel";
        Individual::new(sentinel_iri)
            .assert_without_icon(&mut conn, "foundation:Continuant", "Local sentinel", "test")
            .unwrap();
        let active_status_iri = crate::core_ontology::status::resolve_active_status(&conn).unwrap();
        Individual::new(sentinel_iri)
            .add_property(
                &mut conn,
                "foundation:hasStatus",
                vec![Object::Iri(active_status_iri)],
                "test",
            )
            .unwrap();
        let v1 = make_contract(
            "ValueEntry".to_string(),
            "Presents a value".to_string(),
            serde_json::json!({"type":"object","properties":{"label":{"type":"string"}}}),
            Vec::new(),
            "[]".to_string(),
            serde_json::json!({"root":"value","elements":{"value":{"type":"ValueEntry"}}}),
            "2".to_string(),
        )
        .unwrap();
        create_contract(&mut conn, &v1, 1).unwrap();
        let spec_iri = "foundation:JsonRenderSpec_IncompatibleValue";
        let source = r#"{"root":"value","elements":{"value":{"type":"ValueEntry","props":{"label":"Total","obsoleteOption":true}}}}"#;
        Individual::new(spec_iri)
            .assert_without_icon(&mut conn, SPEC_CLASS, "Obsolete option", "test")
            .unwrap();
        Individual::new(spec_iri)
            .add_property(
                &mut conn,
                SPEC_SOURCE,
                vec![Object::Literal {
                    value: source.to_string(),
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                }],
                "test",
            )
            .unwrap();
        set_spec_contracts(&mut conn, spec_iri, &[component_iri("ValueEntry", 1)]).unwrap();
        let compatible_spec_iri = "foundation:JsonRenderSpec_CompatibleSibling";
        let compatible_source = r#"{"root":"value","elements":{"value":{"type":"ValueEntry","props":{"label":"Total"}}}}"#;
        Individual::new(compatible_spec_iri)
            .assert_without_icon(&mut conn, SPEC_CLASS, "Compatible sibling", "test")
            .unwrap();
        Individual::new(compatible_spec_iri)
            .add_property(
                &mut conn,
                SPEC_SOURCE,
                vec![Object::Literal {
                    value: compatible_source.to_string(),
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                }],
                "test",
            )
            .unwrap();
        set_spec_contracts(
            &mut conn,
            compatible_spec_iri,
            &[component_iri("ValueEntry", 1)],
        )
        .unwrap();
        let mut component = semantic_component("ValueEntry", 2, "Presents a value");
        component.props_schema = serde_json::json!({
            "type":"object",
            "properties":{"label":{"type":"string"}},
            "additionalProperties":false
        });

        let manifest = test_manifest(vec![component]);
        let summary = sync_manifest(&mut conn, manifest.clone()).unwrap();
        assert_eq!(summary.contract_versions_created, 1);
        assert_eq!(summary.specs_requiring_update.len(), 1);
        assert!(summary.specs_requiring_update[0]
            .contains("foundation:JsonRenderSpec_IncompatibleValue"));
        assert!(summary.specs_requiring_update[0].contains("obsoleteOption"));
        assert!(Individual::get(&conn, &component_iri("ValueEntry", 2))
            .unwrap()
            .is_some());
        assert_eq!(
            owl::get_all_iri_properties(&conn, spec_iri, SPEC_CONTRACT).unwrap(),
            vec![component_iri("ValueEntry", 1)]
        );
        assert_eq!(
            owl::get_literal_property(&conn, spec_iri, SPEC_SOURCE)
                .unwrap()
                .as_deref(),
            Some(source)
        );
        assert_eq!(
            owl::get_all_iri_properties(&conn, compatible_spec_iri, SPEC_CONTRACT).unwrap(),
            vec![component_iri("ValueEntry", 2)],
            "the independently compatible spec is repinned while the incompatible spec stays pinned"
        );
        assert_eq!(
            owl::get_literal_property(&conn, compatible_spec_iri, SPEC_SOURCE)
                .unwrap()
                .as_deref(),
            Some(compatible_source)
        );
        assert!(Individual::get(&conn, sentinel_iri).unwrap().is_some());
        assert_eq!(
            owl::get_all_iri_properties(&conn, sentinel_iri, "foundation:hasStatus").unwrap(),
            vec![crate::core_ontology::status::resolve_active_status(&conn).unwrap()],
            "catalog sync preserves unrelated local ontology data"
        );
        let repeated = sync_manifest(&mut conn, manifest).unwrap();
        assert_eq!(repeated.contract_versions_created, 0);
        assert_eq!(repeated.specs_requiring_update.len(), 1);
        assert_eq!(sync_legacy_specs(&mut conn).unwrap(), 0);
        assert_eq!(sync_legacy_specs(&mut conn).unwrap(), 0);
        assert_eq!(
            owl::get_all_iri_properties(&conn, spec_iri, SPEC_CONTRACT).unwrap(),
            vec![component_iri("ValueEntry", 1)]
        );
        assert_eq!(
            owl::get_all_iri_properties(&conn, compatible_spec_iri, SPEC_CONTRACT).unwrap(),
            vec![component_iri("ValueEntry", 2)]
        );
        assert!(Individual::get(&conn, sentinel_iri).unwrap().is_some());
    }

    #[test]
    fn identical_versioned_contract_with_legacy_hash_is_rehashed_once() {
        let mut conn = crate::eavto::test_helpers::setup_test_db();
        seed_catalog_test_ontology(&mut conn);
        sync_schema(&mut conn).unwrap();
        let contract = make_contract(
            "Decision".to_string(),
            "Selects a candidate".to_string(),
            serde_json::json!({
                "type": "object",
                "properties": {"title": {"type": "string"}}
            }),
            Vec::new(),
            "[]".to_string(),
            serde_json::json!({"root":"choice","elements":{"choice":{"type":"Decision"}}}),
            "2".to_string(),
        )
        .unwrap();
        let expected_hash = contract.hash.clone();
        create_contract(&mut conn, &contract, 1).unwrap();
        let iri = component_iri("Decision", 1);
        replace_literal_property(
            &mut conn,
            &iri,
            CONTRACT_HASH,
            "sha256:previous-hash-algorithm",
        )
        .unwrap();

        let first_sync = sync_component(&mut conn, contract.clone(), 1).unwrap();
        assert_eq!(first_sync, 0, "rehashing does not create a new version");
        assert_eq!(
            owl::get_literal_property(&conn, &iri, CONTRACT_HASH)
                .unwrap()
                .as_deref(),
            Some(expected_hash.as_str())
        );
        assert_eq!(
            Class::get_instances(&conn, COMPONENT_CLASS).unwrap(),
            vec![iri.clone()],
            "the canonical version remains the only Decision contract"
        );

        assert_eq!(sync_component(&mut conn, contract, 1).unwrap(), 0);
        assert_eq!(
            owl::get_literal_property(&conn, &iri, CONTRACT_HASH)
                .unwrap()
                .as_deref(),
            Some(expected_hash.as_str())
        );
    }

    #[test]
    fn matching_unversioned_contract_is_repointed_from_existing_version_and_pins_specs_once() {
        let mut conn = crate::eavto::test_helpers::setup_test_db();
        seed_catalog_test_ontology(&mut conn);
        sync_schema(&mut conn).unwrap();
        let legacy_component = semantic_component("Decision", 1, "Selects a candidate");
        let component = semantic_component("Decision", 2, "Selects a candidate with context");
        let examples = serde_json::json!({
            "root": "root",
            "elements": {"root": {"type": "Decision"}}
        });
        let actions = canonical_json(&Value::Array(Vec::new())).unwrap();
        let legacy = make_contract(
            legacy_component.id.clone(),
            legacy_component.description.clone(),
            legacy_component.props_schema.clone(),
            legacy_component.slots.clone(),
            actions,
            examples.clone(),
            "2".to_string(),
        )
        .unwrap();
        create_contract(&mut conn, &legacy, 1).unwrap();
        create_unversioned_contract(&mut conn, &legacy);

        let spec_iri = "foundation:JsonRenderSpec_UnversionedDecision";
        let spec_source =
            r#"{"root":"root","elements":{"root":{"type":"Decision","props":{"label":"Choose"}}}}"#;
        Individual::new(spec_iri)
            .assert_without_icon(&mut conn, SPEC_CLASS, "Decision widget", "test")
            .unwrap();
        Individual::new(spec_iri)
            .add_property(
                &mut conn,
                SPEC_SOURCE,
                vec![Object::Literal {
                    value: spec_source.to_string(),
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                }],
                "test",
            )
            .unwrap();

        let manifest = test_manifest(vec![component]);
        assert_eq!(
            sync_manifest(&mut conn, manifest.clone())
                .unwrap()
                .contract_versions_created,
            1
        );
        assert_eq!(sync_legacy_specs(&mut conn).unwrap(), 0);
        assert_eq!(
            sync_manifest(&mut conn, manifest)
                .unwrap()
                .contract_versions_created,
            0
        );
        assert_eq!(sync_legacy_specs(&mut conn).unwrap(), 0);
        assert!(
            Individual::get(&conn, "foundation:SemanticComponentDefinition_Decision")
                .unwrap()
                .is_some()
        );
        assert!(Individual::get(&conn, &component_iri("Decision", 2))
            .unwrap()
            .is_some());
        assert_eq!(
            owl::get_all_iri_properties(&conn, spec_iri, SPEC_CONTRACT).unwrap(),
            vec![component_iri("Decision", 2)]
        );
        assert_eq!(
            owl::get_literal_property(&conn, spec_iri, SPEC_SOURCE)
                .unwrap()
                .as_deref(),
            Some(spec_source)
        );
    }

    #[test]
    fn differing_unversioned_contract_fails_before_any_manifest_writes() {
        let mut conn = crate::eavto::test_helpers::setup_test_db();
        seed_catalog_test_ontology(&mut conn);
        sync_schema(&mut conn).unwrap();
        let changed = semantic_component("Decision", 1, "Selects a candidate");
        let expected = make_contract(
            changed.id.clone(),
            changed.description.clone(),
            changed.props_schema.clone(),
            changed.slots.clone(),
            "[]".to_string(),
            serde_json::json!({
                "root": "root",
                "elements": {"root": {"type": "Decision"}}
            }),
            "2".to_string(),
        )
        .unwrap();
        let old = make_contract(
            "Decision".to_string(),
            "Old intent".to_string(),
            serde_json::json!({"type":"object","properties":{"label":{"type":"string"}}}),
            Vec::new(),
            "[]".to_string(),
            serde_json::json!({
                "root": "root",
                "elements": {"root": {"type": "Decision"}}
            }),
            "2".to_string(),
        )
        .unwrap();
        create_unversioned_contract(&mut conn, &old);

        let manifest = test_manifest(vec![
            semantic_component("ValueEntry", 1, "Presents a value"),
            changed,
        ]);
        let error = sync_manifest(&mut conn, manifest).unwrap_err();
        assert!(error.contains("foundation:SemanticComponentDefinition_Decision"));
        assert!(error.contains("Decision v1"));
        assert!(error.contains("intent"));
        assert!(error.contains(&format!("legacyCanonicalHash={}", old.hash)));
        assert!(error.contains(&format!("manifestHash={}", expected.hash)));
        assert!(Individual::get(&conn, &component_iri("ValueEntry", 1))
            .unwrap()
            .is_none());
        assert!(Individual::get(&conn, &component_iri("Decision", 1))
            .unwrap()
            .is_none());
        assert!(
            Individual::get(&conn, "foundation:SemanticComponentDefinition_Decision")
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn stored_contract_hash_must_match_immutable_contract_content() {
        let mut conn = crate::eavto::test_helpers::setup_test_db();
        seed_catalog_test_ontology(&mut conn);
        sync_schema(&mut conn).unwrap();
        let contract = make_contract(
            "ValueEntry".to_string(),
            "Shows a value".to_string(),
            serde_json::json!({"type":"object","properties":{"label":{"type":"string"}}}),
            Vec::new(),
            "[]".to_string(),
            serde_json::json!({}),
            "1".to_string(),
        )
        .unwrap();
        create_contract(&mut conn, &contract, 1).unwrap();
        let iri = component_iri(&contract.id, 1);

        Individual::clear_property(&mut conn, &iri, CONTRACT_HASH, "test").unwrap();
        Individual::new(&iri)
            .add_property(
                &mut conn,
                CONTRACT_HASH,
                vec![Object::Literal {
                    value: "sha256:incorrect".to_string(),
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                }],
                "test",
            )
            .unwrap();

        let error = read_contract(&conn, &iri).unwrap_err();
        assert!(error.contains(&format!("iri={iri}")));
        assert!(error.contains("contractVersion=1"));
        assert!(error.contains("storedHash=sha256:incorrect"));
        assert!(error.contains("rawPayloadHash=sha256:"));
        assert!(error.contains("canonicalPayloadHash=sha256:"));
        assert!(error.contains("storedMatchesRaw=false"));
        assert!(error.contains("storedMatchesCanonical=false"));
        assert!(error.contains("expectedRepresentation=canonical"));
    }

    #[test]
    fn hash_mismatch_diagnostic_reports_both_legacy_and_canonical_candidates() {
        let mut conn = crate::eavto::test_helpers::setup_test_db();
        seed_catalog_test_ontology(&mut conn);
        sync_schema(&mut conn).unwrap();
        let legacy_actions = r#"{"actions":[{"id":"announce","description":"Announce","paramsSchema":{"type":"object"}}]}"#;
        let contract = ComponentContract {
            id: "ValueEntry".to_string(),
            intent: "Shows a value".to_string(),
            props_schema: canonical_json(&serde_json::json!({"type":"object"})).unwrap(),
            actions_schema: legacy_actions.to_string(),
            examples: "{}".to_string(),
            manifest_version: "2".to_string(),
            hash: "sha256:incorrect".to_string(),
        };
        create_contract(&mut conn, &contract, 1).unwrap();

        let iri = component_iri("ValueEntry", 1);
        let error = read_contract(&conn, &iri).unwrap_err();
        assert!(error.contains(&format!("iri={iri}")));
        assert!(error.contains("contractVersion=1"));
        assert!(error.contains("storedHash=sha256:incorrect"));
        assert!(error.contains("rawPayloadHash=sha256:"));
        assert!(error.contains("canonicalPayloadHash=sha256:"));
        assert!(error.contains("storedMatchesRaw=false"));
        assert!(error.contains("storedMatchesCanonical=false"));
        assert!(error.contains("expectedRepresentation=raw legacy wrapper"));

        let current = make_contract(
            "ValueEntry".to_string(),
            "Shows a value".to_string(),
            serde_json::json!({
                "type": "object",
                "properties": {
                    "label": {"type": "string"},
                    "description": {"type": "string"}
                }
            }),
            Vec::new(),
            canonical_json(&serde_json::json!([{
                "id": "announce",
                "description": "Announce",
                "paramsSchema": {"type": "object"}
            }]))
            .unwrap(),
            serde_json::json!({}),
            "2".to_string(),
        )
        .unwrap();
        let comparison = validate_component_sync(&conn, &current, 1).unwrap_err();
        assert!(comparison.contains(&format!("currentManifestHash={}", current.hash)));
        assert!(
            comparison.contains("currentManifestDifferingFields=[\"propsSchema\"]"),
            "{comparison}"
        );
    }

    #[test]
    fn legacy_action_wrapper_is_canonicalized_without_changing_its_semantic_version() {
        let mut conn = crate::eavto::test_helpers::setup_test_db();
        seed_catalog_test_ontology(&mut conn);
        sync_schema(&mut conn).unwrap();
        let props_schema = r#"{"type":"object","properties":{"label":{"type":"string"}}}"#;
        let examples = r#"{}"#;
        let legacy_actions = r#"{"actions":[{"id":"announce","description":"Announce a message","paramsSchema":{"type":"object"}}]}"#;
        let legacy = ComponentContract {
            id: "ValueEntry".to_string(),
            intent: "Shows a value".to_string(),
            props_schema: canonical_json_text(props_schema).unwrap(),
            actions_schema: legacy_actions.to_string(),
            examples: canonical_json_text(examples).unwrap(),
            manifest_version: "1".to_string(),
            hash: contract_hash(
                "ValueEntry",
                "Shows a value",
                props_schema,
                legacy_actions,
                examples,
            )
            .unwrap(),
        };
        create_contract(&mut conn, &legacy, 1).unwrap();

        let spec_source = r#"{"root":"value","elements":{"value":{"type":"ValueEntry","props":{"label":"Example"},"bindings":{"value":{"source":"entity","propertyIri":"foundation:value"}}}}}"#;
        let spec_iri = "foundation:JsonRenderSpec_LegacyActions";
        Individual::new(spec_iri)
            .assert_without_icon(&mut conn, SPEC_CLASS, "Legacy actions", "test")
            .unwrap();
        Individual::new(spec_iri)
            .add_property(
                &mut conn,
                SPEC_SOURCE,
                vec![Object::Literal {
                    value: spec_source.to_string(),
                    datatype: Some("xsd:string".to_string()),
                    language: None,
                }],
                "test",
            )
            .unwrap();
        set_spec_contracts(&mut conn, spec_iri, &[component_iri("ValueEntry", 1)]).unwrap();

        let stored = read_contract(&conn, &component_iri("ValueEntry", 1)).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&stored.contract.actions_schema).unwrap(),
            serde_json::json!([{"id":"announce","description":"Announce a message","paramsSchema":{"type":"object"}}])
        );

        let canonical = make_contract(
            "ValueEntry".to_string(),
            "Shows a value".to_string(),
            serde_json::json!({"type":"object","properties":{"label":{"type":"string"}}}),
            Vec::new(),
            canonical_json(&serde_json::json!([{"id":"announce","description":"Announce a message","paramsSchema":{"type":"object"}}])).unwrap(),
            serde_json::json!({}),
            "1".to_string(),
        )
        .unwrap();
        let canonical_actions = canonical.actions_schema.clone();
        let canonical_hash = canonical.hash.clone();
        assert_eq!(sync_component(&mut conn, canonical.clone(), 1).unwrap(), 0);
        assert_eq!(sync_component(&mut conn, canonical, 1).unwrap(), 0);
        assert_eq!(
            Class::get_instances(&conn, COMPONENT_CLASS).unwrap().len(),
            1,
            "canonical representation retains the declared semantic version"
        );
        let migrated = read_contract(&conn, &component_iri("ValueEntry", 1)).unwrap();
        assert!(!migrated.legacy_action_wrapper);
        assert_eq!(migrated.contract.hash, canonical_hash);
        assert_eq!(
            owl::get_all_iri_properties(&conn, spec_iri, SPEC_CONTRACT).unwrap(),
            vec![component_iri("ValueEntry", 1)]
        );
        assert_eq!(
            owl::get_literal_property(
                &conn,
                &component_iri("ValueEntry", 1),
                COMPONENT_ACTIONS_SCHEMA
            )
            .unwrap()
            .as_deref(),
            Some(canonical_actions.as_str())
        );
        assert_eq!(
            owl::get_literal_property(&conn, spec_iri, SPEC_SOURCE)
                .unwrap()
                .as_deref(),
            Some(spec_source),
            "contract migration preserves the widget specification and its bindings"
        );
    }

    #[test]
    fn changed_component_contract_creates_a_new_version_without_overwriting_history() {
        let mut conn = crate::eavto::test_helpers::setup_test_db();
        seed_catalog_test_ontology(&mut conn);
        sync_schema(&mut conn).unwrap();

        let first = make_contract(
            "ValueEntry".to_string(),
            "Shows a value".to_string(),
            serde_json::json!({"type":"object","properties":{"label":{"type":"string"}}}),
            Vec::new(),
            "[]".to_string(),
            serde_json::json!({"root":"value","elements":{"value":{"type":"ValueEntry"}}}),
            "1".to_string(),
        )
        .unwrap();
        create_contract(&mut conn, &first, 1).unwrap();
        replace_literal_property(
            &mut conn,
            &component_iri("ValueEntry", 1),
            CONTRACT_HASH,
            "sha256:historical-algorithm",
        )
        .unwrap();

        let second = make_contract(
            "ValueEntry".to_string(),
            "Shows a value and its provenance".to_string(),
            serde_json::json!({"type":"object","properties":{"label":{"type":"string"},"source":{"type":"string"}}}),
            Vec::new(),
            "[]".to_string(),
            serde_json::json!({"root":"value","elements":{"value":{"type":"ValueEntry"}}}),
            "2".to_string(),
        )
        .unwrap();

        assert_eq!(sync_component(&mut conn, second.clone(), 2).unwrap(), 1);
        assert_eq!(sync_component(&mut conn, second, 2).unwrap(), 0);
        assert!(Individual::get(&conn, &component_iri("ValueEntry", 1))
            .unwrap()
            .is_some());
        assert!(Individual::get(&conn, &component_iri("ValueEntry", 2))
            .unwrap()
            .is_some());
        assert_eq!(sync_core_registry(&mut conn).unwrap().0, 2);
        assert_eq!(sync_core_registry(&mut conn).unwrap().0, 0);
    }

    #[test]
    fn legacy_spec_migration_preserves_source_and_is_idempotent() {
        let mut conn = crate::eavto::test_helpers::setup_test_db();
        seed_catalog_test_ontology(&mut conn);
        sync_schema(&mut conn).unwrap();
        sync(&mut conn).unwrap();

        let source = r#"{"root":"amount","elements":{"amount":{"type":"ValueEntry","props":{"label":"Monthly amount","valueType":"currency","editable":true},"bindings":{"value":{"source":"entity","propertyIri":"foundation:monthlyAmount"}}}}}"#;
        let spec = Individual::new("foundation:JsonRenderSpec_Legacy");
        spec.assert_without_icon(&mut conn, SPEC_CLASS, "Legacy widget", "test")
            .unwrap();
        spec.add_property(
            &mut conn,
            SPEC_SOURCE,
            vec![Object::Literal {
                value: source.to_string(),
                datatype: Some("xsd:string".to_string()),
                language: None,
            }],
            "test",
        )
        .unwrap();

        assert_eq!(sync_legacy_specs(&mut conn).unwrap(), 1);
        assert_eq!(sync_legacy_specs(&mut conn).unwrap(), 0);
        assert_eq!(
            owl::get_literal_property(&conn, "foundation:JsonRenderSpec_Legacy", SPEC_SOURCE)
                .unwrap()
                .as_deref(),
            Some(source)
        );
        assert_eq!(
            owl::get_all_iri_properties(&conn, "foundation:JsonRenderSpec_Legacy", SPEC_CONTRACT)
                .unwrap(),
            vec![component_iri("ValueEntry", 2)]
        );
    }

    #[test]
    fn spec_contract_update_retracts_old_version_and_is_idempotent() {
        let mut conn = crate::eavto::test_helpers::setup_test_db();
        seed_catalog_test_ontology(&mut conn);
        sync_schema(&mut conn).unwrap();
        Class::new(COMPONENT_CLASS)
            .assert(
                &mut conn,
                ClassType::OwlClass,
                "Semantic Component Definition",
                "widgets",
                Some("foundation:AbstractContinuant"),
                "test",
            )
            .unwrap();

        Individual::new("foundation:JsonRenderSpec_Test")
            .assert_without_icon(&mut conn, SPEC_CLASS, "Test spec", "test")
            .unwrap();
        for contract in [
            "foundation:SemanticComponentDefinition_Text_v1",
            "foundation:SemanticComponentDefinition_Text_v2",
        ] {
            Individual::new(contract)
                .assert_without_icon(&mut conn, COMPONENT_CLASS, contract, "test")
                .unwrap();
        }
        set_spec_contracts(
            &mut conn,
            "foundation:JsonRenderSpec_Test",
            &["foundation:SemanticComponentDefinition_Text_v1".to_string()],
        )
        .unwrap();

        assert!(set_spec_contracts(
            &mut conn,
            "foundation:JsonRenderSpec_Test",
            &["foundation:SemanticComponentDefinition_Text_v2".to_string()],
        )
        .unwrap());
        assert!(!set_spec_contracts(
            &mut conn,
            "foundation:JsonRenderSpec_Test",
            &["foundation:SemanticComponentDefinition_Text_v2".to_string()],
        )
        .unwrap());
        assert_eq!(
            owl::get_all_iri_properties(&conn, "foundation:JsonRenderSpec_Test", SPEC_CONTRACT)
                .unwrap(),
            vec!["foundation:SemanticComponentDefinition_Text_v2".to_string()]
        );
    }
}
