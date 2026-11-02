<script lang="ts">
  import WindowControls from './WindowControls.svelte';
  import { pageHeader } from '$lib/stores/pageHeader.svelte';
</script>

<header class="app-header" data-tauri-drag-region>
  <span class="logo" data-tauri-drag-region>FOUNDATION</span>
  <div class="page-actions" data-tauri-drag-region>
    {#if pageHeader.actions}
      {@render pageHeader.actions()}
    {/if}
  </div>
  <WindowControls />
</header>

<style>
  .app-header {
    height: var(--app-header-height, 36px);
    display: flex;
    align-items: stretch;
    background: var(--color-surface-0);
    border-bottom: 1px solid var(--color-surface-3);
    flex-shrink: 0;
    z-index: 200;
    user-select: none;
  }

  .logo {
    display: flex;
    align-items: center;
    padding: 0 var(--spacing-widget-header-inline);
    font-size: 0.8rem;
    color: var(--color-interactive);
    letter-spacing: 0.08em;
  }

  .page-actions {
    flex: 1;
    display: flex;
    align-items: center;
    gap: var(--spacing-inline-related);
    min-width: 0;
    padding-right: 0.5rem;
  }

  /* Page-action icons run smaller than their default so they breathe inside the
     header bar instead of filling its full height. */
  .page-actions :global([data-slot="button"]) {
    height: 30px;
  }

  .page-actions :global(.material-symbols-outlined) {
    font-size: 19px;
  }
</style>
