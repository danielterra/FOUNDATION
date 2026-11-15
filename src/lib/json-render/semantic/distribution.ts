export type DistributionProjectionPage = {
  propertyIri: string;
  unitLabel?: string | null;
  items: Array<{ value: string; label?: string | null; valueLabel?: string | null }>;
};

export type DistributionItem = {
  projectedProperties?: DistributionProjectionPage[] | null;
};

export type DistributionGroup = {
  group: string;
  unitLabel: string | null;
  amount: number;
  count: number;
};

export function resolveDistributionCoverage(hasMore: boolean) {
  return hasMore
    ? { kind: 'partial' as const, totalLabel: 'Subtotal carregado' }
    : { kind: 'complete' as const, totalLabel: 'Total' };
}

export function aggregateDistribution(
  items: DistributionItem[],
  measurePropertyIri: string,
  groupByPropertyIri: string,
): { groups: DistributionGroup[]; total: number; unitLabel: string | null; pricedCount: number; unpricedCount: number; hasMixedUnits: boolean } {
  const grouped = new Map<string, DistributionGroup>();
  let total = 0;
  let pricedCount = 0;
  let unpricedCount = 0;
  const units = new Set<string>();

  for (const item of items) {
    const projections = item.projectedProperties ?? [];
    const measurePage = projections.find((page) => page.propertyIri === measurePropertyIri);
    const groupPage = projections.find((page) => page.propertyIri === groupByPropertyIri);
    const amountValue = measurePage?.items[0]?.value;
    const amount = typeof amountValue === 'string' && amountValue.trim() !== '' ? Number(amountValue) : Number.NaN;
    if (!Number.isFinite(amount)) {
      unpricedCount += 1;
      continue;
    }

    const unitLabel = measurePage?.unitLabel ?? null;
    if (unitLabel) units.add(unitLabel);
    const projectedGroup = groupPage?.items[0];
    const groupName = (projectedGroup?.valueLabel ?? projectedGroup?.label ?? projectedGroup?.value)?.trim() || 'Sem categoria';
    const key = `${unitLabel ?? ''}\u0000${groupName}`;
    const group = grouped.get(key) ?? { group: groupName, unitLabel, amount: 0, count: 0 };
    group.amount += amount;
    group.count += 1;
    grouped.set(key, group);
    total += amount;
    pricedCount += 1;
  }

  const groups = [...grouped.values()].sort((left, right) => right.amount - left.amount || left.group.localeCompare(right.group));
  return { groups, total, unitLabel: units.size === 1 ? [...units][0] : null, pricedCount, unpricedCount, hasMixedUnits: units.size > 1 };
}
