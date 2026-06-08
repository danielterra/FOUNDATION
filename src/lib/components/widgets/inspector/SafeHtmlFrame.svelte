<script lang="ts">
  let {
    value,
    mode = 'contained',
    maxHeight = 300,
  }: {
    value: string;
    mode?: 'fill' | 'contained';
    maxHeight?: number;
  } = $props();

  // CSS vars do app não atravessam a barreira de origem opaca do srcdoc.
  // Solução: valores concretos resolvidos de colors.css injetados diretamente.
  // --color-interactive (#ff8c42), --color-neutral-active (#fff), neutro (~#d9d9d9).
  // Emails HTML tipicamente definem suas próprias cores; estes são apenas fallbacks
  // para conteúdo sem estilo próprio. Fundo padrão é branco (light) — padrão universal
  // de emails —; o conteúdo do email sobrescreve via seus próprios inline styles.
  const srcdoc = $derived(`<!DOCTYPE html>
<html>
<head>
<base target="_blank">
<style>
  html, body {
    margin: 0;
    padding: 8px;
    box-sizing: border-box;
    background: #ffffff;
    color: #111111;
    font-family: system-ui, sans-serif;
  }
  * { box-sizing: border-box; max-width: 100%; }
  img { height: auto; }
  a { color: #ff8c42; }
  table { border-collapse: collapse; max-width: 100%; }
  div, p, span, td, th, li {
    font-size: 14px;
    color: #111111;
    line-height: 1.5;
    word-wrap: break-word;
  }
</style>
</head>
<body>${value ?? ''}</body>
</html>`);
</script>

{#if mode === 'fill'}
  <iframe
    class="safe-frame safe-frame--fill"
    sandbox="allow-popups allow-popups-to-escape-sandbox"
    title="Conteúdo HTML"
    {srcdoc}
  ></iframe>
{:else}
  <iframe
    class="safe-frame safe-frame--contained"
    sandbox="allow-popups allow-popups-to-escape-sandbox"
    title="Conteúdo HTML"
    style="height: {maxHeight}px;"
    {srcdoc}
  ></iframe>
{/if}

<style>
  .safe-frame {
    width: 100%;
    border: none;
    background: #ffffff;
  }

  .safe-frame--fill {
    flex: 1;
    height: 100%;
    display: block;
  }

  .safe-frame--contained {
    display: block;
    overflow-y: auto;
    min-height: 80px;
  }
</style>
