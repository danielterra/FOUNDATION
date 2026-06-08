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
    background: var(--automation-node-task-bg);
    color: var(--automation-node-task-text);
    overflow: hidden;
  }
  .side-tab {
    width: 28px;
    flex-shrink: 0;
    background: var(--automation-node-task-tab);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .icon { font-size: 16px; color: var(--automation-node-task-icon); font-variation-settings: 'FILL' 1; }
  .content { display: flex; flex-direction: column; justify-content: center; padding: 8px 10px; min-width: 0; flex: 1; }
  .label { font-size: 12px; font-weight: 500; line-height: 1.3; color: var(--automation-node-task-text); }
</style>
