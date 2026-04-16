<script>
  import { Handle, Position } from '@xyflow/svelte'
  import StatusBadge from './StatusBadge.svelte'
  import IoHandleLabel from './IoHandleLabel.svelte'
  let { data } = $props()

  const PAYLOAD_PREVIEW_LENGTH = 40

  const preview = $derived(
    data.messagePayload
      ? (data.messagePayload.length > PAYLOAD_PREVIEW_LENGTH ? data.messagePayload.slice(0, PAYLOAD_PREVIEW_LENGTH) + '…' : data.messagePayload)
      : null
  )
</script>

<div class="node-wrap">
  <div class="node">
    <div class="side-tab">
      <span class="material-symbols-outlined icon">smart_toy</span>
    </div>
    <div class="content">
      {#if preview}
        <span class="sub">{preview}</span>
      {/if}
      <span class="label">{data.label}</span>
    </div>
  </div>
  <StatusBadge status={data.status} statusColor={data.statusColor} statusIcon={data.statusIcon} />
  {#if data.inputClassLabel}
    <IoHandleLabel label={data.inputClassLabel} icon={data.inputClassIcon} side="input" />
  {/if}
  {#if data.outputClassLabel}
    <IoHandleLabel label={data.outputClassLabel} icon={data.outputClassIcon} side="output" />
  {/if}
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
    background: #130820;
    color: #FFB74D;
    overflow: hidden;
  }
  .side-tab {
    width: 28px;
    flex-shrink: 0;
    background: #FF6F00;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .icon { font-size: 16px; color: #130820; font-variation-settings: 'FILL' 1; }
  .content { display: flex; flex-direction: column; justify-content: center; gap: 2px; padding: 8px 10px; min-width: 0; flex: 1; }
  .label { font-size: 12px; font-weight: 500; line-height: 1.3; }
  .sub { font-size: 9px; font-family: var(--font-body); opacity: 0.7; }
</style>
