import type { Spec } from '@json-render/core';

export const semanticExamples: Record<string, Spec & { elements: Record<string, Record<string, any>> }> = {
  Decision: {
    root: 'model-choice',
    elements: { 'model-choice': { type: 'Decision', children: [], props: { title: 'Modelo de IA', description: 'Escolha pelo uso e pelos fatores de comparação.', selectionMode: 'single', complexity: 'complex', candidates: [{ value: 'fast', title: 'Rápido', description: 'Responde depressa', comparison: { Preço: 'Baixo', Contexto: 'Médio' } }, { value: 'capable', title: 'Mais capaz', description: 'Bom para análise', comparison: { Preço: 'Médio', Contexto: 'Alto' } }], selected: { $bindState: '/selectedModel' }, comparisonFactors: ['Preço', 'Contexto'], consequence: 'Usado nas próximas conversas.' } } },
    state: { selectedModel: 'capable' },
  },
  ValueEntry: {
    root: 'amount',
    elements: { amount: { type: 'ValueEntry', children: [], props: { label: 'Valor mensal', value: { $bindState: '/monthlyAmount' }, valueType: 'currency', unit: 'BRL', valid: true, editable: true } } },
    state: { monthlyAmount: '1200' },
  },
  Action: {
    root: 'sync',
    elements: { sync: { type: 'Action', children: [], props: { label: 'Sincronizar agora', icon: 'sync', objective: 'Buscar novos dados da conta.', priority: 'primary', frequency: 'occasional', available: true }, on: { press: { action: 'announce', params: { message: 'A sincronização foi solicitada.' } } } } },
  },
  Information: {
    root: 'account-overview',
    elements: {
      'account-overview': { type: 'Information', children: ['account-balance', 'account-status'], props: { title: 'Conta principal', summary: 'Visão organizada em duas colunas.', layout: { columns: 2 } } },
      'account-balance': { type: 'Information', children: [], props: { title: 'Saldo atual', layout: { columns: 1 }, facts: [{ label: 'Disponível', value: 'R$ 4.250,00' }] } },
      'account-status': { type: 'Information', children: [], props: { title: 'Estado da conta', layout: { columns: 1 }, facts: [{ label: 'Última sincronização', value: 'Hoje, 10:30' }, { label: 'Instituição', value: 'Banco local' }] } },
    },
  },
  Details: {
    root: 'transaction',
    elements: { transaction: { type: 'Details', children: [], props: { title: 'Transação', summary: 'Compra no mercado', relevance: 'Detalhes para conferência', details: [{ label: 'Data', value: '05/11/2026' }, { label: 'Valor', value: 'R$ 82,40' }] } } },
  },
  Confirmation: {
    root: 'delete',
    elements: { delete: { type: 'Confirmation', children: [], props: { action: 'Excluir esta entidade', consequence: 'Os dados vinculados serão removidos.', impact: 'A ação não pode ser desfeita.', reversibility: 'irreversible' }, on: { confirm: { action: 'announce', params: { message: 'Consequência confirmada no preview local.' } }, cancel: { action: 'announce', params: { message: 'Confirmação cancelada sem alterar dados.' } } } } },
  },
  Comparison: {
    root: 'monthly-income-comparison',
    elements: {
      'monthly-income-comparison': {
        type: 'Comparison',
        children: [],
        props: { title: 'Receita realizada', metric: 'Receita', currentPeriod: 'Mês atual até hoje', referencePeriod: 'Mesmo intervalo do mês anterior', favorableDirection: 'higher' },
        bindings: {
          current: { source: 'entity', propertyIri: 'foundation:totalActualIncome' },
          reference: { source: 'entity', propertyIri: 'foundation:previousMonthActualIncome' },
          periodStart: { source: 'entity', propertyIri: 'foundation:budgetStartDate' },
          periodEnd: { source: 'entity', propertyIri: 'foundation:budgetEndDate' },
        },
      },
    },
  },
  Distribution: {
    root: 'distribution',
    elements: {
      distribution: {
        type: 'Distribution',
        children: [],
        props: { title: 'Valores por grupo', description: 'Distribuição da medida por agrupamento.' },
        bindings: {
          collection: {
            source: 'entity',
            propertyIri: 'foundation:exampleCollection',
            projections: { measure: 'foundation:exampleMeasure', groupBy: 'foundation:exampleGroup' },
          },
        },
      },
    },
  },
  Records: {
    root: 'ledger',
    elements: {
      ledger: {
        type: 'Records',
        children: [],
        props: {
          title: 'Movimentações',
          columns: [
            { id: 'date', label: 'Data', valueType: 'date', sortable: true, filterOperators: ['equals', 'gt', 'lt'] },
            { id: 'description', label: 'Detalhe', valueType: 'text', sortable: true, filterOperators: ['contains', 'equals'] },
            { id: 'amount', label: 'Valor', valueType: 'currency', unit: 'BRL', sortable: true, filterOperators: ['gt', 'lt', 'equals'] },
          ],
          defaultSort: { columnId: 'date', direction: 'desc' },
          pageSize: 25,
        },
        bindings: {
          records: {
            sources: [{
              source: 'entity',
              alias: 'payments',
              propertyIri: 'foundation:examplePayments',
              fields: { date: 'foundation:hasStartTime', description: 'foundation:transactionDescription', amount: 'foundation:transactionAmount' },
            }],
          },
        },
      },
    },
  },
};
