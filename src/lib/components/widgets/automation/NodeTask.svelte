<script>
  import { Handle, Position } from '@xyflow/svelte'
  import StatusBadge from './StatusBadge.svelte'
  let { data } = $props()
</script>

<div class="node-wrap">
  <div class="node">
    <div class="side-tab">
      <span class="material-symbols-outlined icon">task_alt</span>
    </div>
    <div class="content">
      <span class="label">{data.label}</span>
    </div>
  </div>
  <StatusBadge status={data.status} statusColor={data.statusColor} statusIcon={data.statusIcon} />
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
    background: #1a1f2e;
    color: #94a3b8;
    overflow: hidden;
  }
  .side-tab {
    width: 28px;
    flex-shrink: 0;
    background: #475569;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .icon { font-size: 16px; color: #f1f5f9; font-variation-settings: 'FILL' 1; }
  .content { display: flex; flex-direction: column; justify-content: center; padding: 8px 10px; min-width: 0; flex: 1; }
  .label { font-size: 12px; font-weight: 500; line-height: 1.3; color: #e2e8f0; }
</style>
