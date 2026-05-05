<script>
  import NumberFlow from '@number-flow/svelte';

  let { detailGroup } = $props();

  const CURRENCY_CODES = new Set(['BRL', 'USD', 'EUR', 'GBP', 'JPY', 'CNY', 'ARS', 'CLP', 'MXN', 'PEN', 'COP', 'UYU']);

  // Tauri injeta nonce em style-src; precisa repassar ao NumberFlow para o
  // elemento style do Shadow DOM não ser bloqueado pela CSP
  const cspNonce = typeof document !== 'undefined'
    ? document.querySelector('style[nonce]')?.nonce
    : undefined;

  function isCurrencyCode(label) {
    return !!label && CURRENCY_CODES.has(label);
  }

  const val = $derived(detailGroup.values[0]);
  const numericValue = $derived(
    val && !isNaN(Number(val.value)) && val.value !== '' ? Number(val.value) : null
  );
  const isCurrency = $derived(isCurrencyCode(val?.unitLabel));
</script>

<div class="big-number-card">
  <div class="card-label">{detailGroup.propertyLabel}</div>
  <div class="card-value-row">
    {#if numericValue !== null && !val?.formulaError}
      {#if isCurrency}
        <NumberFlow
          class="big-number"
          value={numericValue}
          locales="pt-BR"
          format={{ style: 'currency', currency: val.unitLabel }}
          nonce={cspNonce}
        />
      {:else}
        <NumberFlow class="big-number" value={numericValue} nonce={cspNonce} />
        {#if val?.unitLabel}
          <span class="card-unit">{val.unitLabel}</span>
        {/if}
      {/if}
    {:else if val?.formulaError}
      <span class="card-error-value">
        <span class="material-symbols-outlined card-error-icon">warning</span>
      </span>
    {:else}
      <span class="card-empty">—</span>
    {/if}
  </div>
  {#if val?.formulaError}
    <div class="card-error" title={val.formulaError}>
      {val.formulaError}
    </div>
  {/if}
</div>

<style>
  .big-number-card {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 12px 14px 10px;
    background: color-mix(in srgb, var(--color-white) 5%, transparent);
    min-width: 0;
  }

  .card-label {
    font-family: var(--font-title);
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: color-mix(in srgb, var(--color-accent) 75%, var(--color-neutral));
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .card-value-row {
    display: flex;
    align-items: baseline;
    gap: 6px;
    order: -1;
  }

  .big-number-card :global(.big-number) {
    font-family: var(--font-title);
    font-size: 36px;
    font-weight: 700;
    color: var(--color-neutral-active);
    line-height: 1;
  }

  .card-unit {
    font-size: 11px;
    font-weight: 600;
    color: var(--color-neutral);
    margin-bottom: 1px;
  }

  .card-empty {
    font-family: var(--font-title);
    font-size: 26px;
    font-weight: 700;
    color: var(--color-neutral);
    opacity: 0.3;
    line-height: 1;
  }

  .card-error-value {
    display: flex;
    align-items: center;
  }

  .card-error-icon {
    font-size: 22px;
    color: var(--color-error, #ef4444);
  }

  .card-error {
    font-family: var(--font-body);
    font-size: 10px;
    color: var(--color-error, #ef4444);
    line-height: 1.4;
    overflow: hidden;
    text-overflow: ellipsis;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
  }
</style>
