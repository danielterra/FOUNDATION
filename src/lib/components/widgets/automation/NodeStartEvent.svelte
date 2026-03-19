<script>
  import { Handle, Position } from '@xyflow/svelte'
  import StatusBadge from './StatusBadge.svelte'
  import IoHandleLabel from './IoHandleLabel.svelte'
  let { data } = $props()
</script>

<div class="node-wrap">
  <div class="node">
    <span class="material-symbols-outlined icon">play_circle</span>
    <span class="label">{data.label}</span>
  </div>
  <StatusBadge status={data.status} statusColor={data.statusColor} statusIcon={data.statusIcon} />
  {#if data.inputConceptLabel}
    <IoHandleLabel label={data.inputConceptLabel} icon={data.inputConceptIcon} side="input" />
  {/if}
  {#if data.outputConceptLabel}
    <IoHandleLabel label={data.outputConceptLabel} icon={data.outputConceptIcon} side="output" />
  {/if}
</div>
<Handle type="target" position={Position.Left} />
<Handle type="source" position={Position.Right} />

<style>
  .node-wrap { position: relative; overflow: visible; }
  .node {
    display: flex;
    flex-direction: row;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 8px 14px;
    border-radius: 24px;
    min-width: 120px;
    cursor: pointer;
    background: #0d2b0d;
    border: 2px solid #43A047;
    color: #a5d6a7;
  }
  .icon { font-size: 16px; }
  .label { font-size: 12px; font-weight: 500; line-height: 1.3; }
</style>
