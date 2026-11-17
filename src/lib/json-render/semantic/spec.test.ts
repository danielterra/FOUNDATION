import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';
import { createJiti } from 'jiti';

const jiti = createJiti(import.meta.url);
const loadedModules: any[] = await Promise.all([
  jiti.import('../catalog.ts'),
  jiti.import('./examples.ts'),
  jiti.import('./catalog.ts'),
  jiti.import('./spec.ts'),
]);
const [{ componentCatalog }, { semanticExamples }, { semanticCatalog }, { validateLegacyJsonSpec, validateSemanticSpec }] = loadedModules;
const decisionPresentationModule: any = await jiti.import('./decisionPresentation.ts');
const { resolveDecisionPresentation } = decisionPresentationModule;
const contractHashModule: any = await jiti.import('./contractHash.ts');
const { buildSemanticContractHashPayload, hashSemanticContract, stableJson } = contractHashModule;
const semanticCatalogModule: any = await jiti.import('./catalog.ts');
const { semanticActionsContract, semanticCatalogManifest, semanticComponentPropsContract } = semanticCatalogModule;
const committedCatalogManifest = JSON.parse(
  await readFile(new URL('./catalog-manifest.json', import.meta.url), 'utf8'),
);
const contractHashGolden = JSON.parse(
  await readFile(new URL('../../../../scripts/ontology-base-pipeline/contract-hash-golden.json', import.meta.url), 'utf8'),
);
const runtimeSpecModule: any = await jiti.import('./runtimeSpec.ts');
const { buildRuntimeSemanticSpec, createSemanticLoadController, formatSemanticSpecLoadError, informationFactLabelVisible, informationPrimaryChildSpans, informationSummaryVisible, isSemanticRequestCurrent, partitionSemanticComponentCompatibility, resolveSemanticRefreshSnapshot, resolveSemanticWidgetSource, semanticComponentContractVersionMap, semanticPaginationAdvanced, semanticWidgetEventAction, semanticWidgetLoadIdentity, shouldContinueSemanticPagination, shouldReloadSemanticTemplate } = runtimeSpecModule;
const domIdsModule: any = await jiti.import('./domIds.ts');
const { createSemanticDomIds } = domIdsModule;
const comparisonModule: any = await jiti.import('./comparison.ts');
const distributionModule: any = await jiti.import('./distribution.ts');
const { areComparisonUnitsCompatible, calculateComparison, canCalculateComparison, comparisonBarGeometry, formatComparisonMonth, formatSemanticAmount, isComparisonPeriodClosed, previousComparisonMonth } = comparisonModule;
const { aggregateDistribution, resolveDistributionCoverage } = distributionModule;

test('compatible widgets remain renderable while only affected widgets require an update', () => {
  const result = partitionSemanticComponentCompatibility([
    { componentId: 'Information', status: 'update_available', reason: 'Compatible with the newer contract.' },
    { componentId: 'Details', status: 'update_required', reason: 'A required field is no longer supported.' },
    { componentId: 'Action', status: 'current', reason: '' },
  ]);

  assert.deepEqual(result.blocked.map((entry: { componentId: string }) => entry.componentId), ['Details']);
  assert.deepEqual(result.updatesAvailable.map((entry: { componentId: string }) => entry.componentId), ['Information']);
  assert.equal(result.blocked.some((entry: { componentId: string }) => entry.componentId === 'Action'), false);
});

test('a component without a registered contract blocks only the widget that uses it', () => {
  const result = partitionSemanticComponentCompatibility([
    { componentId: 'ValueEntry', status: 'current' },
    { componentId: 'UnknownComponent', status: 'missing_contract', reason: 'No ontology contract was returned.' },
  ]);

  assert.deepEqual(result.blocked.map((entry: { componentId: string }) => entry.componentId), ['UnknownComponent']);
  assert.deepEqual(result.updatesAvailable, []);
});

test('structurally invalid semantic specs show a friendly message without internal identifiers', () => {
  const message = formatSemanticSpecLoadError(new Error(JSON.stringify({ code: 'invalid_spec_structure' })));

  assert.equal(
    message,
    'Não foi possível mostrar este widget porque a definição está incompleta. Peça ao agente para revisá-la.',
  );
  assert.doesNotMatch(message, /foundation:|invalid_spec_structure/);
});

test('known backend validation errors do not expose spec IRIs or technical details', () => {
  assert.equal(
    formatSemanticSpecLoadError('JSON render spec foundation:JsonRenderSpec_123 has invalid specSource: expected value at line 1 column 1'),
    'Não foi possível mostrar este widget porque a definição está incompleta. Peça ao agente para revisá-la.',
  );
  assert.equal(
    formatSemanticSpecLoadError('JSON render spec foundation:JsonRenderSpec_123 has invalid component references'),
    'Não foi possível mostrar este widget porque a definição está incompleta. Peça ao agente para revisá-la.',
  );
  assert.equal(formatSemanticSpecLoadError(new Error('unavailable\n')), 'unavailable');
});

test('semantic contract hash payload includes intent, actions, and examples and omits empty slots', () => {
  const payload = buildSemanticContractHashPayload(
    'Information',
    'Organiza fatos.',
    { title: { type: 'string' } },
    [],
    [],
    { root: 'account' },
  );

  assert.deepEqual(payload, {
    componentId: 'Information',
    intent: 'Organiza fatos.',
    propsSchema: { title: { type: 'string' } },
    actionsSchema: [],
    examples: { root: 'account' },
  });
  assert.equal(
    stableJson(payload),
    '{"actionsSchema":[],"componentId":"Information","examples":{"root":"account"},"intent":"Organiza fatos.","propsSchema":{"title":{"type":"string"}}}',
  );
});

test('semantic contract hash includes the slots schema when a component declares slots', () => {
  const payload = buildSemanticContractHashPayload('Information', 'Intent', { title: { type: 'string' } }, ['content'], [], {});

  assert.deepEqual(payload, {
    componentId: 'Information',
    intent: 'Intent',
    propsSchema: { title: { type: 'string' }, 'x-foundation-slots': ['content'] },
    actionsSchema: [],
    examples: {},
  });
});

