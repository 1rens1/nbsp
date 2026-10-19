<script lang="ts">
	import { Button } from '$lib/components/ui/button';
	import * as Dialog from '$lib/components/ui/dialog';
	import * as Field from '$lib/components/ui/field';
	import { Input } from '$lib/components/ui/input';
	import { printer } from '$lib/state/printer.svelte';
	import { settings } from '$lib/state/settings.svelte';
	import { fmtSize } from '$lib/units';
	import NumberInput from './NumberInput.svelte';

	let { kind = $bindable() }: { kind: 'preset' | 'paper' | null } = $props();

	/** The menu offers this only when the driver has a custom range. */
	const L = $derived(printer.custom ?? { minW: 54, maxW: 215.9, minH: 86, maxH: 1200 });
	let name = $state('');
	let w = $state(210);
	let h = $state(297);

	$effect(() => {
		if (kind) name = '';
	});

	/** Saving under an existing preset's name replaces it; the button says so. */
	const replaces = $derived(kind === 'preset' && settings.presets.some((p) => p.name === name.trim()));

	function submit(e: Event) {
		e.preventDefault();
		if (kind === 'preset') {
			if (!name.trim()) return;
			settings.savePreset(name.trim());
		} else if (kind === 'paper') {
			// "100×150 mm" when unnamed.
			const p = settings.addPaper(name.trim() || fmtSize(w, h, settings.unit).replace(' × ', '×'), w, h);
			settings.setPaper(p);
		}
		kind = null;
	}
</script>

<Dialog.Root open={kind !== null} onOpenChange={(o) => !o && (kind = null)}>
	<Dialog.Content class="sm:max-w-sm">
		<form onsubmit={submit} class="contents">
			<Dialog.Header>
				<Dialog.Title>{kind === 'preset' ? 'Save preset' : 'Custom paper size'}</Dialog.Title>
				<Dialog.Description class={kind === 'preset' ? 'sr-only' : ''}>
					{kind === 'preset' ? 'Preset name' : `${fmtSize(L.minW, L.minH, settings.unit)} to ${fmtSize(L.maxW, L.maxH, settings.unit)}`}
				</Dialog.Description>
			</Dialog.Header>
			<Field.Group class="gap-3">
				<Field.Field>
					<Field.Label for="prompt-name">Name</Field.Label>
					<Input id="prompt-name" bind:value={name} placeholder={kind === 'preset' ? 'e.g. Handouts 2-up' : 'Optional'} autofocus />
				</Field.Field>
				{#if kind === 'paper'}
					<div class="grid grid-cols-1 gap-3">
						<Field.Field>
							<Field.Label for="pw">Width</Field.Label>
							<NumberInput id="pw" bind:value={w} min={L.minW} max={L.maxW} step={1} length class="w-full" />
						</Field.Field>
						<Field.Field>
							<Field.Label for="ph">Height</Field.Label>
							<NumberInput id="ph" bind:value={h} min={L.minH} max={L.maxH} step={1} length class="w-full" />
						</Field.Field>
					</div>
				{/if}
			</Field.Group>
			<Dialog.Footer>
				<Button variant="outline" type="button" onclick={() => (kind = null)}>Cancel</Button>
				<Button type="submit" disabled={kind === 'preset' && !name.trim()}>{replaces ? 'Replace' : 'Save'}</Button>
			</Dialog.Footer>
		</form>
	</Dialog.Content>
</Dialog.Root>
