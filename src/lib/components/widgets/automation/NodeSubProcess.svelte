<script>
  import { Handle, Position } from '@xyflow/svelte'
  import StatusBadge from './StatusBadge.svelte'
  import IoHandleLabel from './IoHandleLabel.svelte'
  let { data } = $props()

  const multiOut = $derived(data.outputClasses?.length > 1)

  function handleTop(index, total) {
    return Math.round((index + 1) / (total + 1) * 100)
  }
</script>

<div class="node-wrap">
  <div class="node">
    <div class="side-tab">
      <span class="material-symbols-outlined icon">folder_open</span>
    </div>
    <div class="content">
      {#if data.invokesProcess}
        <span class="sub">Sub-process</span>
      {/if}
      <span class="label">{data.label}</span>
    </div>
  </div>
  <StatusBadge status={data.status} statusColor={data.statusColor} statusIcon={data.statusIcon} />
  {#if data.inputClassLabel}
    <IoHandleLabel label={data.inputClassLabel} icon={data.inputClassIcon} side="input" />
  {/if}
  {#if multiOut}
    {#each data.outputClasses as cls, i}
      {@const top = handleTop(i, data.outputClasses.length)}
      <IoHandleLabel label={cls.label} icon={cls.icon ?? null} side="output" topPercent={top} />
      <Handle type="source" position={Position.Right} id="output-{i}" style="top: {top}%" />
    {/each}
  {:else}
    {#if data.outputClassLabel}
      <IoHandleLabel label={data.outputClassLabel} icon={data.outputClassIcon} side="output" />
    {/if}
    <Handle type="source" position={Position.Right} />
  {/if}
</div>
<Handle type="target" position={Position.Left} />

<style>
  .node-wrap { position: relative; overflow: visible; }
  .node {
    display: flex;
    flex-direction: row;
    align-items: stretch;
    width: 220px;
    cursor: pointer;
    background: var(--automation-node-subprocess-bg);
    color: var(--automation-node-subprocess-text);
    overflow: hidden;
  }
  .side-tab {
    width: 28px;
    flex-shrink: 0;
    background: var(--automation-node-subprocess-tab);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .icon { font-size: 16px; color: var(--automation-node-subprocess-bg); font-variation-settings: 'FILL' 1; }
  .content { display: flex; flex-direction: column; justify-content: center; gap: 2px; padding: 8px 10px; min-width: 0; flex: 1; }
  .label { font-size: 12px; font-weight: 500; line-height: 1.3; }
  .sub { font-size: 9px; font-family: var(--font-body); opacity: 0.65; }
</style>
