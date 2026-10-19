<script lang="ts">
	import { onMount } from 'svelte';
	import { listen } from '@tauri-apps/api/event';
	import { getCurrentWebview } from '@tauri-apps/api/webview';
	import { getCurrentWindow } from '@tauri-apps/api/window';
	import { open as openDialog } from '@tauri-apps/plugin-dialog';
	import { IconFileImport } from '@tabler/icons-svelte';
	import * as Resizable from '$lib/components/ui/resizable';
	import { toast } from 'svelte-sonner';
	import { api, initThumbs, type Launch } from '$lib/api';
	import { caps } from '$lib/state/caps.svelte';
	import { job } from '$lib/state/job.svelte';
	import { output } from '$lib/state/output.svelte';
	import { printer } from '$lib/state/printer.svelte';
	import { settings } from '$lib/state/settings.svelte';
	import DuplexDialog from '$lib/components/app/DuplexDialog.svelte';
	import LargeJobDialog from '$lib/components/app/LargeJobDialog.svelte';
	import PagesPanel from '$lib/components/app/PagesPanel.svelte';
	import Preview from '$lib/components/app/Preview.svelte';
	import PromptDialog from '$lib/components/app/PromptDialog.svelte';
	import ScanDialog from '$lib/components/app/ScanDialog.svelte';
	import SettingsPanel from '$lib/components/app/SettingsPanel.svelte';
	import SettingsDialog from '$lib/components/app/SettingsDialog.svelte';
	import TopBar from '$lib/components/app/TopBar.svelte';

	let ready = $state(false);
	let scanOpen = $state(false);
	let settingsOpen = $state(false);
	let prompt = $state<'preset' | 'paper' | null>(null);
	let dropping = $state(false);
	let winW = $state(1280);
	/** Pane minimums in px, converted to the % paneforge wants. */
	const minPct = (px: number) => Math.min(45, (px / Math.max(winW, 1)) * 100);

	const FILTERS = [
		{
			name: 'Documents',
			extensions: ['pdf', 'png', 'jpg', 'jpeg', 'webp', 'doc', 'docx', 'odt', 'rtf', 'txt', 'xls', 'xlsx', 'ods', 'csv', 'ppt', 'pptx', 'odp']
		}
	];

	async function openFiles() {
		const picked = await openDialog({ multiple: true, filters: FILTERS });
		if (picked) await job.open(Array.isArray(picked) ? picked : [picked]);
	}

	/** Applies a command-line request: `nbsp file.pdf`, `nbsp print …`, `nbsp scan`. */
	async function applyLaunch(l: Launch) {
		if (l.preset) {
			const p = settings.presets.find((x) => x.name === l.preset);
			if (p) settings.applyPreset(p);
			else toast.error(`No preset named "${l.preset}"`);
		}
		if (l.printer) {
			if (printer.list.some((p) => p.name === l.printer)) settings.printer = l.printer;
			else toast.error(`No printer named "${l.printer}"`);
		}
		// The command line asked for something this printer can't do: open it for review instead.
		let bad = '';
		if (Object.keys(l.settings).length || Object.keys(l.ppd).length) {
			if (l.printer || 'paperId' in l.settings) await printer.loadOptions();
			settings.applyOverrides(l.settings, l.ppd);
			const id = settings.s.paperId;
			const paper = printer.papers.find((p) => p.id === id);
			const c = printer.custom;
			if (paper && !('paper' in l.settings)) settings.setPaper(paper);
			else if ('paper' in l.settings) {
				const { w, h } = settings.s.paper;
				if (!c || w < c.minW || w > c.maxW || h < c.minH || h > c.maxH) bad = `${settings.printer} can't print ${w}×${h} mm`;
			} else if ('paperId' in l.settings && !paper) bad = `${settings.printer} has no paper size "${id}"`;
			if (bad) toast.error(bad);
		}
		if (l.scan) scanOpen = true;
		// Virtual-printer jobs that arrived while NBSP was closed. A command-line print is about its
		// own files, so they wait until it's sent, and then keep the window open.
		const inbox = () => (l.inbox.length ? job.open(l.inbox, { consume: true }) : undefined);
		if (!l.print || bad) await inbox();
		if (l.files.length) {
			// Printing from the CLI means "these files", not whatever was already open.
			if (l.print) job.clear();
			await job.open(l.files);
		}
		if (!l.print || bad) return;
		await printer.loadOptions();
		// Asked for on the command line, so no "large job" confirmation.
		const sent = await output.print(true);
		if (sent) {
			// Single pass: done once sent. Double-sided: done after the backs (or cancel).
			while (output.phase !== 'idle') await new Promise((r) => setTimeout(r, 300));
		}
		await inbox();
		if (sent && l.quitAfter && !l.inbox.length && !job.loading) getCurrentWindow().close();
	}

	/** Pasted files and images arrive as bytes; copied files (file manager) as URIs or plain paths. */
	async function paste(e: ClipboardEvent) {
		const t = e.target;
		if (t instanceof HTMLInputElement || t instanceof HTMLTextAreaElement || (t instanceof HTMLElement && t.isContentEditable)) return;
		const data = e.clipboardData;
		if (!data) return;
		const stamp = new Date().toISOString().slice(0, 19).replace(/[:T]/g, '-');
		const blobs = [...data.files].map((f, i) => ({ f, name: /^image\.\w+$/.test(f.name) ? `Pasted ${stamp}${i ? `-${i}` : ''}.${f.name.split('.').pop()}` : f.name }));
		const local = (data.getData('text/uri-list') || data.getData('x-special/gnome-copied-files') || data.getData('text/plain'))
			.split(/\r?\n/)
			.map((l) => l.trim())
			.filter((l) => l.startsWith('file://') || l.startsWith('/'))
			.map((l) => (l.startsWith('file://') ? decodeURIComponent(new URL(l).pathname) : l));
		if (!blobs.length && !local.length) return;
		e.preventDefault();
		try {
			const saved = await Promise.all(blobs.map(async ({ f, name }) => api.savePasted(name, await f.arrayBuffer())));
			if (saved.length) await job.open(saved, { consume: true });
			if (local.length) await job.open(local);
		} catch (err) {
			toast.error(String(err));
		}
	}

	onMount(() => {
		const unlisten: Promise<() => void>[] = [];
		(async () => {
			await Promise.all([settings.init(), initThumbs()]);
			ready = true;
			// Drop the boot loader from app.html once the first real frame is up.
			requestAnimationFrame(() => {
				document.documentElement.dataset.ready = '';
				setTimeout(() => document.getElementById('boot')?.remove(), 200);
			});
			caps.refresh().then(() => {
				const missing = caps.value.missing;
				if (missing.length) toast.error(`Printing needs ${missing.join(', ')}`, { description: 'Install CUPS and curl, then restart NBSP.', duration: Infinity });
			});
			await printer.refresh();
			const launch = await api.takeLaunch();
			if (launch) await applyLaunch(launch);
		})();
		unlisten.push(listen<Launch>('launch', (e) => applyLaunch(e.payload)));
		unlisten.push(listen<string[]>('inbox-files', (e) => job.open(e.payload, { consume: true })));
		unlisten.push(
			getCurrentWebview().onDragDropEvent((e) => {
				const t = e.payload.type;
				dropping = t === 'enter' || t === 'over';
				if (t === 'drop') job.open(e.payload.paths);
			})
		);
		// Pending settings reach disk before the window goes.
		unlisten.push(getCurrentWindow().onCloseRequested(() => settings.flush()));
		const poll = setInterval(() => document.visibilityState === 'visible' && printer.refresh(), 10_000);
		return () => {
			clearInterval(poll);
			for (const u of unlisten) u.then((f) => f());
		};
	});

	// Documents are laid out for the paper, so a paper change converts them again (debounced: custom
	// sizes are typed digit by digit).
	$effect(() => {
		void [settings.s.reflow, settings.s.paper.w, settings.s.paper.h];
		if (!ready) return;
		const t = setTimeout(() => job.reflow(), 500);
		return () => clearTimeout(t);
	});

	$effect(() => {
		if (!ready) return;
		document.documentElement.toggleAttribute('data-custom-titlebar', !settings.titleBarApplied);
	});

	$effect(() => {
		getCurrentWindow().setTitle(job.pages.length ? `${job.title} - NBSP` : 'NBSP');
	});

	// Printer-specific options; re-fetch when the printer changes.
	$effect(() => {
		void settings.printer;
		printer.loadOptions();
	});

	// Persist settings (debounced) whenever anything in them changes.
	$effect(() => {
		if (!ready) return;
		JSON.stringify([settings.s, settings.presets, settings.customPapers, settings.calibrations, settings.printer, settings.systemTitleBar, settings.unit]);
		settings.save();
	});

	function onkeydown(e: KeyboardEvent) {
		const el = e.target as HTMLElement;
		if (el.closest('input, textarea, [role=dialog], [role=alertdialog], [role=menu], [role=slider], [role=listbox][data-select-content]')) return;
		const ctrl = e.ctrlKey || e.metaKey;
		const k = e.key.toLowerCase();
		const run = (f: () => void) => {
			e.preventDefault();
			f();
		};
		if (ctrl && k === 'o') run(openFiles);
		else if (ctrl && k === ',') run(() => (settingsOpen = true));
		else if (ctrl && k === 'p') run(() => output.print());
		else if (ctrl && k === 's') run(() => output.exportPages());
		else if (ctrl && k === 'a') run(() => job.selectAll());
		else if (ctrl && k === 'd') run(() => job.duplicate(job.targets()));
		else if (ctrl && (k === 'y' || (k === 'z' && e.shiftKey))) run(() => job.redo());
		else if (ctrl && k === 'z') run(() => job.undo());
		else if (!ctrl && k === 'r') run(() => job.rotate(job.targets(), e.shiftKey ? -90 : 90));
		else if (k === 'delete' || k === 'backspace') run(() => job.remove(job.targets()));
		else if (k === 'escape') job.selected.clear();
		else if (k.startsWith('arrow') && (el === document.body || el.closest('[aria-multiselectable]'))) {
			// Only from the page itself or the pages list: tabs and toggle groups use arrows too.
			const dir = k === 'arrowleft' || k === 'arrowup' ? -1 : 1;
			run(() => (e.altKey ? job.nudge(dir) : job.step(dir, e.shiftKey)));
		}
	}
