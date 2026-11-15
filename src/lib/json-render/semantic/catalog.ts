import { defineCatalog } from '@json-render/core';
import { schema } from '@json-render/svelte/schema';
import { z } from 'zod';

const text = z.string();
const optionalText = text.optional();
const defaultSlot = ['default'];
const fact = z.object({ label: text, value: text, unit: optionalText }).strict();
const relationship = z.object({ label: text, value: text }).strict();
const entityBinding = z.object({
  source: z.literal('entity'),
  propertyIri: text.min(1),
}).strict();
const projectionBinding = entityBinding.extend({
  projections: z.object({ measure: text.min(1), groupBy: text.min(1) }).strict(),
}).strict();
const candidate = z.object({
  value: text,
  title: text,
  description: optionalText,
  disabled: z.boolean().optional(),
  comparison: z.record(text, text).optional(),
});

export const semanticCatalog = defineCatalog(schema, {
  components: {
    Decision: {
      contractVersion: 1,
      props: z.object({
        title: text,
        description: optionalText,
        domain: optionalText,
        selectionMode: z.enum(['single', 'multiple']),
        complexity: z.enum(['simple', 'complex']),
        groupName: optionalText,
        candidates: z.array(candidate).min(1),
        selected: z.union([text, z.array(text)]).optional(),
        comparisonFactors: z.array(text).optional(),
        consequence: optionalText,
        reversibility: z.enum(['reversible', 'hard_to_reverse', 'irreversible']).optional(),
      }),
      slots: [],
      description: 'Ajuda a escolher uma ou mais alternativas pelo domínio, volume, complexidade e fatores de comparação.',
    },
    ValueEntry: {
      contractVersion: 2,
      props: z.object({
        label: text,
        labelVisible: z.boolean().optional(),
        value: z.union([text, z.array(text)]).optional(),
        cardinality: z.enum(['single', 'multiple']).optional(),
        valueType: z.enum(['text', 'email', 'number', 'boolean', 'date', 'date_time', 'currency', 'percentage', 'url', 'code', 'markdown', 'html', 'query_config', 'recurrence_rule']),
        language: optionalText,
        description: optionalText,
        placeholder: optionalText,
        unit: optionalText,
        validationMessage: optionalText,
        valid: z.boolean().optional(),
        sensitive: z.boolean().optional(),
        editable: z.boolean().default(true),
        required: z.boolean().optional(),
      }),
      slots: [],
      description: 'Apresenta ou coleta um valor respeitando tipo, unidade, validação, sensibilidade e editabilidade.',
    },
    Action: {
      contractVersion: 1,
      props: z.object({
        label: text,
        icon: text,
        objective: optionalText,
        priority: z.enum(['primary', 'supporting', 'destructive']).optional(),
        frequency: z.enum(['routine', 'occasional', 'rare']).optional(),
        impact: optionalText,
        reversibility: z.enum(['reversible', 'hard_to_reverse', 'irreversible']).optional(),
        available: z.boolean().optional(),
        progress: optionalText,
        result: optionalText,
      }),
      slots: [],
      description: 'Expõe uma ação de acordo com sua prioridade, frequência, impacto, disponibilidade e resultado.',
    },
    Information: {
      contractVersion: 0,
      props: z.object({
        title: text,
        summary: optionalText,
        role: optionalText,
        priority: z.enum(['primary', 'supporting', 'supplemental']).optional(),
        urgency: z.enum(['routine', 'attention', 'urgent']).optional(),
        status: optionalText,
        layout: z.object({ columns: z.number().int().min(1).max(6) }).strict(),
        facts: z.array(fact).optional(),
        relationships: z.array(relationship).optional(),
      }).strict(),
      slots: defaultSlot,
      description: 'Organiza informação relacionada em linhas e colunas deliberadas. O primeiro filho recebe hierarquia visual primária, os grupos usam uma superfície semântica compartilhada, descrições ficam subordinadas e um único valor ligado usa o título como rótulo.',
    },
    Details: {
      contractVersion: 2,
      props: z.object({
        title: text,
        summary: optionalText,
        relevance: optionalText,
        details: z.array(z.object({ label: text, value: text })).optional(),
        initiallyExpanded: z.boolean().optional(),
      }),
      slots: [],
      description: 'Revela informação complementar sob demanda sem perder o resumo e o contexto.',
    },
    Confirmation: {
      contractVersion: 1,
      props: z.object({
        action: text,
        consequence: text,
        impact: optionalText,
        reversibility: z.enum(['reversible', 'hard_to_reverse', 'irreversible']),
        confirmLabel: optionalText,
        cancelLabel: optionalText,
        requiresConfirmation: z.boolean().default(true),
      }),
      slots: [],
      description: 'Torna explícita a consequência antes de confirmar ou cancelar uma ação.',
    },
    Comparison: {
      contractVersion: 2,
      props: z.object({
        title: text,
        metric: text,
        currentPeriod: text,
        referencePeriod: text,
        favorableDirection: z.enum(['higher', 'lower', 'neutral']),
        unavailableBaselineMessage: optionalText,
      }).strict(),
      slots: [],
      description: 'Compara valores de períodos equivalentes com barras pareadas em escala comum, valores, variação e interpretação favorável, desfavorável ou neutra; omite comparação quando período, unidade ou baseline não permitem uma leitura válida.',
    },
    Distribution: {
      contractVersion: 2,
      props: z.object({
        title: text,
        description: optionalText,
        emptyMessage: optionalText,
      }).strict(),
      slots: [],
      description: 'Agrupa valores numéricos projetados de uma coleção por valores literais ou referências projetadas, contabiliza itens sem agrupamento e informa total e cobertura da coleção paginada.',
    },
  },
  actions: {
    announce: { params: z.object({ message: text.max(500) }), description: 'Apresenta uma mensagem local no widget.' },
    perform: { params: z.object({}).strict(), description: 'Executa uma ação local explicitamente conectada pelo widget.' },
    setState: { params: z.object({ statePath: text, value: z.unknown() }), description: 'Atualiza estado local do widget.' },
    pushState: { params: z.object({ statePath: text, value: z.unknown() }), description: 'Adiciona valor ao estado local do widget.' },
    removeState: { params: z.object({ statePath: text, index: z.number() }), description: 'Remove valor do estado local do widget.' },
    validateForm: { params: z.object({ formPath: text }), description: 'Valida formulário local.' },
  },
});

