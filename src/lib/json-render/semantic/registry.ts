import type { Component } from 'svelte';
import type { BaseComponentProps } from '@json-render/svelte';
import { defineRegistry } from '@json-render/svelte';
import SemanticComponentAdapter from './SemanticComponentAdapter.svelte';
import { semanticCatalog } from './catalog';

type AdapterProps = BaseComponentProps<Record<string, unknown>> & { componentType: string };
const componentIds = Object.keys(semanticCatalog.data.components);
const components = Object.fromEntries(componentIds.map((componentType) => [
  componentType,
  (anchor, props) => SemanticComponentAdapter(anchor, { ...props, componentType }),
])) as Record<string, Component<AdapterProps>>;

export const { registry, handlers } = defineRegistry(semanticCatalog, { components });
