<script>
  import { convertFileSrc } from '@tauri-apps/api/core';
  import ClassPropertyForm from './ClassPropertyForm.svelte';
  import DisjointSelect from './DisjointSelect.svelte';
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
    showAddDisjointForm = $bindable(false),
    savingDisjoint = false,
    disjointError = null,
    removeDisjointConfirm = null,
    onAddDisjoint = null,
    onRequestRemoveDisjoint = null,
    onConfirmRemoveDisjoint = null,
    onCancelRemoveDisjoint = null,
    onClearDisjointError = null,
  } = $props();

  const disjointGroups = $derived(entityData?.disjointGroups ?? []);
  const disjointExcludeIris = $derived.by(() => {
    const ids = new Set();
    if (entityData?.id) ids.add(entityData.id);
    for (const g of disjointGroups) {
      for (const m of g.members ?? []) ids.add(m.iri);
    }
    return [...ids];
  });

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

{#if entityData.isClass && (disjointGroups.length > 0 || (!isLocked && onAddDisjoint))}
  <div class="section-group">
    <div class="section-label section-label-row" use:sticky={{ top: 0 }}>
      <span>Classes Disjuntas</span>
      {#if !isLocked && onAddDisjoint}
        <button
          class="add-disjoint-btn"
          onclick={() => showAddDisjointForm = !showAddDisjointForm}
          title="Adicionar disjunção"
        >
          <span class="material-symbols-outlined">{showAddDisjointForm ? 'close' : 'add'}</span>
        </button>
      {/if}
    </div>

    {#if showAddDisjointForm}
      <DisjointSelect
        excludeIris={disjointExcludeIris}
        saving={savingDisjoint}
        onsave={(iri) => onAddDisjoint?.(iri)}
        oncancel={() => showAddDisjointForm = false}
      />
    {/if}

    {#if disjointError}
      <div class="disjoint-error">
        <span class="material-symbols-outlined">error</span>
        <span class="disjoint-error-msg">{disjointError}</span>
        <button class="disjoint-error-close" onclick={() => onClearDisjointError?.()}>
          <span class="material-symbols-outlined">close</span>
        </button>
      </div>
    {/if}

    {#if removeDisjointConfirm && removeDisjointConfirm.kind === 'all'}
      <div class="remove-confirm-dialog">
        <div class="remove-confirm-icon">
          <span class="material-symbols-outlined">warning</span>
        </div>
        <div class="remove-confirm-body">
          <p class="remove-confirm-msg">
            Esta disjunção é parte de um <strong>conjunto n-ário</strong> com
            {removeDisjointConfirm.members.length + 1} classes.
            Remover qualquer membro retrai o conjunto inteiro — os outros pares
            ({removeDisjointConfirm.members.map(m => m.label ?? m.iri).join(', ')})
            também deixarão de ser disjuntos entre si.
          </p>
          <div class="remove-confirm-actions">
            <button class="remove-confirm-proceed" onclick={() => onConfirmRemoveDisjoint?.()}>Remover conjunto</button>
            <button class="remove-confirm-cancel" onclick={() => onCancelRemoveDisjoint?.()}>Cancelar</button>
          </div>
        </div>
      </div>
    {/if}

    <div class="item-list">
      {#each disjointGroups as group, gi (group.groupId ?? `pair-${gi}`)}
        {#if group.kind === 'pair'}
          {@const m = group.members[0]}
          <div class="item disjoint-item">
            <div
              class="item-body clickable"
              role="button"
              tabindex="0"
              onclick={() => openEntityInspector(m.iri)}
              onkeydown={(e) => e.key === 'Enter' && openEntityInspector(m.iri)}
            >
              {#if m.icon}
                {#if isIconUrl(m.icon)}
                  <img src={getIconUrl(m.icon)} alt="" class="item-icon-image" />
                {:else}
                  <span class="material-symbols-outlined">{m.icon}</span>
                {/if}
              {/if}
              <span class="item-label">{m.label ?? m.iri}</span>
            </div>
            {#if !isLocked && onRequestRemoveDisjoint}
              <button class="disjoint-remove" onclick={() => onRequestRemoveDisjoint(group)} title="Remover">
                <span class="material-symbols-outlined">close</span>
              </button>
            {/if}
          </div>
        {:else}
          <div class="disjoint-set">
            <div class="disjoint-set-header">
              <span class="material-symbols-outlined">hub</span>
              <span class="disjoint-set-label">Conjunto disjunto</span>
              {#if !isLocked && onRequestRemoveDisjoint}
                <button class="disjoint-remove" onclick={() => onRequestRemoveDisjoint(group)} title="Remover conjunto">
                  <span class="material-symbols-outlined">close</span>
                </button>
              {/if}
            </div>
            <div class="disjoint-set-members">
              {#each group.members as m (m.iri)}
                <div
                  class="item clickable disjoint-member"
                  role="button"
                  tabindex="0"
                  onclick={() => openEntityInspector(m.iri)}
                  onkeydown={(e) => e.key === 'Enter' && openEntityInspector(m.iri)}
                >
                  {#if m.icon}
                    {#if isIconUrl(m.icon)}
                      <img src={getIconUrl(m.icon)} alt="" class="item-icon-image" />
                    {:else}
                      <span class="material-symbols-outlined">{m.icon}</span>
                    {/if}
                  {/if}
                  <span class="item-label">{m.label ?? m.iri}</span>
                </div>
              {/each}
            </div>
          </div>
        {/if}
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

  .section-label-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .add-disjoint-btn {
    display: flex;
    align-items: center;
    padding: 2px 6px;
    background: color-mix(in srgb, var(--color-interactive) 15%, transparent);
    border: none;
    color: var(--color-interactive);
    cursor: pointer;
  }

  .add-disjoint-btn:hover {
    background: color-mix(in srgb, var(--color-interactive) 25%, transparent);
  }

  .add-disjoint-btn .material-symbols-outlined {
    font-size: 14px;
  }

  .disjoint-item {
    justify-content: space-between;
    padding: 0;
  }

  .disjoint-item .item-body {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 1;
    padding: 8px 12px;
  }

  .disjoint-remove {
    display: flex;
    align-items: center;
    padding: 4px 8px;
    background: transparent;
    border: none;
    color: color-mix(in srgb, var(--color-neutral) 60%, transparent);
    cursor: pointer;
  }

  .disjoint-remove:hover {
    color: var(--color-error, #ef4444);
    background: color-mix(in srgb, var(--color-error, #ef4444) 12%, transparent);
  }

  .disjoint-remove .material-symbols-outlined {
    font-size: 16px;
  }

  .disjoint-set {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 8px 12px;
    background: color-mix(in srgb, var(--color-interactive) 6%, transparent);
    border-left: 3px solid color-mix(in srgb, var(--color-interactive) 50%, transparent);
    margin-bottom: 6px;
  }

  .disjoint-set-header {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .disjoint-set-header .material-symbols-outlined {
    font-size: 14px;
    color: var(--color-interactive);
  }

  .disjoint-set-label {
    flex: 1;
    font-family: var(--font-body);
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: color-mix(in srgb, var(--color-interactive) 80%, var(--color-neutral));
  }

  .disjoint-set-members {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding-left: 4px;
  }

  .disjoint-member {
    margin-bottom: 0;
  }

  .disjoint-error {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 8px 10px;
    background: color-mix(in srgb, var(--color-error, #ef4444) 10%, transparent);
    border-left: 3px solid var(--color-error, #ef4444);
    margin-bottom: 8px;
  }

  .disjoint-error .material-symbols-outlined {
    font-size: 16px;
    color: var(--color-error, #ef4444);
    flex-shrink: 0;
  }

  .disjoint-error-msg {
    flex: 1;
    font-family: var(--font-body);
    font-size: 12px;
    color: var(--color-neutral-active);
    line-height: 1.4;
  }

  .disjoint-error-close {
    background: transparent;
    border: none;
    color: color-mix(in srgb, var(--color-neutral) 60%, transparent);
    cursor: pointer;
    padding: 0;
  }

  .disjoint-error-close .material-symbols-outlined {
    font-size: 14px;
  }
</style>
