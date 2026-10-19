<script lang="ts">
	import { untrack } from 'svelte';
	import { listen } from '@tauri-apps/api/event';
	import { IconPhotoScan, IconRefresh, IconScan } from '@tabler/icons-svelte';
	import { toast } from 'svelte-sonner';
	import { Button } from '$lib/components/ui/button';
	import * as Dialog from '$lib/components/ui/dialog';
	import * as Field from '$lib/components/ui/field';
	import * as Select from '$lib/components/ui/select';
	import { Skeleton } from '$lib/components/ui/skeleton';
	import { Slider } from '$lib/components/ui/slider';
	import { Spinner } from '$lib/components/ui/spinner';
	import { fmtSize } from '$lib/units';
	import { settings } from '$lib/state/settings.svelte';
	import { api, pageUrl, type Scanner, type ScanOptions } from '$lib/api';
	import { job } from '$lib/state/job.svelte';
	import { cn } from '$lib/utils';
	import Segmented from './Segmented.svelte';

	let { open = $bindable(false) }: { open: boolean } = $props();

	/** Common sizes; [label, w, h] in mm. */
	const SIZES: Record<string, [string, number, number]> = {
		A4: ['A4', 210, 297],
		Letter: ['Letter', 215.9, 279.4],
		B5: ['B5', 182, 257],
		A5: ['A5', 148, 210],
		A5L: ['A5 landscape', 210, 148],
		B6: ['B6', 128, 182],
		A6: ['A6', 105, 148],
		Postcard: ['Postcard', 100, 148],
		R3: ['3R photo', 89, 127],
		R4: ['4R photo', 101.6, 152.4],
		R5: ['5R photo', 127, 177.8],
		R8: ['8R photo', 203.2, 254],
		Card: ['ID / bank card', 85.6, 54]
	};
	const DPIS = [75, 100, 150, 200, 300, 400, 600, 1200];

	let scanners = $state<Scanner[]>([]);
	let device = $state('');
	/** The scanner's glass, mm (Letter by A4 until it says). */
	let bed = $state({ w: 215.9, h: 297.18 });
	/** Sizes that fit the glass. */
	const AREAS = $derived<Record<string, [string, number, number]>>({
		...Object.fromEntries(Object.entries(SIZES).filter(([, [, w, h]]) => w <= bed.w + 0.5 && h <= bed.h + 0.5)),
		Max: ['Whole glass', bed.w, bed.h]
	});

	$effect(() => {
		if (!device) return;
		const d = device;
		api.scannerGlass(d).then(([w, h]) => {
			if (d !== device) return;
			bed = { w, h };
			// Keep the area on the glass.
			const aw = Math.min(area.w, w);
			const ah = Math.min(area.h, h);
			area = { x: Math.min(area.x, w - aw), y: Math.min(area.y, h - ah), w: aw, h: ah };
		});
	});
	let mode = $state('Color');
	let dpi = $state(300);
	let preset = $state('A4');
	/** Scan area in mm: x, y, w, h. Presets sit in the top-left corner (the glass's reference corner). */
	let area = $state({ x: 0, y: 0, w: 210, h: 297 });
	let brightness = $state(0);
	let contrast = $state(0);
	let busy = $state<'' | 'scan' | 'preview'>('');
	let progress = $state(0);
	let scanned = $state<number[]>([]);
	let searching = $state(false);
	let previewUrl = $state('');

	const sizeMB = $derived(((area.w / 25.4) * dpi * ((area.h / 25.4) * dpi) * (mode === 'Color' ? 3 : mode === 'Grayscale' ? 1 : 1 / 8)) / 1e6);

	// Looks for scanners once per opening (Retry asks again); re-running whenever the search
	// finished empty used to call scanimage in a tight loop.
	$effect(() => {
		if (!open) {
			scanned = [];
			return;
		}
		untrack(() => {
			if (!scanners.length && !searching) find();
		});
	});

	$effect(() => {
		const un = listen<number>('scan-progress', (e) => (progress = e.payload));
		return () => void un.then((f) => f());
	});

	function pick(id: string) {
		preset = id;
		const [, w, h] = AREAS[id];
		area = { x: 0, y: 0, w, h };
	}

	async function find() {
		searching = true;
		try {
			scanners = await api.scanners();
			device ||= scanners[0]?.id ?? '';
		} catch (e) {
			toast.error(String(e));
		} finally {
			searching = false;
		}
	}

	const options = (): ScanOptions => ({ device, mode, dpi, area: [area.x, area.y, area.w, area.h], brightness, contrast });

	async function preview() {
		busy = 'preview';
		progress = 0;
		try {
			const bytes = await api.scanPreview(options());
			if (previewUrl) URL.revokeObjectURL(previewUrl);
			previewUrl = URL.createObjectURL(new Blob([bytes], { type: 'image/jpeg' }));
		} catch (e) {
			if (String(e) !== 'cancelled') toast.error('Preview failed', { description: String(e) });
		} finally {
			busy = '';
		}
	}

	async function scan() {
		busy = 'scan';
		progress = 0;
		try {
			const s = await api.scan(options());
			job.addSource(s);
			scanned = [...scanned, s.id];
		} catch (e) {
			if (String(e) !== 'cancelled') toast.error('Scan failed', { description: String(e) });
		} finally {
			busy = '';
		}
	}

	// Scan area editing on the preview: drag inside to move, handles to resize, elsewhere to draw anew.
	type Handle = 'n' | 's' | 'e' | 'w' | 'ne' | 'nw' | 'se' | 'sw';
	const HANDLES: [Handle, string][] = [
		['nw', 'left-0 top-0 cursor-nwse-resize'],
		['n', 'left-1/2 top-0 cursor-ns-resize'],
		['ne', 'left-full top-0 cursor-nesw-resize'],
		['e', 'left-full top-1/2 cursor-ew-resize'],
		['se', 'left-full top-full cursor-nwse-resize'],
		['s', 'left-1/2 top-full cursor-ns-resize'],
		['sw', 'left-0 top-full cursor-nesw-resize'],
		['w', 'left-0 top-1/2 cursor-ew-resize']
	];
	const MIN = 5;
	let glass = $state<HTMLElement>();
	let drag: { mode: 'new' | 'move' | Handle; from: { x: number; y: number }; orig: typeof area } | null = null;

	function toMm(e: PointerEvent) {
		const r = glass!.getBoundingClientRect();
		return {
			x: Math.min(bed.w, Math.max(0, ((e.clientX - r.left) / r.width) * bed.w)),
			y: Math.min(bed.h, Math.max(0, ((e.clientY - r.top) / r.height) * bed.h))
		};
	}
	function start(e: PointerEvent, mode: 'new' | 'move' | Handle) {
		if (busy || e.button !== 0) return;
		e.stopPropagation();
		glass!.setPointerCapture(e.pointerId);
		drag = { mode, from: toMm(e), orig: { ...area } };
	}
	function onpointermove(e: PointerEvent) {
		if (!drag) return;
		const p = toMm(e);
		const { mode, from, orig: o } = drag;
		if (mode === 'new') {
			const w = Math.abs(p.x - from.x);
			const h = Math.abs(p.y - from.y);
			if (w < MIN || h < MIN) return;
			preset = '';
			area = { x: Math.min(p.x, from.x), y: Math.min(p.y, from.y), w, h };
			return;
		}
		const dx = p.x - from.x;
		const dy = p.y - from.y;
		if (mode === 'move') {
			// Same size, so a preset still describes it.
			area = {
				...o,
				x: Math.min(bed.w - o.w, Math.max(0, o.x + dx)),
				y: Math.min(bed.h - o.h, Math.max(0, o.y + dy))
			};
			return;
		}
		let { x, y, w, h } = o;
		if (mode.includes('w')) {
			x = Math.min(o.x + o.w - MIN, Math.max(0, o.x + dx));
			w = o.x + o.w - x;
		}
		if (mode.includes('e')) w = Math.min(bed.w - o.x, Math.max(MIN, o.w + dx));
		if (mode.includes('n')) {
			y = Math.min(o.y + o.h - MIN, Math.max(0, o.y + dy));
			h = o.y + o.h - y;
		}
		if (mode.includes('s')) h = Math.min(bed.h - o.y, Math.max(MIN, o.h + dy));
		preset = '';
		area = { x, y, w, h };
	}
	const pct = (v: number, of: number) => `${(v / of) * 100}%`;
