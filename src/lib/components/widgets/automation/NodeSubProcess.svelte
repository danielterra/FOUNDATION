<script>
  import { Handle, Position } from '@xyflow/svelte'
  import StatusBadge from './StatusBadge.svelte'
  let { data } = $props()
</script>

<div class="node-wrap">
  <div class="node">
    <span class="material-symbols-outlined icon">folder_open</span>
    <div class="text">
      <span class="label">{data.label}</span>
      {#if data.invokesProcess}
        <span class="sub">Sub-process</span>
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
    padding: 10px 14px;
    min-width: 160px;
    cursor: pointer;
    background: #002030;
    border: 2px solid #00ACC1;
    color: #80deea;
    outline: 3px solid #00ACC133;
    outline-offset: 2px;
  }
  .text { display: flex; flex-direction: column; gap: 2px; }
  .icon { font-size: 20px; flex-shrink: 0; }
  .label { font-size: 12px; font-weight: 500; line-height: 1.3; }
  .sub { font-size: 10px; opacity: 0.65; }
</style>
