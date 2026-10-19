<script lang="ts">
	import type { Icon as TablerIcon } from '@tabler/icons-svelte';
	import { Button, type ButtonVariant, type ButtonSize } from '$lib/components/ui/button';
	import { Kbd } from '$lib/components/ui/kbd';
	import * as Tooltip from '$lib/components/ui/tooltip';

	let {
		icon: Icon,
		label,
		shortcut,
		variant = 'ghost',
		size = 'icon-sm',
		disabled = false,
		onclick
	}: {
		icon: TablerIcon;
		label: string;
		shortcut?: string;
		variant?: ButtonVariant;
		size?: ButtonSize;
		disabled?: boolean;
		onclick?: () => void;
	} = $props();
</script>

<Tooltip.Root>
	<Tooltip.Trigger>
		{#snippet child({ props })}
			<Button {...props} {variant} {size} {disabled} {onclick} aria-label={label}>
				<Icon />
			</Button>
		{/snippet}
	</Tooltip.Trigger>
	<Tooltip.Content>
		{label}
		{#if shortcut}<Kbd>{shortcut}</Kbd>{/if}
	</Tooltip.Content>
</Tooltip.Root>
