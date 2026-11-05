use foundation_core::owl::materialization::ComputedPropertyKind;

/// Append a formula/aggregation result through the trusted generic OWL writer.
/// The caller supplies the domain vocabulary so Core-Ontology remains the only
/// layer that knows Foundation's calculated-property predicates.
pub async fn append_calculated_value(
    executor: &crate::owl::DbExecutor,
    subject: String,
    property: String,
    value: String,
    datatype: String,
    kind: ComputedPropertyKind,
) -> Result<i64, String> {
    foundation_core::owl::materialization::write_computed_property(
        executor,
        subject,
        property,
        value,
        datatype,
        kind,
        "foundation:formula".to_string(),
        "foundation:aggregation".to_string(),
    )
    .await
}
