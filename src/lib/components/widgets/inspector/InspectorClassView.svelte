<script>
  import { convertFileSrc } from '@tauri-apps/api/core';
  import ClassPropertyForm from './ClassPropertyForm.svelte';
  import DisjointSelect from './DisjointSelect.svelte';
  import ProcessSelect from './ProcessSelect.svelte';
  import * as AlertDialog from '$lib/components/ui/alert-dialog';
  import { Button } from '$lib/components/ui/button';
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
    onAddRelatedProcess = null,
    onRemoveRelatedProcess = null,
    onAddInputAutomation = null,
    onRemoveInputAutomation = null,
  } = $props();

  let showAddRelatedProcessForm = $state(false);
  let showAddInputAutomationForm = $state(false);

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

{#if entityData.isClass && ((entityData.relatedProcesses?.length ?? 0) > 0 || (!isLocked && onAddRelatedProcess))}
  <div class="section-group">
    <div class="section-label section-label-row" use:sticky={{ top: 0 }}>
      <span>Processos Relacionados</span>
      {#if !isLocked && onAddRelatedProcess}
        <Button
          variant="ghost"
          size="icon-sm"
          onclick={() => showAddRelatedProcessForm = !showAddRelatedProcessForm}
          title="Vincular processo ou automação"
        >
          <span class="material-symbols-outlined">{showAddRelatedProcessForm ? 'close' : 'add'}</span>
        </Button>
      {/if}
    </div>

    {#if showAddRelatedProcessForm}
      <ProcessSelect
        excludeIris={(entityData.relatedProcesses ?? []).map(p => p.iri)}
        onsave={async (iri) => { await onAddRelatedProcess?.(iri); showAddRelatedProcessForm = false; }}
        oncancel={() => showAddRelatedProcessForm = false}
      />
    {/if}

    <div class="item-list">
      {#each (entityData.relatedProcesses ?? []) as proc (proc.iri)}
        <div class="item disjoint-item">
          <div
            class="item-body clickable"
            role="button"
            tabindex="0"
            onclick={() => openEntityInspector(proc.iri)}
            onkeydown={(e) => e.key === 'Enter' && openEntityInspector(proc.iri)}
          >
            <span class="material-symbols-outlined">
              {proc.type === 'foundation:Automation' ? 'bolt' : 'schema'}
            </span>
            <span class="item-label">{proc.label}</span>
          </div>
          {#if !isLocked && onRemoveRelatedProcess}
            <Button variant="ghost" size="icon-sm" class="disjoint-remove" onclick={() => onRemoveRelatedProcess(proc.iri)} title="Desvincular">
              <span class="material-symbols-outlined">close</span>
            </Button>
          {/if}
        </div>
      {/each}
    </div>
  </div>
{/if}

{#if entityData.isClass && ((entityData.applicableAutomations?.length ?? 0) > 0 || (!isLocked && onAddInputAutomation))}
  <div class="section-group">
    <div class="section-label section-label-row" use:sticky={{ top: 0 }}>
      <span>Automações de Entrada</span>
      {#if !isLocked && onAddInputAutomation}
        <Button
          variant="ghost"
          size="icon-sm"
          onclick={() => showAddInputAutomationForm = !showAddInputAutomationForm}
          title="Vincular automação de entrada"
        >
          <span class="material-symbols-outlined">{showAddInputAutomationForm ? 'close' : 'add'}</span>
        </Button>
      {/if}
    </div>

    {#if showAddInputAutomationForm}
      <ProcessSelect
        excludeIris={(entityData.applicableAutomations ?? []).map(a => a.iri)}
        automationOnly={true}
        onsave={async (iri) => { await onAddInputAutomation?.(iri); showAddInputAutomationForm = false; }}
        oncancel={() => showAddInputAutomationForm = false}
      />
    {/if}

    <div class="item-list">
      {#each (entityData.applicableAutomations ?? []) as auto (auto.iri)}
        <div class="item disjoint-item">
          <div
            class="item-body clickable"
            role="button"
            tabindex="0"
            onclick={() => openEntityInspector(auto.iri)}
            onkeydown={(e) => e.key === 'Enter' && openEntityInspector(auto.iri)}
          >
            <span class="material-symbols-outlined">bolt</span>
            <span class="item-label">{auto.label}</span>
          </div>
          {#if !isLocked && onRemoveInputAutomation}
            <Button variant="ghost" size="icon-sm" class="disjoint-remove" onclick={() => onRemoveInputAutomation(auto.iri)} title="Desvincular">
              <span class="material-symbols-outlined">close</span>
            </Button>
          {/if}
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
        <Button
          variant="ghost"
          size="icon-sm"
          onclick={() => showAddDisjointForm = !showAddDisjointForm}
          title="Adicionar disjunção"
        >
          <span class="material-symbols-outlined">{showAddDisjointForm ? 'close' : 'add'}</span>
        </Button>
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
        <Button variant="ghost" size="icon-sm" onclick={() => onClearDisjointError?.()}>
          <span class="material-symbols-outlined">close</span>
        </Button>
      </div>
    {/if}

    <AlertDialog.Root
      open={!!(removeDisjointConfirm && removeDisjointConfirm.kind === 'all')}
      onOpenChange={(v) => { if (!v) onCancelRemoveDisjoint?.(); }}
    >
      <AlertDialog.Content>
        <div class="adlg-icon-row">
          <span class="material-symbols-outlined adlg-warning-icon">warning</span>
        </div>
        <AlertDialog.Title class="adlg-title">Remover conjunto disjunto</AlertDialog.Title>
        <AlertDialog.Description class="adlg-description">
          Esta disjunção é parte de um <strong>conjunto n-ário</strong> com
          {removeDisjointConfirm?.members?.length != null ? removeDisjointConfirm.members.length + 1 : 0} classes.
          Remover qualquer membro retrai o conjunto inteiro — os outros pares
          ({(removeDisjointConfirm?.members ?? []).map(m => m.label ?? m.iri).join(', ')})
          também deixarão de ser disjuntos entre si.
        </AlertDialog.Description>
        <div class="adlg-actions">
          <AlertDialog.Action
            class="adlg-proceed"
            onclick={() => onConfirmRemoveDisjoint?.()}
          >
            Remover conjunto
          </AlertDialog.Action>
          <AlertDialog.Cancel
            class="adlg-cancel"
            onclick={() => onCancelRemoveDisjoint?.()}
          >
            Cancelar
          </AlertDialog.Cancel>
        </div>
      </AlertDialog.Content>
    </AlertDialog.Root>

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
              <Button variant="ghost" size="icon-sm" class="disjoint-remove" onclick={() => onRequestRemoveDisjoint(group)} title="Remover">
                <span class="material-symbols-outlined">close</span>
              </Button>
            {/if}
          </div>
        {:else}
          <div class="disjoint-set">
            <div class="disjoint-set-header">
              <span class="material-symbols-outlined">hub</span>
              <span class="disjoint-set-label">Conjunto disjunto</span>
              {#if !isLocked && onRequestRemoveDisjoint}
                <Button variant="ghost" size="icon-sm" class="disjoint-remove" onclick={() => onRequestRemoveDisjoint(group)} title="Remover conjunto">
                  <span class="material-symbols-outlined">close</span>
                </Button>
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
      <Button
        variant="ghost"
        size="sm"
        onclick={() => showAddPropertyForm = !showAddPropertyForm}
        title="Add property"
      >
        <span class="material-symbols-outlined">{showAddPropertyForm ? 'close' : 'add'}</span>
        {showAddPropertyForm ? 'Cancel' : 'Add property'}
      </Button>
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
  <AlertDialog.Root
    open={!!removeConfirmProp}
    onOpenChange={(v) => { if (!v) onCancelRemoveProperty?.(); }}
  >
    <AlertDialog.Content>
      <div class="adlg-icon-row">
        <span class="material-symbols-outlined adlg-warning-icon">warning</span>
      </div>
      <AlertDialog.Title class="adlg-title">Remove property</AlertDialog.Title>
      <AlertDialog.Description class="adlg-description">
        <strong>{removeConfirmCount}</strong> individual{removeConfirmCount !== 1 ? 's' : ''} of this class
        {removeConfirmCount !== 1 ? 'have' : 'has'} a value for <em>{removeConfirmProp?.label}</em>.
        Removing it will hide the property from the schema but existing values will be preserved.
      </AlertDialog.Description>
      {#if (removeConfirmExamples?.length ?? 0) > 0}
        <p class="adlg-examples">{removeConfirmExamples.join(', ')}{removeConfirmCount > removeConfirmExamples.length ? '…' : ''}</p>
      {/if}
      <div class="adlg-actions">
        <AlertDialog.Action
          class="adlg-proceed"
          onclick={onConfirmRemoveProperty}
        >
          Remove anyway
        </AlertDialog.Action>
        <AlertDialog.Cancel
          class="adlg-cancel"
          onclick={onCancelRemoveProperty}
        >
          Cancel
        </AlertDialog.Cancel>
      </div>
    </AlertDialog.Content>
  </AlertDialog.Root>
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

  .adlg-icon-row {
    display: flex;
    justify-content: center;
  }

  .adlg-warning-icon {
    font-size: 28px;
    color: var(--color-warning);
  }

  :global([data-slot="alert-dialog-content"] .adlg-title) {
    font-family: var(--font-body);
    font-size: 15px;
    font-weight: 700;
    color: var(--color-neutral-active);
    text-align: center;
    margin: 0;
  }

  :global([data-slot="alert-dialog-content"] .adlg-description) {
    font-family: var(--font-body);
    font-size: 13px;
    color: var(--color-neutral);
    line-height: 1.5;
    margin: 0;
    text-align: center;
  }

  .adlg-examples {
    font-family: var(--font-body);
    font-size: 11px;
    color: var(--color-neutral);
    margin: 0;
    font-style: italic;
    text-align: center;
  }

  .adlg-actions {
    display: flex;
    gap: 8px;
    justify-content: center;
    margin-top: 4px;
  }

  :global([data-slot="alert-dialog-content"] .adlg-proceed) {
    padding: 6px 14px;
    background: color-mix(in srgb, var(--color-danger) 20%, transparent);
    border: none;
    color: var(--color-danger);
    font-family: var(--font-body);
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
    border-radius: var(--radius);
    transition: background 0.15s;
  }

  :global([data-slot="alert-dialog-content"] .adlg-proceed:hover) {
    background: color-mix(in srgb, var(--color-danger) 30%, transparent);
  }

  :global([data-slot="alert-dialog-content"] .adlg-cancel) {
    padding: 6px 14px;
    background: color-mix(in srgb, var(--color-neutral) 12%, transparent);
    border: none;
    color: var(--color-neutral);
    font-family: var(--font-body);
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
    border-radius: var(--radius);
    transition: background 0.15s;
  }

  :global([data-slot="alert-dialog-content"] .adlg-cancel:hover) {
    background: color-mix(in srgb, var(--color-neutral) 22%, transparent);
  }

  .section-label-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
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

  :global([data-slot="button"].disjoint-remove) {
    color: color-mix(in srgb, var(--color-neutral) 60%, transparent);
  }

  :global([data-slot="button"].disjoint-remove:hover) {
    color: var(--color-error, #ef4444);
    background: color-mix(in srgb, var(--color-error, #ef4444) 12%, transparent);
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

</style>
