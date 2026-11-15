import { validateSpec, type Spec } from '@json-render/core';
import { componentCatalog } from '../catalog';
import { semanticCatalog } from './catalog';

type ActionBinding = { action?: string; params?: Record<string, unknown> };
type EntityPropertyBinding = { source?: string; propertyIri?: string; projections?: Record<string, string> };
type ElementWithActions = { type?: string; children?: string[]; on?: Record<string, ActionBinding | ActionBinding[]>; bindings?: Record<string, EntityPropertyBinding> };

const allowedEvents: Record<string, string[]> = {
  Decision: ['change'],
  ValueEntry: ['change'],
  Action: ['press'],
  Information: [],
  Details: [],
  Confirmation: ['confirm', 'cancel'],
  Comparison: [],
  Distribution: [],
};

export type SemanticSpecValidation = { success: true; spec: Spec } | { success: false; message: string };

function validateSemanticTree(input: Record<string, unknown>) {
  if (typeof input.root !== 'string' || !input.root.trim() || !input.elements || typeof input.elements !== 'object' || Array.isArray(input.elements)) {
    return 'A especificação está incompleta. Informe o elemento raiz e o mapa de elementos.';
  }

  const elements = input.elements as Record<string, Record<string, unknown>>;
  if (!Object.keys(elements).length || !Object.hasOwn(elements, input.root)) {
    return 'A especificação está incompleta. O elemento raiz precisa existir no mapa de elementos.';
  }

  const edges = new Map<string, string[]>();
  for (const [elementId, element] of Object.entries(elements)) {
    if (!element || typeof element !== 'object' || Array.isArray(element)) {
      return `O elemento ${elementId} precisa ser um objeto.`;
    }
    if (Object.keys(element).some((key) => !['type', 'props', 'children', 'on', 'visible', 'bindings'].includes(key))) {
      return `O elemento ${elementId} contém campos fora do contrato semântico.`;
    }
    if (!Array.isArray(element.children) || element.children.some((child) => typeof child !== 'string' || !child.trim())) {
      return `O elemento ${elementId} precisa declarar children como uma lista explícita de identificadores.`;
    }

    edges.set(elementId, [...element.children] as string[]);
  }

  const reachable = new Set<string>();
  const active = new Set<string>();
  const parentByChild = new Map<string, string>();
  const visit = (elementId: string): string | null => {
    if (!Object.hasOwn(elements, elementId)) return `A especificação referencia o elemento inexistente ${elementId}.`;
    if (active.has(elementId)) return `A especificação contém um ciclo no elemento ${elementId}.`;
    if (reachable.has(elementId)) return `O elemento ${elementId} está conectado mais de uma vez à árvore.`;
    active.add(elementId);
    reachable.add(elementId);
    for (const child of edges.get(elementId) ?? []) {
      const previousParent = parentByChild.get(child);
      if (previousParent) return `O elemento ${child} possui mais de um elemento pai.`;
      parentByChild.set(child, elementId);
      const error = visit(child);
      if (error) return error;
    }
    active.delete(elementId);
    return null;
  };

  const treeError = visit(input.root);
  if (treeError) return treeError;
  const orphan = Object.keys(elements).find((elementId) => !reachable.has(elementId));
  if (orphan) return `O elemento ${orphan} não está conectado à raiz por um slot declarado.`;
  return null;
}

