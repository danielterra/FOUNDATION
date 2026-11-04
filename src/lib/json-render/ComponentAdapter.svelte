<script lang="ts">
  import type { BaseComponentProps } from '@json-render/svelte';
  import * as ButtonUI from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { Textarea } from '$lib/components/ui/textarea';
  import { Label } from '$lib/components/ui/label';
  import { Checkbox } from '$lib/components/ui/checkbox';
  import { Badge } from '$lib/components/ui/badge';
  import * as CardUI from '$lib/components/ui/card';
  import * as CollapsibleUI from '$lib/components/ui/collapsible';
  import * as DialogUI from '$lib/components/ui/dialog';
  import * as PopoverUI from '$lib/components/ui/popover';
  import * as SelectUI from '$lib/components/ui/select';
  import * as RadioGroupUI from '$lib/components/ui/radio-group';
  import * as ToggleGroupUI from '$lib/components/ui/toggle-group';
  import * as AlertDialogUI from '$lib/components/ui/alert-dialog';
  import * as TooltipUI from '$lib/components/ui/tooltip';

  type RenderProps = BaseComponentProps<Record<string, unknown>> & { componentType: string };

  let { props: componentProps, componentType, children, emit, on }: RenderProps = $props();
  let controlledValue = $state<string | string[] | undefined>(undefined);
  let controlledOpen = $state(false);

  $effect(() => {
    controlledValue = typeof componentProps.value === 'string' || Array.isArray(componentProps.value)
      ? componentProps.value as string | string[]
      : undefined;
    controlledOpen = Boolean(componentProps.open);
  });
  const renderComponents: Record<string, any> = {
    Card: CardUI.Card,
    CardHeader: CardUI.CardHeader,
    CardTitle: CardUI.CardTitle,
    CardDescription: CardUI.CardDescription,
    CardAction: CardUI.CardAction,
    CardContent: CardUI.CardContent,
    CardFooter: CardUI.CardFooter,
    Collapsible: CollapsibleUI.Collapsible,
    CollapsibleTrigger: CollapsibleUI.CollapsibleTrigger,
    CollapsibleContent: CollapsibleUI.CollapsibleContent,
    Dialog: DialogUI.Dialog,
    DialogTrigger: DialogUI.DialogTrigger,
    DialogPortal: DialogUI.DialogPortal,
    DialogOverlay: DialogUI.DialogOverlay,
    DialogContent: DialogUI.DialogContent,
    DialogHeader: DialogUI.DialogHeader,
    DialogTitle: DialogUI.DialogTitle,
    DialogDescription: DialogUI.DialogDescription,
    DialogFooter: DialogUI.DialogFooter,
    DialogClose: DialogUI.DialogClose,
    Popover: PopoverUI.Popover,
    PopoverTrigger: PopoverUI.PopoverTrigger,
    PopoverClose: PopoverUI.PopoverClose,
    PopoverContent: PopoverUI.PopoverContent,
    Select: SelectUI.Select,
    SelectGroup: SelectUI.SelectGroup,
    SelectValue: SelectUI.SelectValue,
    SelectTrigger: SelectUI.SelectTrigger,
    SelectContent: SelectUI.SelectContent,
    SelectItem: SelectUI.SelectItem,
    SelectLabel: SelectUI.SelectLabel,
    SelectSeparator: SelectUI.SelectSeparator,
    RadioGroup: RadioGroupUI.RadioGroup,
    RadioGroupItem: RadioGroupUI.RadioGroupItem,
    ToggleGroup: ToggleGroupUI.ToggleGroup,
    ToggleGroupItem: ToggleGroupUI.ToggleGroupItem,
    AlertDialog: AlertDialogUI.AlertDialog,
    AlertDialogTrigger: AlertDialogUI.AlertDialogTrigger,
    AlertDialogPortal: AlertDialogUI.AlertDialogPortal,
    AlertDialogOverlay: AlertDialogUI.AlertDialogOverlay,
    AlertDialogContent: AlertDialogUI.AlertDialogContent,
    AlertDialogTitle: AlertDialogUI.AlertDialogTitle,
    AlertDialogDescription: AlertDialogUI.AlertDialogDescription,
    AlertDialogAction: AlertDialogUI.AlertDialogAction,
    AlertDialogCancel: AlertDialogUI.AlertDialogCancel,
    Tooltip: TooltipUI.Tooltip,
    TooltipProvider: TooltipUI.TooltipProvider,
    TooltipTrigger: TooltipUI.TooltipTrigger,
    TooltipContent: TooltipUI.TooltipContent,
  };

  let selectedComponent = $derived(renderComponents[componentType]);

  function handlePress(event: MouseEvent) {
    const action = on('press');
    if (!action.bound) return;
    if (action.shouldPreventDefault) event.preventDefault();
    action.emit();
  }

  function handleChange() {
    emit('change');
  }

  function handleValueChange(value: string | string[] | undefined) {
    controlledValue = value;
    handleChange();
  }

  function handleOpenChange(open: boolean) {
    controlledOpen = open;
  }
</script>

