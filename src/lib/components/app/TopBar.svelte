<script lang="ts">
	import {
		IconArrowBackUp,
		IconArrowForwardUp,
		IconFileExport,
		IconFolderOpen,
		IconMenu2,
		IconScan,
		IconSettings,
		IconX
	} from '@tabler/icons-svelte';
	import { Badge } from '$lib/components/ui/badge';
	import { Button } from '$lib/components/ui/button';
	import * as DropdownMenu from '$lib/components/ui/dropdown-menu';
	import * as Select from '$lib/components/ui/select';
	import { Separator } from '$lib/components/ui/separator';
	import { Spinner } from '$lib/components/ui/spinner';
	import { caps } from '$lib/state/caps.svelte';
	import { job } from '$lib/state/job.svelte';
	import { output } from '$lib/state/output.svelte';
	import { printer } from '$lib/state/printer.svelte';
	import { settings } from '$lib/state/settings.svelte';
	import IconButton from './IconButton.svelte';
	import WindowControls from './window/WindowControls.svelte';

	let { onopen, onscan, onsettings }: { onopen: () => void; onscan: () => void; onsettings: () => void } = $props();

	const printers = $derived(printer.list.filter((p) => p.name !== 'NBSP'));
</script>

<header class="flex h-11 shrink-0 items-center gap-1 border-b pr-1.5 pl-1.5" data-tauri-drag-region>
	<DropdownMenu.Root>
		<DropdownMenu.Trigger>
			{#snippet child({ props })}
				<Button {...props} variant="ghost" size="icon-sm" aria-label="Menu" title="Menu"><IconMenu2 /></Button>
			{/snippet}
		</DropdownMenu.Trigger>
		<DropdownMenu.Content align="start" class="w-60">
			<DropdownMenu.Group>
				<DropdownMenu.Item disabled={!job.pages.length} onclick={() => output.exportPages()}>
					<IconFileExport /> Save pages as PDF… <DropdownMenu.Shortcut>Ctrl S</DropdownMenu.Shortcut>
				</DropdownMenu.Item>
				<DropdownMenu.Item disabled={!output.pages.length} onclick={() => output.exportLayout()}>
					<IconFileExport /> Save print layout as PDF…
				</DropdownMenu.Item>
			</DropdownMenu.Group>
			<DropdownMenu.Separator />
			<DropdownMenu.Group>
				<DropdownMenu.Item onclick={onsettings}>
					<IconSettings /> Settings… <DropdownMenu.Shortcut>Ctrl ,</DropdownMenu.Shortcut>
				</DropdownMenu.Item>
				<DropdownMenu.Item disabled={!job.pages.length} variant="destructive" onclick={() => job.clear()}>
					<IconX /> Close all
				</DropdownMenu.Item>
			</DropdownMenu.Group>
		</DropdownMenu.Content>
	</DropdownMenu.Root>

	<Button variant="ghost" size="sm" onclick={onopen} title="Open files (Ctrl O)">
		{#if job.loading}<Spinner data-icon="inline-start" />{:else}<IconFolderOpen data-icon="inline-start" />{/if}
		<span class="max-md:sr-only">Open</span>
	</Button>
	{#if caps.value.scan}
		<Button variant="ghost" size="sm" onclick={onscan} title="Scan">
			<IconScan data-icon="inline-start" />
			<span class="max-md:sr-only">Scan</span>
		</Button>
	{/if}

	<Separator orientation="vertical" class="mx-1 data-vertical:h-4 data-vertical:self-center" />
	<IconButton icon={IconArrowBackUp} label="Undo" shortcut="Ctrl Z" disabled={!job.canUndo} onclick={() => job.undo()} />
	<IconButton icon={IconArrowForwardUp} label="Redo" shortcut="Ctrl ⇧ Z" disabled={!job.canRedo} onclick={() => job.redo()} />

	<div class="flex h-full min-w-0 flex-1 items-center justify-center px-3" data-tauri-drag-region>
		{#if !settings.titleBarApplied}
			<span class="truncate text-sm text-muted-foreground" title={job.pages.length ? job.title : undefined} data-tauri-drag-region>
				{job.pages.length ? job.title : 'NBSP'}
			</span>
		{/if}
	</div>

	<Select.Root type="single" bind:value={settings.printer}>
		<Select.Trigger size="sm" class="max-w-48 min-w-0" title={settings.printer}>
			<span class="truncate">{settings.printer || 'No printer'}</span>
			{#if printer.current && printer.current.state !== 'idle'}
				<Badge variant={printer.current.state === 'disabled' ? 'destructive' : 'secondary'}>{printer.current.state}</Badge>
			{/if}
		</Select.Trigger>
		<Select.Content align="end">
			<Select.Group>
				{#each printers as p (p.name)}
					<Select.Item value={p.name} label={p.name}>{p.name}</Select.Item>
				{/each}
			</Select.Group>
		</Select.Content>
	</Select.Root>
	{#if !settings.titleBarApplied}
		<Separator orientation="vertical" class="mx-1 data-vertical:h-4 data-vertical:self-center" />
		<WindowControls />
	{/if}
</header>
