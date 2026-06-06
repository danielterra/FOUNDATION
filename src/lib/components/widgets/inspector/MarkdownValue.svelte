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
        if (cb) { cb(html); callbacks.delete(id); }
      };
      worker.onerror = () => {
        for (const [id, cb] of callbacks) cb('');
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

  const SYNC_THRESHOLD = 1000;
  const PRE_THRESHOLD = 50_000;

  const IRI_REGEX = /\b[a-zA-Z][a-zA-Z0-9_]*:(?!\/\/)([a-zA-Z][a-zA-Z0-9_.-]*)\b/g;

  let _asyncHtml = $state('');
  let html = $derived.by(() => {
    const text = value ?? '';
    if (text.length > PRE_THRESHOLD) return '';
    if (text.length <= SYNC_THRESHOLD) return marked.parse(text);
    return _asyncHtml;
  });
  let loading = $state(false);
  let iriResolutions = $state({});

  $effect(() => {
    const text = value ?? '';
    if (text.length > PRE_THRESHOLD || text.length <= SYNC_THRESHOLD) { loading = false; return; }
    loading = true;
    _asyncHtml = '';
    let cancelled = false;
    parseAsync(text).then(result => {
      if (!cancelled) { _asyncHtml = result; loading = false; }
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

  // Shadow DOM action — isolates injected CSS from email style tags
  // so the app styles do not leak into rendered email content.
  function shadowContent(node, params) {
    const shadow = node.attachShadow({ mode: 'open' });

    const styleEl = document.createElement('style');
    styleEl.textContent = `
      :host { display: block; }
      * { box-sizing: border-box; }
      div, p, span, li, td, th { font-size: 14px; color: var(--color-neutral-active); line-height: 1.5; word-wrap: break-word; }
      a { color: var(--color-interactive); }
      code, pre { font-family: var(--font-mono, monospace); font-size: 13px; }
      pre { background: color-mix(in srgb, var(--color-white) 5%, transparent); padding: 10px 14px; overflow: auto; }
      blockquote { border-left: 3px solid var(--color-interactive); margin: 0; padding-left: 12px; opacity: 0.8; }
      table { border-collapse: collapse; width: 100%; }
      th, td { border: 1px solid color-mix(in srgb, var(--color-white) 10%, transparent); padding: 6px 10px; }
      .iri-pill { display: inline-flex; align-items: center; gap: 4px; padding: 1px 6px; border-radius: var(--radius); background: color-mix(in srgb, var(--color-interactive) 12%, transparent); color: var(--color-interactive); font-size: 13px; line-height: 1.4; cursor: pointer; vertical-align: middle; }
      .iri-pill-icon { font-family: 'Material Symbols Outlined'; font-size: 14px; line-height: 1; }
      .iri-pill-icon-img { width: 14px; height: 14px; object-fit: contain; vertical-align: middle; }
      .iri-pill-label { font-size: 13px; }
    `;

    const container = document.createElement('div');
    shadow.appendChild(styleEl);
    shadow.appendChild(container);

    let current = params;

    function handleClick(e) {
      if (!current.onIriClick) return;
      const pill = e.target.closest('[data-iri]');
      if (pill) { e.stopPropagation(); current.onIriClick(pill.dataset.iri); }
    }
    shadow.addEventListener('click', handleClick);
    container.innerHTML = params.html ?? '';

    return {
      update(newParams) {
        current = newParams;
        container.innerHTML = newParams.html ?? '';
      },
      destroy() { shadow.removeEventListener('click', handleClick); }
    };
  }
</script>

{#if loading}
  <div class="markdown-loading">
    <span class="material-symbols-outlined spinning">progress_activity</span>
  </div>
{:else}
  <div
    use:shadowContent={{ html: injectIriPills(html, iriResolutions), onIriClick: openEntityInspector }}
    class="value-markdown"
  ></div>
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
</style>
