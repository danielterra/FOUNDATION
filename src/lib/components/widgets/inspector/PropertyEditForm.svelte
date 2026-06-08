<script>
  import { Textarea } from '$lib/components/ui/textarea';
  import { Button } from '$lib/components/ui/button';
  import { cn } from '$lib/utils';
  let { propertyIri, draftValue = $bindable(), saving, mono = false, onsave, oncancel } = $props();

  let textareaEl = $state(null);

  $effect(() => {
    if (!textareaEl) return;
    draftValue;
    textareaEl.style.height = 'auto';
    textareaEl.style.height = Math.min(textareaEl.scrollHeight, 400) + 'px';
  });

  $effect(() => {
    if (textareaEl) textareaEl.focus();
  });
</script>

<div class="edit-container">
  <Textarea
    class={cn('edit-textarea', mono && 'mono')}
    bind:value={draftValue}
    bind:ref={textareaEl}
    onblur={() => onsave(propertyIri)}
    onkeydown={(e) => {
      if (e.key === 'Escape') oncancel();
      else if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) onsave(propertyIri);
    }}
  />
  <div class="edit-actions">
    <Button
      variant="default"
      size="sm"
      onmousedown={(e) => e.preventDefault()}
      onclick={() => onsave(propertyIri)}
      disabled={saving}
    >
      {#if saving}
        <span class="material-symbols-outlined spinning-small">progress_activity</span>
      {:else}
        <span class="material-symbols-outlined">check</span>
      {/if}
      Save
    </Button>
    <Button
      variant="ghost"
      size="sm"
      onmousedown={(e) => e.preventDefault()}
      onclick={oncancel}
    >
      <span class="material-symbols-outlined">close</span>
      Cancel
    </Button>
  </div>
</div>

<style>
  .edit-container {
    display: flex;
    flex-direction: column;
    gap: 6px;
    flex: 1;
    min-width: 0;
  }

  :global([data-slot="textarea"].edit-textarea) {
    width: 100%;
    min-height: 36px;
    background: color-mix(in srgb, var(--color-black) 50%, transparent);
    border: none;
    color: var(--color-neutral-active);
    font-family: var(--font-body);
    font-size: 14px;
    line-height: 1.5;
    padding: 8px;
    resize: none;
    box-sizing: border-box;
    outline: none;
    box-shadow: none;
    overflow-y: auto;
    field-sizing: fixed;
  }

  :global([data-slot="textarea"].edit-textarea.mono) {
    font-family: var(--font-mono, monospace);
    font-size: 13px;
  }

  .edit-actions {
    display: flex;
    gap: 6px;
  }

  .spinning-small {
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }
</style>
