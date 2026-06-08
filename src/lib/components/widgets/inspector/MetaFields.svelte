<script>
  import MarkdownValue from './MarkdownValue.svelte';
  import { Input } from '$lib/components/ui/input';
  import { Textarea } from '$lib/components/ui/textarea';

  let { label, comment, onSave, openEntityInspector = null } = $props();

  let editingMeta = $state(null);
  let metaDraft = $state('');

  function startMetaEdit(field) {
    if (!onSave) return;
    editingMeta = field;
    metaDraft = field === 'label' ? (label ?? '') : (comment ?? '');
  }

  function cancelMetaEdit() {
    editingMeta = null;
    metaDraft = '';
  }

  async function saveMetaEdit() {
    if (!editingMeta) return;
    const iri = editingMeta === 'label' ? 'rdfs:label' : 'rdfs:comment';
    await onSave(iri, metaDraft);
    editingMeta = null;
    metaDraft = '';
  }

  function onKeydown(e) {
    if (e.key === 'Escape') { e.preventDefault(); cancelMetaEdit(); }
    if (e.key === 'Enter' && (editingMeta === 'label' || e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      saveMetaEdit();
    }
  }

  let commentTextareaEl = $state(null);
  let labelInputEl = $state(null);

  $effect(() => {
    if (!commentTextareaEl) return;
    metaDraft;
    commentTextareaEl.style.height = 'auto';
    commentTextareaEl.style.height = Math.min(commentTextareaEl.scrollHeight, 400) + 'px';
  });

  $effect(() => {
    if (editingMeta === 'label' && labelInputEl) {
      labelInputEl.focus();
      const len = labelInputEl.value.length;
      labelInputEl.setSelectionRange(len, len);
    }
    if (editingMeta === 'comment' && commentTextareaEl) {
      commentTextareaEl.focus();
      const len = commentTextareaEl.value.length;
      commentTextareaEl.setSelectionRange(len, len);
    }
  });
</script>

<div class="meta-field-row" class:editing={editingMeta === 'label'}>
  {#if editingMeta === 'label'}
    <Input
      class="meta-edit-input"
      bind:value={metaDraft}
      bind:ref={labelInputEl}
      onkeydown={onKeydown}
      onblur={saveMetaEdit}
    />
  {:else if onSave}
    <div
      class="entity-full-name section-title editable"
      role="button"
      tabindex="0"
      onclick={() => startMetaEdit('label')}
      onkeydown={(e) => e.key === 'Enter' && startMetaEdit('label')}
    >{label || '—'}</div>
  {:else}
    <div class="entity-full-name section-title" title="Entity is system-locked">{label || '—'}</div>
  {/if}
</div>

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
      onclick={() => startMetaEdit('comment')}
      onkeydown={(e) => e.key === 'Enter' && startMetaEdit('comment')}
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
    margin-bottom: 12px;
  }

  .entity-full-name {
    line-height: 1.4;
    word-break: break-word;
  }

  .entity-full-name.editable,
  .description.editable {
    cursor: text;
    padding: 4px 6px;
    margin: 0 -6px;
    transition: background 0.15s;
  }

  .entity-full-name.editable:hover,
  .description.editable:hover {
    background: color-mix(in srgb, var(--color-white) 5%, transparent);
  }

  :global([data-slot="input"].meta-edit-input) {
    width: 100%;
    background: color-mix(in srgb, var(--color-white) 5%, transparent);
    border: none;
    font-family: var(--font-title);
    font-size: 16px;
    font-weight: 700;
    color: var(--color-neutral-active);
    outline: none;
    box-shadow: none;
  }

  :global([data-slot="textarea"].meta-edit-textarea) {
    width: 100%;
    min-height: 36px;
    background: color-mix(in srgb, var(--color-white) 5%, transparent);
    border: none;
    font-family: inherit;
    font-size: 14px;
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
    font-size: 14px;
    line-height: 1.6;
    color: var(--color-neutral);
    word-wrap: break-word;
  }

  .meta-empty-hint {
    font-size: 14px;
    color: var(--color-neutral);
    opacity: 0.35;
    font-style: italic;
  }
</style>
