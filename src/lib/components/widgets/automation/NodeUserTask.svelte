<script>
  import { Handle, Position } from '@xyflow/svelte'
  import StatusBadge from './StatusBadge.svelte'
  import IoHandleLabel from './IoHandleLabel.svelte'
  let { data } = $props()

  const hasMeta = $derived(data.assignedToRoles?.length > 0 || data.assignedToUsers?.length > 0 || data.usesTools?.length > 0)
  const toolsLabel = $derived(data.usesTools?.join(', ') ?? '')
</script>

<div class="node-wrap">
  <div class="node">
    <div class="side-tab">
      <span class="material-symbols-outlined icon">person</span>
    </div>
    <div class="content">
      {#if hasMeta}
        <div class="meta-row">
          {#each data.assignedToRoles ?? [] as role}
            <span class="meta-item">
              <span class="material-symbols-outlined meta-icon">groups</span>
              <span class="meta-label">{role}</span>
            </span>
          {/each}
          {#each data.assignedToUsers ?? [] as user}
            <span class="meta-item">
              <span class="material-symbols-outlined meta-icon">account_circle</span>
              <span class="meta-label">{user}</span>
            </span>
          {/each}
          {#if toolsLabel}
            <span class="meta-item">
              <span class="material-symbols-outlined meta-icon">build</span>
              <span class="meta-label">{toolsLabel}</span>
            </span>
          {/if}
        </div>
      {/if}
      <span class="label">{data.label}</span>
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
    align-items: stretch;
    width: 220px;
    cursor: pointer;
    border-radius: 8px;
    background: #201500;
    border: 2px solid #FDD835;
    color: #fff59d;
    overflow: hidden;
  }
  .side-tab {
    width: 28px;
    flex-shrink: 0;
    background: #FDD835;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .icon {
    font-size: 16px;
    color: #201500;
    font-variation-settings: 'FILL' 1;
  }
  .content {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 8px 10px;
    min-width: 0;
    flex: 1;
  }
  .meta-row {
    display: flex;
    flex-wrap: wrap;
    gap: 3px 8px;
    align-items: center;
  }
  .meta-item {
    display: flex;
    align-items: center;
    gap: 2px;
    color: #a18a3a;
    min-width: 0;
  }
  .meta-icon { font-size: 11px; flex-shrink: 0; }
  .meta-label {
    font-size: 9px;
    font-family: var(--font-body);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 100px;
  }
  .label { font-size: 12px; font-weight: 500; line-height: 1.3; }
</style>
