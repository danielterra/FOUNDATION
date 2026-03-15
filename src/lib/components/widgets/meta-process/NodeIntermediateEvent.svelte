<script>
  import { Handle, Position } from '@xyflow/svelte'
  import StatusBadge from './StatusBadge.svelte'
  let { data } = $props()

  const TRIGGER_ICONS = {
    'App Launch':    'rocket_launch',
    'HTTP Request':  'http',
    'CRON Schedule': 'schedule',
    'System Signal': 'cell_tower',
    'User Action':   'touch_app',
  }
  const icon = $derived(TRIGGER_ICONS[data.triggerType] ?? 'notifications')
</script>

<div class="node-wrap">
  <div class="node intermediate-event">
    <span class="material-symbols-outlined icon">{icon}</span>
    <span class="label">{data.label}</span>
  </div>
  <StatusBadge status={data.status} statusColor={data.statusColor} statusIcon={data.statusIcon} />
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
    gap: 2px;
    padding: 8px 14px;
    border-radius: 24px;
    min-width: 120px;
    text-align: center;
    cursor: pointer;
  }
  .intermediate-event {
    background: #2d2600;
    border: 2px dashed #FDD835;
    color: #fff9c4;
  }
  .icon {
    font-size: 16px;
  }
  .label {
    font-size: 12px;
    font-weight: 500;
    line-height: 1.3;
  }
</style>
