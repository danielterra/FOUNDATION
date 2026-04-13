<script>
  let { label, icon = null, side, topPercent = 50 } = $props()
</script>

<div class="io-label" class:input={side === 'input'} class:output={side === 'output'} style="top: {topPercent}%">
  {#if icon}
    <span class="material-symbols-outlined icon">{icon}</span>
  {/if}
  <span class="text">{label}</span>
</div>

<style>
  .io-label {
    position: absolute;
    transform: translateY(-50%);
    display: flex;
    align-items: center;
    gap: 3px;
    font-size: 9px;
    font-weight: 700;
    padding: 2px 6px;
    background: color-mix(in srgb, #64B5F6 18%, #000);
    border: 1px solid color-mix(in srgb, #64B5F6 35%, transparent);
    color: #90CAF9;
    white-space: nowrap;
    pointer-events: none;
    z-index: 10;
    letter-spacing: 0.3px;
  }

  .io-label .icon {
    font-size: 10px;
    line-height: 1;
  }

  .input {
    right: calc(100% + 14px);
  }

  .output {
    left: calc(100% + 14px);
  }
</style>
