<script lang="ts">
	import { untrack } from 'svelte';
	import {
		IconArrowAutofitHeight,
		IconArrowAutofitWidth,
		IconChevronUp,
		IconChevronDown,
		IconChevronsLeft,
		IconChevronsRight,
		IconDotsVertical,
		IconHandStop,
		IconLayoutColumns,
		IconLayoutGrid,
		IconLayoutRows,
		IconMinus,
		IconPlus,
		IconPointer
	} from '@tabler/icons-svelte';
	import { Button } from '$lib/components/ui/button';
	import * as ContextMenu from '$lib/components/ui/context-menu';
	import * as DropdownMenu from '$lib/components/ui/dropdown-menu';
	import * as InputGroup from '$lib/components/ui/input-group';
	import * as Select from '$lib/components/ui/select';
	import { pageUrl } from '$lib/api';
	import { MM } from '$lib/layout';
	import { job } from '$lib/state/job.svelte';
	import { output } from '$lib/state/output.svelte';
	import { settings } from '$lib/state/settings.svelte';
	import { cn } from '$lib/utils';
	import IconButton from './IconButton.svelte';

	type Zoom = 'width' | 'page' | number;
	type Flow = 'vertical' | 'horizontal' | 'wrapped';

	let zoom = $state<Zoom>('page');
	let flow = $state<Flow>('vertical');
	let panning = $state(false);
	let showBacks = $state(true);
	let showMargins = $state(false);
	let grayOverride = $state<boolean | null>(null);
	let current = $state(0);
	let cw = $state(600);
	let ch = $state(400);
	let scroller = $state<HTMLElement>();

	const PAD = 24;
	const GAP = 16;
	const ZOOMS = [0.25, 0.5, 0.75, 1, 1.25, 1.5, 2, 3, 4];
	/** 100% = physical size on a 96 dpi screen. */
	const PX_PER_PT = 96 / 72;

	const dpr = typeof window === 'undefined' ? 1 : window.devicePixelRatio || 1;
	const gray = $derived(grayOverride ?? settings.s.ppd.Color === 'Grayscale');
	const duplex = $derived(output.layout.duplex !== 'off');
	const beside = $derived(duplex && showBacks);
	const sheets = $derived.by(() => {
		const sides = output.layout.sides;
		if (!duplex) return sides.map((s) => [s]);
		const out = [];
		for (let i = 0; i < sides.length; i += 2) out.push(showBacks ? sides.slice(i, i + 2) : [sides[i]]);
		return out;
	});
	const first = $derived(output.layout.sides[0] ?? { w: 595, h: 842 });
	/** Pixels per point. */
	const k = $derived.by(() => {
		const availW = cw - PAD * 2;
		const availH = ch - PAD * 2;
		if (zoom === 'width') return Math.max(0.05, (availW - (beside ? GAP : 0)) / (first.w * (beside ? 2 : 1)));
		if (zoom === 'page') return Math.max(0.05, Math.min((availW - (beside ? GAP : 0)) / (first.w * (beside ? 2 : 1)), availH / first.h));
		return zoom * PX_PER_PT;
	});
	const pct = $derived(Math.round((k / PX_PER_PT) * 100));
	const zoomLabel = $derived(zoom === 'width' ? 'Fit width' : zoom === 'page' ? 'Fit sheet' : `${pct}%`);
	const m = $derived(settings.s.margins);

	function step(dir: 1 | -1) {
		const now = k / PX_PER_PT;
		const next = dir > 0 ? ZOOMS.find((z) => z > now + 0.01) : [...ZOOMS].reverse().find((z) => z < now - 0.01);
		if (next) zoom = next;
	}

	function go(i: number) {
		if (!Number.isFinite(i)) return;
		const n = Math.max(0, Math.min(sheets.length - 1, Math.round(i)));
		scroller?.querySelector<HTMLElement>(`[data-sheet="${n}"]`)?.scrollIntoView({ block: 'start', inline: 'start' });
		current = n;
	}

	/**
	 * The current sheet is the first one still showing past the top (left, when scrolling
	 * horizontally) of the viewport. Measured on scroll, so sheets taller than the viewport count too.
	 */
	let trackRaf = 0;
	function track() {
		cancelAnimationFrame(trackRaf);
		trackRaf = requestAnimationFrame(() => {
			if (!scroller) return;
			const box = scroller.getBoundingClientRect();
			const els = scroller.querySelectorAll<HTMLElement>('[data-sheet]');
			for (const el of els) {
				const r = el.getBoundingClientRect();
				if (flow === 'horizontal' ? r.right > box.left + 24 : r.bottom > box.top + 24) {
					current = +el.dataset.sheet!;
					return;
				}
			}
		});
	}
	// Picking a page in the pages panel scrolls to the sheet that holds it.
	$effect(() => {
		const id = job.reveal?.id;
		const p = job.pages.find((x) => x.id === id);
		if (p?.kind !== 'page') return;
		const n = untrack(() => sheets.findIndex((s) => s.some((side) => side.cells.some((c) => c.src === p.src && c.index === p.index))));
		if (n >= 0) go(n);
	});
	$effect(() => {
		void sheets.length;
		void k;
		void flow;
		track();
	});

	// The zoom menu hides in a narrow pane; don't leave a zoom there that can't be seen.
	$effect(() => {
		if (cw < 320 && typeof zoom === 'number') zoom = 'page';
	});

	// The grayscale view follows the print setting again whenever that changes.
	$effect(() => {
		void settings.s.ppd.Color;
		grayOverride = null;
	});

	/** Drag-to-pan: grab anywhere in the preview and move it like a hand tool; release while moving to flick. */
	let pan = $state<{ x: number; y: number; left: number; top: number } | null>(null);
	let vx = 0;
	let vy = 0;
	let last = { x: 0, y: 0, t: 0 };
	let flickRaf = 0;
	function onpointerdown(e: PointerEvent) {
		cancelAnimationFrame(flickRaf);
		if (!panning || e.button !== 0 || !scroller) return;
		pan = { x: e.clientX, y: e.clientY, left: scroller.scrollLeft, top: scroller.scrollTop };
		last = { x: e.clientX, y: e.clientY, t: e.timeStamp };
		vx = vy = 0;
		scroller.setPointerCapture(e.pointerId);
	}
	function onpointermove(e: PointerEvent) {
		if (!pan || !scroller) return;
		scroller.scrollLeft = pan.left - (e.clientX - pan.x);
		scroller.scrollTop = pan.top - (e.clientY - pan.y);
		// Smoothed px/ms of the pointer, so one jittery event doesn't decide the flick.
		const dt = Math.max(1, e.timeStamp - last.t);
		vx = vx * 0.6 + ((e.clientX - last.x) / dt) * 0.4;
		vy = vy * 0.6 + ((e.clientY - last.y) / dt) * 0.4;
		last = { x: e.clientX, y: e.clientY, t: e.timeStamp };
	}
	function onpointerup(e: PointerEvent) {
		pan = null;
		// A pause before release means the hand stopped: no flick.
		if (!scroller || e.timeStamp - last.t > 80 || Math.hypot(vx, vy) < 0.1) return;
		let t = performance.now();
		const el = scroller;
		const tick = (now: number) => {
			const dt = now - t;
			t = now;
			el.scrollLeft -= vx * dt;
			el.scrollTop -= vy * dt;
			const decay = Math.exp(-dt / 325);
			vx *= decay;
			vy *= decay;
			if (Math.hypot(vx, vy) > 0.02) flickRaf = requestAnimationFrame(tick);
		};
		flickRaf = requestAnimationFrame(tick);
	}

	function onwheel(e: WheelEvent) {
		cancelAnimationFrame(flickRaf);
		if (!e.ctrlKey) return;
		e.preventDefault();
		step(e.deltaY < 0 ? 1 : -1);
	}