function describeCatalogIssues(issues: Array<{ path: PropertyKey[]; message: string }>) {
  return issues.slice(0, 3).map((issue) => {
    const path = issue.path.map((segment) => typeof segment === 'number' ? `[${segment}]` : String(segment)).join('.').replace(/\.\[/g, '[') || 'raiz';
    return `${path}: dados fora do contrato`;
  });
}

export function validateSemanticSpec(input: unknown): SemanticSpecValidation {
  if (!input || typeof input !== 'object' || Array.isArray(input)) {
    return { success: false, message: 'A especificação semântica precisa ser um objeto JSON.' };
  }

  const treeError = validateSemanticTree(input as Record<string, unknown>);
  if (treeError) return { success: false, message: treeError };

  const catalogResult = semanticCatalog.validate(input);
  if (!catalogResult.success) {
    const details = catalogResult.error ? describeCatalogIssues(catalogResult.error.issues) : [];
    const detailText = details.length ? ` Campos incompatíveis: ${details.join('; ')}.` : '';
    return { success: false, message: `A especificação não corresponde ao catálogo semântico. Verifique os tipos de intenção e seus dados obrigatórios.${detailText}` };
  }

  const structuralResult = validateSpec(input as Spec, { checkOrphans: true });
  if (!structuralResult.valid) {
    return { success: false, message: 'A especificação está incompleta. Verifique o elemento principal e os componentes filhos.' };
  }

  const elements = (input as Spec).elements as Record<string, ElementWithActions>;
  for (const element of Object.values(elements)) {
    const type = element?.type;
    if (!type || !(type in allowedEvents)) {
      return { success: false, message: 'A especificação semântica contém uma intenção não reconhecida.' };
    }

    const componentSchema = semanticCatalog.data.components[type as keyof typeof semanticCatalog.data.components];
    const props = (element as ElementWithActions & { props?: Record<string, unknown> }).props ?? {};
    const resolvedProps = Object.fromEntries(Object.entries(props).map(([key, value]) => {
      if (value && typeof value === 'object' && !Array.isArray(value) && '$bindState' in value) return [key, ''];
      return [key, value];
    }));
    const propsValidation = componentSchema?.props.safeParse(resolvedProps);
    if (!propsValidation?.success) {
      const details = propsValidation?.error ? describeCatalogIssues(propsValidation.error.issues) : [];
      const detailText = details.length ? ` Campos incompatíveis: ${details.join('; ')}.` : '';
      return { success: false, message: `Uma intenção contém dados ou tipos incompatíveis com o catálogo semântico.${detailText}` };
    }
    if (element.children?.length && !componentSchema.slots.includes('default')) {
      return { success: false, message: 'A intenção contém componentes filhos, mas não declara um slot compatível.' };
    }
    const propertyBindings = element.bindings ?? {};
    const bindingTargets: Record<string, string[]> = {
      ValueEntry: ['value'],
      Information: ['facts', 'relationships'],
      Details: ['details'],
      Comparison: ['current', 'reference', 'periodStart', 'periodEnd'],
      Distribution: ['collection'],
    };
    const allowedBindingKeys = type === 'Distribution' ? ['source', 'propertyIri', 'projections'] : ['source', 'propertyIri'];
    for (const [target, binding] of Object.entries(propertyBindings)) {
      if (!bindingTargets[type]?.includes(target)
        || !binding
        || typeof binding !== 'object'
        || Object.keys(binding).some((key) => !allowedBindingKeys.includes(key))
        || binding.source !== 'entity'
        || typeof binding.propertyIri !== 'string'
        || !binding.propertyIri.trim()) {
        return { success: false, message: 'Um vínculo de dados não é compatível com a intenção ou não informa uma propriedade da entidade.' };
      }
      if (type === 'Distribution') {
        const projections = binding.projections;
        if (!projections || typeof projections !== 'object' || Array.isArray(projections)
          || Object.keys(projections).sort().join(',') !== 'groupBy,measure'
          || Object.values(projections).some((iri) => typeof iri !== 'string' || !iri.trim())) {
          return { success: false, message: 'A intenção Distribution precisa vincular uma projeção numérica e uma projeção de agrupamento.' };
        }
      }
    }
    if (type === 'Details' && !propertyBindings.details && !Array.isArray(props.details)) {
      return { success: false, message: 'A intenção Details precisa declarar detalhes ou vinculá-los a uma propriedade da entidade.' };
    }
    if (type === 'Comparison' && (!propertyBindings.current || !propertyBindings.reference || !propertyBindings.periodStart || !propertyBindings.periodEnd)) {
      return { success: false, message: 'A intenção Comparison precisa vincular a métrica e o período para evitar comparar intervalos incompatíveis.' };
    }
    if (type === 'Distribution' && !propertyBindings.collection) {
      return { success: false, message: 'A intenção Distribution precisa vincular uma coleção da entidade.' };
    }

    for (const [eventName, bindingValue] of Object.entries(element.on ?? {})) {
      if (!allowedEvents[type].includes(eventName)) {
        return { success: false, message: 'A especificação solicita uma interação que não está disponível para essa intenção.' };
      }

      for (const binding of Array.isArray(bindingValue) ? bindingValue : [bindingValue]) {
        if (!binding.action || !(binding.action in semanticCatalog.data.actions)) {
          return { success: false, message: 'A especificação solicita uma ação local não permitida.' };
        }
        const actionSchema = semanticCatalog.data.actions[binding.action as keyof typeof semanticCatalog.data.actions]?.params;
        if (!actionSchema?.safeParse(binding.params ?? {}).success) {
          return { success: false, message: 'A especificação contém parâmetros inválidos para uma ação local.' };
        }
      }
    }
  }

  return { success: true, spec: input as Spec };
}

export function validateLegacyJsonSpec(input: unknown) {
  const catalogResult = componentCatalog.validate(input);
  const structuralResult = input && typeof input === 'object' ? validateSpec(input as Spec) : { valid: false };
  return catalogResult.success && structuralResult.valid;
}
