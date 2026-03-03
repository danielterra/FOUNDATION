<script>
  import { convertFileSrc } from '@tauri-apps/api/core';
  import { slide } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';

  let { backlinks, openEntityInspector } = $props();

  let collapsedGroups = $state(new Set());
  let initialized = $state(false);

  const groupedByClass = $derived(
    (backlinks ?? []).reduce((acc, backlink) => {
      const className = backlink.sourceClassLabel || 'Unknown';
      const classIri = backlink.sourceClass || 'unknown';

      if (!acc[classIri]) {
        acc[classIri] = { className, classIri, entities: {} };
      }

      if (!acc[classIri].entities[backlink.value]) {
        acc[classIri].entities[backlink.value] = {
          entity: backlink.value,
          entityLabel: backlink.valueLabel || backlink.value,
          entityIcon: backlink.valueIcon,
          entityStatus: backlink.valueStatus,
          properties: []
        };
      }

      acc[classIri].entities[backlink.value].properties.push({
        property: backlink.property,
        propertyLabel: backlink.propertyLabel,
        propertyComment: backlink.propertyComment
      });

      return acc;
    }, {})
  );

  $effect(() => {
    if (!initialized && Object.keys(groupedByClass).length > 0) {
      const autoCollapsed = new Set(
        Object.values(groupedByClass)
          .filter(g => Object.keys(g.entities).length > 5)
          .map(g => g.classIri)
      );
      collapsedGroups = autoCollapsed;
      initialized = true;
    }
  });

  function toggleClassGroup(classIri) {
    if (collapsedGroups.has(classIri)) {
      collapsedGroups.delete(classIri);
    } else {
      collapsedGroups.add(classIri);
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
    if (icon.startsWith('http://') || icon.startsWith('https://') ||
        icon.startsWith('data:')) return icon;
    if (icon.startsWith('file://')) return convertFileSrc(icon.replace(/^file:\/\//, ''));
    if (icon.startsWith('/')) return convertFileSrc(icon);
    return icon;
  }
</script>

{#if backlinks?.length > 0}
  <div class="backlinks-list">
    {#each Object.values(groupedByClass) as classGroup}
      {@const entityCount = Object.keys(classGroup.entities).length}
      {@const isCollapsed = collapsedGroups.has(classGroup.classIri)}
      <div class="class-group" transition:slide={{ duration: 400, easing: cubicOut }}>
        <button
          class="class-header"
          onclick={() => toggleClassGroup(classGroup.classIri)}
        >
          <span class="material-symbols-outlined chevron" class:expanded={!isCollapsed}>
            chevron_right
          </span>
          <span class="material-symbols-outlined class-icon">category</span>
          <span class="class-name">{classGroup.className}</span>
          <span class="class-count">{entityCount}</span>
        </button>

        {#if !isCollapsed}
          {#each Object.values(classGroup.entities) as group}
            {@const relCount = group.properties.length}
            <div class="backlink-group" transition:slide={{ duration: 400, easing: cubicOut }}>
              <div
                class="backlink-entity clickable"
                role="button"
                tabindex="0"
                onclick={() => openEntityInspector(group.entity)}
                onkeydown={(e) => e.key === 'Enter' && openEntityInspector(group.entity)}
              >
                {#if group.entityIcon}
                  {#if isIconUrl(group.entityIcon)}
                    <img src={getIconUrl(group.entityIcon)} alt="" class="entity-icon-image" />
                  {:else}
                    <span class="material-symbols-outlined entity-icon">{group.entityIcon}</span>
                  {/if}
                {:else}
                  <span class="material-symbols-outlined entity-icon">link</span>
                {/if}
                <div class="entity-info">
                  <div class="entity-label">{group.entityLabel}</div>
                  <div class="entity-count">
                    {relCount} {relCount === 1 ? 'relationship' : 'relationships'}
                  </div>
                </div>
                {#if group.entityStatus}
                  <span
                    class="inline-status"
                    style="--status-color: {group.entityStatus.color || 'var(--color-neutral)'}"
                    title={group.entityStatus.iri}
                  >
                    <span class="material-symbols-outlined inline-status-icon">
                      radio_button_checked
                    </span>
                    <span class="inline-status-label">{group.entityStatus.label}</span>
                  </span>
                {/if}
                <span class="material-symbols-outlined arrow">arrow_forward</span>
              </div>

              <div class="backlink-properties">
                {#each group.properties as prop}
                  <div class="backlink-property">
                    <span class="material-symbols-outlined prop-icon">arrow_back</span>
                    <div class="prop-info">
                      <span class="prop-label">{prop.propertyLabel}</span>
                      {#if prop.propertyComment}
                        <span class="prop-comment">{prop.propertyComment}</span>
                      {/if}
                    </div>
                  </div>
                {/each}
              </div>
            </div>
          {/each}
        {/if}
      </div>
    {/each}
  </div>
{/if}

<style>
  .backlinks-list {
    display: flex;
    flex-direction: column;
    gap: 20px;
    margin-bottom: 16px;
  }

  .class-group {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .class-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    background: color-mix(in srgb, var(--color-white) 5%, transparent);
    border-radius: 8px;
    border: none;
    width: 100%;
    cursor: pointer;
    transition: all 0.2s;
    text-align: left;
  }

  .class-header:hover {
    background: color-mix(in srgb, var(--color-white) 10%, transparent);
  }

  .chevron {
    font-size: 20px;
    color: var(--color-neutral);
    transition: transform 0.2s;
  }

  .chevron.expanded {
    transform: rotate(90deg);
  }

  .class-icon {
    font-size: 20px;
    color: var(--color-neutral);
  }

  .class-name {
    font-family: var(--font-title);
    font-size: 13px;
    font-weight: 700;
    color: var(--color-neutral-active);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    flex: 1;
  }

  .class-count {
    font-size: 11px;
    font-weight: 600;
    color: var(--color-neutral);
    padding: 2px 8px;
    background: color-mix(in srgb, var(--color-white) 10%, transparent);
    border-radius: 12px;
  }

  .backlink-group {
    background: color-mix(in srgb, var(--color-white) 3%, transparent);
    border-radius: 8px;
    overflow: hidden;
    border: 1px solid color-mix(in srgb, var(--color-white) 10%, transparent);
  }

  .backlink-entity {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px;
    background: color-mix(in srgb, var(--color-white) 5%, transparent);
    border-bottom: 1px solid color-mix(in srgb, var(--color-white) 10%, transparent);
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
    font-size: 24px;
    color: var(--color-neutral);
  }

  .entity-icon-image {
    width: 40px;
    height: 40px;
    border-radius: 6px;
    object-fit: cover;
  }

  .entity-info {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .entity-label {
    font-family: var(--font-title);
    font-size: 14px;
    font-weight: 600;
    color: var(--color-neutral-active);
  }

  .entity-count {
    font-size: 11px;
    color: var(--color-neutral);
  }

  .arrow {
    font-size: 20px;
    color: var(--color-neutral);
    opacity: 0.5;
    transition: all 0.2s;
  }

  .backlink-entity:hover .arrow {
    opacity: 1;
    transform: translateX(4px);
  }

  .backlink-properties {
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .backlink-property {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px;
    background: color-mix(in srgb, var(--color-black) 20%, transparent);
    border-radius: 6px;
  }

  .prop-icon {
    font-size: 16px;
    color: var(--color-neutral);
    opacity: 0.6;
  }

  .prop-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1;
  }

  .prop-label {
    font-family: var(--font-body);
    font-size: 12px;
    font-weight: 500;
    color: var(--color-neutral-active);
  }

  .prop-comment {
    font-size: 11px;
    color: var(--color-neutral);
    line-height: 1.3;
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
