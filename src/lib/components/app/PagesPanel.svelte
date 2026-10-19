<script lang="ts">
	import {
		IconArrowBackUp,
		IconArrowBarToDown,
		IconArrowBarToUp,
		IconArrowForwardUp,
		IconCopy,
		IconFilePlus,
		IconFolderOpen,
		IconRotate,
		IconRotateClockwise,
		IconRotate360,
		IconSelectAll,
		IconSwitchVertical,
		IconTrash,
		IconFiles,
		IconDotsVertical
	} from '@tabler/icons-svelte';
	import { Button } from '$lib/components/ui/button';
	import * as ContextMenu from '$lib/components/ui/context-menu';
	import * as DropdownMenu from '$lib/components/ui/dropdown-menu';
	import * as Empty from '$lib/components/ui/empty';
	import { Slider } from '$lib/components/ui/slider';
	import { Spinner } from '$lib/components/ui/spinner';
	import { pageUrl } from '$lib/api';
	import { job } from '$lib/state/job.svelte';
	import { cn } from '$lib/utils';
	import IconButton from './IconButton.svelte';

	let { onopen }: { onopen: () => void } = $props();

	let thumb = $state(130);
	/** WebKitGTK paints a scroller's native scrollbar over portaled menus; hidden while one is open. */
	let menuOpen = $state(false);
	let scroller = $state<HTMLElement>();
	let grid = $state<HTMLElement>();
	let ctxId = $state<string | undefined>();

	let press: { id: string; x: number; y: number; e: PointerEvent } | null = null;
	let drag = $state<{ ids: Set<string>; at: number; marker: { x: number; y: number; h: number } | null } | null>(null);
	let scrollRaf = 0;
	let moveRaf = 0;

	const dpr = typeof window === 'undefined' ? 1 : window.devicePixelRatio || 1;
	const sel = $derived(job.selected.size);
	/** One scale for every page (px per pt), so sizes compare truthfully. `thumb` is an A4 page's width. */
	const scale = $derived(thumb / 595.28);

	function onpointerdown(e: PointerEvent, id: string) {
		if (e.button !== 0) return;
		press = { id, x: e.clientX, y: e.clientY, e };
	}

	function dropTarget(x: number, y: number) {
		const items = [...grid!.querySelectorAll<HTMLElement>('[data-index]')];
		const box = grid!.getBoundingClientRect();
		let best = { at: job.pages.length, d: Infinity, marker: null as null | { x: number; y: number; h: number } };
		for (const el of items) {
			const r = el.getBoundingClientRect();
			const i = +el.dataset.index!;
			for (const [edge, at] of [
				[r.left, i],
				[r.right, i + 1]
			] as const) {
				const dy = y < r.top ? r.top - y : y > r.bottom ? y - r.bottom : 0;
				const d = Math.hypot(edge - x, dy * 3);
				if (d < best.d) best = { at, d, marker: { x: edge - box.left, y: r.top - box.top, h: r.height } };
			}
		}
		return best;
	}

	function autoscroll(y: number) {
		cancelAnimationFrame(scrollRaf);
		if (!scroller || !drag) return;
		const r = scroller.getBoundingClientRect();
		const v = y < r.top + 40 ? -12 : y > r.bottom - 40 ? 12 : 0;
		if (!v) return;
		const step = () => {
			scroller!.scrollTop += v;
			scrollRaf = requestAnimationFrame(step);
		};
		scrollRaf = requestAnimationFrame(step);
	}

	function onpointermove(e: PointerEvent) {
		if (!press) return;
		if (!drag) {
			if (Math.hypot(e.clientX - press.x, e.clientY - press.y) < 6) return;
			if (!job.selected.has(press.id)) job.select(press.id, 'set');
			drag = { ids: new Set(job.selected), at: 0, marker: null };
		}
		// Hit-testing measures every page: once per frame is plenty.
		const { clientX: x, clientY: y } = e;
		cancelAnimationFrame(moveRaf);
		moveRaf = requestAnimationFrame(() => {
			if (!drag) return;
			const t = dropTarget(x, y);
			drag.at = t.at;
			drag.marker = t.marker;
		});
		autoscroll(y);
	}

	function onpointerup(e: PointerEvent) {
		cancelAnimationFrame(scrollRaf);
		cancelAnimationFrame(moveRaf);
		if (drag) {
			job.move(drag.ids, dropTarget(e.clientX, e.clientY).at);
			drag = null;
		} else if (press) {
			const ev = press.e;
			job.select(press.id, ev.shiftKey ? 'range' : ev.ctrlKey || ev.metaKey ? 'toggle' : 'set');
		}
		press = null;
	}

	function oncontextmenu(e: MouseEvent) {
		const el = (e.target as HTMLElement).closest<HTMLElement>('[data-id]');
		ctxId = el?.dataset.id;
		if (ctxId && !job.selected.has(ctxId)) job.select(ctxId, 'set');
	}

	const targets = () => job.targets(ctxId);
	/** Opened on empty space with nothing selected: page actions have nothing to act on. */
	const noTargets = $derived(!ctxId && !sel);

	/**
	 * Header actions in display order. `rank` is usefulness (0 = most): as the panel narrows, the
	 * least useful move into the menu first.
	 */
	const actions = $derived([
		{ label: 'Rotate left', shortcut: '⇧R', icon: IconRotate, rank: 4, disabled: !sel, run: () => job.rotate(job.targets(), -90) },
		{ label: 'Rotate right', shortcut: 'R', icon: IconRotateClockwise, rank: 0, disabled: !sel, run: () => job.rotate(job.targets(), 90) },
		{ label: 'Duplicate', shortcut: 'Ctrl D', icon: IconCopy, rank: 2, disabled: !sel, run: () => job.duplicate(job.targets()) },
		{ label: 'Insert blank page', icon: IconFilePlus, rank: 3, disabled: !job.pages.length, run: () => job.insertBlank([...job.selected].at(-1)) },
		{ label: 'Delete', shortcut: 'Del', icon: IconTrash, rank: 1, danger: true, disabled: !sel, run: () => job.remove(job.targets()) }
	]);
	let headerW = $state(300);
	/** Fixed part: padding, title, menu button; each inline button is 32px. */
	const fit = $derived(Math.max(0, Math.min(actions.length, Math.floor((headerW - 150) / 32))));
	const shown = $derived(actions.filter((a) => a.rank < fit));
	const hidden = $derived(actions.filter((a) => a.rank >= fit));

	// Keep the keyboard cursor (arrow keys) in view.
	$effect(() => {
		const id = job.cursor;
		if (id) grid?.querySelector<HTMLElement>(`[data-id="${id}"]`)?.scrollIntoView({ block: 'nearest' });
	});
