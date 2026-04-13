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
      worker.onerror = () => {
        for (const [id, cb] of callbacks) {
          cb('');
        }
        callbacks.clear();
        worker = null;
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
  import { invoke, convertFileSrc } from '@tauri-apps/api/core';

  let { value, openEntityInspector = null } = $props();

  // Below this threshold, parse synchronously — it's fast enough
  const SYNC_THRESHOLD = 200;
  // Above this threshold, skip markdown entirely (catastrophic backtracking risk)
  const PRE_THRESHOLD = 50_000;

  const IRI_REGEX = /\b[a-zA-Z][a-zA-Z0-9_]*:(?!\/\/)([a-zA-Z][a-zA-Z0-9_.-]*)\b/g;

  let html = $state('');
  let loading = $state(false);
  let iriResolutions = $state({});

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

  $effect(() => {
    if (!openEntityInspector) return;
    const text = value ?? '';
    const matches = [...new Set([...text.matchAll(IRI_REGEX)].map(m => m[0]))];
    if (matches.length === 0) return;

    invoke('entity__resolve_iris', { iris: matches })
      .then(json => { iriResolutions = JSON.parse(json); })
      .catch(() => {});
  });

  function iconHtml(icon) {
    if (icon.startsWith('http://') || icon.startsWith('https://') || icon.startsWith('data:')) {
      return `<img src="${icon}" alt="" class="iri-pill-icon-img" />`;
    }
    if (icon.startsWith('file://')) {
      const src = convertFileSrc(icon.replace(/^file:\/\//, ''));
      return `<img src="${src}" alt="" class="iri-pill-icon-img" />`;
    }
    return `<span class="material-symbols-outlined iri-pill-icon">${icon}</span>`;
  }

  function injectIriPills(rawHtml, resolutions) {
    if (!rawHtml || Object.keys(resolutions).length === 0) return rawHtml;
    return rawHtml.replace(
      /(<[^>]*>)|(\b[a-zA-Z][a-zA-Z0-9_]*:(?!\/\/)([a-zA-Z][a-zA-Z0-9_.-]*)\b)/g,
      (match, tag) => {
        if (tag) return tag;
        const res = resolutions[match];
        if (!res) return match;
        const icon = res.icon ? iconHtml(res.icon) : '';
        return `<span class="iri-pill" data-iri="${match}">`
          + `${icon}<span class="iri-pill-label">${res.label}</span></span>`;
      }
    );
  }

  function handleClick(e) {
    if (!openEntityInspector) return;
    const pill = e.target.closest('[data-iri]');
    if (pill) {
      e.stopPropagation();
      openEntityInspector(pill.dataset.iri);
    }
  }
</script>

{#if loading}
  <div class="markdown-loading">
    <span class="material-symbols-outlined spinning">progress_activity</span>
  </div>
{:else}
  <div class="value-markdown markdown-content" onclick={handleClick} role="presentation">
    {@html injectIriPills(html, iriResolutions)}
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
    font-size: 14px;
    color: var(--color-neutral-active);
    line-height: 1.5;
    word-wrap: break-word;
    min-width: 0;
  }

  :global(.iri-pill) {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 1px 6px;
    border-radius: 4px;
    background: color-mix(in srgb, var(--color-interactive) 12%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-interactive) 25%, transparent);
    color: var(--color-interactive);
    font-size: 13px;
    line-height: 1.4;
    cursor: pointer;
    vertical-align: middle;
  }

  :global(.iri-pill-icon) {
    font-size: 14px;
    line-height: 1;
  }

  :global(.iri-pill-label) {
    font-size: 13px;
  }

  :global(.iri-pill-icon-img) {
    width: 14px;
    height: 14px;
    object-fit: cover;
    border-radius: 2px;
    flex-shrink: 0;
    vertical-align: middle;
  }
</style>
