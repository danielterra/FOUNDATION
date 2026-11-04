<script>
  import rhaiLogo from '$lib/assets/languages/rhai.svg';
  import rustLogo from '$lib/assets/languages/rust.svg';
  import typescriptLogo from '$lib/assets/languages/typescript.svg';

  let {
    value = $bindable(''),
    language = null,
    fieldLabel = 'Código',
    readonly = false,
    onblur = null,
    onkeydown = null,
  } = $props();

  let inputElement = $state(null);
  let highlightElement = $state(null);

  function isKeyword(word, codeLanguage) {
    if (codeLanguage === 'rhai') {
      return ['if', 'else', 'for', 'while', 'loop', 'in', 'let', 'const', 'fn', 'return', 'break', 'continue', 'true', 'false', 'private', 'throw', 'try', 'catch', 'this'].includes(word);
    }
    if (codeLanguage === 'typescript') {
      return ['abstract', 'as', 'asserts', 'async', 'await', 'boolean', 'break', 'case', 'catch', 'class', 'const', 'continue', 'debugger', 'declare', 'default', 'delete', 'do', 'else', 'enum', 'export', 'extends', 'false', 'finally', 'for', 'from', 'function', 'if', 'implements', 'import', 'in', 'instanceof', 'interface', 'is', 'keyof', 'let', 'module', 'namespace', 'never', 'new', 'null', 'number', 'object', 'of', 'package', 'private', 'protected', 'public', 'readonly', 'return', 'satisfies', 'static', 'string', 'super', 'switch', 'this', 'throw', 'true', 'try', 'type', 'typeof', 'undefined', 'unique', 'unknown', 'var', 'void', 'while', 'with', 'yield'].includes(word);
    }
    if (codeLanguage === 'rust') {
      return ['as', 'async', 'await', 'break', 'const', 'continue', 'crate', 'dyn', 'else', 'enum', 'extern', 'false', 'fn', 'for', 'if', 'impl', 'in', 'let', 'loop', 'match', 'mod', 'move', 'mut', 'pub', 'ref', 'return', 'self', 'Self', 'static', 'struct', 'super', 'trait', 'true', 'type', 'unsafe', 'use', 'where', 'while', 'abstract', 'become', 'box', 'do', 'final', 'macro', 'override', 'priv', 'typeof', 'unsized', 'virtual', 'yield'].includes(word);
    }
    return false;
  }

  function rustRawStringEnd(source, start) {
    let quoteIndex = start;
    if (source.startsWith('br', start)) quoteIndex += 2;
    else if (source[start] === 'r') quoteIndex += 1;
    else return -1;
    let hashCount = 0;
    while (source[quoteIndex] === '#') {
      hashCount += 1;
      quoteIndex += 1;
    }
    if (source[quoteIndex] !== '"') return -1;
    const closing = `"${'#'.repeat(hashCount)}`;
    const closingIndex = source.indexOf(closing, quoteIndex + 1);
    return closingIndex < 0 ? source.length : closingIndex + closing.length;
  }

  function rustCharLiteralEnd(source, start) {
    let index = start + 1;
    if (source[index] === '\\') index += 2;
    else index += 1;
    return source[index] === "'" ? index + 1 : -1;
  }

  function quotedStringEnd(source, start, quote) {
    let index = start + 1;
    while (index < source.length) {
      if (source[index] === '\\') {
        index += 2;
        continue;
      }
      if (source[index] === quote) return index + 1;
      index += 1;
    }
    return source.length;
  }

  function rustBlockCommentEnd(source, start) {
    let depth = 1;
    let index = start + 2;
    while (index < source.length && depth > 0) {
      if (source[index] === '/' && source[index + 1] === '*') {
        depth += 1;
        index += 2;
      } else if (source[index] === '*' && source[index + 1] === '/') {
        depth -= 1;
        index += 2;
      } else {
        index += 1;
      }
    }
    return index;
  }

  function tokenizeWithLanguage(source, codeLanguage) {
    const tokens = [];
    let index = 0;
    let functionNameExpected = false;
    while (index < source.length) {
      const start = index;
      const char = source[index];
      if (char === '\n') {
        tokens.push({ text: char, kind: 'plain' });
        index += 1;
        continue;
      }
      if (char === '/' && source[index + 1] === '/') {
        while (index < source.length && source[index] !== '\n') index += 1;
        tokens.push({ text: source.slice(start, index), kind: 'comment' });
        continue;
      }
      if ((codeLanguage === 'typescript' || codeLanguage === 'rust') && char === '/' && source[index + 1] === '*') {
        if (codeLanguage === 'rust') index = rustBlockCommentEnd(source, index);
        else {
          index += 2;
          while (index < source.length && !(source[index] === '*' && source[index + 1] === '/')) index += 1;
          index = Math.min(source.length, index + 2);
        }
        tokens.push({ text: source.slice(start, index), kind: 'comment' });
        continue;
      }
      if (codeLanguage === 'rust') {
        const rawStringEnd = rustRawStringEnd(source, index);
        if (rawStringEnd > index) {
          index = rawStringEnd;
          tokens.push({ text: source.slice(start, index), kind: 'string' });
          continue;
        }
        if (char === 'b' && (source[index + 1] === '"' || source[index + 1] === "'")) {
          const quote = source[index + 1];
          const end = quote === '"' ? quotedStringEnd(source, index + 1, quote) : rustCharLiteralEnd(source, index + 1);
          if (end > index) {
            index = end;
            tokens.push({ text: source.slice(start, index), kind: 'string' });
            continue;
          }
        }
        if (char === "'") {
          const charEnd = rustCharLiteralEnd(source, index);
          if (charEnd > index) {
            index = charEnd;
            tokens.push({ text: source.slice(start, index), kind: 'string' });
            continue;
          }
        }
      }
      if (char === '"' || (codeLanguage !== 'rust' && char === "'") || (codeLanguage === 'typescript' && char === '`')) {
        const quote = char;
        index = quotedStringEnd(source, index, quote);
        tokens.push({ text: source.slice(start, index), kind: 'string' });
        continue;
      }
      if (/[0-9]/.test(char)) {
        index += 1;
        while (index < source.length && /[A-Za-z0-9._]/.test(source[index])) index += 1;
        tokens.push({ text: source.slice(start, index), kind: 'number' });
        continue;
      }
      if (/[A-Za-z_$]/.test(char)) {
        index += 1;
        while (index < source.length && /[A-Za-z0-9_$]/.test(source[index])) index += 1;
        const word = source.slice(start, index);
        const keyword = isKeyword(word, codeLanguage);
        let kind = keyword ? 'keyword' : 'plain';
        if (!keyword && functionNameExpected) kind = 'function';
        if (!keyword && !functionNameExpected) {
          let nextIndex = index;
          while (nextIndex < source.length && /\s/.test(source[nextIndex])) nextIndex += 1;
          if (source[nextIndex] === '(' || (codeLanguage === 'rust' && source[nextIndex] === '!')) kind = 'function';
        }
        tokens.push({ text: word, kind });
        functionNameExpected = (word === 'function' && codeLanguage === 'typescript') || (word === 'fn' && codeLanguage === 'rust');
        continue;
      }
      if ('+-*/%=!<>|&'.includes(char)) {
        index += 1;
        while (index < source.length && '+-*/%=!<>|&'.includes(source[index])) index += 1;
        tokens.push({ text: source.slice(start, index), kind: 'operator' });
        continue;
      }
      index += 1;
      tokens.push({ text: source.slice(start, index), kind: 'plain' });
    }
    return tokens;
  }

  function tokenizeCode(source, codeLanguage) {
    if (codeLanguage === 'rhai' || codeLanguage === 'typescript' || codeLanguage === 'rust') {
      return tokenizeWithLanguage(source, codeLanguage);
    }
    return [{ text: source, kind: 'plain' }];
  }

  const tokens = $derived(tokenizeCode(value ?? '', language));
  const languageLogo = $derived(language === 'rhai' ? rhaiLogo : language === 'typescript' ? typescriptLogo : language === 'rust' ? rustLogo : null);
  const lineNumbers = $derived.by(() => {
    const total = (value ?? '').split('\n').length;
    const numbers = [];
    for (let line = 1; line <= total; line += 1) numbers.push(line);
    return numbers;
  });

  $effect(() => {
    if (!readonly && inputElement) inputElement.focus();
  });

  function syncScroll() {
    if (inputElement && highlightElement) {
      highlightElement.scrollTop = inputElement.scrollTop;
    }
  }

  function editorKeydown(event) {
    if (event.key === 'Tab') {
      event.preventDefault();
      const start = inputElement.selectionStart;
      const end = inputElement.selectionEnd;
      value = `${value.slice(0, start)}  ${value.slice(end)}`;
      requestAnimationFrame(function restoreCaret() {
        if (inputElement) inputElement.setSelectionRange(start + 2, start + 2);
      });
    }
    if (onkeydown) onkeydown(event);
  }