</script>

<svelte:window {onpointermove} {onpointerup} />

<section class="flex h-full min-w-0 flex-col">
	<header bind:clientWidth={headerW} class="flex h-11 shrink-0 items-center gap-1 overflow-hidden border-b px-2">
		<span class="px-1 text-sm font-medium">Pages</span>
		<span class="min-w-0 truncate text-xs whitespace-nowrap text-muted-foreground tabular-nums">
			{sel > 1 ? `${sel} selected` : ''}
		</span>
		<div class="ml-auto flex shrink-0 items-center">
			{#each shown as a (a.label)}
				<IconButton icon={a.icon} label={a.label} shortcut={a.shortcut} disabled={a.disabled} onclick={a.run} />
			{/each}
		</div>
		{#if hidden.length}
			<DropdownMenu.Root>
				<DropdownMenu.Trigger>
					{#snippet child({ props })}
						<Button {...props} variant="ghost" size="icon-sm" aria-label="Page actions" title="Page actions">
							<IconDotsVertical />
						</Button>
					{/snippet}
				</DropdownMenu.Trigger>
				<DropdownMenu.Content align="end" class="w-52">
					{#each hidden as a (a.label)}
						{#if a.danger}<DropdownMenu.Separator />{/if}
						<DropdownMenu.Item variant={a.danger ? 'destructive' : 'default'} disabled={a.disabled} onclick={a.run}>
							<a.icon /> {a.label}
						</DropdownMenu.Item>
					{/each}
				</DropdownMenu.Content>
			</DropdownMenu.Root>
		{/if}
	</header>

	{#if job.pages.length === 0}
		<Empty.Root class="flex-1">
			<Empty.Header>
				<Empty.Media variant="icon"><IconFiles /></Empty.Media>
				<Empty.Title>No pages</Empty.Title>
				<Empty.Description>Drop files here</Empty.Description>
			</Empty.Header>
			<Empty.Content>
				<Button onclick={onopen}>
					{#if job.loading}<Spinner data-icon="inline-start" />{:else}<IconFolderOpen data-icon="inline-start" />{/if}
					Open files
				</Button>
			</Empty.Content>
		</Empty.Root>
	{:else}
		<ContextMenu.Root bind:open={menuOpen}>
			<ContextMenu.Trigger class="min-h-0 flex-1">
				{#snippet child({ props })}
					<div
						{...props}
						bind:this={scroller}
						class={cn('min-h-0 flex-1 overflow-y-auto p-3', menuOpen && '[scrollbar-width:none]')}
						oncontextmenu={(e) => {
							oncontextmenu(e);
							(props.oncontextmenu as ((e: MouseEvent) => void) | undefined)?.(e);
						}}
						onpointerdown={(e) => {
							if (e.target === e.currentTarget || e.target === grid) job.selected.clear();
						}}
						role="listbox"
						tabindex="-1"
						aria-multiselectable="true"
					>
						<div bind:this={grid} class="relative flex flex-wrap items-end justify-center gap-x-4 gap-y-5">
							{#each job.pages as p, i (p.id)}
								{@const [w, h] = job.size(p)}
								{@const tw = w * scale}
								{@const th = h * scale}
								<div
									data-id={p.id}
									data-index={i}
									class={cn(
										'flex shrink-0 touch-none flex-col items-center gap-1.5 [content-visibility:auto]',
										drag?.ids.has(p.id) && 'opacity-40'
									)}
									style:width="{tw}px"
									style:contain-intrinsic-size="{tw}px {th + 24}px"
									onpointerdown={(e) => onpointerdown(e, p.id)}
									role="option"
									aria-selected={job.selected.has(p.id)}
									tabindex="-1"
									title={p.kind === 'page' ? `${job.sources.get(p.src)?.name}, page ${p.index + 1}` : 'Blank page'}
								>
									<div>
										<div
											class={cn(
												'relative overflow-hidden rounded-[2px] bg-white shadow-sm ring-1 ring-border',
												job.selected.has(p.id) && 'ring-2 ring-primary ring-offset-2 ring-offset-background'
											)}
											style:width="{tw}px"
											style:height="{th}px"
										>
											{#if p.kind === 'page'}
												<img
													src={pageUrl(p.src, p.index, p.rot, tw * dpr)}
													alt="Page {i + 1}"
													loading="lazy"
													decoding="async"
													draggable="false"
													class="size-full object-contain"
												/>
											{/if}
										</div>
									</div>
									<span
										class={cn(
											'rounded-full px-1.5 text-xs tabular-nums text-muted-foreground',
											job.selected.has(p.id) && 'bg-primary text-primary-foreground'
										)}
									>
										{i + 1}{p.kind === 'blank' ? ' (blank)' : ''}
									</span>
								</div>
							{/each}
							{#if drag?.marker}
								<div
									class="pointer-events-none absolute w-0.5 -translate-x-[7px] rounded-full bg-primary"
									style:left="{drag.marker.x}px"
									style:top="{drag.marker.y}px"
									style:height="{drag.marker.h}px"
								></div>
							{/if}
						</div>
					</div>
				{/snippet}
			</ContextMenu.Trigger>
			<ContextMenu.Content class="w-52">
				<ContextMenu.Group>
					<ContextMenu.Item disabled={noTargets} onclick={() => job.rotate(targets(), 90)}>
						<IconRotateClockwise /> Rotate right <ContextMenu.Shortcut>R</ContextMenu.Shortcut>
					</ContextMenu.Item>
					<ContextMenu.Item disabled={noTargets} onclick={() => job.rotate(targets(), -90)}>
						<IconRotate /> Rotate left <ContextMenu.Shortcut>⇧R</ContextMenu.Shortcut>
					</ContextMenu.Item>
					<ContextMenu.Item disabled={noTargets} onclick={() => job.rotate(targets(), 180)}>
						<IconRotate360 /> Rotate 180°
					</ContextMenu.Item>
				</ContextMenu.Group>
				<ContextMenu.Separator />
				<ContextMenu.Group>
					<ContextMenu.Item disabled={noTargets} onclick={() => job.duplicate(targets())}>
						<IconCopy /> Duplicate <ContextMenu.Shortcut>Ctrl D</ContextMenu.Shortcut>
					</ContextMenu.Item>
					<ContextMenu.Item onclick={() => job.insertBlank(ctxId)}>
						<IconFilePlus /> Insert blank after
					</ContextMenu.Item>
					<ContextMenu.Item onclick={() => job.reverse(targets())}>
						<IconSwitchVertical /> Reverse order
					</ContextMenu.Item>
				</ContextMenu.Group>
				<ContextMenu.Separator />
				<ContextMenu.Group>
					<ContextMenu.Item disabled={noTargets} onclick={() => job.move(targets(), 0)}>
						<IconArrowBarToUp /> Move to start
					</ContextMenu.Item>
					<ContextMenu.Item disabled={noTargets} onclick={() => job.move(targets(), job.pages.length)}>
						<IconArrowBarToDown /> Move to end
					</ContextMenu.Item>
					<ContextMenu.Item onclick={() => job.selectAll()}>
						<IconSelectAll /> Select all <ContextMenu.Shortcut>Ctrl A</ContextMenu.Shortcut>
					</ContextMenu.Item>
				</ContextMenu.Group>
				<ContextMenu.Separator />
				<ContextMenu.Group>
					<ContextMenu.Item disabled={!job.canUndo} onclick={() => job.undo()}>
						<IconArrowBackUp /> Undo <ContextMenu.Shortcut>Ctrl Z</ContextMenu.Shortcut>
					</ContextMenu.Item>
					<ContextMenu.Item disabled={!job.canRedo} onclick={() => job.redo()}>
						<IconArrowForwardUp /> Redo <ContextMenu.Shortcut>Ctrl Y</ContextMenu.Shortcut>
					</ContextMenu.Item>
				</ContextMenu.Group>
				<ContextMenu.Separator />
				<ContextMenu.Group>
					<ContextMenu.Item variant="destructive" disabled={noTargets} onclick={() => job.remove(targets())}>
						<IconTrash /> Delete <ContextMenu.Shortcut>Del</ContextMenu.Shortcut>
					</ContextMenu.Item>
				</ContextMenu.Group>
			</ContextMenu.Content>
		</ContextMenu.Root>
		<footer class="flex h-9 shrink-0 items-center gap-2 border-t px-3">
			<span class="text-xs text-muted-foreground">Size</span>
			<Slider type="single" bind:value={thumb} min={80} max={280} step={10} class="max-w-40" />
		</footer>
	{/if}
</section>

