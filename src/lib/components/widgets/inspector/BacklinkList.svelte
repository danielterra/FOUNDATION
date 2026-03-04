<script>
  import { convertFileSrc } from '@tauri-apps/api/core';
  import { slide } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';

  let { backlinks, openEntityInspector } = $props();

  let collapsedGroups = $state(new Set());
  let initialized = $state(false);

  const groups = $derived(
    (backlinks ?? []).reduce((acc, backlink) => {
      const key = `${backlink.property}:${backlink.sourceClass || 'unknown'}`;
      if (!acc[key]) {
        acc[key] = {
          key,
          propertyLabel: backlink.propertyLabel || backlink.property,
          conceptName: backlink.sourceClassLabel || 'Unknown',
          entities: {}
        };
      }
      const iri = backlink.value;
      if (!acc[key].entities[iri]) {
        acc[key].entities[iri] = {
          iri,
          label: backlink.valueLabel || iri,
          icon: backlink.valueIcon,
          status: backlink.valueStatus
        };
      }
      return acc;
    }, {})
  );

  $effect(() => {
    const keys = Object.keys(groups);
    if (keys.length > 0 && !initialized) {
      collapsedGroups = new Set(keys);
      initialized = true;
    }
  });

  function toggleGroup(key) {
    if (collapsedGroups.has(key)) {
      collapsedGroups.delete(key);
    } else {
      collapsedGroups.add(key);
    }
    collapsedGroups = new Set(collapsedGroups);
  }

  function isIconUrl(icon) {
    if (!icon) return false;
    return icon.startsWith('http://') || icon.startsWith('https://') ||
           icon.startsWith('data:') || icon.startsWith('file://') || icon.startsWith('/');
  }

  function getIconUrl(icon) {
    if (!icon) return '';
    if (icon.startsWith('http://') || icon.startsWith('https://') || icon.startsWith('data:'))
      return icon;
    if (icon.startsWith('file://')) return convertFileSrc(icon.replace(/^file:\/\//, ''));
    if (icon.startsWith('/')) return convertFileSrc(icon);
    return icon;
  }
</script>

{#if backlinks?.length > 0}
  <div class="backlinks-list">
    {#each Object.values(groups) as group}
      {@const entities = Object.values(group.entities)}
      {@const isCollapsed = collapsedGroups.has(group.key)}
      <div class="group" transition:slide={{ duration: 300, easing: cubicOut }}>
        <button class="group-header" onclick={() => toggleGroup(group.key)}>
          <span class="material-symbols-outlined chevron" class:expanded={!isCollapsed}>
            chevron_right
          </span>
          <span class="group-label">
            <span class="group-concept">{group.conceptName}</span>
            <span class="group-arrow">→</span>
            <span class="group-prop">{group.propertyLabel}</span>
          </span>
          <span class="group-count">{entities.length}</span>
        </button>

        {#if !isCollapsed}
          <div class="entity-list" transition:slide={{ duration: 300, easing: cubicOut }}>
            {#each entities as entity}
              <div
                class="entity-item clickable"
                role="button"
                tabindex="0"
                onclick={() => openEntityInspector(entity.iri)}
                onkeydown={(e) => e.key === 'Enter' && openEntityInspector(entity.iri)}
              >
                {#if entity.icon}
                  {#if isIconUrl(entity.icon)}
                    <img src={getIconUrl(entity.icon)} alt="" class="entity-icon-image" />
                  {:else}
                    <span class="material-symbols-outlined entity-icon">{entity.icon}</span>
                  {/if}
                {:else}
                  <span class="material-symbols-outlined entity-icon">link</span>
                {/if}
                <span class="entity-label">{entity.label}</span>
                {#if entity.status}
                  <span
                    class="inline-status"
                    style="--status-color: {entity.status.color || 'var(--color-neutral)'}"
                    title={entity.status.iri}
                  >
                    <span class="material-symbols-outlined inline-status-icon">radio_button_checked</span>
                    <span class="inline-status-label">{entity.status.label}</span>
                  </span>
                {/if}
                <span class="material-symbols-outlined arrow">arrow_forward</span>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    {/each}
  </div>
{/if}

<style>
  .backlinks-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-bottom: 16px;
  }

  .group {
    display: flex;
    flex-direction: column;
  }

  .group-header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    background: color-mix(in srgb, var(--color-white) 5%, transparent);
    border-radius: 8px;
    border: none;
    width: 100%;
    cursor: pointer;
    transition: background 0.2s;
    text-align: left;
  }

  .group-header:hover {
    background: color-mix(in srgb, var(--color-interactive) 20%, transparent);
  }

  .chevron {
    font-size: 18px;
    color: var(--color-interactive);
    opacity: 0.8;
    transition: transform 0.2s;
    flex-shrink: 0;
  }

  .chevron.expanded {
    transform: rotate(90deg);
  }

  .group-label {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 5px;
    min-width: 0;
  }

  .group-concept {
    font-family: var(--font-body);
    font-size: 11px;
    font-weight: 700;
    color: var(--color-neutral-active);
    text-transform: uppercase;
    letter-spacing: 0.4px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .group-arrow {
    font-size: 11px;
    color: var(--color-neutral);
    opacity: 0.4;
    flex-shrink: 0;
  }

  .group-prop {
    font-family: var(--font-body);
    font-size: 11px;
    font-weight: 500;
    color: var(--color-neutral);
    opacity: 0.7;
    white-space: nowrap;
  }

  .group-count {
    font-size: 10px;
    font-weight: 600;
    color: var(--color-neutral);
    padding: 1px 6px;
    background: color-mix(in srgb, var(--color-neutral) 15%, transparent);
    border-radius: 10px;
    flex-shrink: 0;
  }

  .entity-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 4px 0 4px 8px;
  }

  .entity-item {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    background: color-mix(in srgb, var(--color-white) 3%, transparent);
    border-radius: 6px;
    border: 1px solid color-mix(in srgb, var(--color-white) 8%, transparent);
    transition: all 0.2s;
  }

  .clickable {
    cursor: pointer;
    user-select: none;
  }

  .clickable:hover {
    background: color-mix(in srgb, var(--color-interactive) 20%, transparent) !important;
    transform: translateX(2px);
  }

  .clickable:active {
    transform: translateX(1px);
  }

  .entity-icon {
    font-size: 18px;
    color: var(--color-neutral);
    flex-shrink: 0;
  }

  .entity-icon-image {
    width: 28px;
    height: 28px;
    border-radius: 5px;
    object-fit: cover;
    flex-shrink: 0;
  }

  .entity-label {
    flex: 1;
    font-family: var(--font-title);
    font-size: 12px;
    font-weight: 600;
    color: var(--color-neutral-active);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .arrow {
    font-size: 18px;
    color: var(--color-neutral);
    opacity: 0.4;
    transition: all 0.2s;
    flex-shrink: 0;
  }

  .entity-item:hover .arrow {
    opacity: 1;
    transform: translateX(3px);
  }

  .inline-status {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    padding: 2px 7px 2px 4px;
    background: color-mix(in srgb, var(--status-color) 18%, transparent);
    border: 1px solid color-mix(in srgb, var(--status-color) 40%, transparent);
    border-radius: 20px;
    flex-shrink: 0;
  }

  .inline-status-icon {
    font-size: 12px;
    color: var(--status-color);
  }

  .inline-status-label {
    font-family: var(--font-body);
    font-size: 10px;
    font-weight: 600;
    color: var(--status-color);
    white-space: nowrap;
  }
</style>
