<script lang="ts">
	import { cn } from "$lib/utils.js";
	import { ContextMenu as ContextMenuPrimitive } from "bits-ui";

	let {
		ref = $bindable(null),
		class: className,
		...restProps
	}: ContextMenuPrimitive.ContentProps = $props();
</script>

<ContextMenuPrimitive.Portal>
	<ContextMenuPrimitive.Content
		bind:ref
		data-slot="context-menu-content"
		class={cn(
			"data-open:animate-in data-closed:animate-out data-closed:fade-out-0 data-open:fade-in-0 data-closed:zoom-out-95 data-open:zoom-in-95 ring-foreground/10 bg-popover text-popover-foreground min-w-44 rounded-md p-1 shadow-md ring-1 duration-100 z-50 overflow-x-hidden overflow-y-auto outline-none data-closed:overflow-hidden",
			className
		)}
		{...restProps}
	/>
</ContextMenuPrimitive.Portal>
