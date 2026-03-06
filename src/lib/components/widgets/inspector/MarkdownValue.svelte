<script module>
  import MarkdownWorker from '../../../workers/markdown.worker.js?worker';

  let worker = null;
  let idCounter = 0;
  const callbacks = new Map();

  function ensureWorker() {
    if (!worker) {
      worker = new MarkdownWorker();
      worker.onmessage = ({ data: { id, html } }) => {
        const cb = callbacks.get(id);
        if (cb) {
          cb(html);
          callbacks.delete(id);
        }
      };
    }
    return worker;
  }

  function parseAsync(text) {
    return new Promise(resolve => {
      const id = idCounter++;
      callbacks.set(id, resolve);
      ensureWorker().postMessage({ id, text });
    });
  }
</script>

<script>
  import { marked } from 'marked';

  let { value } = $props();

  // Below this threshold, parse synchronously — it's fast enough
  const SYNC_THRESHOLD = 200;
  // Above this threshold, skip markdown entirely (catastrophic backtracking risk)
  const PRE_THRESHOLD = 50_000;

  let html = $state('');
  let loading = $state(false);

  $effect(() => {
    const text = value ?? '';

    if (text.length > PRE_THRESHOLD) {
      html = '';
      loading = false;
      return;
    }

    if (text.length <= SYNC_THRESHOLD) {
      html = marked.parse(text);
      loading = false;
      return;
    }

    loading = true;
    html = '';
    let cancelled = false;

    parseAsync(text).then(result => {
      if (!cancelled) {
        html = result;
        loading = false;
      }
    });

    return () => { cancelled = true; };
  });
</script>

{#if loading}
  <div class="markdown-loading">
    <span class="material-symbols-outlined spinning">progress_activity</span>
  </div>
{:else}
  <div class="value-markdown markdown-content">
    {@html html}
  </div>
{/if}

<style>
  .markdown-loading {
    display: flex;
    align-items: center;
    color: var(--color-neutral);
    opacity: 0.5;
  }

  .markdown-loading .material-symbols-outlined {
    font-size: 16px;
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }

  .value-markdown {
    flex: 1;
    font-size: 13px;
    color: var(--color-neutral-active);
    line-height: 1.5;
    word-wrap: break-word;
    min-width: 0;
  }
</style>
