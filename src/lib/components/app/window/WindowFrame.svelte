<script lang="ts">
	import { onMount, type Snippet } from 'svelte';
	import { getCurrentWindow } from '@tauri-apps/api/window';
	import { settings } from '$lib/state/settings.svelte';

	let { children }: { children: Snippet } = $props();

	let maximized = $state(false);

	// Thin grab strips on the window edges (the window is frameless, so the compositor adds none).
	const EDGES = [
		['North', 'top-0 inset-x-2 h-1 cursor-n-resize'],
		['South', 'bottom-0 inset-x-2 h-1 cursor-s-resize'],
		['West', 'left-0 inset-y-2 w-1 cursor-w-resize'],
		['East', 'right-0 inset-y-2 w-1 cursor-e-resize'],
		['NorthWest', 'top-0 left-0 size-2 cursor-nw-resize'],
		['NorthEast', 'top-0 right-0 size-2 cursor-ne-resize'],
		['SouthWest', 'bottom-0 left-0 size-2 cursor-sw-resize'],
		['SouthEast', 'bottom-0 right-0 size-2 cursor-se-resize']
	] as const;

	onMount(() => {
		const win = getCurrentWindow();
		const sync = async () => {
			maximized = (await win.isMaximized()) || (await win.isFullscreen());
			document.documentElement.toggleAttribute('data-maximized', maximized);
		};
		sync();
		const un = win.onResized(sync);
		return () => void un.then((f) => f());
	});
</script>

<div class="window-frame">
	{@render children()}
</div>

{#if !maximized && !settings.titleBarApplied}
	{#each EDGES as [dir, cls] (dir)}
		<div
			class="fixed z-[60] {cls}"
			role="presentation"
			onpointerdown={(e) => e.button === 0 && getCurrentWindow().startResizeDragging(dir)}
		></div>
	{/each}
{/if}
