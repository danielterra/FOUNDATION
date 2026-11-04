import type { Component } from 'svelte';
import type { BaseComponentProps } from '@json-render/svelte';
import { defineRegistry } from '@json-render/svelte';
import ComponentAdapter from './ComponentAdapter.svelte';
import { componentCatalog } from './catalog';

type AdapterProps = BaseComponentProps<Record<string, unknown>>;

const componentIds = Object.keys(componentCatalog.data.components);
const components = Object.fromEntries(componentIds.map((componentId) => [
  componentId,
  (anchor, props) => ComponentAdapter(anchor, { ...props, componentType: componentId }),
])) as Record<string, Component<AdapterProps>>;

export const { registry } = defineRegistry(componentCatalog, {
  components,
});