</script>

<Dialog.Root bind:open>
	<Dialog.Content class="flex max-h-[calc(100dvh-4rem)] flex-col sm:max-w-3xl">
		<Dialog.Header>
			<Dialog.Title>Scan</Dialog.Title>
			<Dialog.Description class="sr-only">Scanned pages are added to the job</Dialog.Description>
		</Dialog.Header>

		<div class="grid min-h-0 flex-1 grid-cols-1 gap-6 overflow-y-auto sm:grid-cols-[1fr_auto]">
			<Field.Group class="gap-4">
				<Field.Field class="gap-2">
					<Field.Label>Scanner</Field.Label>
					{#if searching}
						<Skeleton class="h-8 w-full rounded-2xl" />
					{:else if !scanners.length}
						<div class="flex h-8 items-center justify-between gap-2 text-sm text-muted-foreground">
							No scanner found. Is it on and connected?
							<Button variant="outline" size="sm" onclick={find}><IconRefresh data-icon="inline-start" /> Retry</Button>
						</div>
					{:else}
						<Select.Root type="single" bind:value={device} disabled={!!busy || !scanners.length}>
							<Select.Trigger class="w-full min-w-0">
								<span class="truncate">{scanners.find((s) => s.id === device)?.name ?? 'Choose a scanner'}</span>
							</Select.Trigger>
							<Select.Content>
								<Select.Group>
									{#each scanners as s (s.id)}
										<Select.Item value={s.id} label={s.name}>{s.name}</Select.Item>
									{/each}
								</Select.Group>
							</Select.Content>
						</Select.Root>
					{/if}
				</Field.Field>
				<Field.Field class="gap-2">
					<Field.Label>Mode</Field.Label>
					<Segmented
						bind:value={mode}
						disabled={!!busy}
						options={[
							{ value: 'Color', label: 'Color' },
							{ value: 'Grayscale', label: 'Grayscale' },
							{ value: 'Monochrome', label: 'Black & White' }
						]}
					/>
				</Field.Field>
				<div class="grid grid-cols-2 gap-3">
					<Field.Field class="gap-2">
						<Field.Label>Resolution</Field.Label>
						<Select.Root type="single" value={String(dpi)} onValueChange={(v) => (dpi = +v)} disabled={!!busy}>
							<Select.Trigger class="w-full">{dpi} dpi</Select.Trigger>
							<Select.Content>
								<Select.Group>
									{#each DPIS as d (d)}
										<Select.Item value={String(d)} label="{d} dpi">
											<span class="flex-1">{d} dpi</span>
											<span class="text-xs text-muted-foreground">
												{d <= 100 ? 'Screen' : d <= 200 ? 'Documents' : d <= 300 ? 'Print' : d <= 600 ? 'Photos' : 'Archive'}
											</span>
										</Select.Item>
									{/each}
								</Select.Group>
							</Select.Content>
						</Select.Root>
					</Field.Field>
					<Field.Field class="gap-2">
						<Field.Label>Size</Field.Label>
						<Select.Root type="single" value={preset} onValueChange={pick} disabled={!!busy}>
							<Select.Trigger class="w-full min-w-0">
								<span class="truncate">{(preset && AREAS[preset]?.[0]) || 'Custom area'}</span>
							</Select.Trigger>
							<Select.Content class="max-h-72">
								<Select.Group>
									{#each Object.entries(AREAS) as [id, [name, w, h]] (id)}
										<Select.Item value={id} label={name}>
											<span class="flex-1">{name}</span>
											<span class="text-xs text-muted-foreground tabular-nums">{Math.round(w)}×{Math.round(h)}</span>
										</Select.Item>
									{/each}
								</Select.Group>
							</Select.Content>
						</Select.Root>
					</Field.Field>
				</div>
				{#if device.startsWith('epsonscan2')}
					<Field.Field class="gap-2">
						<div class="flex items-center justify-between">
							<Field.Label class="font-normal">Brightness</Field.Label>
							<span class="text-xs text-muted-foreground tabular-nums">{brightness}</span>
						</div>
						<Slider type="single" bind:value={brightness} min={-100} max={100} step={5} disabled={!!busy} />
					</Field.Field>
					<Field.Field class="gap-2">
						<div class="flex items-center justify-between">
							<Field.Label class="font-normal">Contrast</Field.Label>
							<span class="text-xs text-muted-foreground tabular-nums">{contrast}</span>
						</div>
						<Slider type="single" bind:value={contrast} min={-100} max={100} step={5} disabled={!!busy} />
					</Field.Field>
				{/if}

				<div class="flex h-16 items-center gap-2 overflow-x-auto">
					{#each scanned as src (src)}
						<img src={pageUrl(src, 0, 0, 120)} alt="Scanned page" class="h-full rounded-sm shadow-sm ring-1 ring-border" />
					{/each}
				</div>
			</Field.Group>

			<div class="flex flex-col items-center gap-2 p-2">
				<div
					bind:this={glass}
					class={cn(
						'relative w-56 touch-none rounded-md bg-muted ring-1 ring-border sm:w-64',
						!busy && 'cursor-crosshair'
					)}
					style:aspect-ratio="{bed.w} / {bed.h}"
					onpointerdown={(e) => start(e, 'new')}
					{onpointermove}
					onpointerup={() => (drag = null)}
					onpointercancel={() => (drag = null)}
					role="presentation"
				>
					<!-- Clipped layer: preview image and the dimming outside the area. -->
					<div class="pointer-events-none absolute inset-0 overflow-hidden rounded-md">
						{#if previewUrl}
							<img src={previewUrl} alt="Preview" class="absolute inset-0 size-full object-fill" draggable="false" />
						{:else if busy !== 'preview'}
							<div class="absolute inset-0 flex items-center justify-center text-muted-foreground">
								<IconPhotoScan class="size-8 opacity-50" />
							</div>
						{/if}
						<div
							class="absolute shadow-[0_0_0_9999px_rgb(0_0_0/0.35)]"
							style:left={pct(area.x, bed.w)}
							style:top={pct(area.y, bed.h)}
							style:width={pct(area.w, bed.w)}
							style:height={pct(area.h, bed.h)}
						></div>
					</div>
					<!-- Unclipped so handles on the glass edge stay grabbable. -->
					<div
						class={cn('absolute border-2 border-primary bg-primary/10', busy ? 'pointer-events-none' : 'cursor-move')}
						style:left={pct(area.x, bed.w)}
						style:top={pct(area.y, bed.h)}
						style:width={pct(area.w, bed.w)}
						style:height={pct(area.h, bed.h)}
						onpointerdown={(e) => start(e, 'move')}
						role="presentation"
					>
						{#if !busy}
							{#each HANDLES as [h, cls] (h)}
								<div
									class={cn('absolute size-3 -translate-1/2 rounded-full border-2 border-primary bg-background shadow-sm', cls)}
									onpointerdown={(e) => start(e, h)}
									role="presentation"
								></div>
							{/each}
						{/if}
					</div>
					{#if busy}
						<div class="absolute inset-x-0 top-0 h-0.5 bg-primary transition-[width]" style:width="{progress}%"></div>
					{/if}
				</div>
				<span class="text-xs text-muted-foreground tabular-nums">
					{preset ? '' : `${fmtSize(Math.round(area.w), Math.round(area.h), settings.unit)}, `}~{sizeMB < 1 ? sizeMB.toFixed(1) : Math.round(sizeMB)} MB
				</span>
			</div>
		</div>

		<Dialog.Footer>
			{#if busy}
				<span class="mr-auto flex items-center gap-2 self-center text-sm text-muted-foreground tabular-nums">
					<Spinner />
					{busy === 'preview' ? 'Previewing' : 'Scanning'}…{progress > 0 ? ` ${Math.round(progress)}%` : ''}
				</span>
				<Button variant="outline" onclick={() => api.cancelScan()}>Cancel</Button>
			{:else}
				<Button variant="outline" onclick={() => (open = false)}>{scanned.length ? 'Done' : 'Close'}</Button>
				<Button variant="secondary" onclick={preview} disabled={!device}>
					<IconPhotoScan data-icon="inline-start" /> Preview
				</Button>
			{/if}
			<Button onclick={scan} disabled={!!busy || !device} class="min-w-32">
				<IconScan data-icon="inline-start" />
				{scanned.length ? 'Scan another' : 'Scan'}
			</Button>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>
