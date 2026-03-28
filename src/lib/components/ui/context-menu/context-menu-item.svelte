<script lang="ts">
	import { cn } from "$lib/utils.js";
	import { ContextMenu as ContextMenuPrimitive } from "bits-ui";

	let {
		ref = $bindable(null),
		class: className,
		inset,
		...restProps
	}: ContextMenuPrimitive.ItemProps & { inset?: boolean } = $props();
</script>

<ContextMenuPrimitive.Item
	bind:ref
	data-slot="context-menu-item"
	data-inset={inset}
	class={cn(
		"focus:bg-accent focus:text-accent-foreground gap-2 rounded-sm px-2 py-1.5 text-sm data-inset:pl-8 [&_svg:not([class*='size-'])]:size-4 group/context-menu-item relative flex cursor-default items-center outline-hidden select-none data-disabled:pointer-events-none data-disabled:opacity-50 [&_svg]:pointer-events-none [&_svg]:shrink-0",
		className
	)}
	{...restProps}
/>
