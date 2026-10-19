<script lang="ts">
	import { IconInfoCircle } from '@tabler/icons-svelte';
	import * as Tooltip from '$lib/components/ui/tooltip';

	let { text }: { text: string } = $props();
</script>

<Tooltip.Root>
	<Tooltip.Trigger class="inline-flex text-muted-foreground hover:text-foreground" aria-label={text}>
		<IconInfoCircle class="size-3.5" />
	</Tooltip.Trigger>
	<Tooltip.Content class="max-w-60">{text}</Tooltip.Content>
</Tooltip.Root>
