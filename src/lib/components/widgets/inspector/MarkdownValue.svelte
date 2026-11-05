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
  import DOMPurify from 'dompurify';
  import { invoke, convertFileSrc } from '@tauri-apps/api/core';
  import { openUrl } from '@tauri-apps/plugin-opener';
  import { parseMarkdown, replaceRemoteImageTags } from './markdownPrivacy.js';
  import { inspectorIconSymbol } from './iconSource';

  let { value, openEntityInspector = null } = $props();

  const SYNC_THRESHOLD = 1000;
  const PRE_THRESHOLD = 50_000;

  const IRI_REGEX = /\b[a-zA-Z][a-zA-Z0-9_]*:(?!\/\/)([a-zA-Z][a-zA-Z0-9_.-]*)\b/g;
  const ALLOWED_MARKDOWN_TAGS = [
    'a', 'b', 'blockquote', 'br', 'code', 'del', 'div', 'em', 'h1', 'h2', 'h3', 'h4',
    'h5', 'h6', 'hr', 'i', 'img', 'input', 'li', 'ol', 'p', 'pre', 's', 'span', 'strong',
    'sub', 'sup', 'table', 'tbody', 'td', 'th', 'thead', 'tr', 'ul'
  ];
  const ALLOWED_MARKDOWN_ATTRIBUTES = [
    'alt', 'checked', 'class', 'colspan', 'disabled', 'href', 'rowspan', 'src', 'title', 'type'
  ];
  const ALLOWED_MARKDOWN_URI = /^(?:(?:https?|mailto|tel|file):|data:image\/(?:png|jpe?g|gif|webp|avif);base64,|#|\/(?!\/)|\.\.?\/|(?!\/\/)[^:]+$)/i;
  const MARKDOWN_SANITIZE_CONFIG = {
    ALLOWED_TAGS: ALLOWED_MARKDOWN_TAGS,
    ALLOWED_ATTR: ALLOWED_MARKDOWN_ATTRIBUTES,
    ALLOWED_URI_REGEXP: ALLOWED_MARKDOWN_URI,
    FORBID_TAGS: ['iframe', 'math', 'object', 'script', 'style', 'svg'],
    FORBID_ATTR: ['style'],
    ALLOW_DATA_ATTR: false,
    ALLOW_ARIA_ATTR: false
  };

  let _asyncHtml = $state('');
  let html = $derived.by(() => {
    const text = value ?? '';
    if (text.length > PRE_THRESHOLD) return '';
    if (text.length <= SYNC_THRESHOLD) return parseMarkdown(text);
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

  function safeIconSource(icon) {
    if (typeof icon !== 'string' || icon.length > 1_500_000) return null;
    icon = icon.trim();

    if (/^data:/i.test(icon)) {
      return /^data:image\/(?:png|jpe?g|gif|webp|avif);base64,[a-z0-9+/]+={0,2}$/i.test(icon)
        ? icon
        : null;
    }

    if (/^https?:/i.test(icon) || icon.startsWith('//')) return null;
    if (icon.startsWith('/') || /^[a-z]:[\\/]/i.test(icon)) {
      try {
        return convertFileSrc(icon);
      } catch {
        return null;
      }
    }

    let url;
    try {
      url = new URL(icon);
    } catch {
      return null;
    }

    if (url.protocol === 'file:' && !url.search && !url.hash) {
      try {
        return convertFileSrc(decodeURIComponent(url.pathname));
      } catch {
        return null;
      }
    }
    return null;
  }

  function createPillIcon(icon) {
    if (typeof icon !== 'string' || icon.length === 0) return null;

    const imageSource = safeIconSource(icon);
    if (imageSource) {
      const image = document.createElement('img');
      image.src = imageSource;
      image.alt = '';
      image.className = 'iri-pill-icon-img';
      image.referrerPolicy = 'no-referrer';
      return image;
    }

    const symbol = document.createElement('span');
    symbol.className = 'material-symbols-outlined iri-pill-icon';
    symbol.textContent = inspectorIconSymbol(icon);
    return symbol;
  }

  function createIriPill(iri, resolution) {
    const pill = document.createElement('span');
    pill.className = 'iri-pill';
    pill.dataset.iri = iri;
    pill.setAttribute('role', 'button');
    pill.tabIndex = 0;
    pill.setAttribute('aria-label', `Abrir ${String(resolution.label ?? iri)}`);

    const icon = createPillIcon(resolution.icon);
    if (icon) pill.appendChild(icon);

    const label = document.createElement('span');
    label.className = 'iri-pill-label';
    label.textContent = String(resolution.label ?? iri);
    pill.appendChild(label);
    return pill;
  }

  function replaceIriTextNodes(root, resolutions) {
    if (Object.keys(resolutions).length === 0) return;

    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
      acceptNode(node) {
        if (!node.parentElement || node.parentElement.closest('a, code, pre')) {
          return NodeFilter.FILTER_REJECT;
        }
        IRI_REGEX.lastIndex = 0;
        return IRI_REGEX.test(node.textContent ?? '')
          ? NodeFilter.FILTER_ACCEPT
          : NodeFilter.FILTER_SKIP;
      }
    });
    const textNodes = [];
    while (walker.nextNode()) textNodes.push(walker.currentNode);

    for (const textNode of textNodes) {
      const text = textNode.textContent ?? '';
      const matches = [...text.matchAll(new RegExp(IRI_REGEX.source, 'g'))];
      if (matches.length === 0 || !textNode.parentNode) continue;

      const replacement = document.createDocumentFragment();
      let cursor = 0;
      for (const match of matches) {
        const iri = match[0];
        const resolution = resolutions[iri];
        if (!resolution || typeof resolution !== 'object') continue;
        const index = match.index ?? cursor;
        replacement.append(document.createTextNode(text.slice(cursor, index)));
        replacement.append(createIriPill(iri, resolution));
        cursor = index + iri.length;
      }

      if (cursor === 0) continue;
      replacement.append(document.createTextNode(text.slice(cursor)));
      textNode.parentNode.replaceChild(replacement, textNode);
    }
  }

  function renderMarkdown(container, rawHtml, resolutions) {
    const safeHtml = DOMPurify.sanitize(replaceRemoteImageTags(rawHtml ?? ''), MARKDOWN_SANITIZE_CONFIG);
    const template = document.createElement('template');
    template.innerHTML = safeHtml;
    for (const input of template.content.querySelectorAll('input')) {
      if (input.type !== 'checkbox') {
        input.remove();
      } else {
        input.disabled = true;
      }
    }
    for (const image of template.content.querySelectorAll('img')) {
      const source = image.getAttribute('src') ?? '';
      const alt = image.getAttribute('alt')?.trim() ?? '';
      let imageSource = null;
      try {
        imageSource = safeIconSource(source);
      } catch {
        imageSource = null;
      }
      if (imageSource) {
        image.src = imageSource;
        image.referrerPolicy = 'no-referrer';
      } else {
        image.remove();
      }
    }
    replaceIriTextNodes(template.content, resolutions);
    container.replaceChildren(template.content);
  }

  function safeExternalUrl(href) {
    try {
      const url = new URL(href, window.location.href);
      return ['http:', 'https:', 'mailto:', 'tel:'].includes(url.protocol) && !url.username && !url.password
        ? url.href
        : null;
    } catch {
      return null;
    }
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
      .iri-pill:focus-visible { outline: 2px solid var(--color-interactive); outline-offset: 2px; }
      .external-image-blocked { display: inline-block; padding: 2px 6px; border: 1px solid color-mix(in srgb, var(--color-neutral) 25%, transparent); color: var(--color-neutral); opacity: 0.75; font-size: 12px; }
    `;

    const container = document.createElement('div');
    shadow.appendChild(styleEl);
    shadow.appendChild(container);

    let current = params;

    function handleClick(e) {
      const a = e.target.closest('a[href]');
      if (a) {
        const href = a.getAttribute('href');
        if (href && !href.startsWith('#')) {
          e.preventDefault();
          e.stopPropagation();
          const safeHref = safeExternalUrl(href);
          if (safeHref) openUrl(safeHref).catch(() => {});
          return;
        }
      }
      if (!current.onIriClick) return;
      const pill = e.target.closest('[data-iri]');
      if (pill) { e.stopPropagation(); current.onIriClick(pill.dataset.iri); }
    }

    function handleKeydown(e) {
      if (e.key !== 'Enter' && e.key !== ' ') return;
      const pill = e.target.closest('[data-iri]');
      if (!pill || !current.onIriClick) return;
      e.preventDefault();
      e.stopPropagation();
      current.onIriClick(pill.dataset.iri);
    }
    shadow.addEventListener('click', handleClick);
    shadow.addEventListener('keydown', handleKeydown);
    renderMarkdown(container, params.html, params.resolutions ?? {});

    return {
      update(newParams) {
        current = newParams;
        renderMarkdown(container, newParams.html, newParams.resolutions ?? {});
      },
      destroy() {
        shadow.removeEventListener('click', handleClick);
        shadow.removeEventListener('keydown', handleKeydown);
      }
    };
  }
</script>

{#if loading}
  <div class="markdown-loading">
    <span class="material-symbols-outlined spinning">progress_activity</span>
  </div>
{:else}
  <div
    use:shadowContent={{ html, resolutions: iriResolutions, onIriClick: openEntityInspector }}
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
