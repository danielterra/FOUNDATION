<script>
  import MarkdownValue from './MarkdownValue.svelte';
  import { Textarea } from '$lib/components/ui/textarea';

  let { comment, onSave, openEntityInspector = null } = $props();

  let editingMeta = $state(null);
  let metaDraft = $state('');

  function startMetaEdit() {
    if (!onSave) return;
    editingMeta = 'comment';
    metaDraft = comment ?? '';
  }

  function handleDescriptionClick() {
    startMetaEdit();
  }

  function handleDescriptionKeydown(event) {
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      startMetaEdit();
    }
  }

  function cancelMetaEdit() {
    editingMeta = null;
    metaDraft = '';
  }

  async function saveMetaEdit() {
    if (!editingMeta) return;
    await onSave('rdfs:comment', metaDraft);
    editingMeta = null;
    metaDraft = '';
  }

  function onKeydown(e) {
    if (e.key === 'Escape') { e.preventDefault(); cancelMetaEdit(); }
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      saveMetaEdit();
    }
  }

  let commentTextareaEl = $state(null);

  $effect(() => {
    if (!commentTextareaEl) return;
    metaDraft;
    commentTextareaEl.style.height = 'auto';
    commentTextareaEl.style.height = Math.min(commentTextareaEl.scrollHeight, 400) + 'px';
  });

  $effect(() => {
    if (editingMeta === 'comment' && commentTextareaEl) {
      commentTextareaEl.focus();
      const len = commentTextareaEl.value.length;
      commentTextareaEl.setSelectionRange(len, len);
    }
  });
</script>

<div class="meta-field-row" class:editing={editingMeta === 'comment'}>
  {#if editingMeta === 'comment'}
    <Textarea
      class="meta-edit-textarea"
      bind:value={metaDraft}
      bind:ref={commentTextareaEl}
      onkeydown={onKeydown}
      onblur={saveMetaEdit}
    />
  {:else if onSave}
    <div
      class="description editable"
      class:empty={!comment}
      role="button"
      tabindex="0"
      onclick={handleDescriptionClick}
      onkeydown={handleDescriptionKeydown}
    >
      {#if comment}
        <MarkdownValue value={comment} {openEntityInspector} />
      {:else}
        <span class="meta-empty-hint">Add description…</span>
      {/if}
    </div>
  {:else}
    <div class="description" class:empty={!comment} title="Entity is system-locked">
      {#if comment}
        <MarkdownValue value={comment} {openEntityInspector} />
      {:else}
        <span class="meta-empty-hint">Add description…</span>
      {/if}
    </div>
  {/if}
</div>

<style>
  .meta-field-row {
    margin-bottom: var(--spacing-field-group);
  }

  .description.editable {
    cursor: text;
    padding: 4px 6px;
    margin: 0 -6px;
    transition: background 0.15s;
  }

  .description.editable:hover {
    background: var(--color-surface-hover-subtle);
  }

  :global([data-slot="textarea"].meta-edit-textarea) {
    width: 100%;
    min-height: 36px;
    background: color-mix(in srgb, var(--color-white) 5%, transparent);
    border: none;
    font-family: inherit;
    font-size: var(--type-body);
    color: var(--color-neutral-active);
    line-height: 1.6;
    resize: none;
    overflow-y: auto;
    outline: none;
    box-shadow: none;
    field-sizing: fixed;
  }

  .description {
    margin: 0;
    font-size: var(--type-body);
    line-height: 1.6;
    color: var(--color-neutral);
    word-wrap: break-word;
  }

  .meta-empty-hint {
    font-size: var(--type-body);
    color: var(--color-neutral);
    opacity: 0.35;
  }
</style>