</script>

<div class="code-editor" class:code-editor-readonly={readonly}>
  {#if language}
    <div class="code-language">
      {#if languageLogo}
        <span class="code-language-logo-frame">
          <img class="code-language-logo" src={languageLogo} alt={language} title={language} />
        </span>
      {:else}
        <span class="material-symbols-outlined code-language-icon" role="img" aria-label={language} title={language}>code</span>
      {/if}
    </div>
  {/if}
  <div class="code-frame">
    <div class="code-gutter" aria-hidden="true">
      {#each lineNumbers as line (line)}<span>{line}</span>{/each}
    </div>
    <div class="code-content">
      <pre class="code-highlight" bind:this={highlightElement} aria-hidden="true"><code>{#each tokens as token, index (index)}<span class="token-{token.kind}">{token.text}</span>{/each}</code></pre>
      {#if readonly}
        <div class="code-reader" role="textbox" aria-readonly="true" tabindex="0" aria-label={`${fieldLabel}${language ? `, ${language}` : ''}`}>{value}</div>
      {:else}
        <textarea
          bind:this={inputElement}
          bind:value
          class="code-input"
          aria-label={`${fieldLabel}${language ? `, ${language}` : ''}`}
          spellcheck="false"
          wrap="soft"
          onscroll={syncScroll}
          onkeydown={editorKeydown}
          onblur={onblur}
        ></textarea>
      {/if}
    </div>
  </div>
</div>

<style>
  .code-editor {
    width: 100%;
    min-width: 0;
    overflow: hidden;
    border: 1px solid var(--color-hud-line-subtle);
    border-radius: var(--radius);
    background: var(--color-surface-1);
  }

  .code-language {
    display: flex;
    align-items: center;
    gap: var(--spacing-inline-tight);
    padding: var(--space-1) var(--space-3);
    border-bottom: 1px solid var(--color-hud-line-subtle);
    color: var(--text-secondary);
    font-family: var(--font-mono);
    font-size: var(--type-caption-compact);
    line-height: var(--line-height-compact);
  }

  .code-language-icon {
    color: var(--text-secondary);
    font-size: var(--type-icon-small);
  }

  .code-language-logo {
    width: auto;
    height: var(--size-code-language-logo);
    object-fit: contain;
  }

  .code-language-logo-frame {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: var(--space-1);
    border: var(--size-code-language-logo-border) solid var(--color-hud-line-subtle);
    border-radius: var(--radius);
    background: var(--color-code-logo-surface);
  }

  .code-frame {
    display: flex;
    min-width: 0;
    max-height: var(--size-code-editor-max-height);
    overflow-x: hidden;
    overflow-y: auto;
    background: var(--color-surface-1);
    font-family: var(--font-mono);
    font-size: var(--type-body-compact);
    line-height: var(--line-height-body);
    tab-size: 2;
  }

  .code-gutter {
    position: sticky;
    left: 0;
    z-index: 1;
    display: flex;
    flex: 0 0 auto;
    flex-direction: column;
    min-width: var(--size-code-editor-gutter);
    padding: var(--space-3) var(--space-2);
    border-right: 1px solid var(--color-hud-line-subtle);
    background: var(--color-surface-2);
    color: var(--text-tertiary);
    text-align: right;
    user-select: none;
  }

  .code-content {
    position: relative;
    flex: 1 1 0;
    min-width: 0;
    min-height: var(--size-code-editor-min-height);
  }

  .code-highlight,
  .code-input,
  .code-reader {
    box-sizing: border-box;
    width: 100%;
    min-width: 0;
    min-height: 100%;
    margin: 0;
    padding: var(--space-3);
    border: 0;
    font: inherit;
    line-height: inherit;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    tab-size: 2;
  }

  .code-highlight {
    position: absolute;
    inset: 0;
    overflow: hidden;
    color: var(--text-primary);
    pointer-events: none;
  }

  .code-highlight code { font: inherit; }
  .code-reader {
    position: relative;
    overflow: visible;
    background: transparent;
    color: transparent;
    -webkit-text-fill-color: transparent;
  }

  .code-input {
    position: relative;
    display: block;
    resize: vertical;
    overflow: auto;
    background: transparent;
    color: transparent;
    caret-color: var(--color-interactive);
    outline: none;
    -webkit-text-fill-color: transparent;
  }

  .code-input::selection,
  .code-reader::selection {
    background: var(--color-selection-subtle);
    -webkit-text-fill-color: var(--text-primary);
  }
  .code-input:focus-visible { outline: var(--size-code-editor-focus-outline) solid var(--color-selection); outline-offset: -1px; }
  .code-reader:focus-visible { outline: var(--size-code-editor-focus-outline) solid var(--color-selection); outline-offset: -1px; }
  .token-keyword { color: var(--color-code-keyword); }
  .token-string { color: var(--color-code-string); }
  .token-number { color: var(--color-code-number); }
  .token-comment { color: var(--color-code-comment); }
  .token-operator { color: var(--color-code-operator); }
  .token-function { color: var(--color-code-function); }
</style>