test('the exported manifest and runtime use the same component and action contracts', () => {
  assert.equal(semanticCatalogManifest.version, 2);
  assert.deepEqual(
    semanticCatalogManifest.components.map((component: { id: string }) => component.id).sort(),
    ['Action', 'Comparison', 'Confirmation', 'Decision', 'Details', 'Distribution', 'Information', 'Records', 'ValueEntry'].sort(),
  );
  for (const component of semanticCatalogManifest.components as Array<{ id: string; contractVersion: number }>) {
    assert.equal(Number.isInteger(component.contractVersion), true, `${component.id} must have an independent integer contract version`);
    assert.ok(component.id === 'Information' ? component.contractVersion === 0 : component.contractVersion > 0, `${component.id} must have its canonical contract version`);
    assert.equal(
      semanticCatalog.data.components[component.id as keyof typeof semanticCatalog.data.components].contractVersion,
      component.contractVersion,
      `${component.id} manifest version must come from its runtime definition`,
    );
  }
  const information = semanticCatalogManifest.components.find((component: { id: string }) => component.id === 'Information');
  const details = semanticCatalogManifest.components.find((component: { id: string }) => component.id === 'Details');
  assert.deepEqual(information.slots, ['default']);
  assert.deepEqual(semanticComponentPropsContract('Information'), information.propsSchema);
  assert.deepEqual(semanticComponentPropsContract('Details'), details.propsSchema);
  assert.deepEqual(semanticActionsContract(), semanticCatalogManifest.actionIds.map((id: string) => {
    const action = semanticCatalog.data.actions[id];
    return { id, description: action.description, paramsSchema: action.params.toJSONSchema() };
  }));

  const runtimePayload = buildSemanticContractHashPayload(
    information.id,
    information.description,
    semanticComponentPropsContract(information.id),
    semanticCatalog.data.components.Information.slots,
    semanticActionsContract(),
    semanticExamples.Information,
  );
  const ontologyPayload = buildSemanticContractHashPayload(
    information.id,
    information.description,
    information.propsSchema,
    information.slots,
    semanticActionsContract(),
    semanticExamples.Information,
  );
  assert.equal(stableJson(runtimePayload), stableJson(ontologyPayload));
});

test('runtime contract hashes match Rust golden vectors for every semantic component', async () => {
  for (const component of semanticCatalogManifest.components) {
    const payload = buildSemanticContractHashPayload(
      component.id,
      component.description,
      component.propsSchema,
      component.slots,
      committedCatalogManifest.actions,
      committedCatalogManifest.examples[component.id],
    );
    assert.equal(
      await hashSemanticContract(payload),
      contractHashGolden.hashes[component.id]?.hash,
      `${component.id} must match the Rust golden hash`,
    );
  }

  const action = semanticCatalogManifest.components.find((component: { id: string }) => component.id === 'Action');
  const actionPayload = buildSemanticContractHashPayload(
    action.id,
    action.description,
    action.propsSchema,
    action.slots,
    committedCatalogManifest.actions,
    committedCatalogManifest.examples.Action,
  );
  assert.equal(stableJson(actionPayload), contractHashGolden.actionCanonicalPayload);
});

test('semantic contract hashes change for every contract field and ignore object key order', async () => {
  const base = buildSemanticContractHashPayload(
    'Sample',
    'Base intent',
    { type: 'object', properties: { title: { type: 'string' } } },
    ['default'],
    [{ id: 'announce', paramsSchema: { type: 'object' } }],
    { root: 'account', elements: {} },
  );
  const baseHash = await hashSemanticContract(base);
  const variants = [
    { ...base, intent: 'Changed intent' },
    { ...base, propsSchema: { ...base.propsSchema, required: ['title'] } },
    { ...base, propsSchema: { ...base.propsSchema, 'x-foundation-slots': ['header'] } },
    { ...base, actionsSchema: [{ id: 'perform', paramsSchema: {} }] },
    { ...base, examples: { root: 'different' } },
  ];

  for (const variant of variants) assert.notEqual(await hashSemanticContract(variant), baseHash);

  const reordered = {
    examples: { elements: {}, root: 'account' },
    actionsSchema: [{ paramsSchema: { type: 'object' }, id: 'announce' }],
    propsSchema: { properties: { title: { type: 'string' } }, type: 'object', 'x-foundation-slots': ['default'] },
    intent: 'Base intent',
    componentId: 'Sample',
  };
  assert.equal(await hashSemanticContract(reordered), baseHash);
});

test('the build manifest matches the runtime renderer contracts and examples', () => {
  assert.deepEqual(committedCatalogManifest, {
    ...semanticCatalogManifest,
    actions: semanticActionsContract(),
    examples: semanticExamples,
  });
});

test('ontology-published component contract versions match the frontend declarations', () => {
  const versions = Object.fromEntries(
    semanticCatalogManifest.components.map((component: { id: string; contractVersion: number }) => [component.id, component.contractVersion]),
  );

  assert.equal(semanticCatalogManifest.version, 2, 'component contract updates must not bump the global manifest version');
  assert.deepEqual(
    { ValueEntry: versions.ValueEntry, Information: versions.Information, Details: versions.Details, Comparison: versions.Comparison, Distribution: versions.Distribution },
    { ValueEntry: 2, Information: 0, Details: 2, Comparison: 3, Distribution: 2 },
  );
});

test('all semantic examples validate against the semantic catalog', () => {
  assert.deepEqual(Object.keys(semanticExamples).sort(), ['Action', 'Comparison', 'Confirmation', 'Decision', 'Details', 'Distribution', 'Information', 'Records', 'ValueEntry'].sort());
  for (const [intent, example] of Object.entries(semanticExamples)) {
    assert.equal(semanticCatalog.validate(example).success, true, `${intent} example should match its schema`);
    assert.equal(validateSemanticSpec(example).success, true, `${intent} example should be renderable`);
  }
});

test('Comparison requires live current and reference entity bindings', () => {
  const valid = {
    root: 'comparison',
    elements: {
      comparison: {
        type: 'Comparison',
        children: [],
        props: { title: 'Receita', metric: 'Receita', currentPeriod: 'Este mês', referencePeriod: 'Mesmo período anterior', favorableDirection: 'higher' },
        bindings: {
          current: { source: 'entity', propertyIri: 'foundation:totalActualIncome' },
          reference: { source: 'entity', propertyIri: 'foundation:previousMonthActualIncome' },
          periodStart: { source: 'entity', propertyIri: 'foundation:budgetStartDate' },
          periodEnd: { source: 'entity', propertyIri: 'foundation:budgetEndDate' },
        },
      },
    },
  };
  assert.equal(validateSemanticSpec(valid).success, true);
  const missingReference = structuredClone(valid);
  delete (missingReference.elements.comparison as { bindings: Record<string, unknown> }).bindings.reference;
  assert.equal(validateSemanticSpec(missingReference).success, false);
});

test('Information v0 establishes the default deliberate layout and preserves child order', () => {
  const valid = {
    root: 'group',
    elements: {
      group: { type: 'Information', children: ['primary', 'supporting'], props: { title: 'Monthly summary', layout: { columns: 2 } } },
      primary: { type: 'Information', children: [], props: { title: 'Realized', layout: { columns: 1 } } },
      supporting: { type: 'Information', children: [], props: { title: 'Pending', layout: { columns: 1 } } },
    },
  };
  assert.equal(validateSemanticSpec(valid).success, true);
  assert.equal(semanticCatalog.data.components.Information.contractVersion, 0);
  const missingLayout = structuredClone(valid);
  delete (missingLayout.elements.group.props as Record<string, unknown>).layout;
  assert.equal(validateSemanticSpec(missingLayout).success, false);
  const invalidColumns = structuredClone(valid);
  (invalidColumns.elements.group.props as Record<string, unknown>).layout = { columns: 7 };
  assert.equal(validateSemanticSpec(invalidColumns).success, false);
});

