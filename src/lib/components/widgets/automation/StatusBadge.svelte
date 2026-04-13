<script>
  let { status, statusColor, statusIcon } = $props()

  const color = $derived(statusColor ?? '#9e9e9e')
  const icon  = $derived(statusIcon  ?? 'radio_button_unchecked')
</script>

{#if status}
  <div class="status-badge" style="background:{color}22;color:{color};border-color:{color}55">
    <span class="material-symbols-outlined icon">{icon}</span>
    <span class="label">{status}</span>
  </div>
{/if}

<style>
  .status-badge {
    position: absolute;
    top: calc(100% + 6px);
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 3px;
    font-size: 9px;
    font-weight: 700;
    padding: 2px 7px 2px 4px;
    border: none;
    letter-spacing: 0.4px;
    white-space: nowrap;
    text-transform: uppercase;
    pointer-events: none;
  }
  .icon {
    font-size: 11px;
  }
</style>
