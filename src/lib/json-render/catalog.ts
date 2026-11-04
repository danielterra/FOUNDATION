import { defineCatalog } from '@json-render/core';
import { schema } from '@json-render/svelte/schema';
import { z } from 'zod';

const childrenSlot = ['default'];
const noProps = z.object({});
const textProps = z.object({ text: z.string() });
const containerProps = z.object({ class: z.string().optional(), id: z.string().optional() });
const fieldProps = z.object({
  id: z.string().optional(),
  name: z.string().optional(),
  label: z.string().optional(),
  placeholder: z.string().optional(),
  value: z.string().optional(),
  disabled: z.boolean().optional(),
  required: z.boolean().optional(),
  type: z.enum(['text', 'email', 'number', 'search', 'url', 'tel']).optional(),
});

export const componentCatalog = defineCatalog(schema, {
  components: {
    Text: { props: textProps, slots: [], description: 'Texto simples, quebrado em linhas conforme necessário.' },
    Button: { props: z.object({ label: z.string(), variant: z.enum(['default', 'destructive', 'outline', 'secondary', 'ghost', 'link']).optional(), size: z.enum(['default', 'sm', 'lg', 'icon', 'icon-sm', 'icon-lg', 'icon-xs', 'icon-touch']).optional(), disabled: z.boolean().optional() }), slots: [], description: 'Botão local. Evento permitido: press.' },
    Input: { props: fieldProps, slots: [], description: 'Campo de entrada de texto.' },
    Textarea: { props: fieldProps.extend({ rows: z.number().int().min(1).max(20).optional() }), slots: [], description: 'Campo de texto multilinha.' },
    Label: { props: textProps.extend({ for: z.string().optional() }), slots: [], description: 'Rótulo acessível de formulário.' },
    Checkbox: { props: z.object({ label: z.string(), checked: z.boolean().optional(), disabled: z.boolean().optional() }), slots: [], description: 'Controle de seleção booleana.' },
    Badge: { props: z.object({ text: z.string(), variant: z.enum(['default', 'secondary', 'destructive', 'outline']).optional() }), slots: [], description: 'Indicador curto de estado.' },
    Card: { props: containerProps, slots: childrenSlot, description: 'Contêiner visual de conteúdo.' },
    CardHeader: { props: containerProps, slots: childrenSlot, description: 'Cabeçalho de cartão.' },
    CardTitle: { props: textProps, slots: [], description: 'Título de cartão.' },
    CardDescription: { props: textProps, slots: [], description: 'Descrição de cartão.' },
    CardAction: { props: containerProps, slots: childrenSlot, description: 'Ação alinhada ao cabeçalho de cartão.' },
    CardContent: { props: containerProps, slots: childrenSlot, description: 'Conteúdo de cartão.' },
    CardFooter: { props: containerProps, slots: childrenSlot, description: 'Rodapé de cartão.' },
    Collapsible: { props: z.object({ open: z.boolean().optional(), disabled: z.boolean().optional() }), slots: childrenSlot, description: 'Raiz de uma seção recolhível.' },
    CollapsibleTrigger: { props: containerProps, slots: childrenSlot, description: 'Controle para abrir ou recolher uma seção.' },
    CollapsibleContent: { props: containerProps, slots: childrenSlot, description: 'Conteúdo de uma seção recolhível.' },
    Dialog: { props: z.object({ open: z.boolean().optional() }), slots: childrenSlot, description: 'Raiz de diálogo modal com estado definido pela spec.' },
    DialogTrigger: { props: containerProps, slots: childrenSlot, description: 'Gatilho para abrir um diálogo.' },
    DialogPortal: { props: noProps, slots: childrenSlot, description: 'Portal do diálogo.' },
    DialogOverlay: { props: noProps, slots: [], description: 'Sobreposição de diálogo.' },
    DialogContent: { props: z.object({ showCloseButton: z.boolean().optional() }), slots: childrenSlot, description: 'Conteúdo modal de diálogo.' },
    DialogHeader: { props: containerProps, slots: childrenSlot, description: 'Cabeçalho de diálogo.' },
    DialogTitle: { props: textProps, slots: [], description: 'Título acessível de diálogo.' },
    DialogDescription: { props: textProps, slots: [], description: 'Descrição acessível de diálogo.' },
    DialogFooter: { props: containerProps, slots: childrenSlot, description: 'Rodapé de diálogo.' },
    DialogClose: { props: containerProps, slots: childrenSlot, description: 'Controle para fechar o diálogo.' },
    Popover: { props: z.object({ open: z.boolean().optional() }), slots: childrenSlot, description: 'Raiz de popover.' },
    PopoverTrigger: { props: containerProps, slots: childrenSlot, description: 'Gatilho de popover.' },
    PopoverClose: { props: containerProps, slots: childrenSlot, description: 'Controle para fechar popover.' },
    PopoverContent: { props: z.object({ side: z.enum(['top', 'right', 'bottom', 'left']).optional(), align: z.enum(['start', 'center', 'end']).optional() }), slots: childrenSlot, description: 'Conteúdo de popover.' },
    Select: { props: z.object({ value: z.string().optional(), placeholder: z.string().optional() }), slots: childrenSlot, description: 'Raiz de seleção de opção.' },
    SelectGroup: { props: noProps, slots: childrenSlot, description: 'Grupo de opções.' },
    SelectValue: { props: z.object({ placeholder: z.string().optional() }), slots: [], description: 'Valor visível do seletor.' },
    SelectTrigger: { props: containerProps, slots: childrenSlot, description: 'Gatilho do seletor.' },
    SelectContent: { props: noProps, slots: childrenSlot, description: 'Lista de opções.' },
    SelectItem: { props: z.object({ value: z.string(), label: z.string(), disabled: z.boolean().optional() }), slots: [], description: 'Opção do seletor.' },
    SelectLabel: { props: textProps, slots: [], description: 'Rótulo de grupo de opções.' },
    SelectSeparator: { props: noProps, slots: [], description: 'Separador de opções.' },
    RadioGroup: { props: z.object({ value: z.string().optional(), name: z.string().optional(), disabled: z.boolean().optional() }), slots: childrenSlot, description: 'Grupo de seleção única.' },
    RadioGroupItem: { props: z.object({ value: z.string(), disabled: z.boolean().optional(), label: z.string().optional() }), slots: [], description: 'Opção de seleção única.' },
    ToggleGroup: { props: z.object({ value: z.union([z.string(), z.array(z.string())]).optional(), type: z.enum(['single', 'multiple']).optional() }), slots: childrenSlot, description: 'Grupo de alternância.' },
    ToggleGroupItem: { props: z.object({ value: z.string(), label: z.string(), disabled: z.boolean().optional() }), slots: [], description: 'Opção de alternância.' },
    AlertDialog: { props: z.object({ open: z.boolean().optional() }), slots: childrenSlot, description: 'Raiz de diálogo de alerta.' },
    AlertDialogTrigger: { props: containerProps, slots: childrenSlot, description: 'Gatilho de alerta.' },
    AlertDialogPortal: { props: noProps, slots: childrenSlot, description: 'Portal de alerta.' },
    AlertDialogOverlay: { props: noProps, slots: [], description: 'Sobreposição de alerta.' },
    AlertDialogContent: { props: noProps, slots: childrenSlot, description: 'Conteúdo do alerta.' },
    AlertDialogTitle: { props: textProps, slots: [], description: 'Título acessível do alerta.' },
    AlertDialogDescription: { props: textProps, slots: [], description: 'Descrição acessível do alerta.' },
    AlertDialogAction: { props: containerProps, slots: childrenSlot, description: 'Ação do alerta.' },
    AlertDialogCancel: { props: containerProps, slots: childrenSlot, description: 'Cancelamento do alerta.' },
    Tooltip: { props: z.object({ open: z.boolean().optional() }), slots: childrenSlot, description: 'Raiz de dica contextual.' },
    TooltipProvider: { props: noProps, slots: childrenSlot, description: 'Provedor de dicas.' },
    TooltipTrigger: { props: containerProps, slots: childrenSlot, description: 'Gatilho de dica.' },
    TooltipContent: { props: z.object({ text: z.string().optional(), side: z.enum(['top', 'right', 'bottom', 'left']).optional() }), slots: childrenSlot, description: 'Conteúdo da dica.' },
  },
  actions: {
    announce: { params: z.object({ message: z.string().max(500) }), description: 'Mostra uma notificação local no widget; não executa operações externas.' },
  },
});

export const componentCatalogManifest = {
  version: 1,
  source: 'src/lib/json-render/catalog.ts',
  actionIds: ['announce'],
  components: Object.entries(componentCatalog.data.components).map(([id, definition]) => ({
    id,
    description: definition.description,
    propsSchema: definition.props.toJSONSchema(),
    slots: definition.slots,
  })),
} as const;
