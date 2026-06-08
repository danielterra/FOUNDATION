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
    background: var(--card);
    flex-shrink: 0;
    z-index: 200;
    user-select: none;
  }

  .logo {
    display: flex;
    align-items: center;
    padding: 0 1rem;
    font-size: 0.8rem;
    font-weight: 600;
    color: var(--accent-foreground);
    letter-spacing: 0.08em;
  }

  .page-actions {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 0.5rem;
    min-width: 0;
    padding-right: 0.5rem;
  }
</style>