test('Information v0 establishes the base presentation while v4-v7 renderer behavior remains available', () => {
  assert.equal(informationPrimaryChildSpans(4), true);
  assert.equal(informationPrimaryChildSpans(5), true);
  assert.equal(informationPrimaryChildSpans(6), false);
  assert.equal(informationPrimaryChildSpans(7), false);
  assert.equal(informationPrimaryChildSpans(0), false);
  assert.equal(informationSummaryVisible(6, 'Data inicial', 'Data inicial'), true);
  assert.equal(informationSummaryVisible(0, 'Data inicial', 'Data inicial'), false);
  assert.equal(informationSummaryVisible(0, 'Período selecionado', 'Data inicial'), true);
  assert.equal(informationSummaryVisible(7, 'Data inicial', 'Data inicial'), false);
  assert.equal(informationFactLabelVisible(6, 1), true);
  assert.equal(informationFactLabelVisible(0, 1), false);
  assert.equal(informationFactLabelVisible(0, 2), true);
  assert.equal(informationFactLabelVisible(7, 1), false);
  assert.equal(informationFactLabelVisible(7, 2), true);
  assert.deepEqual(
    semanticComponentContractVersionMap([
      { componentId: 'Information', pinnedVersion: 6, targetVersion: 6, status: 'current' },
      { componentId: 'Details', pinnedVersion: 2, targetVersion: 2, status: 'current' },
    ]),
    { Information: 6, Details: 2 },
  );
});

test('Distribution requires a paginated collection and measure/grouping projections', () => {
  const valid = semanticExamples.Distribution;
  assert.equal(validateSemanticSpec(valid).success, true);
  const missingCategory = structuredClone(valid);
  delete (missingCategory.elements.distribution as { bindings: { collection: { projections: Record<string, unknown> } } }).bindings.collection.projections.groupBy;
  assert.equal(validateSemanticSpec(missingCategory).success, false);
});

test('runtime bindings preserve named projections for generic widget snapshots', () => {
  const runtime = buildRuntimeSemanticSpec(semanticExamples.Distribution as any, [{
    elementId: 'distribution',
    target: 'collection',
    propertyIri: 'foundation:exampleCollection',
    projections: { measure: 'foundation:exampleMeasure', groupBy: 'foundation:exampleGroup' },
  }]);

  assert.deepEqual(runtime.spec.elements.distribution.props?.__ontologyBindings, {
    collection: 'foundation:exampleCollection',
  });
  assert.deepEqual(runtime.spec.elements.distribution.props?.__ontologyProjectionBindings, {
    collection: { measure: 'foundation:exampleMeasure', groupBy: 'foundation:exampleGroup' },
  });
  assert.deepEqual(runtime.bindingTargets, { collection: ['foundation:exampleCollection'] });
});

test('comparison trend semantics distinguish favorable and unfavorable decreases', () => {
  assert.equal(calculateComparison('80', '100', 'lower').interpretation, 'favorable');
  assert.equal(calculateComparison('120', '100', 'lower').interpretation, 'unfavorable');
  assert.equal(calculateComparison('120', '100', 'higher').interpretation, 'favorable');
  assert.equal(canCalculateComparison(true, '10', '0', true), false, 'a zero baseline has no comparison chart or directional trend');
});

test('comparison treats a zero baseline as unavailable without inventing a delta or trend', () => {
  assert.equal(calculateComparison('12', '0', 'higher'), null);
  assert.equal(calculateComparison('1', '1', 'lower')?.direction, 'unchanged');
  assert.equal(calculateComparison('12', '', 'higher'), null, 'an absent baseline must not be coerced to zero');
  assert.equal(calculateComparison(' ', '10', 'higher'), null, 'whitespace-only current values are missing, not zero');
  assert.equal(calculateComparison('not-a-number', '10', 'higher'), null);
});

test('comparison bars use one shared scale and retain the zero origin for negative values', () => {
  assert.deepEqual(comparisonBarGeometry(50, 100), {
    current: { start: 0, width: 50 },
    reference: { start: 0, width: 100 },
    zero: 0,
  });
  assert.deepEqual(comparisonBarGeometry(5, -10), {
    current: { start: 66.66666666666666, width: 33.33333333333334 },
    reference: { start: 0, width: 66.66666666666666 },
    zero: 66.66666666666666,
  });
});

test('comparison only treats matching or jointly unknown units as compatible', () => {
  assert.equal(areComparisonUnitsCompatible('foundation:BRL', 'BRL', 'foundation:BRL', 'BRL'), true);
  assert.equal(areComparisonUnitsCompatible('foundation:BRL', 'BRL', 'foundation:USD', 'USD'), false);
  assert.equal(areComparisonUnitsCompatible(null, null, 'foundation:USD', 'USD'), false, 'missing units cannot be compared with a known unit');
  assert.equal(areComparisonUnitsCompatible(null, null, null, undefined), true, 'both untyped values retain the existing shared-unit behavior');
});

test('an incomplete period never calculates or displays a comparison delta', () => {
  assert.equal(canCalculateComparison(false, '150', '100', true), false);
  assert.equal(canCalculateComparison(true, '150', '100', false), false, 'incompatible or missing units suppress comparison');
  assert.equal(canCalculateComparison(true, '150', '', true), false, 'a missing baseline is not treated as zero');
  assert.equal(canCalculateComparison(true, '150', '100', true), true);
  assert.equal(isComparisonPeriodClosed('2026-11-30', new Date('2026-11-12T12:00:00')), false);
});

test('monthly comparison labels periods and withholds a full-month baseline while the period is open', () => {
  assert.equal(formatComparisonMonth('2026-11-01', 'pt-BR'), 'novembro de 2026');
  assert.equal(previousComparisonMonth('2026-01-01', 'pt-BR'), 'dezembro de 2025');
  assert.equal(isComparisonPeriodClosed('2026-11-30', new Date('2026-11-12T12:00:00')), false);
  assert.equal(isComparisonPeriodClosed('2026-10-31', new Date('2026-11-12T12:00:00')), true);
});

test('comparison formatting uses ontology unit rather than assuming a currency', () => {
  assert.match(formatSemanticAmount(1234.5, 'USD', 'en-US'), /\$/);
  assert.match(formatSemanticAmount(1234.5, 'unidade-customizada', 'pt-BR'), /unidade-customizada/);
});

test('distribution supports literal and reference groups and keeps units separate', () => {
  const report = aggregateDistribution([
    { projectedProperties: [
      { propertyIri: 'amount', unitLabel: 'USD', items: [{ value: '10' }] },
      { propertyIri: 'category', items: [{ value: 'Food' }] },
    ] },
    { projectedProperties: [
      { propertyIri: 'amount', unitLabel: 'USD', items: [{ value: '5' }] },
      { propertyIri: 'category', items: [] },
    ] },
    { projectedProperties: [
      { propertyIri: 'amount', unitLabel: 'EUR', items: [{ value: '7' }] },
      { propertyIri: 'category', items: [{ value: 'Food', valueLabel: 'Alimentação' }] },
    ] },
  ], 'amount', 'category');

  assert.equal(report.groups.length, 3);
  assert.equal(report.groups.some((group: { group: string }) => group.group === 'Sem categoria'), true);
  assert.equal(report.groups.some((group: { group: string }) => group.group === 'Food'), true);
  assert.equal(report.groups.some((group: { group: string }) => group.group === 'Alimentação'), true);
  assert.equal(report.hasMixedUnits, true);
  assert.equal(report.unitLabel, null);
  assert.equal(report.total, 22);
});

