<script>
  let { status } = $props()

  const STATUS_COLORS = {
    'Completed':   '#4caf50',
    'Active':      '#1E88E5',
    'In Progress': '#FB8C00',
    'Planned':     '#9e9e9e',
    'Blocked':     '#ef5350',
  }

  const STATUS_ICONS = {
    'Completed':   'check_circle',
    'Active':      'play_circle',
    'In Progress': 'pending',
    'Planned':     'schedule',
    'Blocked':     'block',
  }

  const color = $derived(STATUS_COLORS[status] ?? '#9e9e9e')
  const icon  = $derived(STATUS_ICONS[status]  ?? 'radio_button_unchecked')
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
    border-radius: 10px;
    border: 1px solid;
    letter-spacing: 0.4px;
    white-space: nowrap;
    text-transform: uppercase;
    pointer-events: none;
  }
  .icon {
    font-size: 11px;
  }
</style>
