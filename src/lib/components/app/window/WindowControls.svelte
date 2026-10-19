<script lang="ts">
	import { onMount } from 'svelte';
	import { getCurrentWindow } from '@tauri-apps/api/window';
	import { IconMinus, IconSquare, IconSquares, IconX } from '@tabler/icons-svelte';
	import { Button } from '$lib/components/ui/button';

	let maximized = $state(false);

	onMount(() => {
		const win = getCurrentWindow();
		const sync = async () => (maximized = await win.isMaximized());
		sync();
		const un = win.onResized(sync);
		return () => void un.then((f) => f());
	});

	const win = () => getCurrentWindow();
</script>

<div class="flex items-center gap-0.5">
	<Button variant="ghost" size="icon-sm" aria-label="Minimize" title="Minimize" onclick={() => win().minimize()}>
		<IconMinus />
	</Button>
	<Button
		variant="ghost"
		size="icon-sm"
		aria-label={maximized ? 'Restore' : 'Maximize'}
		title={maximized ? 'Restore' : 'Maximize'}
		onclick={() => win().toggleMaximize()}
	>
		{#if maximized}<IconSquares class="size-3.5" />{:else}<IconSquare class="size-3.5" />{/if}
	</Button>
	<Button
		variant="ghost"
		size="icon-sm"
		aria-label="Close"
		title="Close"
		class="hover:bg-destructive hover:text-white dark:hover:bg-destructive"
		onclick={() => win().close()}
	>
		<IconX />
	</Button>
</div>
