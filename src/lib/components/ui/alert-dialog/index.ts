import { AlertDialog as AlertDialogPrimitive } from "bits-ui";

import Content from "./alert-dialog-content.svelte";
import Overlay from "./alert-dialog-overlay.svelte";

const Root = AlertDialogPrimitive.Root;
const Trigger = AlertDialogPrimitive.Trigger;
const Title = AlertDialogPrimitive.Title;
const Description = AlertDialogPrimitive.Description;
const Action = AlertDialogPrimitive.Action;
const Cancel = AlertDialogPrimitive.Cancel;
const Portal = AlertDialogPrimitive.Portal;

export {
	Root,
	Trigger,
	Title,
	Description,
	Action,
	Cancel,
	Portal,
	Content,
	Overlay,
	Root as AlertDialog,
	Content as AlertDialogContent,
	Overlay as AlertDialogOverlay,
	Trigger as AlertDialogTrigger,
	Title as AlertDialogTitle,
	Description as AlertDialogDescription,
	Action as AlertDialogAction,
	Cancel as AlertDialogCancel,
	Portal as AlertDialogPortal,
};
