<script>
  let {
    visible = false,
    x = 0,
    y = 0,
    desc = '',
    sourceLabel = '',
    sourceIcon = '',
  } = $props();

  function bodyPortal(node) {
    document.body.appendChild(node);
    return {
      destroy() {
        node.remove();
      }
    };
  }
</script>

<div
  use:bodyPortal
  class="prop-hint-portal"
  class:prop-hint-visible={visible}
  style:left="{x}px"
  style:top="{y}px"
>
  {#if desc}
    <p class="prop-hint-desc">{desc}</p>
  {/if}
  {#if sourceLabel}
    <div class="prop-hint-source" class:with-sep={desc}>
      <span class="prop-hint-chip">
        {#if sourceIcon}
          <span class="material-symbols-outlined prop-hint-chip-icon">{sourceIcon}</span>
        {/if}
        {sourceLabel}
      </span>
    </div>
  {/if}
</div>

<style>
  :global(.prop-hint-portal) {
    position: fixed;
    z-index: 2147483647;
    transform: translate(-50%, -100%);
    background: color-mix(in srgb, var(--color-black) 88%, var(--color-white) 12%);
    border: 1px solid color-mix(in srgb, var(--color-neutral) 22%, transparent);
    padding: 8px 10px;
    min-width: 160px;
    max-width: 260px;
    pointer-events: none;
    opacity: 0;
    visibility: hidden;
    transition: opacity 0.12s, visibility 0.12s;
    box-shadow: 0 4px 16px color-mix(in srgb, var(--color-black) 60%, transparent);
    text-transform: none;
    letter-spacing: normal;
    font-family: var(--font-body);
  }

  :global(.prop-hint-portal.prop-hint-visible) {
    opacity: 1;
    visibility: visible;
  }

  :global(.prop-hint-desc) {
    font-size: 11px;
    color: var(--color-neutral-active);
    line-height: 1.5;
    margin: 0;
  }

  :global(.prop-hint-source) {
    display: flex;
    align-items: center;
  }

  :global(.prop-hint-source.with-sep) {
    margin-top: 6px;
    padding-top: 6px;
    border-top: 1px solid color-mix(in srgb, var(--color-neutral) 15%, transparent);
  }

  :global(.prop-hint-chip) {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 7px 2px 5px;
    background: color-mix(in srgb, var(--color-neutral) 14%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-neutral) 20%, transparent);
    font-size: 10px;
    font-weight: 600;
    color: var(--color-neutral);
  }

  :global(.prop-hint-chip-icon) {
    font-size: 12px;
    opacity: 0.8;
  }
</style>