test('Distribution groups an eight-item generic collection by literal values and keeps missing groups explicit', () => {
  const values = [5, 7, 4, 6, 8, 3, 2, 10];
  const labels = ['North', 'South', 'East', 'West', 'Central', 'North', 'South', null];
  const report = aggregateDistribution(values.map((value, index) => ({ projectedProperties: [
    { propertyIri: 'measure', unitLabel: 'points', items: [{ value: String(value) }] },
    { propertyIri: 'group', items: labels[index] ? [{ value: labels[index] as string }] : [] },
  ] })), 'measure', 'group');

  assert.equal(report.total, 45);
  assert.equal(report.pricedCount, 8);
  assert.deepEqual(report.groups.map(({ group, amount }) => [group, amount]), [
    ['Sem categoria', 10],
    ['South', 9],
    ['Central', 8],
    ['North', 8],
    ['West', 6],
    ['East', 4],
  ]);
});

test('distribution keeps items without a category projection under the uncategorized group', () => {
  const report = aggregateDistribution([{
    projectedProperties: [{ propertyIri: 'amount', unitLabel: 'BRL', items: [{ value: '19.50' }] }],
  }], 'amount', 'category');

  assert.equal(report.groups.length, 1);
  assert.deepEqual(report.groups[0], { group: 'Sem categoria', unitLabel: 'BRL', amount: 19.5, count: 1 });
  assert.equal(report.total, 19.5);
});

test('distribution does not turn missing or blank amounts into zero-value payments', () => {
  const report = aggregateDistribution([
    { projectedProperties: [{ propertyIri: 'amount', items: [{ value: '' }] }] },
    { projectedProperties: [{ propertyIri: 'amount', items: [{ value: '  ' }] }] },
    { projectedProperties: [{ propertyIri: 'amount', items: [{ value: '0' }] }] },
  ], 'amount', 'category');

  assert.equal(report.pricedCount, 1);
  assert.equal(report.unpricedCount, 2);
  assert.equal(report.total, 0);
  assert.equal(report.groups[0].count, 1, 'only the explicit numeric zero is counted as a payment');
});

test('distribution totals and groups follow changed item amount and category snapshots', () => {
  const firstSnapshot = aggregateDistribution([{
    projectedProperties: [
      { propertyIri: 'amount', unitLabel: 'BRL', items: [{ value: '25' }] },
      { propertyIri: 'category', items: [{ value: 'food', label: 'Alimentação' }] },
    ],
  }], 'amount', 'category');
  const updatedSnapshot = aggregateDistribution([{
    projectedProperties: [
      { propertyIri: 'amount', unitLabel: 'BRL', items: [{ value: '31' }] },
      { propertyIri: 'category', items: [{ value: 'transport', label: 'Transporte' }] },
    ],
  }], 'amount', 'category');

  assert.equal(firstSnapshot.total, 25);
  assert.equal(firstSnapshot.groups[0].group, 'Alimentação');
  assert.equal(updatedSnapshot.total, 31);
  assert.deepEqual(updatedSnapshot.groups.map((group: { group: string; amount: number }) => ({ group: group.group, amount: group.amount })), [
    { group: 'Transporte', amount: 31 },
  ]);
});

test('distribution coverage distinguishes a partial page from a complete period', () => {
  assert.deepEqual(resolveDistributionCoverage(true), { kind: 'partial', totalLabel: 'Subtotal carregado' });
  assert.deepEqual(resolveDistributionCoverage(false), { kind: 'complete', totalLabel: 'Total' });
});

test('distribution pagination updates totals and completeness as pages are appended', () => {
  const firstPage = [
    { projectedProperties: [
      { propertyIri: 'amount', unitLabel: 'BRL', items: [{ value: '10' }] },
      { propertyIri: 'category', items: [{ value: 'food', label: 'Alimentação' }] },
    ] },
    { projectedProperties: [
      { propertyIri: 'amount', unitLabel: 'BRL', items: [{ value: '5' }] },
      { propertyIri: 'category', items: [{ value: 'food', label: 'Alimentação' }] },
    ] },
  ];
  const nextPage = [{
    projectedProperties: [
      { propertyIri: 'amount', unitLabel: 'BRL', items: [{ value: '3' }] },
      { propertyIri: 'category', items: [{ value: 'travel', label: 'Viagem' }] },
    ],
  }];
  const partial = aggregateDistribution(firstPage, 'amount', 'category');
  const complete = aggregateDistribution([...firstPage, ...nextPage], 'amount', 'category');

  assert.equal(resolveDistributionCoverage(true).kind, 'partial');
  assert.equal(partial.total, 15);
  assert.equal(partial.groups.length, 1);
  assert.equal(resolveDistributionCoverage(false).kind, 'complete');
  assert.equal(complete.total, 18);
  assert.deepEqual(complete.groups.map((group: { group: string; count: number }) => [group.group, group.count]), [
    ['Alimentação', 2],
    ['Viagem', 1],
  ]);
});

test('Information maps children through its declared default slot', () => {
  const composedInformation = {
    root: 'overview',
    elements: {
      overview: { type: 'Information', children: ['details'], props: { title: 'Orçamento', layout: { columns: 1 } } },
      details: { type: 'Details', children: [], props: { title: 'Obrigações', details: [{ label: 'Conta', value: 'Conta corrente' }] } },
    },
  };
  const unsupportedChildren = {
    root: 'action',
    elements: {
      action: { type: 'Action', children: ['details'], props: { label: 'Atualizar', icon: 'refresh' } },
      details: { type: 'Details', children: [], props: { title: 'Detalhes', details: [] } },
    },
  };

  assert.equal(semanticCatalog.data.components.Information.slots.includes('default'), true);
  assert.equal(validateSemanticSpec(composedInformation).success, true);
  assert.equal(validateSemanticSpec({
    root: 'overview',
    elements: {
      overview: { type: 'Information', children: ['details'], props: { title: 'Orçamento', layout: { columns: 1 } } },
      details: { type: 'Details', children: [], props: { title: 'Obrigações', details: [] } },
    },
  }).success, true, 'a Details leaf with children: [] is a valid child of Information');
  const result = validateSemanticSpec(unsupportedChildren);
  assert.equal(result.success, false);
  if (result.success === false) assert.match(result.message, /slot compatível/);
});

