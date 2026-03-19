<script>
  import { Handle, Position } from '@xyflow/svelte'
  import StatusBadge from './StatusBadge.svelte'
  import IoHandleLabel from './IoHandleLabel.svelte'
  let { data } = $props()

  const preview = $derived(
    data.messagePayload
      ? (data.messagePayload.length > 40 ? data.messagePayload.slice(0, 40) + '…' : data.messagePayload)
      : null
  )
</script>

<div class="node-wrap">
  <div class="node">
    <span class="material-symbols-outlined icon">smart_toy</span>
    <div class="text">
      <span class="label">{data.label}</span>
      {#if preview}
        <span class="sub">{preview}</span>
      {/if}
    </div>
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
    gap: 8px;
    padding: 8px 14px;
    min-width: 150px;
    cursor: pointer;
    background: #130820;
    border: 2px solid #FF6F00;
    color: #FFB74D;
  }
  .text { display: flex; flex-direction: column; gap: 2px; }
  .icon { font-size: 18px; flex-shrink: 0; }
  .label { font-size: 12px; font-weight: 500; line-height: 1.3; }
  .sub { font-size: 10px; opacity: 0.7; }
</style>
