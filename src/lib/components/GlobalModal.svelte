<script lang="ts">
  import { modal } from '$lib/stores/modal';
  import * as Dialog from '$lib/components/ui/dialog';

  function close() {
    modal.set(null);
  }

  function highlightJson(json: string): string {
    return json
      .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
      .replace(
        /("(\\u[a-zA-Z0-9]{4}|\\[^u]|[^\\"])*"(\s*:)?|\b(true|false|null)\b|-?\d+(?:\.\d*)?(?:[eE][+\-]?\d+)?)/g,
        (match) => {
          if (/^"/.test(match)) {
            if (/:$/.test(match)) return `<span class="json-key">${match}</span>`;
            return `<span class="json-string">${match}</span>`;
          }
          if (/true|false/.test(match)) return `<span class="json-boolean">${match}</span>`;
          if (/null/.test(match)) return `<span class="json-null">${match}</span>`;
          return `<span class="json-number">${match}</span>`;
        }
      );
  }
</script>

<Dialog.Root open={$modal !== null} onOpenChange={(o) => { if (!o) close(); }}>
  <Dialog.Content class="global-modal-content sm:max-w-[640px] max-h-[80vh] flex flex-col">
    {#if $modal}
      {@const m = $modal}
      <Dialog.Header>
        <Dialog.Title><span class="modal-title">{m.title}</span></Dialog.Title>
      </Dialog.Header>
      <div class="modal-body">
        {#if m.html}
          <div class="modal-markdown markdown-content">{@html m.html}</div>
        {/if}
        {#if m.sections?.length}
          <div class="modal-sections">
            {#each m.sections as section}
              <div class="modal-section">
                <span class="modal-section-label" class:error={section.isError}>{section.label}</span>
                <pre class="modal-section-json" class:error={section.isError}>{@html highlightJson(section.content)}</pre>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    {/if}
  </Dialog.Content>
</Dialog.Root>

<style>
  :global(.global-modal-content) {
    overflow: hidden;
  }

  .modal-title {
    font-size: 12px;
    font-family: 'SF Mono', 'Monaco', 'Courier New', monospace;
    color: var(--color-neutral);
    font-weight: 600;
  }

  .modal-body {
    overflow-y: auto;
    padding: 14px 0 0;
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .modal-markdown {
    font-size: 12px;
    line-height: 1.6;
    color: var(--color-neutral);
  }

  .modal-sections {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .modal-section {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .modal-section-label {
    font-size: 9px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--color-neutral-disabled);
    padding-left: 2px;
  }

  .modal-section-label.error {
    color: var(--color-error);
  }

  .modal-section-json {
    margin: 0;
    padding: 10px;
    background: color-mix(in srgb, var(--color-white) 4%, transparent);
    font-size: 11px;
    font-family: 'SF Mono', 'Monaco', 'Courier New', monospace;
    overflow-x: auto;
    white-space: pre-wrap;
    word-break: break-all;
    max-height: 400px;
    overflow-y: auto;
    color: var(--color-neutral);
    line-height: 1.5;
  }

  .modal-section-json :global(.json-key)     { color: #9cdcfe; }
  .modal-section-json :global(.json-string)  { color: #ce9178; }
  .modal-section-json :global(.json-number)  { color: #b5cea8; }
  .modal-section-json :global(.json-boolean) { color: #569cd6; }
  .modal-section-json :global(.json-null)    { color: #569cd6; }
</style>
