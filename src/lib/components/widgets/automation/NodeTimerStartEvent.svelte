<script>
  import { Handle, Position } from '@xyflow/svelte'
  import StatusBadge from './StatusBadge.svelte'
  import cronstrue from 'cronstrue'
  import 'cronstrue/locales/pt_BR'

  let { data } = $props()

  function describeCron(expr) {
    if (!expr) return null
    try {
      return cronstrue.toString(expr, { locale: 'pt_BR', use24HourTimeFormat: true })
    } catch {
      return expr
    }
  }

  const cronDescription = $derived(describeCron(data.timerCycle))
</script>

<div class="node-wrap">
  <div class="node">
    <div class="side-tab">
      <span class="material-symbols-outlined icon">schedule</span>
    </div>
    <div class="content">
      <span class="label">{data.label}</span>
      {#if cronDescription}
        <span class="cron-desc">{cronDescription}</span>
      {/if}
    </div>
  </div>
  <StatusBadge status={data.status} statusColor={data.statusColor} statusIcon={data.statusIcon} />
</div>
<Handle type="source" position={Position.Right} />

<style>
  .node-wrap { position: relative; overflow: visible; }
  .node {
    display: flex;
    flex-direction: row;
    align-items: stretch;
    width: 220px;
    cursor: pointer;
    background: var(--automation-node-start-bg);
    color: var(--automation-node-start-text);
    overflow: hidden;
  }
  .side-tab {
    width: 28px;
    flex-shrink: 0;
    background: var(--automation-node-start-tab);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .icon { font-size: 16px; color: var(--automation-node-start-bg); font-variation-settings: 'FILL' 1; }
  .content { display: flex; flex-direction: column; justify-content: center; padding: 8px 10px; min-width: 0; flex: 1; }
  .label { font-size: 12px; font-weight: 500; line-height: 1.3; }
  .cron-desc { font-size: 10px; opacity: 0.65; margin-top: 2px; }
</style>