test('semantic specs require explicit children arrays and reject orphaned nodes', () => {
  const missingChildren = {
    root: 'budget',
    elements: { budget: { type: 'Information', props: { title: 'Orçamento' } } },
  };
  const orphanedDecision = {
    root: 'budget',
    elements: {
      budget: { type: 'Information', children: [], props: { title: 'Orçamento' } },
      decision: { type: 'Decision', children: [], props: { title: 'Escolha', selectionMode: 'single', complexity: 'simple', candidates: [{ value: 'a', title: 'A' }] } },
    },
  };
  const unconnectedActionAndDecision = {
    root: 'budget',
    elements: {
      budget: { type: 'Information', children: [], props: { title: 'Orçamento' } },
      decision: { type: 'Decision', children: [], props: { title: 'Escolha', selectionMode: 'single', complexity: 'simple', candidates: [{ value: 'a', title: 'A' }] } },
      action: { type: 'Action', children: [], props: { label: 'Atualizar', icon: 'refresh' } },
    },
  };

  for (const invalid of [missingChildren, orphanedDecision, unconnectedActionAndDecision]) {
    assert.equal(validateSemanticSpec(invalid).success, false);
  }
  const missingResult = validateSemanticSpec(missingChildren);
  if (missingResult.success === false) assert.match(missingResult.message, /children como uma lista explícita/);
  const orphanResult = validateSemanticSpec(orphanedDecision);
  if (orphanResult.success === false) assert.match(orphanResult.message, /não está conectado à raiz/);
});

test('entity bindings must be declared under the element bindings object with known strict fields', () => {
  const validFactsBinding = {
    root: 'budget',
    elements: {
      budget: {
        type: 'Information',
        children: [],
        props: { title: 'Orçamento', layout: { columns: 1 }, facts: [] },
        bindings: { facts: { source: 'entity', propertyIri: 'foundation:budgetStartDate' } },
      },
    },
  };
  const rootFactsBinding = {
    root: 'budget',
    elements: {
      budget: {
        type: 'Information',
        children: [],
        props: { title: 'Orçamento' },
        facts: { source: 'entity', propertyIri: 'foundation:budgetStartDate' },
      },
    },
  };
  const unknownBindingTarget = {
    root: 'budget',
    elements: {
      budget: {
        type: 'Information',
        children: [],
        props: { title: 'Orçamento' },
        bindings: { budgetStartDate: { source: 'entity', propertyIri: 'foundation:budgetStartDate' } },
      },
    },
  };
  const unknownBindingField = {
    root: 'budget',
    elements: {
      budget: {
        type: 'Information',
        children: [],
        props: { title: 'Orçamento' },
        bindings: { facts: { source: 'entity', propertyIri: 'foundation:budgetStartDate', transform: 'date' } },
      },
    },
  };
  const connectedBudgetWidget = {
    root: 'budget',
    elements: {
      budget: {
        type: 'Information',
        children: ['decision', 'action'],
        props: { title: 'Orçamento mensal', layout: { columns: 2 }, facts: [] },
        bindings: { facts: { source: 'entity', propertyIri: 'foundation:budgetStartDate' } },
      },
      decision: {
        type: 'Decision',
        children: [],
        props: { title: 'Período', selectionMode: 'single', complexity: 'simple', candidates: [{ value: 'current', title: 'Atual' }] },
      },
      action: {
        type: 'Action',
        children: [],
        props: { label: 'Abrir orçamento', icon: 'open_in_new' },
        on: { press: { action: 'perform' } },
      },
    },
  };

  assert.equal(validateSemanticSpec(validFactsBinding).success, true);
  assert.equal(validateSemanticSpec(connectedBudgetWidget).success, true, 'every component must be reachable through a declared Information slot');
  for (const invalid of [rootFactsBinding, unknownBindingTarget, unknownBindingField]) {
    assert.equal(validateSemanticSpec(invalid).success, false);
  }
});

test('Information rejects unsupported per-fact entity bindings instead of silently dropping them', () => {
  const unsupportedBindingShape = {
    root: 'budget',
    elements: {
      budget: {
        type: 'Information',
        children: [],
        props: {
          title: 'Orçamento mensal',
          facts: [{ label: 'Realizado', value: '', source: 'entity', propertyIri: 'foundation:actualMargin' }],
        },
      },
    },
  };

  const result = validateSemanticSpec(unsupportedBindingShape);
  assert.equal(result.success, false);
  if (result.success === false) {
    assert.match(result.message, /catálogo semântico/);
    assert.match(result.message, /facts\[0\]: dados fora do contrato/);
  }
});

test('nested Information and Details retain their own entity bindings at runtime', () => {
  const source = {
    root: 'overview',
    elements: {
      overview: {
        type: 'Information',
        children: ['details'],
        props: { title: 'Orçamento', layout: { columns: 1 }, facts: [] },
        bindings: { facts: { source: 'entity', propertyIri: 'foundation:budgetStartDate' } },
      },
      details: {
        type: 'Details',
        children: [],
        props: { title: 'Obrigações' },
        bindings: { details: { source: 'entity', propertyIri: 'foundation:budgetObligations' } },
      },
    },
  };
  assert.equal(validateSemanticSpec(source).success, true, 'the real Information → Details shape is accepted by the catalog contract');
  const result = buildRuntimeSemanticSpec(source, [
    { elementId: 'overview', target: 'facts', propertyIri: 'foundation:budgetStartDate' },
    { elementId: 'details', target: 'details', propertyIri: 'foundation:budgetObligations' },
  ]);

  assert.deepEqual(result.spec.elements.overview.props.__ontologyBindings, { facts: 'foundation:budgetStartDate' });
  assert.deepEqual(result.spec.elements.details.props.__ontologyBindings, { details: 'foundation:budgetObligations' });
  assert.deepEqual(result.spec.elements.overview.children, ['details'], 'runtime binding expansion preserves the child reference for the default-slot renderer');
  assert.deepEqual(result.bindingTargets, {
    facts: ['foundation:budgetStartDate'],
    details: ['foundation:budgetObligations'],
  });
  assert.equal(Object.hasOwn(source.elements.overview.props, '__ontologyBindings'), false, 'runtime bindings must not mutate the saved spec');
});

test('ValueEntry v2 preserves entity bindings and multi-value cardinality at runtime', () => {
  const source = {
    root: 'aliases',
    elements: {
      aliases: {
        type: 'ValueEntry',
        children: [],
        props: { label: 'Aliases', cardinality: 'multiple', valueType: 'text', value: ['A', 'B'], editable: true },
      },
    },
  };
  const result = buildRuntimeSemanticSpec(source, [
    { elementId: 'aliases', target: 'value', propertyIri: 'foundation:alternateNames' },
  ]);

  assert.equal(semanticCatalog.data.components.ValueEntry.contractVersion, 2);
  assert.equal(result.spec.elements.aliases.props?.cardinality, 'multiple');
  assert.deepEqual(result.spec.elements.aliases.props?.value, ['A', 'B']);
  assert.deepEqual(result.spec.elements.aliases.props?.__ontologyBindings, { value: 'foundation:alternateNames' });
  assert.deepEqual(result.bindingTargets, { value: ['foundation:alternateNames'] });
  assert.equal(Object.hasOwn(source.elements.aliases.props, '__ontologyBindings'), false, 'runtime bindings must not mutate the saved spec');
});

