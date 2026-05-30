<script>
  import { Handle, Position } from '@xyflow/svelte'
  import { onMount } from 'svelte'
  import { invoke } from '@tauri-apps/api/core'
  import StatusBadge from './StatusBadge.svelte'
  import IoHandleLabel from './IoHandleLabel.svelte'

  let { data } = $props()

  let serviceLabel = $state('');
  let modelLabel = $state('');
  let supportsToolCalling = $state(true);

  onMount(async () => {
    if (!data.assignedAgentIri) return;
    try {
      const cfg = await invoke('agent__get_ai_config', { agentIri: data.assignedAgentIri });
      serviceLabel = cfg.serviceOverridden ? cfg.serviceLabel : '';
      modelLabel = cfg.modelOverridden ? cfg.modelLabel : '';
      supportsToolCalling = cfg.supportsToolCalling ?? true;
    } catch {
      // agent config unavailable — no indicator shown
    }
  });
</script>

<div class="node-wrap">
  <div class="node">
    <div class="side-tab">
      <span class="material-symbols-outlined icon">assignment_ind</span>
    </div>
    <div class="content">
      {#if data.assignedAgent}
        <span class="sub">{data.assignedAgent}</span>
      {/if}
      <span class="label">{data.label}</span>
      {#if serviceLabel || modelLabel}
        <span class="model-info">{serviceLabel}{modelLabel ? (serviceLabel ? ' · ' : '') + modelLabel : ''}</span>
      {/if}
      {#if !supportsToolCalling}
        <span class="tool-warn" title="Modelo sem suporte a tool calling">⚠ tools</span>
      {/if}
    </div>
  </div>
  <StatusBadge status={data.status} statusColor={data.statusColor} statusIcon={data.statusIcon} />
  {#if data.inputClassLabel}
    <IoHandleLabel label={data.inputClassLabel} icon={data.inputClassIcon} side="input" />
  {/if}
  {#if data.outputClassLabel}
    <IoHandleLabel label={data.outputClassLabel} icon={data.outputClassIcon} side="output" />
  {/if}
</div>
<Handle type="target" position={Position.Left} />
<Handle type="source" position={Position.Right} />

<style>
  .node-wrap { position: relative; overflow: visible; }
  .node {
    display: flex;
    flex-direction: row;
    align-items: stretch;
    width: 220px;
    cursor: pointer;
    background: #1e0d36;
    color: #ce93d8;
    overflow: hidden;
  }
  .side-tab {
    width: 28px;
    flex-shrink: 0;
    background: #8E24AA;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .icon { font-size: 16px; color: #1e0d36; font-variation-settings: 'FILL' 1; }
  .content { display: flex; flex-direction: column; justify-content: center; gap: 2px; padding: 8px 10px; min-width: 0; flex: 1; }
  .label { font-size: 12px; font-weight: 500; line-height: 1.3; }
  .sub { font-size: 9px; font-family: var(--font-body); opacity: 0.7; }
  .model-info { font-size: 8px; opacity: 0.6; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .tool-warn { font-size: 9px; color: #ffb74d; font-weight: 600; }
</style>
