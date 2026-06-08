import { Popover as PopoverPrimitive } from "bits-ui";

import Content from "./popover-content.svelte";

const Root = PopoverPrimitive.Root;
const Trigger = PopoverPrimitive.Trigger;
const Close = PopoverPrimitive.Close;

export {
	Root,
	Trigger,
	Close,
	Content,
	Root as Popover,
	Trigger as PopoverTrigger,
	Close as PopoverClose,
	Content as PopoverContent,
};
