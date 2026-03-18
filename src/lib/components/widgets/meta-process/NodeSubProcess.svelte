<script>
  import { Handle, Position } from '@xyflow/svelte'
  import StatusBadge from './StatusBadge.svelte'
  import IoHandleLabel from './IoHandleLabel.svelte'
  let { data } = $props()
</script>

<div class="node-wrap">
  <div class="node sub-process">
    <div class="main-row">
      <span class="material-symbols-outlined icon">account_tree</span>
      <span class="label">{data.label}</span>
    </div>
  </div>
  <StatusBadge status={data.status} statusColor={data.statusColor} statusIcon={data.statusIcon} />
</div>
{#if data.inputConcepts?.length > 0}
  {#each data.inputConcepts as concept, i}
    <Handle type="target" position={Position.Left} id="in-{concept.iri}"
      style="top: {(i+1) / (data.inputConcepts.length + 1) * 100}%">
      <IoHandleLabel icon={concept.icon} label={concept.label} align="left" />
    </Handle>
  {/each}
{:else}
  <Handle type="target" position={Position.Left} />
{/if}
{#if data.outputConcepts?.length > 0}
  {#each data.outputConcepts as concept, i}
    <Handle type="source" position={Position.Right} id="out-{concept.iri}"
      style="top: {(i+1) / (data.outputConcepts.length + 1) * 100}%">
      <IoHandleLabel icon={concept.icon} label={concept.label} align="right" />
    </Handle>
  {/each}
{:else}
  <Handle type="source" position={Position.Right} />
{/if}

<style>
  .node-wrap {
    position: relative;
  }
  .node {
    display: flex;
    flex-direction: column;
    gap: 0;
    padding: 8px 14px;
    border-radius: 6px;
    min-width: 130px;
    cursor: pointer;
    outline: 2px solid #00ACC1;
    outline-offset: 3px;
  }
  .sub-process {
    background: #002f36;
    border: 2px solid #00ACC1;
    color: #80deea;
  }
  .main-row {
    display: flex;
    flex-direction: row;
    align-items: center;
    justify-content: center;
    gap: 6px;
  }
  .icon {
    font-size: 16px;
    flex-shrink: 0;
  }
  .label {
    font-size: 12px;
    font-weight: 500;
    line-height: 1.3;
  }
</style>
