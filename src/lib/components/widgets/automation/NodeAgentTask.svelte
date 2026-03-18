<script>
  import { Handle, Position } from '@xyflow/svelte'
  import StatusBadge from './StatusBadge.svelte'
  let { data } = $props()
</script>

<div class="node-wrap">
  <div class="node">
    <span class="material-symbols-outlined icon">assignment_ind</span>
    <div class="text">
      <span class="label">{data.label}</span>
      {#if data.assignedAgent}
        <span class="sub">{data.assignedAgent}</span>
      {/if}
    </div>
  </div>
  <StatusBadge status={data.status} statusColor={data.statusColor} statusIcon={data.statusIcon} />
</div>
<Handle type="target" position={Position.Left} />
<Handle type="source" position={Position.Right} />

<style>
  .node-wrap { position: relative; }
  .node {
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 8px;
    padding: 8px 14px;
    min-width: 150px;
    cursor: pointer;
    background: #1e0d36;
    border: 2px solid #8E24AA;
    color: #ce93d8;
  }
  .text { display: flex; flex-direction: column; gap: 2px; }
  .icon { font-size: 18px; flex-shrink: 0; }
  .label { font-size: 12px; font-weight: 500; line-height: 1.3; }
  .sub { font-size: 10px; opacity: 0.7; }
</style>
