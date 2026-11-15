<script>
  import ClassPropertyForm from './ClassPropertyForm.svelte';
  import DisjointSelect from './DisjointSelect.svelte';
  import ProcessSelect from './ProcessSelect.svelte';
  import OntologyEntityReferenceChip from './OntologyEntityReferenceChip.svelte';
  import * as AlertDialog from '$lib/components/ui/alert-dialog';
  import { Button } from '$lib/components/ui/button';

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

  function openEntityReference(entityIri) {
    if (entityIri) openEntityInspector(entityIri);
  }
</script>

{#if entityData.superClasses?.length > 0}
  <div class="section-group">
    <div class="semantic-label">Parent Classes</div>
    <div class="item-list">
      {#each entityData.superClasses as superClass}
        <OntologyEntityReferenceChip
          kind="class"
          label={superClass.label}
          icon={superClass.icon}
          entityIri={superClass.iri}
          onopen={openEntityReference}
        />
      {/each}
    </div>
  </div>
{/if}

{#if entityData.subClasses?.length > 0}
  <div class="section-group">
    <div class="semantic-label">Child Classes</div>
    <div class="item-list">
      {#each entityData.subClasses as subClass}
        <OntologyEntityReferenceChip
          kind="class"
          label={subClass.label}
          icon={subClass.icon}
          entityIri={subClass.iri}
          onopen={openEntityReference}
        />
      {/each}
    </div>
  </div>
{/if}

{#if entityData.isClass && ((entityData.relatedProcesses?.length ?? 0) > 0 || (!isLocked && onAddRelatedProcess))}
  <div class="section-group">
    <div class="semantic-label section-label-row">
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
          <OntologyEntityReferenceChip
            kind="individual"
            label={proc.label}
            icon={proc.type === 'foundation:Automation' ? 'bolt' : 'schema'}
            entityIri={proc.iri}
            onopen={openEntityReference}
          />
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
    <div class="semantic-label section-label-row">
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
          <OntologyEntityReferenceChip
            kind="individual"
            label={auto.label}
            icon="bolt"
            entityIri={auto.iri}
            onopen={openEntityReference}
          />
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
    <div class="semantic-label section-label-row">
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
            <OntologyEntityReferenceChip
              kind="class"
              label={m.label ?? m.iri}
              icon={m.icon}
              entityIri={m.iri}
              onopen={openEntityReference}
            />
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
              <span class="semantic-label disjoint-set-label">Conjunto disjunto</span>
              {#if !isLocked && onRequestRemoveDisjoint}
                <Button variant="ghost" size="icon-sm" class="disjoint-remove" onclick={() => onRequestRemoveDisjoint(group)} title="Remover conjunto">
                  <span class="material-symbols-outlined">close</span>
                </Button>
              {/if}
            </div>
            <div class="disjoint-set-members">
              {#each group.members as m (m.iri)}
                <OntologyEntityReferenceChip
                  kind="class"
                  label={m.label ?? m.iri}
                  icon={m.icon}
                  entityIri={m.iri}
                  onopen={openEntityReference}
                />
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
    <div class="semantic-label">Allowed Statuses</div>
    <div class="item-list">
      {#each entityData.allowedStatuses as status}
          <OntologyEntityReferenceChip
            kind="individual"
            label={status.label}
            icon={status.icon || 'radio_button_checked'}
            entityIri={status.iri}
            onopen={openEntityReference}
            statusColor={status.color || 'var(--color-neutral)'}
          />
      {/each}
    </div>
  </div>
{/if}

{#if entityData.isClass}
  <div class="class-props-header">
    <span class="semantic-label">Properties</span>
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
      <OntologyEntityReferenceChip
        kind="individual"
        label={instance.label}
        icon={instance.icon}
        entityIri={instance.iri}
        onopen={openEntityReference}
      />
    {/each}
  </div>
{/if}

<style>
  .section-group {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-label-to-value);
  }

  .item-list {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-inline-tight);
    margin-bottom: var(--spacing-section);
  }

  .item {
    display: flex;
    align-items: center;
    gap: var(--spacing-inline-related);
    padding: var(--spacing-control-internal) var(--spacing-widget-inset);
  }

  .item .material-symbols-outlined {
    font-size: var(--type-icon-medium);
    color: var(--color-interactive);
  }

  .class-props-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding-block: var(--spacing-inline-tight) var(--spacing-control-internal);
    margin-top: var(--spacing-control-internal);
  }

  .adlg-icon-row {
    display: flex;
    justify-content: center;
  }

  .adlg-warning-icon {
    font-size: var(--type-display);
    color: var(--color-warning);
  }

  :global([data-slot="alert-dialog-content"] .adlg-title) {
    font-family: var(--font-body);
    font-size: var(--type-body-compact);
    font-weight: 700;
    color: var(--color-neutral-active);
    text-align: center;
    margin: 0;
  }

  :global([data-slot="alert-dialog-content"] .adlg-description) {
    font-family: var(--font-body);
    font-size: var(--type-body-compact);
    color: var(--color-neutral);
    line-height: 1.5;
    margin: 0;
    text-align: center;
  }

  .adlg-examples {
    font-family: var(--font-body);
    font-size: var(--type-caption-compact);
    color: var(--color-neutral);
    margin: 0;
    font-style: italic;
    text-align: center;
  }

  .adlg-actions {
    display: flex;
    gap: var(--spacing-control-internal);
    justify-content: center;
    margin-top: var(--spacing-inline-tight);
  }

  :global([data-slot="alert-dialog-content"] .adlg-proceed) {
    padding: var(--spacing-inline-tight) var(--spacing-control-internal);
    background: color-mix(in srgb, var(--color-danger) 20%, transparent);
    border: none;
    color: var(--color-danger);
    font-family: var(--font-body);
    font-size: var(--type-body-compact);
    font-weight: 600;
    cursor: pointer;
    border-radius: var(--radius);
    transition: background-color var(--duration-setup-transition) ease;
  }

  :global([data-slot="alert-dialog-content"] .adlg-proceed:hover) {
    background: color-mix(in srgb, var(--color-danger) 30%, transparent);
  }

  :global([data-slot="alert-dialog-content"] .adlg-cancel) {
    padding: var(--spacing-inline-tight) var(--spacing-control-internal);
    background: color-mix(in srgb, var(--color-neutral) 12%, transparent);
    border: none;
    color: var(--color-neutral);
    font-family: var(--font-body);
    font-size: var(--type-body-compact);
    font-weight: 600;
    cursor: pointer;
    border-radius: var(--radius);
    transition: background-color var(--duration-setup-transition) ease;
  }

  :global([data-slot="alert-dialog-content"] .adlg-cancel:hover) {
    background: color-mix(in srgb, var(--color-neutral) 22%, transparent);
  }

  .section-label-row {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
  }

  .disjoint-item {
    justify-content: space-between;
    padding: 0;
  }

  :global([data-slot="button"].disjoint-remove) {
    color: color-mix(in srgb, var(--color-neutral) 60%, transparent);
  }

  :global([data-slot="button"].disjoint-remove:hover) {
    color: var(--color-error);
    background: color-mix(in srgb, var(--color-error) 12%, transparent);
  }

  .disjoint-set {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-inline-tight);
    padding: var(--spacing-control-internal) var(--spacing-widget-inset);
    background: var(--color-surface-2);
    margin-bottom: var(--spacing-control-internal);
  }

  .disjoint-set-header {
    display: flex;
    align-items: center;
    gap: var(--spacing-inline-related);
  }

  .disjoint-set-header .material-symbols-outlined {
    font-size: var(--type-icon-small);
    color: var(--color-neutral-active);
  }

  .disjoint-set-label {
    flex: 1;
  }

  .disjoint-set-members {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-inline-tight);
    padding-left: var(--spacing-inline-tight);
  }

  .disjoint-error {
    display: flex;
    align-items: flex-start;
    gap: var(--spacing-inline-related);
    padding: var(--spacing-control-internal);
    background: color-mix(in srgb, var(--color-error) 10%, transparent);
    border-left: var(--space-1) solid var(--color-error);
    margin-bottom: var(--spacing-control-internal);
  }

  .disjoint-error .material-symbols-outlined {
    font-size: var(--type-icon-medium);
    color: var(--color-error);
    flex-shrink: 0;
  }

  .disjoint-error-msg {
    flex: 1;
    font-family: var(--font-body);
    font-size: var(--type-label);
    color: var(--color-neutral-active);
    line-height: var(--line-height-hud-compact);
  }

</style>