{#if componentType === 'Text'}
  <p class="rendered-text">{String(componentProps.text ?? '')}</p>
{:else if componentType === 'Button'}
  <ButtonUI.Button
    variant={componentProps.variant as ButtonUI.ButtonVariant | undefined}
    size={componentProps.size as ButtonUI.ButtonSize | undefined}
    disabled={Boolean(componentProps.disabled)}
    onclick={handlePress}
  >{componentProps.label}</ButtonUI.Button>
{:else if componentType === 'Input'}
  <Input
    id={componentProps.id as string | undefined}
    name={componentProps.name as string | undefined}
    placeholder={componentProps.placeholder as string | undefined}
    value={componentProps.value as string | undefined}
    type={componentProps.type as 'text' | 'email' | 'number' | 'search' | 'url' | 'tel' | undefined}
    disabled={Boolean(componentProps.disabled)}
    required={Boolean(componentProps.required)}
    oninput={handleChange}
  />
{:else if componentType === 'Textarea'}
  <Textarea
    id={componentProps.id as string | undefined}
    name={componentProps.name as string | undefined}
    placeholder={componentProps.placeholder as string | undefined}
    value={componentProps.value as string | undefined}
    rows={componentProps.rows as number | undefined}
    disabled={Boolean(componentProps.disabled)}
    required={Boolean(componentProps.required)}
    oninput={handleChange}
  />
{:else if componentType === 'Label'}
  <Label for={componentProps.for as string | undefined}>{componentProps.text}</Label>
{:else if componentType === 'Checkbox'}
  <label class="checkbox-field">
    <Checkbox checked={Boolean(componentProps.checked)} disabled={Boolean(componentProps.disabled)} onchange={handleChange} />
    <span>{componentProps.label}</span>
  </label>
{:else if componentType === 'Badge'}
  <Badge variant={componentProps.variant as 'default' | 'secondary' | 'destructive' | 'outline' | undefined}>{componentProps.text}</Badge>
{:else if componentType === 'Select'}
  <SelectUI.Select
    type="single"
    value={typeof controlledValue === 'string' ? controlledValue : undefined}
    onValueChange={handleValueChange}
    open={componentProps.open as boolean | undefined}
    onOpenChange={handleOpenChange}
  >{@render children?.()}</SelectUI.Select>
{:else if componentType === 'RadioGroup'}
  <RadioGroupUI.RadioGroup
    value={typeof controlledValue === 'string' ? controlledValue : undefined}
    onValueChange={handleValueChange}
    name={componentProps.name as string | undefined}
    disabled={Boolean(componentProps.disabled)}
  >{@render children?.()}</RadioGroupUI.RadioGroup>
{:else if componentType === 'ToggleGroup'}
  {#if componentProps.type === 'multiple'}
    <ToggleGroupUI.ToggleGroup type="multiple" value={Array.isArray(controlledValue) ? controlledValue : []} onValueChange={(value) => handleValueChange(value)}>{@render children?.()}</ToggleGroupUI.ToggleGroup>
  {:else}
    <ToggleGroupUI.ToggleGroup type="single" value={typeof controlledValue === 'string' ? controlledValue : ''} onValueChange={handleValueChange}>{@render children?.()}</ToggleGroupUI.ToggleGroup>
  {/if}
{:else if componentType === 'SelectItem'}
  <SelectUI.SelectItem value={String(componentProps.value)} disabled={Boolean(componentProps.disabled)}>{String(componentProps.label)}</SelectUI.SelectItem>
{:else if componentType === 'SelectLabel' || componentType === 'CardTitle' || componentType === 'CardDescription' || componentType === 'DialogTitle' || componentType === 'DialogDescription' || componentType === 'AlertDialogTitle' || componentType === 'AlertDialogDescription'}
  {@const Component = selectedComponent}
  <Component>{componentProps.text}</Component>
{:else if componentType === 'Dialog'}
  <DialogUI.Dialog bind:open={controlledOpen} onOpenChange={handleOpenChange}>{@render children?.()}</DialogUI.Dialog>
{:else if componentType === 'Popover'}
  <PopoverUI.Popover bind:open={controlledOpen} onOpenChange={handleOpenChange}>{@render children?.()}</PopoverUI.Popover>
{:else if componentType === 'AlertDialog'}
  <AlertDialogUI.AlertDialog bind:open={controlledOpen} onOpenChange={handleOpenChange}>{@render children?.()}</AlertDialogUI.AlertDialog>
{:else if componentType === 'DialogTrigger' || componentType === 'DialogClose' || componentType === 'PopoverTrigger' || componentType === 'PopoverClose' || componentType === 'AlertDialogTrigger' || componentType === 'AlertDialogAction' || componentType === 'AlertDialogCancel' || componentType === 'CollapsibleTrigger'}
  {@const Component = selectedComponent}
  <Component {...componentProps} onclick={handlePress}>{@render children?.()}</Component>
{:else if componentType === 'Collapsible'}
  <CollapsibleUI.Collapsible bind:open={controlledOpen} onOpenChange={handleOpenChange} disabled={Boolean(componentProps.disabled)}>{@render children?.()}</CollapsibleUI.Collapsible>
{:else if selectedComponent}
  {@const Component = selectedComponent}
  <Component {...componentProps}>
    {@render children?.()}
  </Component>
{:else}
  <span class="unsupported" role="status">Componente indisponível</span>
{/if}

<style>
  .rendered-text {
    max-width: 100%;
    overflow-wrap: anywhere;
    white-space: pre-wrap;
  }

  .checkbox-field {
    display: inline-flex;
    align-items: center;
    gap: var(--spacing-control-internal);
    color: var(--color-neutral-text);
  }

  .unsupported {
    color: var(--color-error);
  }
</style>