</script>

<svelte:window onpaste={paste} {onkeydown} bind:innerWidth={winW} />

<div class="flex min-h-0 flex-1 flex-col">
	<TopBar onopen={openFiles} onscan={() => (scanOpen = true)} onsettings={() => (settingsOpen = true)} />
	<Resizable.PaneGroup direction="horizontal" class="min-h-0 flex-1" autoSaveId="nbsp-panes">
		<Resizable.Pane defaultSize={30} minSize={minPct(190)}>
			<PagesPanel onopen={openFiles} />
		</Resizable.Pane>
		<Resizable.Handle />
		<Resizable.Pane defaultSize={46} minSize={minPct(260)}>
			<Preview />
		</Resizable.Pane>
		<Resizable.Handle />
		<Resizable.Pane defaultSize={24} minSize={minPct(270)} maxSize={45}>
			<SettingsPanel onprompt={(k) => (prompt = k)} />
		</Resizable.Pane>
	</Resizable.PaneGroup>
</div>

{#if dropping}
	<div
		class="pointer-events-none fixed inset-2 flex items-center justify-center rounded-xl border-2 border-dashed border-primary bg-background/80"
	>
		<div class="flex flex-col items-center gap-2 text-muted-foreground">
			<IconFileImport class="size-10" />
			<span class="text-sm">Drop to add pages</span>
		</div>
	</div>
{/if}

<ScanDialog bind:open={scanOpen} />
<SettingsDialog bind:open={settingsOpen} />
<PromptDialog bind:kind={prompt} />
<DuplexDialog />
<LargeJobDialog />