</script>

<svelte:window
	onkeydown={(e) => {
		if (!(e.ctrlKey || e.metaKey) || (e.target as HTMLElement).closest('input, textarea, [role=dialog]')) return;
		if (e.key === '=' || e.key === '+') step(1);
		else if (e.key === '-') step(-1);
		else if (e.key === '0') zoom = 'page';
		else return;
		e.preventDefault();
	}}
/>

<section class="flex h-full min-w-0 flex-col bg-muted/50">
	<header class="@container flex h-11 shrink-0 items-center gap-1 overflow-hidden border-b bg-background px-2">
		{#if sheets.length > 1}
				<div class="contents @max-sm:hidden">
				<IconButton icon={IconChevronUp} label="Previous sheet" disabled={current === 0} onclick={() => go(current - 1)} />
				<IconButton icon={IconChevronDown} label="Next sheet" disabled={current >= sheets.length - 1} onclick={() => go(current + 1)} />
				<InputGroup.Root class="h-7 w-20 @max-lg:hidden">
					<InputGroup.Input
						class="text-center tabular-nums"
						aria-label="Sheet"
						value={current + 1}
						onchange={(e) => {
							const input = e.currentTarget as HTMLInputElement;
							go(+input.value - 1);
							input.value = String(current + 1);
						}}
					/>
					<InputGroup.Addon align="inline-end"><InputGroup.Text class="tabular-nums">/ {sheets.length}</InputGroup.Text></InputGroup.Addon>
				</InputGroup.Root>
				</div>
		{/if}

		<div class="ml-auto flex shrink-0 items-center gap-1">
			<div class="contents @max-md:hidden">
				<IconButton icon={IconMinus} label="Zoom out" shortcut="Ctrl −" onclick={() => step(-1)} />
			</div>
			<div class="contents @max-md:hidden">
				<IconButton icon={IconPlus} label="Zoom in" shortcut="Ctrl +" onclick={() => step(1)} />
			</div>
			<Select.Root
				type="single"
				value={typeof zoom === 'number' ? String(zoom) : zoom}
				onValueChange={(v) => (zoom = v === 'width' || v === 'page' ? v : +v)}
			>
				<Select.Trigger size="sm" class="w-28 @max-xs:hidden" title="Zoom">
					<span class="truncate">{zoomLabel}</span>
				</Select.Trigger>
				<Select.Content>
					<Select.Group>
						<Select.Item value="page" label="Fit sheet"><IconArrowAutofitHeight /> Fit sheet</Select.Item>
						<Select.Item value="width" label="Fit width"><IconArrowAutofitWidth /> Fit width</Select.Item>
					</Select.Group>
					<Select.Separator />
					<Select.Group>
						{#each ZOOMS as z (z)}
							<Select.Item value={String(z)} label="{z * 100}%">{z * 100}%</Select.Item>
						{/each}
					</Select.Group>
				</Select.Content>
			</Select.Root>
			<IconButton
				icon={panning ? IconHandStop : IconPointer}
				label={panning ? 'Scroll normally' : 'Drag to pan'}
				onclick={() => (panning = !panning)}
			/>

			<DropdownMenu.Root>
				<DropdownMenu.Trigger>
					{#snippet child({ props })}
						<Button {...props} variant="ghost" size="icon-sm" aria-label="View options" title="View options"><IconDotsVertical /></Button>
					{/snippet}
				</DropdownMenu.Trigger>
				<DropdownMenu.Content align="end" class="w-56">
					{#if cw < 448 || cw < 320 || (sheets.length > 1 && cw < 384)}
						<DropdownMenu.Group>
							{#if cw < 448}
								<DropdownMenu.Item onclick={() => step(1)}><IconPlus /> Zoom in</DropdownMenu.Item>
								<DropdownMenu.Item onclick={() => step(-1)}><IconMinus /> Zoom out</DropdownMenu.Item>
							{/if}
							{#if cw < 320}
								<DropdownMenu.Item onclick={() => (zoom = 'page')}><IconArrowAutofitHeight /> Fit sheet</DropdownMenu.Item>
								<DropdownMenu.Item onclick={() => (zoom = 'width')}><IconArrowAutofitWidth /> Fit width</DropdownMenu.Item>
							{/if}
							{#if sheets.length > 1 && cw < 384}
								<DropdownMenu.Item disabled={current === 0} onclick={() => go(current - 1)}><IconChevronUp /> Previous sheet</DropdownMenu.Item>
								<DropdownMenu.Item disabled={current >= sheets.length - 1} onclick={() => go(current + 1)}>
									<IconChevronDown /> Next sheet
								</DropdownMenu.Item>
							{/if}
						</DropdownMenu.Group>
						<DropdownMenu.Separator />
					{/if}
					{#if sheets.length > 1}
						<DropdownMenu.Group>
							<DropdownMenu.Item disabled={current === 0} onclick={() => go(0)}><IconChevronsLeft /> First sheet</DropdownMenu.Item>
							<DropdownMenu.Item disabled={current >= sheets.length - 1} onclick={() => go(sheets.length - 1)}>
								<IconChevronsRight /> Last sheet
							</DropdownMenu.Item>
						</DropdownMenu.Group>
						<DropdownMenu.Separator />
					{/if}
					<DropdownMenu.RadioGroup bind:value={flow}>
						<DropdownMenu.Label>Scrolling</DropdownMenu.Label>
						<DropdownMenu.RadioItem value="vertical"><IconLayoutRows /> Vertical</DropdownMenu.RadioItem>
						<DropdownMenu.RadioItem value="horizontal"><IconLayoutColumns /> Horizontal</DropdownMenu.RadioItem>
						<DropdownMenu.RadioItem value="wrapped"><IconLayoutGrid /> Wrapped</DropdownMenu.RadioItem>
					</DropdownMenu.RadioGroup>
					<DropdownMenu.Separator />
					<DropdownMenu.Group>
						<DropdownMenu.CheckboxItem bind:checked={showBacks} disabled={!duplex}>Show backs beside fronts</DropdownMenu.CheckboxItem>
						<DropdownMenu.CheckboxItem bind:checked={showMargins}>Show margins</DropdownMenu.CheckboxItem>
						<DropdownMenu.CheckboxItem bind:checked={() => gray, (v) => (grayOverride = v)}>Grayscale</DropdownMenu.CheckboxItem>
					</DropdownMenu.Group>
				</DropdownMenu.Content>
			</DropdownMenu.Root>
		</div>
	</header>

	<ContextMenu.Root>
		<ContextMenu.Trigger class="min-h-0 flex-1" disabled={!output.pages.length || !!output.layout.error}>
			{#snippet child({ props })}
	<div
		{...props}
		role="presentation"
		bind:this={scroller}
		class={cn('min-h-0 flex-1 overflow-auto', panning && 'touch-none cursor-grab select-none', pan && 'cursor-grabbing')}
		bind:clientWidth={cw}
		bind:clientHeight={ch}
		{onwheel}
		{onpointerdown}
		{onpointermove}
		{onpointerup}
		onpointercancel={onpointerup}
		onscroll={track}
	>
		{#if !output.pages.length}
			<p class="py-24 text-center text-sm text-muted-foreground">
				{output.rangeError ? 'Invalid page range' : 'Nothing to print'}
			</p>
		{:else if output.layout.error}
			<p class="px-6 py-24 text-center text-sm text-destructive">{output.layout.error}</p>
		{:else}
			<div
				class={cn(
					'flex min-h-full w-max min-w-full gap-6 p-6',
					flow === 'vertical' && 'flex-col items-center',
					flow === 'horizontal' && 'flex-row items-start',
					flow === 'wrapped' && 'w-full flex-row flex-wrap content-start justify-center'
				)}
			>
				{#each sheets as group, i (i)}
					<!-- Explicit size: with content-visibility, offscreen sheets would otherwise keep a
					     stale size after zooming, and jumps would land on the wrong sheet. -->
					<div
						class="shrink-0 [content-visibility:auto]"
						data-sheet={i}
						style:width="{group.reduce((w, side) => w + side.w * k, 0) + GAP * (group.length - 1)}px"
						style:height="{Math.max(...group.map((side) => side.h)) * k}px"
					>
						<div class="flex items-start gap-4">
							{#each group as side, j (j)}
								<div
									class="relative overflow-hidden bg-white shadow-md ring-1 ring-black/5"
									style:width="{side.w * k}px"
									style:height="{side.h * k}px"
									title={duplex ? (j === 0 ? 'Front' : 'Back') : undefined}
								>
									{#each side.cells as c, n (n)}
										<img
											src={pageUrl(c.src, c.index, c.rot, c.w * k * dpr, gray)}
											alt=""
											loading="lazy"
											decoding="async"
											draggable="false"
											class={cn('absolute max-w-none', c.border && 'outline outline-neutral-500')}
											style:left="{c.x * k}px"
											style:top="{c.y * k}px"
											style:width="{c.w * k}px"
											style:height="{c.h * k}px"
										/>
									{/each}
									{#each side.lines ?? [] as [x1, y1, , y2], n (n)}
										<div
											class="absolute border-l border-dashed border-neutral-400"
											style:left="{x1 * k}px"
											style:top="{Math.min(y1, y2) * k}px"
											style:height="{Math.abs(y2 - y1) * k}px"
										></div>
									{/each}
									{#each side.labels ?? [] as l, n (n)}
										<div
											class="pointer-events-none absolute origin-top-left"
											style:left="{l.x * k}px"
											style:top="{l.y * k}px"
											style:transform="rotate({l.angle}deg)"
										>
											<span
												class="absolute leading-none whitespace-nowrap text-neutral-800"
												style:font-family="Helvetica, Arial, 'Liberation Sans', sans-serif"
												style:font-size="{l.size * k}px"
												style:transform="translate({l.align === 'center' ? -50 : l.align === 'right' ? -100 : 0}%, -0.77em)"
											>
												{l.text}
											</span>
										</div>
									{/each}
									{#if showMargins}
										<div
											class="pointer-events-none absolute border border-dashed border-sky-500/70"
											style:left="{m.left * MM * k}px"
											style:top="{m.top * MM * k}px"
											style:right="{m.right * MM * k}px"
											style:bottom="{m.bottom * MM * k}px"
										></div>
									{/if}
								</div>
							{/each}
						</div>
					</div>
				{/each}
			</div>
		{/if}
	</div>
			{/snippet}
		</ContextMenu.Trigger>
		<ContextMenu.Content class="w-56">
			<ContextMenu.Group>
				<ContextMenu.Item onclick={() => step(1)}><IconPlus /> Zoom in <ContextMenu.Shortcut>Ctrl +</ContextMenu.Shortcut></ContextMenu.Item>
				<ContextMenu.Item onclick={() => step(-1)}><IconMinus /> Zoom out <ContextMenu.Shortcut>Ctrl −</ContextMenu.Shortcut></ContextMenu.Item>
				<ContextMenu.Item onclick={() => (zoom = 'page')}><IconArrowAutofitHeight /> Fit sheet <ContextMenu.Shortcut>Ctrl 0</ContextMenu.Shortcut></ContextMenu.Item>
				<ContextMenu.Item onclick={() => (zoom = 'width')}><IconArrowAutofitWidth /> Fit width</ContextMenu.Item>
			</ContextMenu.Group>
			{#if sheets.length > 1}
				<ContextMenu.Separator />
				<ContextMenu.Group>
					<ContextMenu.Item disabled={current === 0} onclick={() => go(current - 1)}><IconChevronUp /> Previous sheet</ContextMenu.Item>
					<ContextMenu.Item disabled={current >= sheets.length - 1} onclick={() => go(current + 1)}><IconChevronDown /> Next sheet</ContextMenu.Item>
					<ContextMenu.Item disabled={current === 0} onclick={() => go(0)}><IconChevronsLeft /> First sheet</ContextMenu.Item>
					<ContextMenu.Item disabled={current >= sheets.length - 1} onclick={() => go(sheets.length - 1)}>
						<IconChevronsRight /> Last sheet
					</ContextMenu.Item>
				</ContextMenu.Group>
			{/if}
			<ContextMenu.Separator />
			<ContextMenu.Group>
				<ContextMenu.CheckboxItem bind:checked={panning}>Drag to pan</ContextMenu.CheckboxItem>
				<ContextMenu.CheckboxItem bind:checked={showBacks} disabled={!duplex}>Show backs beside fronts</ContextMenu.CheckboxItem>
				<ContextMenu.CheckboxItem bind:checked={showMargins}>Show margins</ContextMenu.CheckboxItem>
				<ContextMenu.CheckboxItem bind:checked={() => gray, (v) => (grayOverride = v)}>Grayscale</ContextMenu.CheckboxItem>
			</ContextMenu.Group>
		</ContextMenu.Content>
	</ContextMenu.Root>
</section>

