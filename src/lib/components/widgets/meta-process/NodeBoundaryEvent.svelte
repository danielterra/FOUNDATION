<script>
  import { Handle, Position } from '@xyflow/svelte'
  import StatusBadge from './StatusBadge.svelte'
  let { data } = $props()

  const ICONS = {
    error:   'error',
    timer:   'schedule',
    signal:  'cell_tower',
    message: 'mail',
  }

  const icon = $derived(ICONS[data.eventType] ?? 'warning')
</script>

<div class="node-wrap">
  <div class="node boundary-event">
    <span class="material-symbols-outlined icon">{icon}</span>
    <span class="label">{data.label}</span>
  </div>
  <StatusBadge status={data.status} />
</div>
<Handle type="target" position={Position.Left} />
<Handle type="source" position={Position.Right} />

<style>
  .node-wrap {
    position: relative;
  }
  .node {
    display: flex;
    flex-direction: row;
    align-items: center;
    justify-content: center;
    gap: 4px;
    padding: 6px 12px;
    border-radius: 50px;
    min-width: 120px;
    text-align: center;
    cursor: pointer;
  }
  .boundary-event {
    background: #1c0f0f;
    border: 2px dashed #EF5350;
    color: #ef9a9a;
  }
  .icon {
    font-size: 15px;
  }
  .label {
    font-size: 11px;
    font-weight: 500;
    line-height: 1.3;
  }
</style>
