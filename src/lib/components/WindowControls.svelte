<script lang="ts">
  import { onMount, onDestroy } from 'svelte';

  type WindowHandle = {
    minimize: () => Promise<void>;
    toggleMaximize: () => Promise<void>;
    close: () => Promise<void>;
    isMaximized: () => Promise<boolean>;
    onResized: (cb: () => void | Promise<void>) => Promise<() => void>;
  };

  let win = $state<WindowHandle | null>(null);
  let maximized = $state(false);
  let unlisten: (() => void) | undefined;

  onMount(async () => {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      const handle = getCurrentWindow() as unknown as WindowHandle;
      win = handle;
      maximized = await handle.isMaximized();
      unlisten = await handle.onResized(async () => {
        maximized = await handle.isMaximized();
      });
    } catch (e) {
      console.error('[WindowControls] init failed:', e);
    }
  });

  onDestroy(() => { unlisten?.(); });
</script>

{#if win}
  <div class="window-controls">
    <button class="ctrl" onclick={() => win!.minimize()} aria-label="Minimizar" title="Minimizar">
      <span class="material-symbols-outlined">horizontal_rule</span>
    </button>
    <button class="ctrl" onclick={() => win!.toggleMaximize()} aria-label={maximized ? 'Restaurar' : 'Maximizar'} title={maximized ? 'Restaurar' : 'Maximizar'}>
      <span class="material-symbols-outlined">{maximized ? 'filter_none' : 'check_box_outline_blank'}</span>
    </button>
    <button class="ctrl close" onclick={() => win!.close()} aria-label="Fechar" title="Fechar">
      <span class="material-symbols-outlined">close</span>
    </button>
  </div>
{/if}

<style>
  .window-controls {
    display: flex;
    align-items: stretch;
    height: 100%;
    flex-shrink: 0;
  }

  .ctrl {
    width: 46px;
    height: 100%;
    background: none;
    border: none;
    padding: 0;
    margin: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--accent-foreground);
    cursor: pointer;
    transition: background 0.1s;
  }

  .ctrl .material-symbols-outlined {
    font-size: 16px;
  }

  .ctrl:hover {
    background: color-mix(in srgb, var(--accent-foreground) 10%, transparent);
  }

  .ctrl.close:hover {
    background: var(--destructive);
    color: var(--destructive-foreground);
  }
</style>
