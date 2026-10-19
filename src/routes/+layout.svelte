<script lang="ts">
	import './layout.css';
	import { ModeWatcher } from 'mode-watcher';
	import { Toaster } from '$lib/components/ui/sonner';
	import * as Tooltip from '$lib/components/ui/tooltip';
	import WindowFrame from '$lib/components/app/window/WindowFrame.svelte';

	const { children } = $props();
</script>

<!-- No browser menu (Reload, Inspect...) in an app; text fields keep theirs for copy/paste. -->
<svelte:window
	oncontextmenu={(e) => {
		if (!(e.target as HTMLElement).closest('input, textarea, [contenteditable]')) e.preventDefault();
	}}
/>

<ModeWatcher />
<Tooltip.Provider delayDuration={400}>
	<WindowFrame>
		{@render children()}
	</WindowFrame>
</Tooltip.Provider>
<Toaster position="bottom-center" richColors closeButton offset={24} />
