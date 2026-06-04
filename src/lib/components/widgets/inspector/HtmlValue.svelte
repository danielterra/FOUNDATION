<script>
  import DOMPurify from 'dompurify';
  import { openUrl } from '@tauri-apps/plugin-opener';

  let { value } = $props();

  // Sanitize email HTML: DOMPurify strips active content (scripts, on* handlers,
  // javascript: URLs) by default; we additionally forbid embedding tags. Inline
  // styles and style elements are kept so the message renders close to the
  // original — the Shadow DOM below isolates that CSS from the app.
  const clean = $derived(DOMPurify.sanitize(value ?? '', {
    FORBID_TAGS: ['script', 'iframe', 'object', 'embed', 'form', 'base', 'meta'],
    FORBID_ATTR: ['ping', 'srcset'],
  }));

  function htmlContent(node, html) {
    const shadow = node.attachShadow({ mode: 'open' });

    const styleEl = document.createElement('style');
    styleEl.textContent = `
      :host { display: block; }
      * { box-sizing: border-box; max-width: 100%; }
      img { height: auto; }
      a { color: var(--color-interactive); }
      table { border-collapse: collapse; max-width: 100%; }
      div, p, span, td, th, li { font-size: 14px; color: var(--color-neutral-active); line-height: 1.5; word-wrap: break-word; }
    `;

    const container = document.createElement('div');
    shadow.appendChild(styleEl);
    shadow.appendChild(container);
    container.innerHTML = html ?? '';

    // Links inside the Shadow DOM would otherwise navigate the whole webview away
    // from the app — intercept and open them in the OS browser instead.
    function handleClick(e) {
      const a = e.target.closest('a[href]');
      if (!a) return;
      const href = a.getAttribute('href');
      if (!href || href.startsWith('#')) return;
      e.preventDefault();
      e.stopPropagation();
      openUrl(href).catch(() => {});
    }
    shadow.addEventListener('click', handleClick);

    return {
      update(newHtml) { container.innerHTML = newHtml ?? ''; },
      destroy() { shadow.removeEventListener('click', handleClick); },
    };
  }
</script>

<div use:htmlContent={clean} class="value-html"></div>

<style>
  .value-html {
    flex: 1;
    min-width: 0;
    overflow-x: auto;
  }
</style>