test('a class template keeps its source separate from the entity whose data it renders', () => {
  assert.deepEqual(resolveSemanticWidgetSource({
    entityId: 'foundation:MonthlyBudget_2026_08',
    specTemplateIri: 'foundation:JsonRenderSpec_MonthlyBudget',
  }), {
    templateEntityIri: 'foundation:JsonRenderSpec_MonthlyBudget',
    targetEntityIri: 'foundation:MonthlyBudget_2026_08',
  });
});

test('changing the rendered entity changes the widget load identity', () => {
  const template = 'foundation:JsonRenderSpec_MonthlyBudget';
  assert.notEqual(
    semanticWidgetLoadIdentity('foundation:MonthlyBudget_A', template),
    semanticWidgetLoadIdentity('foundation:MonthlyBudget_B', template),
  );
  assert.notEqual(
    semanticWidgetLoadIdentity('foundation:MonthlyBudget_A', template),
    semanticWidgetLoadIdentity('foundation:MonthlyBudget_A', 'foundation:JsonRenderSpec_Other'),
  );
  assert.equal(
    semanticWidgetLoadIdentity('foundation:MonthlyBudget_A', null),
    semanticWidgetLoadIdentity('foundation:MonthlyBudget_A', undefined),
  );
});

test('a standalone semantic widget uses its declared render target', () => {
  assert.deepEqual(resolveSemanticWidgetSource({
    entityId: 'foundation:JsonRenderSpec_Standalone',
    templateSourceEntityIri: 'foundation:Task_Example',
  }), {
    templateEntityIri: 'foundation:JsonRenderSpec_Standalone',
    targetEntityIri: 'foundation:Task_Example',
  });
});

test('a target data update refreshes bindings without reloading its template', () => {
  assert.equal(semanticWidgetEventAction({
    eventType: 'updated',
    eventEntityIri: 'foundation:MonthlyBudget_2026_08',
    templateEntityIri: 'foundation:JsonRenderSpec_MonthlyBudget',
    targetEntityIri: 'foundation:MonthlyBudget_2026_08',
    referencedEntityIris: [],
    changedPredicates: ['foundation:actualMargin'],
    watchedPredicates: ['foundation:actualMargin'],
  }), 'refresh-target');
});

test('deleting a row referenced by a Records collection refreshes the collection', () => {
  assert.equal(semanticWidgetEventAction({
    eventType: 'deleted',
    eventEntityIri: 'foundation:Transaction_42',
    templateEntityIri: 'foundation:JsonRenderSpec_Statement',
    targetEntityIri: 'foundation:MonthlyStatement_2026_08',
    referencedEntityIris: ['foundation:Transaction_42'],
    watchedPredicates: ['foundation:statementTransactions'],
  }), 'refresh-target');
});

test('a target update uses its event transaction as the binding snapshot', () => {
  assert.deepEqual(resolveSemanticRefreshSnapshot(100, 101), { kind: 'event', snapshotTx: 101 });
  assert.deepEqual(resolveSemanticRefreshSnapshot(101, 101), { kind: 'ignore' });
  assert.deepEqual(resolveSemanticRefreshSnapshot(101, 99), { kind: 'ignore' });
  assert.deepEqual(resolveSemanticRefreshSnapshot(101), { kind: 'reload' });
  assert.deepEqual(resolveSemanticRefreshSnapshot(101, 0), { kind: 'reload' });
});

test('an unversioned query materialization update on the owner refreshes its bound collection', () => {
  assert.equal(semanticWidgetEventAction({
    eventType: 'updated',
    eventEntityIri: 'foundation:MonthlyBudget_2026_11',
    templateEntityIri: 'foundation:JsonRenderSpec_MonthlyBudget',
    targetEntityIri: 'foundation:MonthlyBudget_2026_11',
    referencedEntityIris: [],
    changedPredicates: [],
    watchedPredicates: ['foundation:categorizedExpensesThisMonth'],
  }), 'refresh-target');
  assert.deepEqual(resolveSemanticRefreshSnapshot(100, 0), { kind: 'reload' });
});

test('an active widget subscription refreshes a query collection after an unversioned owner event', async () => {
  const listeners = new Set<(event: { type: 'updated'; entityId: string; changedPredicates: string[]; tx: number }) => void>();
  const watch = (handler: (event: { type: 'updated'; entityId: string; changedPredicates: string[]; tx: number }) => void) => {
    listeners.add(handler);
    return () => listeners.delete(handler);
  };
  let snapshotTx = 100;
  let items = [{ projectedProperties: [
    { propertyIri: 'amount', unitLabel: 'BRL', items: [{ value: '10' }] },
    { propertyIri: 'category', items: [{ value: 'food', label: 'Alimentação' }] },
  ] }];
  let refreshCount = 0;
  const stopWatching = watch((event) => {
    const action = semanticWidgetEventAction({
      eventType: event.type,
      eventEntityIri: event.entityId,
      templateEntityIri: 'foundation:JsonRenderSpec_Generic',
      targetEntityIri: 'foundation:Entity_owner',
      referencedEntityIris: [],
      changedPredicates: event.changedPredicates,
      watchedPredicates: ['foundation:paymentsQuery'],
    });
    if (action !== 'refresh-target') return;
    const refresh = resolveSemanticRefreshSnapshot(snapshotTx, event.tx);
    if (refresh.kind === 'ignore') return;
    snapshotTx = refresh.kind === 'event' ? refresh.snapshotTx : snapshotTx + 1;
    items = [{ projectedProperties: [
      { propertyIri: 'amount', unitLabel: 'BRL', items: [{ value: '27' }] },
      { propertyIri: 'category', items: [{ value: 'transport', label: 'Transporte' }] },
    ] }];
    refreshCount += 1;
  });

  assert.equal(listeners.size, 1, 'the widget remains registered while its render is visible');
  for (const listener of listeners) listener({
    type: 'updated',
    entityId: 'foundation:Entity_owner',
    changedPredicates: [],
    tx: 0,
  });

  assert.equal(refreshCount, 1, 'an owner event with unknown revision triggers a current snapshot reload');
  assert.equal(snapshotTx, 101);
  assert.deepEqual(aggregateDistribution(items, 'amount', 'category').groups.map((group: { group: string; amount: number }) => [group.group, group.amount]), [
    ['Transporte', 27],
  ]);
  stopWatching();
  assert.equal(listeners.size, 0, 'closing the mock widget detaches its listener');
});

