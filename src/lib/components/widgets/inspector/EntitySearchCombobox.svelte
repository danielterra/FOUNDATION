<script lang="ts">
  import { convertFileSrc } from '@tauri-apps/api/core'
  import { onDestroy } from 'svelte'
  import { Input } from '$lib/components/ui/input'
  import { Button } from '$lib/components/ui/button'

  type SearchResult = { id: string; label: string; icon?: string | null }

  let {
    searchFn,
    debounceMs = 200,
    value = null,
    onSelect,
    placeholder = 'Buscar…',
    emptyText = 'Nenhum resultado.',
    disabled = false,
    saving = false,
    multiple = false,
  }: {
    searchFn: (query: string) => Promise<SearchResult[]>
    debounceMs?: number
    value?: SearchResult | null
    onSelect: (item: SearchResult) => void
    placeholder?: string
    emptyText?: string
    disabled?: boolean
    saving?: boolean
    multiple?: boolean
  } = $props()

  let inputValue = $state('')
  let results = $state<SearchResult[]>([])
  let searching = $state(false)
  let listOpen = $state(false)
  let activeIndex = $state(-1)
  let debounceTimer: ReturnType<typeof setTimeout> | null = null
  let inputEl = $state<HTMLInputElement | null>(null)
  let listEl = $state<HTMLDivElement | null>(null)
  const listId = `esc-list-${Math.random().toString(36).slice(2)}`

  // Portal state — list position tracked from input rect
  let listTop = $state(0)
  let listLeft = $state(0)
  let listWidth = $state(0)

  function isIconUrl(icon: string): boolean {
    return (
      icon.startsWith('http://') ||
      icon.startsWith('https://') ||
      icon.startsWith('data:') ||
      icon.startsWith('file://') ||
      icon.startsWith('/')
    )
  }

  function getIconUrl(icon: string): string {
    if (icon.startsWith('file://')) return convertFileSrc(icon.replace(/^file:\/\//, ''))
    if (icon.startsWith('/')) return convertFileSrc(icon)
    return icon
  }

  function updateListPosition() {
    if (!inputEl) return
    const rect = inputEl.getBoundingClientRect()
    listTop = rect.bottom + 2
    listLeft = rect.left
    listWidth = rect.width
  }

  async function runSearch(q: string) {
    if (!q.trim()) {
      results = []
      searching = false
      listOpen = false
      return
    }
    searching = true
    listOpen = true
    updateListPosition()
    try {
      results = await searchFn(q)
    } catch {
      results = []
    } finally {
      searching = false
    }
    activeIndex = -1
  }

  function handleInput(e: Event) {
    const q = (e.target as HTMLInputElement).value
    inputValue = q
    if (debounceTimer) clearTimeout(debounceTimer)
    if (!q.trim()) {
      results = []
      listOpen = false
      return
    }
    debounceTimer = setTimeout(() => runSearch(q), debounceMs)
  }

  function selectItem(item: SearchResult) {
    onSelect(item)
    if (multiple) {
      inputValue = ''
      results = []
      listOpen = false
      activeIndex = -1
      if (debounceTimer) clearTimeout(debounceTimer)
      // Keep focus for next search
      setTimeout(() => inputEl?.focus(), 0)
    } else {
      inputValue = ''
      results = []
      listOpen = false
      activeIndex = -1
    }
  }

  function closeList() {
    listOpen = false
    activeIndex = -1
  }

  function handleKeydown(e: KeyboardEvent) {
    if (!listOpen) return

    const itemCount = results.length

    if (e.key === 'ArrowDown') {
      e.preventDefault()
      activeIndex = activeIndex < itemCount - 1 ? activeIndex + 1 : 0
    } else if (e.key === 'ArrowUp') {
      e.preventDefault()
      activeIndex = activeIndex > 0 ? activeIndex - 1 : itemCount - 1
    } else if (e.key === 'Enter') {
      e.preventDefault()
      if (activeIndex >= 0 && activeIndex < itemCount) {
        selectItem(results[activeIndex])
      }
    } else if (e.key === 'Escape') {
      e.preventDefault()
      closeList()
    }
  }

  function handleFocus() {
    if (inputValue.trim() && (results.length > 0 || searching)) {
      listOpen = true
      updateListPosition()
    }
  }

  function handleBlur(e: FocusEvent) {
    // Delay to allow click on list items to register before closing
    setTimeout(() => {
      if (!listEl?.contains(document.activeElement)) {
        closeList()
      }
    }, 150)
  }

  // Auto-focus when mounted
  $effect(() => {
    if (inputEl) {
      setTimeout(() => inputEl?.focus(), 0)
    }
  })

  // Reposition list on scroll/resize while open
  $effect(() => {
    if (!listOpen) return
    const handler = () => updateListPosition()
    window.addEventListener('scroll', handler, true)
    window.addEventListener('resize', handler)
    return () => {
      window.removeEventListener('scroll', handler, true)
      window.removeEventListener('resize', handler)
    }
  })

  // Portal action — appends node to body and removes on destroy
  function bodyPortal(node: HTMLElement) {
    document.body.appendChild(node)
    return {
      destroy() {
        node.remove()
      },
    }
  }

  onDestroy(() => {
    if (debounceTimer) clearTimeout(debounceTimer)
  })
</script>

<div class="esc-inline">
  <div class="esc-input-wrap">
    <span class="material-symbols-outlined esc-search-icon">search</span>
    <Input
      bind:ref={inputEl}
      type="text"
      class="esc-input"
      value={inputValue}
      {placeholder}
      disabled={disabled || saving}
      oninput={handleInput}
      onkeydown={handleKeydown}
      onfocus={handleFocus}
      onblur={handleBlur}
      autocomplete="off"
      spellcheck={false}
      role="combobox"
      aria-expanded={listOpen}
      aria-autocomplete="list"
      aria-haspopup="listbox"
      aria-controls={listId}
    />
    {#if saving}
      <span class="material-symbols-outlined esc-saving">progress_activity</span>
    {/if}
  </div>

  {#if listOpen}
    <div
      use:bodyPortal
      bind:this={listEl}
      id={listId}
      class="esc-dropdown z-dropdown"
      style:top="{listTop}px"
      style:left="{listLeft}px"
      style:width="{listWidth}px"
      role="listbox"
      tabindex="-1"
      onmousedown={(e) => e.preventDefault()}
    >
      {#if searching}
        <div class="esc-loading">
          <span class="material-symbols-outlined esc-spinner">progress_activity</span>
        </div>
      {:else if results.length === 0}
        <div class="esc-empty">{emptyText}</div>
      {:else}
        {#each results as item, i (item.id)}
          <Button
            variant="ghost"
            class={`esc-item${i === activeIndex ? ' esc-item-active' : ''}`}
            role="option"
            aria-selected={i === activeIndex}
            tabindex={-1}
            onclick={() => selectItem(item)}
            onmouseenter={() => { activeIndex = i }}
          >
            {#if item.icon}
              {#if isIconUrl(item.icon)}
                <img src={getIconUrl(item.icon)} alt="" class="esc-item-img" />
              {:else}
                <span class="material-symbols-outlined esc-item-icon">{item.icon}</span>
              {/if}
            {/if}
            <span class="esc-item-label">{item.label ?? item.id}</span>
          </Button>
        {/each}
      {/if}
    </div>
  {/if}
</div>

<style>
  .esc-inline {
    position: relative;
    width: 100%;
  }

  .esc-input-wrap {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 8px;
    background: color-mix(in srgb, var(--color-black) 50%, transparent);
    width: 100%;
    box-sizing: border-box;
  }

  .esc-search-icon {
    font-size: 14px;
    color: var(--color-neutral-disabled);
    flex-shrink: 0;
  }

  :global([data-slot="input"].esc-input) {
    flex: 1;
    min-width: 0;
    background: none;
    border: none;
    outline: none;
    box-shadow: none;
    font-family: var(--font-body);
    font-size: 14px;
    color: var(--color-neutral-active);
    padding: 0;
    height: auto;
  }

  :global([data-slot="input"].esc-input::placeholder) {
    color: var(--color-neutral-disabled);
  }

  :global([data-slot="input"].esc-input:disabled) {
    opacity: 0.5;
    cursor: default;
  }

  .esc-saving {
    font-size: 14px;
    color: var(--color-neutral);
    animation: esc-spin 1s linear infinite;
    flex-shrink: 0;
  }

  /* Dropdown is portalized to body — all rules must be :global */
  :global(.esc-dropdown) {
    position: fixed;
    background: var(--color-surface-3);
    box-shadow: 0 4px 16px color-mix(in srgb, var(--color-black) 60%, transparent);
    max-height: 220px;
    overflow-y: auto;
    box-sizing: border-box;
  }

  :global(.esc-loading) {
    display: flex;
    justify-content: center;
    padding: 12px 0;
  }

  :global(.esc-spinner) {
    font-size: 16px;
    color: var(--color-neutral);
    animation: esc-spin 1s linear infinite;
  }

  :global(.esc-empty) {
    padding: 10px 12px;
    font-family: var(--font-body);
    font-size: 13px;
    color: var(--color-neutral-disabled);
  }

  :global(.esc-item) {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 7px 10px;
    cursor: pointer;
    color: var(--color-neutral-active);
    font-family: var(--font-body);
    font-size: 14px;
    width: 100%;
    box-sizing: border-box;
    text-align: left;
    background: none;
    border: none;
  }

  :global(.esc-item-active) {
    background: color-mix(in srgb, var(--color-interactive) 18%, transparent);
  }

  :global(.esc-item-icon) {
    font-size: 16px;
    color: var(--color-interactive);
    flex-shrink: 0;
  }

  :global(.esc-item-img) {
    width: 16px;
    height: 16px;
    object-fit: cover;
    flex-shrink: 0;
  }

  :global(.esc-item-label) {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  @keyframes esc-spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }
</style>
