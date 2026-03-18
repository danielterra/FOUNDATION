<script>
  import { Handle, Position } from '@xyflow/svelte'
  import StatusBadge from './StatusBadge.svelte'
  let { data } = $props()

  const TRIGGER_ICONS = {
    'App Launch':         'rocket_launch',
    'HTTP Request':       'http',
    'HTTP Response':      'http',
    'HTTP Error Response':'http',
    'CRON Schedule':      'schedule',
    'System Signal':      'cell_tower',
    'User Action':        'touch_app',
  }
  const icon = $derived(TRIGGER_ICONS[data.triggerType] ?? 'stop_circle')
  const isError = $derived(data.triggerType === 'HTTP Error Response')
</script>

<div class="node-wrap">
  <div class="node" class:end-event={!isError} class:end-event-error={isError}>
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
    gap: 6px;
    padding: 8px 14px;
    border-radius: 24px;
    width: 200px;
    box-sizing: border-box;
    cursor: pointer;
  }
  .end-event {
    background: #0a2a0a;
    border: 2px solid #43A047;
    color: #a5d6a7;
  }
  .end-event-error {
    background: #3d0a0a;
    border: 2px solid #E53935;
    color: #ef9a9a;
  }
  .icon {
    font-size: 16px;
  }
  .label {
    font-size: 12px;
    font-weight: 500;
    line-height: 1.3;
    word-break: break-word;
  }
</style>
