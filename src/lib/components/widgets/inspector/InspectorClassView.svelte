<script>
  import { convertFileSrc } from '@tauri-apps/api/core';
  import ClassPropertyForm from './ClassPropertyForm.svelte';
  import { sticky } from '$lib/utils/actions';

  let {
    entityData,
    isLocked,
    showAddPropertyForm = $bindable(),
    savingClassProperty,
    removeConfirmProp = $bindable(),
    removeConfirmCount,
    removeConfirmExamples,
    checkingUsage,
    openEntityInspector,
    onDefineProperty,
    onConfirmRemoveProperty,
    onCancelRemoveProperty,
  } = $props();

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

{#if entityData.superClasses?.length > 0}
  <div class="section-group">
    <div class="section-label" use:sticky={{ top: 0 }}>Parent Classes</div>
    <div class="item-list">
      {#each entityData.superClasses as superClass}
        <div
          class="item clickable"
          role="button"
          tabindex="0"
          onclick={() => openEntityInspector(superClass.iri)}
          onkeydown={(e) => e.key === 'Enter' && openEntityInspector(superClass.iri)}
        >
          {#if superClass.icon}
            {#if isIconUrl(superClass.icon)}
              <img src={getIconUrl(superClass.icon)} alt="" class="item-icon-image" />
            {:else}
              <span class="material-symbols-outlined">{superClass.icon}</span>
            {/if}
          {/if}
          <span class="item-label">{superClass.label}</span>
        </div>
      {/each}
    </div>
  </div>
{/if}

{#if entityData.subClasses?.length > 0}
  <div class="section-group">
    <div class="section-label" use:sticky={{ top: 0 }}>Child Classes</div>
    <div class="item-list">
      {#each entityData.subClasses as subClass}
        <div
          class="item clickable"
          role="button"
          tabindex="0"
          onclick={() => openEntityInspector(subClass.iri)}
          onkeydown={(e) => e.key === 'Enter' && openEntityInspector(subClass.iri)}
        >
          {#if subClass.icon}
            {#if isIconUrl(subClass.icon)}
              <img src={getIconUrl(subClass.icon)} alt="" class="item-icon-image" />
            {:else}
              <span class="material-symbols-outlined">{subClass.icon}</span>
            {/if}
          {/if}
          <span class="item-label">{subClass.label}</span>
        </div>
      {/each}
    </div>
  </div>
{/if}

{#if entityData.isClass && entityData.allowedStatuses?.length > 0}
  <div class="section-group">
    <div class="section-label" use:sticky={{ top: 0 }}>Allowed Statuses</div>
    <div class="item-list">
      {#each entityData.allowedStatuses as status}
        <div
          class="item clickable status-item"
          style="--status-color: {status.color || 'var(--color-neutral)'}"
          role="button"
          tabindex="0"
          onclick={() => openEntityInspector(status.iri)}
          onkeydown={(e) => e.key === 'Enter' && openEntityInspector(status.iri)}
        >
          {#if status.icon}
            {#if isIconUrl(status.icon)}
              <img src={getIconUrl(status.icon)} alt="" class="item-icon-image" />
            {:else}
              <span class="material-symbols-outlined status-dot">{status.icon}</span>
            {/if}
          {:else}
            <span class="material-symbols-outlined status-dot">radio_button_checked</span>
          {/if}
          <span class="item-label">{status.label}</span>
        </div>
      {/each}
    </div>
  </div>
{/if}

{#if entityData.isClass}
  <div class="class-props-header">
    <span class="class-props-title">Properties</span>
    {#if !isLocked}
      <button
        class="add-property-btn"
        onclick={() => showAddPropertyForm = !showAddPropertyForm}
        title="Add property"
      >
        <span class="material-symbols-outlined">{showAddPropertyForm ? 'close' : 'add'}</span>
        {showAddPropertyForm ? 'Cancel' : 'Add property'}
      </button>
    {/if}
  </div>
  {#if showAddPropertyForm}
    <ClassPropertyForm
      mode="add"
      saving={savingClassProperty}
      onsave={(vals) => onDefineProperty(null, vals)}
      oncancel={() => showAddPropertyForm = false}
    />
  {/if}
  {#if removeConfirmProp}
    <div class="remove-confirm-dialog">
      <div class="remove-confirm-icon">
        <span class="material-symbols-outlined">warning</span>
      </div>
      <div class="remove-confirm-body">
        <p class="remove-confirm-msg">
          <strong>{removeConfirmCount}</strong> individual{removeConfirmCount !== 1 ? 's' : ''} of this class
          {removeConfirmCount !== 1 ? 'have' : 'has'} a value for <em>{removeConfirmProp.label}</em>.
          Removing it will hide the property from the schema but existing values will be preserved.
        </p>
        {#if removeConfirmExamples.length > 0}
          <p class="remove-confirm-examples">{removeConfirmExamples.join(', ')}{removeConfirmCount > removeConfirmExamples.length ? '…' : ''}</p>
        {/if}
        <div class="remove-confirm-actions">
          <button class="remove-confirm-proceed" onclick={onConfirmRemoveProperty}>Remove anyway</button>
          <button class="remove-confirm-cancel" onclick={onCancelRemoveProperty}>Cancel</button>
        </div>
      </div>
    </div>
  {/if}
{/if}

{#if entityData.instances?.length > 0}
  <div class="item-list">
    {#each entityData.instances as instance}
      <div
        class="item instance clickable"
        role="button"
        tabindex="0"
        onclick={() => openEntityInspector(instance.iri)}
        onkeydown={(e) => e.key === 'Enter' && openEntityInspector(instance.iri)}
      >
        {#if instance.icon}
          {#if isIconUrl(instance.icon)}
            <img src={getIconUrl(instance.icon)} alt="" class="item-icon-image" />
          {:else}
            <span class="material-symbols-outlined">{instance.icon}</span>
          {/if}
        {/if}
        <span class="item-label">{instance.label}</span>
      </div>
    {/each}
  </div>
{/if}

<style>
  .section-group {
    display: flex;
    flex-direction: column;
  }

  .section-label {
    font-family: var(--font-body);
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.6px;
    color: color-mix(in srgb, var(--color-neutral) 60%, transparent);
    margin-bottom: 6px;
    z-index: 2;
    padding: 6px 0 4px;
    background: var(--color-surface-1);
  }

  .item-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-bottom: 16px;
  }

  .item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    background: color-mix(in srgb, var(--color-white) 5%, transparent);
    transition: all 0.2s;
  }

  .item:hover {
    background: color-mix(in srgb, var(--color-white) 8%, transparent);
  }

  .clickable {
    cursor: pointer;
    user-select: none;
  }

  .clickable:hover {
    background: color-mix(in srgb, var(--color-interactive) 20%, transparent) !important;
  }

  .clickable:active {
    transform: translateX(1px);
  }


  .status-dot {
    font-size: 16px;
    color: var(--status-color);
    flex-shrink: 0;
  }

  .item .material-symbols-outlined {
    font-size: 18px;
    color: var(--color-interactive);
  }

  .item-icon-image {
    width: 28px;
    height: 28px;
    object-fit: cover;
  }

  .item-label {
    font-family: var(--font-body);
    font-size: 14px;
    color: var(--color-neutral-active);
  }

  .class-props-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 4px 0 8px;
    margin-top: 4px;
  }

  .class-props-title {
    font-family: var(--font-body);
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.6px;
    color: color-mix(in srgb, var(--color-neutral) 60%, transparent);
  }

  .add-property-btn {
    display: flex;
    align-items: center;
    gap: 3px;
    padding: 3px 8px;
    background: color-mix(in srgb, var(--color-interactive) 15%, transparent);
    border: none;
    color: var(--color-interactive);
    font-family: var(--font-body);
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
    transition: background 0.15s;
  }

  .add-property-btn:hover {
    background: color-mix(in srgb, var(--color-interactive) 25%, transparent);
  }

  .add-property-btn .material-symbols-outlined {
    font-size: 14px;
  }

  .remove-confirm-dialog {
    display: flex;
    gap: 10px;
    padding: 12px;
    background: color-mix(in srgb, var(--color-warning, #f59e0b) 8%, transparent);
    margin-bottom: 8px;
  }

  .remove-confirm-icon .material-symbols-outlined {
    font-size: 20px;
    color: var(--color-warning, #f59e0b);
  }

  .remove-confirm-body {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .remove-confirm-msg {
    font-family: var(--font-body);
    font-size: 12px;
    color: var(--color-neutral-active);
    margin: 0;
    line-height: 1.5;
  }

  .remove-confirm-examples {
    font-family: var(--font-body);
    font-size: 11px;
    color: var(--color-neutral);
    margin: 0;
    font-style: italic;
  }

  .remove-confirm-actions {
    display: flex;
    gap: 6px;
    margin-top: 2px;
  }

  .remove-confirm-proceed {
    padding: 4px 10px;
    background: color-mix(in srgb, var(--color-error, #ef4444) 20%, transparent);
    border: none;
    color: var(--color-error, #ef4444);
    font-family: var(--font-body);
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
    transition: background 0.15s;
  }

  .remove-confirm-proceed:hover {
    background: color-mix(in srgb, var(--color-error, #ef4444) 30%, transparent);
  }

  .remove-confirm-cancel {
    padding: 4px 10px;
    background: color-mix(in srgb, var(--color-neutral) 12%, transparent);
    border: none;
    color: var(--color-neutral);
    font-family: var(--font-body);
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
    transition: background 0.15s;
  }

  .remove-confirm-cancel:hover {
    background: color-mix(in srgb, var(--color-neutral) 22%, transparent);
  }
</style>
