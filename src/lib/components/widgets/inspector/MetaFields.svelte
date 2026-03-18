<script>
  import { marked } from 'marked';

  let { label, comment, onSave } = $props();

  let editingMeta = $state(null);
  let metaDraft = $state('');

  function startMetaEdit(field) {
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

  function focusOnMount(node) {
    node.focus();
    if (node.tagName === 'INPUT' || node.tagName === 'TEXTAREA') {
      const len = node.value.length;
      node.setSelectionRange(len, len);
    }
  }
</script>

<div class="meta-field-row" class:editing={editingMeta === 'label'}>
  {#if editingMeta === 'label'}
    <input
      class="meta-edit-input"
      bind:value={metaDraft}
      onkeydown={onKeydown}
      onblur={saveMetaEdit}
      use:focusOnMount
    />
  {:else}
    <div
      class="entity-full-name editable"
      role="button"
      tabindex="0"
      onclick={() => startMetaEdit('label')}
      onkeydown={(e) => e.key === 'Enter' && startMetaEdit('label')}
    >{label || '—'}</div>
  {/if}
</div>

<div class="meta-field-row" class:editing={editingMeta === 'comment'}>
  {#if editingMeta === 'comment'}
    <textarea
      class="meta-edit-textarea"
      bind:value={metaDraft}
      onkeydown={onKeydown}
      onblur={saveMetaEdit}
      rows="4"
      use:focusOnMount
    ></textarea>
  {:else}
    <div
      class="description editable"
      class:empty={!comment}
      role="button"
      tabindex="0"
      onclick={() => startMetaEdit('comment')}
      onkeydown={(e) => e.key === 'Enter' && startMetaEdit('comment')}
    >
      {#if comment}
        <div class="markdown-content">{@html marked.parse(comment)}</div>
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
    font-family: var(--font-title);
    font-size: 16px;
    font-weight: 700;
    color: var(--color-neutral-active);
    line-height: 1.4;
    word-break: break-word;
  }

  .entity-full-name.editable,
  .description.editable {
    cursor: text;
    border-radius: 6px;
    padding: 4px 6px;
    margin: 0 -6px;
    transition: background 0.15s;
  }

  .entity-full-name.editable:hover,
  .description.editable:hover {
    background: color-mix(in srgb, var(--color-white) 5%, transparent);
  }

  .meta-edit-input {
    width: 100%;
    box-sizing: border-box;
    background: color-mix(in srgb, var(--color-white) 5%, transparent);
    border: 1px solid var(--color-interactive);
    border-radius: 6px;
    padding: 6px 10px;
    font-family: var(--font-title);
    font-size: 16px;
    font-weight: 700;
    color: var(--color-neutral-active);
    outline: none;
  }

  .meta-edit-textarea {
    width: 100%;
    box-sizing: border-box;
    background: color-mix(in srgb, var(--color-white) 5%, transparent);
    border: 1px solid var(--color-interactive);
    border-radius: 6px;
    padding: 8px 10px;
    font-family: inherit;
    font-size: 14px;
    color: var(--color-neutral-active);
    line-height: 1.6;
    resize: vertical;
    outline: none;
  }

  .description {
    margin: 0;
    font-size: 14px;
    line-height: 1.6;
    color: var(--color-neutral);
    word-wrap: break-word;
  }

  .meta-empty-hint {
    font-size: 13px;
    color: var(--color-neutral);
    opacity: 0.35;
    font-style: italic;
  }
</style>