test('a live entity update invalidates outstanding initial reads and pagination requests', () => {
  const action = semanticWidgetEventAction({
    eventType: 'updated',
    eventEntityIri: 'foundation:MonthlyBudget_2026_08',
    templateEntityIri: 'foundation:JsonRenderSpec_MonthlyBudget',
    targetEntityIri: 'foundation:MonthlyBudget_2026_08',
    referencedEntityIris: [],
    changedPredicates: ['foundation:budgetStartDate'],
    watchedPredicates: ['foundation:budgetStartDate', 'foundation:budgetAccounts'],
  });
  assert.equal(action, 'refresh-target');

  const initialRead = { loadGeneration: 4, refreshGeneration: 0 };
  const refreshedRead = { loadGeneration: 4, refreshGeneration: 1 };
  const currentLoad = { loadGeneration: 4, refreshGeneration: 1 };

  assert.equal(
    isSemanticRequestCurrent(initialRead.loadGeneration, currentLoad.loadGeneration, initialRead.refreshGeneration, currentLoad.refreshGeneration),
    false,
    'a response from the pre-update snapshot cannot overwrite the fresh field value',
  );
  assert.equal(
    isSemanticRequestCurrent(refreshedRead.loadGeneration, currentLoad.loadGeneration, refreshedRead.refreshGeneration, currentLoad.refreshGeneration),
    true,
    'the field read for the update snapshot is accepted immediately, without waiting for another control',
  );
});

test('a live refresh invalidates an older paginated response without changing the loaded-page count', () => {
  const firstPageRequest = { loadGeneration: 7, refreshGeneration: 2 };
  const secondPageRequest = { loadGeneration: 7, refreshGeneration: 2 };
  const currentAfterPropertyUpdate = { loadGeneration: 7, refreshGeneration: 3 };

  assert.equal(
    isSemanticRequestCurrent(firstPageRequest.loadGeneration, currentAfterPropertyUpdate.loadGeneration, firstPageRequest.refreshGeneration, currentAfterPropertyUpdate.refreshGeneration),
    false,
    'a stale first page cannot replace a collection from the newer snapshot',
  );
  assert.equal(
    isSemanticRequestCurrent(secondPageRequest.loadGeneration, currentAfterPropertyUpdate.loadGeneration, secondPageRequest.refreshGeneration, currentAfterPropertyUpdate.refreshGeneration),
    false,
    'an in-flight next page cannot append items from the previous snapshot',
  );
  assert.equal(
    isSemanticRequestCurrent(7, 7, 3, 3),
    true,
    'pagination requests for the current snapshot remain valid and preserve their loaded-page state',
  );
});

test('live collection refresh stops when pagination fails or makes no progress', () => {
  assert.equal(shouldContinueSemanticPagination({ loadedCount: 5, targetCount: 8, hasMore: true, error: '' }), true);
  assert.equal(shouldContinueSemanticPagination({ loadedCount: 5, targetCount: 8, hasMore: true, error: 'Falha ao buscar página' }), false);
  assert.equal(shouldContinueSemanticPagination({ loadedCount: 8, targetCount: 8, hasMore: true, error: '' }), false);
  assert.equal(semanticPaginationAdvanced(
    { items: ['A', 'B'], cursor: { value: '2' } },
    { items: ['A', 'B'], cursor: { value: '2' } },
  ), false);
  assert.equal(semanticPaginationAdvanced(
    { items: ['A', 'B'], cursor: { value: '2' } },
    { items: ['A', 'B', 'C'], cursor: { value: '3' } },
  ), true);
});

test('a template update reloads the structure instead of refreshing target values', () => {
  assert.equal(semanticWidgetEventAction({
    eventType: 'updated',
    eventEntityIri: 'foundation:JsonRenderSpec_MonthlyBudget',
    templateEntityIri: 'foundation:JsonRenderSpec_MonthlyBudget',
    targetEntityIri: 'foundation:MonthlyBudget_2026_08',
    referencedEntityIris: [],
    changedPredicates: ['foundation:specSource'],
    watchedPredicates: ['foundation:actualMargin'],
  }), 'reload-template');
});

test('template reload classification ignores snapshot events and accepts newer or unversioned events', () => {
  assert.equal(shouldReloadSemanticTemplate(100, 100), false);
  assert.equal(shouldReloadSemanticTemplate(99, 100), false);
  assert.equal(shouldReloadSemanticTemplate(101, 100), true);
  assert.equal(shouldReloadSemanticTemplate(undefined, 100), true);
});

test('overlapping semantic reads coalesce into one follow-up and loading settles only after the latest read', async () => {
  const loadingStates: boolean[] = [];
  const controller = createSemanticLoadController({
    onLoadingChange: (loading: boolean) => loadingStates.push(loading),
    onUnexpectedError: (error: unknown) => { throw error; },
  });
  const deferred: Array<{ resolve: () => void }> = [];
  let readCount = 0;
  const run = async () => {
    readCount += 1;
    await new Promise<void>((resolve) => deferred.push({ resolve }));
  };

  const completion = controller.request(run);
  await Promise.resolve();
  controller.request(run);
  controller.request(run);
  assert.equal(readCount, 1, 'overlapping reload requests must not start concurrent reads');
  assert.deepEqual(loadingStates, [true], 'the queued refresh must not clear loading between reads');

  deferred[0].resolve();
  await Promise.resolve();
  await Promise.resolve();
  assert.equal(readCount, 2, 'multiple requests while reading collapse to one follow-up');
  assert.deepEqual(loadingStates, [true]);

  deferred[1].resolve();
  await completion;
  assert.deepEqual(loadingStates, [true, false]);
  assert.equal(controller.generation, 2);
});

test('a new widget identity supersedes the old read and stale output cannot replace current content', async () => {
  const loadingStates: boolean[] = [];
  const controller = createSemanticLoadController({
    onLoadingChange: (loading: boolean) => loadingStates.push(loading),
    onUnexpectedError: (error: unknown) => { throw error; },
  });
  let resolveOld!: () => void;
  let resolveNew!: () => void;
  let displayed = '';
  const oldRead = controller.request(async (generation: number) => {
    await new Promise<void>((resolve) => { resolveOld = resolve; });
    if (controller.isCurrent(generation)) displayed = 'stale';
  });
  await Promise.resolve();
  const newRead = controller.request(async (generation: number) => {
    await new Promise<void>((resolve) => { resolveNew = resolve; });
    if (controller.isCurrent(generation)) displayed = 'current';
  }, true);
  await Promise.resolve();

  resolveOld();
  await oldRead;
  assert.equal(displayed, '', 'the superseded read is ignored');
  assert.deepEqual(loadingStates, [true, true], 'the stale request cannot clear the new loading state');

  resolveNew();
  await newRead;
  assert.equal(displayed, 'current');
  assert.deepEqual(loadingStates, [true, true, false]);
});

test('a failed current semantic read reports an error and always exits loading', async () => {
  const loadingStates: boolean[] = [];
  const errors: unknown[] = [];
  const controller = createSemanticLoadController({
    onLoadingChange: (loading: boolean) => loadingStates.push(loading),
    onUnexpectedError: (error: unknown) => errors.push(error),
  });

  await controller.request(async () => { throw new Error('read failed'); });

  assert.equal(errors.length, 1);
  assert.match(String(errors[0]), /read failed/);
  assert.deepEqual(loadingStates, [true, false]);
});

