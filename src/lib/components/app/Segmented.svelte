<script lang="ts" generics="T extends string | number">
	import * as ToggleGroup from '$lib/components/ui/toggle-group';
	import { cn } from '$lib/utils';

	let {
		value = $bindable(),
		options,
		disabled = false,
		class: className
	}: {
		value: T;
		options: { value: T; label: string; disabled?: boolean }[];
		disabled?: boolean;
		class?: string;
	} = $props();
</script>

<ToggleGroup.Root
	type="single"
	variant="outline"
	size="sm"
	spacing={0}
	{disabled}
	class={cn('w-full', className)}
	value={String(value)}
	onValueChange={(v) => {
		// Single-choice: clicking the active item must not clear it.
		const hit = options.find((o) => String(o.value) === v);
		if (hit) value = hit.value;
	}}
>
	{#each options as o (o.value)}
		<ToggleGroup.Item value={String(o.value)} disabled={o.disabled} class="min-w-0 flex-1 px-2" title={o.label}>
			<span class="truncate">{o.label}</span>
		</ToggleGroup.Item>
	{/each}
</ToggleGroup.Root>
