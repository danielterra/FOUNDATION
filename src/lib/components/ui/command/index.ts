import { Command as CommandPrimitive } from "bits-ui";

import Root from "./command.svelte";
import Input from "./command-input.svelte";
import List from "./command-list.svelte";
import Empty from "./command-empty.svelte";
import Group from "./command-group.svelte";
import Item from "./command-item.svelte";
import Separator from "./command-separator.svelte";

const GroupHeading = CommandPrimitive.GroupHeading;
const Loading = CommandPrimitive.Loading;

export {
	Root,
	Input,
	List,
	Empty,
	Group,
	GroupHeading,
	Item,
	Separator,
	Loading,
	Root as Command,
	Input as CommandInput,
	List as CommandList,
	Empty as CommandEmpty,
	Group as CommandGroup,
	GroupHeading as CommandGroupHeading,
	Item as CommandItem,
	Separator as CommandSeparator,
	Loading as CommandLoading,
};
