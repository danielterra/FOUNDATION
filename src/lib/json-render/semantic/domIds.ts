export function createSemanticDomIds(instanceId: string) {
  return {
    decisionTitle: `${instanceId}-decision-title`,
    decisionSearch: `${instanceId}-decision-search`,
    valueLabel: `${instanceId}-semantic-value-label`,
    valueDescription: `${instanceId}-semantic-value-description`,
    value: `${instanceId}-semantic-value`,
    informationTitle: `${instanceId}-information-title`,
    comparisonTitle: `${instanceId}-comparison-title`,
    distributionTitle: `${instanceId}-distribution-title`,
    recordsTitle: `${instanceId}-records-title`,
    recordsFilterColumn: `${instanceId}-records-filter-column`,
    recordsFilterValue: `${instanceId}-records-filter-value`,
    recordsFilterOperator: `${instanceId}-records-filter-operator`,
    confirmationTitle: `${instanceId}-confirmation-title`,
  };
}
