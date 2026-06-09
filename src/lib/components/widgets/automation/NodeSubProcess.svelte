<script lang="ts">
  import { Handle, Position } from '@xyflow/svelte'
  import StatusBadge from './StatusBadge.svelte'
  import IoHandleLabel from './IoHandleLabel.svelte'

  interface BatchProgress {
    done: number
    failed: number
    total: number | null
  }

  interface NodeData {
    label: string
    invokesProcess?: string | null
    status?: string | null
    statusColor?: string | null
    statusIcon?: string | null
    inputClassLabel?: string | null
    inputClassIcon?: string | null
    outputClassLabel?: string | null
    outputClassIcon?: string | null
    outputClasses?: { label: string; icon?: string | null }[]
    isMultiInstance?: boolean
    isSequential?: boolean
    batchProgress?: BatchProgress
  }

  let { data }: { data: NodeData } = $props()

  const multiOut = $derived((data.outputClasses?.length ?? 0) > 1)

  const batchMarkerLabel = $derived(
    data.isMultiInstance
      ? (data.isSequential ? 'Lote sequencial' : 'Lote paralelo')
      : null
  )

  function computeBatchBadge(p: BatchProgress | undefined): { text: string; color: string; tooltip: string } | null {
    if (!p) return null
    const { done, failed, total } = p
    if (done === 0 && failed === 0 && total === null) return null

    const denominator = total !== null ? `/${total}` : ''
    const text = `${done}${denominator}`

    let color: string
    if (total !== null && done + failed >= total) {
      color = failed > 0 ? 'var(--color-danger)' : 'var(--color-success)'
    } else {
      color = 'var(--color-warning)'
    }

    let tooltip = total !== null
      ? `${done} de ${total} iterações concluídas`
      : `${done} iterações concluídas`
    if (failed > 0) tooltip += `, ${failed} com falha`

    return { text, color, tooltip }
  }

  const badge = $derived(computeBatchBadge(data.batchProgress))

  function handleTop(index: number, total: number): number {
    return Math.round((index + 1) / (total + 1) * 100)
  }
</script>

<div class="node-wrap">
  <div class="node">
    <div class="side-tab">
      <span class="material-symbols-outlined icon">folder_open</span>
    </div>
    <div class="content">
      {#if data.invokesProcess}
        <span class="sub">Sub-process</span>
      {/if}
      <span class="label">{data.label}</span>

      {#if data.isMultiInstance}
        <div class="batch-row">
          <span
            class="batch-marker"
            title={batchMarkerLabel ?? undefined}
            aria-label={batchMarkerLabel ?? undefined}
          >
            {#if data.isSequential}
              <span class="batch-marker-sequential" aria-hidden="true">≡</span>
            {:else}
              <span class="batch-marker-parallel" aria-hidden="true">⦀</span>
            {/if}
            <span>{batchMarkerLabel}</span>
          </span>

          {#if badge}
            <span
              class="batch-progress-badge"
              style="background:color-mix(in srgb, {badge.color} 13%, transparent);color:{badge.color};border-color:color-mix(in srgb, {badge.color} 35%, transparent)"
              title={badge.tooltip}
              aria-label={badge.tooltip}
              aria-live="polite"
            >
              {#if (data.batchProgress?.failed ?? 0) > 0}
                <span class="material-symbols-outlined badge-icon" aria-hidden="true">error</span>
              {:else if data.batchProgress?.total != null && (data.batchProgress.done + data.batchProgress.failed) >= data.batchProgress.total}
                <span class="material-symbols-outlined badge-icon" aria-hidden="true">check_circle</span>
              {/if}
              {badge.text}
            </span>
          {/if}
        </div>
      {/if}
    </div>
  </div>
  <StatusBadge status={data.status} statusColor={data.statusColor} statusIcon={data.statusIcon} />
  {#if data.inputClassLabel}
    <IoHandleLabel label={data.inputClassLabel} icon={data.inputClassIcon} side="input" />
  {/if}
  {#if multiOut}
    {#each data.outputClasses ?? [] as cls, i}
      {@const top = handleTop(i, (data.outputClasses ?? []).length)}
      <IoHandleLabel label={cls.label} icon={cls.icon ?? null} side="output" topPercent={top} />
      <Handle type="source" position={Position.Right} id="output-{i}" style="top: {top}%" />
    {/each}
  {:else}
    {#if data.outputClassLabel}
      <IoHandleLabel label={data.outputClassLabel} icon={data.outputClassIcon} side="output" />
    {/if}
    <Handle type="source" position={Position.Right} />
  {/if}
</div>
<Handle type="target" position={Position.Left} />

<style>
  .node-wrap { position: relative; overflow: visible; }
  .node {
    display: flex;
    flex-direction: row;
    align-items: stretch;
    width: 220px;
    cursor: pointer;
    background: var(--automation-node-subprocess-bg);
    color: var(--automation-node-subprocess-text);
    overflow: hidden;
  }
  .side-tab {
    width: 28px;
    flex-shrink: 0;
    background: var(--automation-node-subprocess-tab);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .icon { font-size: 16px; color: var(--automation-node-subprocess-bg); font-variation-settings: 'FILL' 1; }
  .content { display: flex; flex-direction: column; justify-content: center; gap: 2px; padding: 8px 10px; min-width: 0; flex: 1; }
  .label { font-size: 12px; font-weight: 500; line-height: 1.3; }
  .sub { font-size: 9px; font-family: var(--font-body); opacity: 0.65; }

  .batch-row {
    display: flex;
    flex-direction: row;
    align-items: center;
    justify-content: space-between;
    margin-top: 4px;
  }

  .batch-marker {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-size: 10px;
    font-weight: 600;
    padding: 1px 6px;
    border-radius: 999px;
    border: 1px solid color-mix(in srgb, var(--automation-node-subprocess-text) 25%, transparent);
    background: color-mix(in srgb, var(--automation-node-subprocess-text) 10%, transparent);
    color: var(--automation-node-subprocess-text);
    opacity: 0.8;
    white-space: nowrap;
    pointer-events: none;
  }

  .batch-marker-parallel,
  .batch-marker-sequential {
    font-style: normal;
    font-size: 11px;
    line-height: 1;
  }

  .batch-progress-badge {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-size: 10px;
    font-weight: 700;
    padding: 1px 6px;
    border-radius: 999px;
    border: 1px solid;
    letter-spacing: 0.3px;
    white-space: nowrap;
    pointer-events: none;
  }

  .badge-icon {
    font-size: 11px;
    font-variation-settings: 'FILL' 1;
  }
</style>