test('an initial mount performs one read without an artificial template reload', async () => {
  const loadingStates: boolean[] = [];
  const controller = createSemanticLoadController({
    onLoadingChange: (loading: boolean) => loadingStates.push(loading),
    onUnexpectedError: (error: unknown) => { throw error; },
  });
  let reads = 0;

  await controller.request(async () => { reads += 1; });

  assert.equal(reads, 1);
  assert.equal(shouldReloadSemanticTemplate(42, 42), false);
  assert.deepEqual(loadingStates, [true, false]);
});

test('invalid semantic inputs produce an explicit validation failure', () => {
  const missingDecisionData = { root: 'choice', elements: { choice: { type: 'Decision', props: { title: 'Escolha', selectionMode: 'single', complexity: 'simple' } } } };
  assert.equal(validateSemanticSpec(missingDecisionData).success, false);

  const visualPrimitive = { root: 'button', elements: { button: { type: 'Button', children: [], props: { label: 'Salvar' } } } };
  const result = validateSemanticSpec(visualPrimitive);
  assert.equal(result.success, false);
  if (result.success === false) assert.match(result.message, /catálogo semântico/);
});

test('Action accepts only the empty local perform binding used by widget-owned controls', () => {
  const localAction = {
    root: 'edit',
    elements: {
      edit: {
        type: 'Action',
        children: [],
        props: { label: 'Editar propriedades', icon: 'edit', priority: 'supporting' },
        on: { press: { action: 'perform' } },
      },
    },
  };
  assert.equal(validateSemanticSpec(localAction).success, true);
  assert.equal(semanticCatalog.data.components.Action.props.safeParse({ label: 'Salvar' }).success, false, 'semantic actions require a representative icon');
  assert.equal(semanticCatalog.data.actions.perform.params.safeParse({}).success, true);
  assert.equal(semanticCatalog.data.actions.perform.params.safeParse({ unapproved: true }).success, false);
});

test('ValueEntry accepts the inspector specialized value types and rejects unknown dispatch values', () => {
  const valueTypes = ['text', 'email', 'number', 'boolean', 'date', 'date_time', 'currency', 'percentage', 'url', 'code', 'markdown', 'html', 'query_config', 'recurrence_rule'];
  for (const valueType of valueTypes) {
    const result = validateSemanticSpec({
      root: 'entry',
      elements: { entry: { type: 'ValueEntry', children: [], props: { label: 'Campo', valueType, value: { $bindState: '/value' }, language: 'typescript', editable: true } } },
      state: { value: '' },
    });
    assert.equal(result.success, true, `${valueType} should be a valid ValueEntry dispatch`);
  }
  const invalid = validateSemanticSpec({
    root: 'entry',
    elements: { entry: { type: 'ValueEntry', children: [], props: { label: 'Campo', valueType: 'executable_html' } } },
  });
  assert.equal(invalid.success, false, 'unsupported or executable content types must be rejected');
  const multipleValues = validateSemanticSpec({
    root: 'entry',
    elements: { entry: { type: 'ValueEntry', children: [], props: { label: 'Aliases', value: { $bindState: '/values' }, cardinality: 'multiple', valueType: 'text', editable: true } } },
    state: { values: ['A', 'B'] },
  });
  assert.equal(multipleValues.success, true, 'multiple ValueEntry should accept a complete string set');
});

test('decision presentation derives detail from complexity and search from candidate volume', () => {
  assert.deepEqual(resolveDecisionPresentation(3, 'simple'), { searchable: false, comparison: false });
  assert.deepEqual(resolveDecisionPresentation(3, 'complex'), { searchable: false, comparison: true });
  assert.deepEqual(resolveDecisionPresentation(5, 'complex'), { searchable: true, comparison: true });
});

test('semantic component DOM ids are unique per instance and keep accessible relationships local', async () => {
  const first = createSemanticDomIds('svelte-instance-a');
  const second = createSemanticDomIds('svelte-instance-b');
  const firstIds = Object.values(first);
  const secondIds = Object.values(second);

  assert.equal(new Set(firstIds).size, firstIds.length, 'ids within one component instance must be unique');
  assert.equal(new Set([...firstIds, ...secondIds]).size, firstIds.length + secondIds.length, 'separate instances must not share ids');
  assert.equal(first.decisionTitle, 'svelte-instance-a-decision-title');
  assert.equal(first.decisionSearch, 'svelte-instance-a-decision-search');
  assert.equal(first.value, 'svelte-instance-a-semantic-value');

  const adapter = await readFile(new URL('./SemanticComponentAdapter.svelte', import.meta.url), 'utf8');
  for (const fixedId of ['decision-title', 'decision-search', 'semantic-value', 'information-title', 'comparison-title', 'distribution-title', 'records-title', 'records-filter-column', 'records-filter-value', 'records-filter-operator', 'confirmation-title']) {
    assert.doesNotMatch(adapter, new RegExp(`(?:id|for|aria-labelledby)="${fixedId}"`), `${fixedId} must not be a fixed DOM id`);
  }
  assert.match(adapter, /aria-labelledby=\{semanticDomIds\.decisionTitle\}/);
  assert.match(adapter, /for=\{semanticDomIds\.decisionSearch\}/);
  assert.match(adapter, /for=\{hasLabelableValueControl \? semanticDomIds\.value : undefined\}/);
  assert.match(adapter, /aria-labelledby=\{semanticDomIds\.valueLabel\}/);
  assert.match(adapter, /aria-describedby=\{props\.description \? semanticDomIds\.valueDescription : undefined\}/);
  assert.match(adapter, /aria-labelledby=\{semanticDomIds\.informationTitle\}/);
  assert.match(adapter, /aria-labelledby=\{semanticDomIds\.comparisonTitle\}/);
  assert.match(adapter, /id=\{semanticDomIds\.comparisonTitle\}/);
  assert.match(adapter, /data-layout-columns=\{informationColumns\}/);
  assert.match(adapter, /class:first-child-spans=\{informationPrimaryChildSpans\}/);
  assert.match(adapter, /information-block\.first-child-spans > \.semantic-children > \.semantic-block:first-child/);
  assert.match(adapter, /semantic-children > \.semantic-block:first-child \.fact-list dd/);
  assert.match(adapter, /aria-label=\{`\$\{props\.metric\}: \$\{comparison\.interpretation/);
  assert.match(adapter, /comparison-change-divider/);
  assert.match(adapter, /id=\{semanticDomIds\.recordsTitle\}/);
  assert.match(adapter, /aria-labelledby=\{semanticDomIds\.confirmationTitle\}/);
});

test('legacy component specs continue validating in the original catalog', () => {
  const legacySpecs = [
    { root: 'card', elements: { card: { type: 'Card', children: [], props: {} } } },
    { root: 'button', elements: { button: { type: 'Button', children: [], props: { label: 'Continuar' } } } },
    { root: 'text', elements: { text: { type: 'Text', children: [], props: { text: 'Resumo' } } } },
  ];
  for (const legacySpec of legacySpecs) {
    assert.equal(validateLegacyJsonSpec(legacySpec), true);
  }
  assert.equal(Object.hasOwn(componentCatalog.data.components, 'EntityList'), false, 'EntityList is not currently part of the legacy catalog');
});
