<script>
  let { propertyIri, draftValue = $bindable(), saving, onsave, oncancel } = $props();
</script>

<div class="edit-container">
  <textarea
    class="edit-textarea"
    bind:value={draftValue}
    onblur={() => onsave(propertyIri)}
    onkeydown={(e) => {
      if (e.key === 'Escape') oncancel();
      else if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) onsave(propertyIri);
    }}
    autofocus
    rows="5"
  ></textarea>
  <div class="edit-actions">
    <button
      class="edit-save-btn"
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
    </button>
    <button
      class="edit-cancel-btn"
      onmousedown={(e) => e.preventDefault()}
      onclick={oncancel}
    >
      <span class="material-symbols-outlined">close</span>
      Cancel
    </button>
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

  .edit-textarea {
    width: 100%;
    min-height: 80px;
    background: color-mix(in srgb, var(--color-black) 50%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-interactive) 50%, transparent);
    border-radius: 6px;
    color: var(--color-neutral-active);
    font-family: var(--font-body);
    font-size: 13px;
    line-height: 1.5;
    padding: 8px;
    resize: vertical;
    box-sizing: border-box;
    outline: none;
    transition: border-color 0.15s;
  }

  .edit-textarea:focus {
    border-color: var(--color-interactive);
  }

  .edit-actions {
    display: flex;
    gap: 6px;
  }

  .edit-save-btn,
  .edit-cancel-btn {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 10px;
    border: none;
    border-radius: 4px;
    cursor: pointer;
    font-family: var(--font-body);
    font-size: 12px;
    font-weight: 600;
    transition: background 0.15s;
  }

  .edit-save-btn {
    background: color-mix(in srgb, var(--color-interactive) 25%, transparent);
    color: var(--color-interactive);
  }

  .edit-save-btn:hover:not(:disabled) {
    background: color-mix(in srgb, var(--color-interactive) 40%, transparent);
  }

  .edit-save-btn:disabled {
    opacity: 0.6;
    cursor: default;
  }

  .edit-cancel-btn {
    background: color-mix(in srgb, var(--color-neutral) 12%, transparent);
    color: var(--color-neutral);
  }

  .edit-cancel-btn:hover {
    background: color-mix(in srgb, var(--color-neutral) 22%, transparent);
  }

  .edit-save-btn .material-symbols-outlined,
  .edit-cancel-btn .material-symbols-outlined {
    font-size: 14px;
  }

  .spinning-small {
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }
</style>