const entityBindingContract = {
  type: 'object',
  additionalProperties: false,
  properties: {
    source: { type: 'string', const: 'entity', description: 'Lê da entidade declarada em foundation:rendersEntity.' },
    propertyIri: { type: 'string', minLength: 1, description: 'IRI da propriedade da ontologia lida no runtime.' },
  },
  required: ['source', 'propertyIri'],
};

const componentBindingTargets: Record<string, string[]> = {
  ValueEntry: ['value'],
  Information: ['facts', 'relationships'],
  Details: ['details'],
  Comparison: ['current', 'reference', 'periodStart', 'periodEnd'],
  Distribution: ['collection'],
};

export function semanticComponentPropsContract(componentId: string) {
  const definition = semanticCatalog.data.components[componentId as keyof typeof semanticCatalog.data.components];
  if (!definition) return null;
  const propsSchema = definition.props.toJSONSchema() as Record<string, unknown>;
  const targets = componentBindingTargets[componentId];
  if (targets) {
    propsSchema['x-foundation-element-bindings'] = {
      type: 'object',
      additionalProperties: false,
      properties: Object.fromEntries(targets.map((target) => [target, componentId === 'Distribution' && target === 'collection'
        ? projectionBinding.toJSONSchema()
        : entityBindingContract])),
      description: 'Bindings declarados no elemento JSON, ao lado de type/props. O valor é lido da entidade em foundation:rendersEntity e nunca copiado para specSource.',
    };
  }
  return propsSchema;
}

export function semanticActionsContract() {
  return Object.entries(semanticCatalog.data.actions).map(([id, definition]) => ({
    id,
    description: definition.description,
    paramsSchema: definition.params.toJSONSchema(),
  }));
}

export const semanticCatalogManifest = {
  version: 2,
  source: 'src/lib/json-render/semantic/catalog.ts',
  componentIds: Object.keys(semanticCatalog.data.components),
  actionIds: Object.keys(semanticCatalog.data.actions),
  components: Object.entries(semanticCatalog.data.components).map(([id, definition]) => ({
    id,
    contractVersion: definition.contractVersion,
    description: definition.description,
    propsSchema: semanticComponentPropsContract(id),
    slots: definition.slots,
  })),
} as const;
