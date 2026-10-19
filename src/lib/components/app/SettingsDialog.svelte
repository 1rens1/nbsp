<script lang="ts">
	import { setMode, userPrefersMode } from 'mode-watcher';
	import { IconPrinter, IconRefresh, IconTrash, IconX } from '@tabler/icons-svelte';
	import { toast } from 'svelte-sonner';
	import * as AlertDialog from '$lib/components/ui/alert-dialog';
	import { Badge } from '$lib/components/ui/badge';
	import { Button } from '$lib/components/ui/button';
	import * as Dialog from '$lib/components/ui/dialog';
	import * as Field from '$lib/components/ui/field';
	import { Separator } from '$lib/components/ui/separator';
	import { Skeleton } from '$lib/components/ui/skeleton';
	import { Spinner } from '$lib/components/ui/spinner';
	import { Switch } from '$lib/components/ui/switch';
	import * as Tabs from '$lib/components/ui/tabs';
	import { api, type DeviceStatus, type PrinterInfo } from '$lib/api';
	import { caps } from '$lib/state/caps.svelte';
	import { output } from '$lib/state/output.svelte';
	import { printer } from '$lib/state/printer.svelte';
	import { settings } from '$lib/state/settings.svelte';
	import { fmtSize } from '$lib/units';
	import Hint from './Hint.svelte';
	import Segmented from './Segmented.svelte';

	let { open = $bindable(false) }: { open: boolean } = $props();

	let tab = $state('general');
	let busy = $state('');
	let info = $state<PrinterInfo | null>(null);
	let device = $state<DeviceStatus | null>(null);
	let confirm = $state<'nozzle' | 'clean' | null>(null);
	/** Nozzle check and cleaning speak Epson's ESC/P protocol (through gutenprint's escputil). */
	const epson = $derived(/epson/i.test(info?.model ?? ''));
	const cal = $derived(settings.calibrationFor(settings.printer));

	$effect(() => {
		if (open) caps.refresh();
	});

	$effect(() => {
		if (open && tab === 'printer' && settings.printer) refreshPrinter();
	});

	async function refreshPrinter() {
		busy = 'info';
		try {
			info = await api.printerInfo(settings.printer);
			// The status query talks to the raw USB device; never while CUPS is sending it a job.
			device = !epson
				? null
				: info.jobs.length
				? { available: false, status: '', detail: 'Maintenance is available when the queue is empty.' }
				: await api.deviceStatus(info.model);
		} catch (e) {
			toast.error(String(e));
		} finally {
			busy = '';
		}
	}

	async function act(name: string, f: () => Promise<unknown>, done?: string) {
		busy = name;
		try {
			await f();
			if (done) toast.success(done);
		} catch (e) {
			toast.error(String(e));
		} finally {
			busy = '';
		}
	}

	const copyDiagnostics = () =>
		act(
			'diag',
			async () => navigator.clipboard.writeText(await api.diagnostics(settings.printer || null)),
			'Diagnostics copied'
		);

	const virtualPrinter = (remove: boolean) =>
		act(
			'vp',
			async () => {
				await api.setupVirtualPrinter(remove);
				await caps.refresh();
				await printer.refresh();
			},
			remove ? 'Virtual printer removed' : 'Virtual printer "NBSP" installed'
		);

	const maintenance = (action: 'nozzle' | 'clean') =>
		act(
			action,
			() => api.maintenance(settings.printer, action),
			action === 'nozzle' ? 'Nozzle check pattern sent' : 'Head cleaning started (about a minute)'
		);

	async function restart() {
		try {
			await settings.flush();
			await api.restartApp();
		} catch (e) {
			toast.error(String(e));
		}
	}

	/** Prints the 2-sheet test through the normal manual-duplex flow with the current calibration. */
	async function test() {
		busy = 'test';
		const sent = await output.printTest();
		busy = '';
		// The flip dialog takes over from here.
		if (sent) open = false;
	}

	const REASONS: Record<string, string> = {
		'media-empty': 'Out of paper',
		'media-jam': 'Paper jam',
		'marker-supply-low': 'Ink low',
		'marker-supply-empty': 'Out of ink',
		offline: 'Offline',
		paused: 'Paused',
		'cover-open': 'Cover open'
	};
	const reason = (r: string) => REASONS[r.replace(/-(error|warning|report)$/, '')] ?? r;
	const kb = (n: number) => (n > 1e6 ? `${(n / 1e6).toFixed(1)} MB` : `${Math.round(n / 1e3)} KB`);
</script>

{#snippet row(label: string, value: string)}
	<div class="flex items-baseline justify-between gap-4 text-sm">
		<span class="shrink-0 text-muted-foreground">{label}</span>
		<span class="min-w-0 truncate text-right" title={value}>{value || 'None'}</span>
	</div>
{/snippet}

<Dialog.Root bind:open>
	<Dialog.Content class="flex h-[min(36rem,calc(100dvh-4rem))] flex-col sm:max-w-lg" onOpenAutoFocus={(e) => e.preventDefault()}>
		<Dialog.Header>
			<Dialog.Title>Settings</Dialog.Title>
			<Dialog.Description class="sr-only">App, printer and double-sided settings</Dialog.Description>
		</Dialog.Header>

		<Tabs.Root bind:value={tab} class="min-h-0 flex-1 gap-4">
			<Tabs.List class="w-full">
				<Tabs.Trigger value="general">General</Tabs.Trigger>
				<Tabs.Trigger value="printer">Printer</Tabs.Trigger>
				<Tabs.Trigger value="duplex">Output</Tabs.Trigger>
			</Tabs.List>

			<Tabs.Content value="general" class="min-h-0 overflow-y-auto overflow-x-hidden">
				<Field.Group class="gap-5">
					<Field.Field class="gap-2">
						<Field.Label>Theme</Field.Label>
						<Segmented
							bind:value={() => userPrefersMode.current, (v) => setMode(v)}
							options={[
								{ value: 'light', label: 'Light' },
								{ value: 'dark', label: 'Dark' },
								{ value: 'system', label: 'System' }
							]}
						/>
					</Field.Field>
					<Field.Field class="gap-2">
						<Field.Label>Units</Field.Label>
						<Segmented
							bind:value={settings.unit}
							options={[
								{ value: 'mm', label: 'Millimetres' },
								{ value: 'in', label: 'Inches' }
							]}
						/>
					</Field.Field>
					<Field.Field orientation="horizontal">
						<Field.Label for="titlebar">
							System title bar
							<Hint text="Uses your desktop's window frame: its shadow, rounded corners and buttons. Turn off for NBSP's compact title bar." />
						</Field.Label>
						<Switch id="titlebar" bind:checked={settings.systemTitleBar} />
					</Field.Field>
					{#if settings.systemTitleBar !== settings.titleBarApplied}
						<Field.Field orientation="horizontal">
							<Field.Description>Takes effect after a restart.</Field.Description>
							<Button size="sm" onclick={restart}>Restart</Button>
						</Field.Field>
					{/if}
					{#if settings.customPapers.length}
						<Separator />
						<Field.Set class="gap-2">
							<Field.Legend variant="label" class="mb-0">Custom paper sizes</Field.Legend>
							{#each settings.customPapers as p (p.id)}
								<div class="flex items-center gap-2 text-sm">
									<span class="min-w-0 flex-1 truncate" title={p.name}>{p.name}</span>
									<span class="text-xs text-muted-foreground tabular-nums">{fmtSize(p.w, p.h, settings.unit)}</span>
									<Button
										variant="ghost"
										size="icon-xs"
										aria-label="Delete {p.name}"
										title="Delete"
										onclick={() => {
											settings.deletePaper(p.id);
											if (settings.s.paperId === p.id) settings.setPaper(printer.papers[0]);
										}}
									>
										<IconTrash />
									</Button>
								</div>
							{/each}
						</Field.Set>
					{/if}
					<Separator />
					{#if caps.value.installer}
						<Field.Field orientation="horizontal">
							<Field.Content>
								<Field.Label>Virtual printer</Field.Label>
								<Field.Description>Print to “NBSP” from any app to open the job here.</Field.Description>
							</Field.Content>
							<Button variant="outline" size="sm" onclick={() => virtualPrinter(caps.value.virtualPrinter)} disabled={!!busy}>
								{#if busy === 'vp'}<Spinner data-icon="inline-start" />{/if}
								{caps.value.virtualPrinter ? 'Remove' : 'Install'}
							</Button>
						</Field.Field>
					{/if}
					<Field.Field orientation="horizontal">
						<Field.Content>
							<Field.Label>Office documents</Field.Label>
							<Field.Description>.docx, .xlsx, .pptx, .odt… via LibreOffice</Field.Description>
						</Field.Content>
						<Badge variant={caps.value.office ? 'secondary' : 'outline'}>{caps.value.office ? 'Available' : 'Not installed'}</Badge>
					</Field.Field>
					<Field.Field orientation="horizontal">
						<Field.Content>
							<Field.Label>Diagnostics</Field.Label>
							<Field.Description>Copy versions and printer capabilities for a bug report.</Field.Description>
						</Field.Content>
						<Button variant="outline" size="sm" onclick={copyDiagnostics}>Copy</Button>
					</Field.Field>
				</Field.Group>
			</Tabs.Content>

			<Tabs.Content value="printer" class="min-h-0 overflow-y-auto overflow-x-hidden">
				<Field.Group class="gap-5">
					<div class="flex flex-col gap-2">
						<div class="flex items-center gap-2">
							<span class="truncate text-sm font-medium" title={settings.printer}>{settings.printer || 'No printer'}</span>
							{#if info}
								<!-- The printer's own report beats CUPS' queue state, except when the queue is stopped. -->
								<Badge
									variant={info.state === 'stopped' ? 'destructive' : 'secondary'}
									title={device?.available && info.state !== 'stopped' ? 'Reported by the printer' : 'CUPS queue state'}
								>
									{device?.available && info.state !== 'stopped' ? device.status : info.state}
								</Badge>
							{/if}
							<Button variant="ghost" size="icon-xs" class="ml-auto" aria-label="Refresh" title="Refresh" onclick={refreshPrinter} disabled={!!busy}>
								{#if busy === 'info'}<Spinner />{:else}<IconRefresh />{/if}
							</Button>
						</div>
						{#if info}
							{@const [model, driver] = info.model.split(' - ')}
							{@render row('Model', model)}
							{#if driver}{@render row('Driver', driver)}{/if}
							{@render row('Connection', info.uri.replace(/\?.*/, '').replace(/%20/g, ' '))}
							{#if info.location}{@render row('Location', info.location)}{/if}
							{#if info.reasons.length}
								<div class="flex flex-wrap gap-1">
									{#each info.reasons as r (r)}<Badge variant="destructive">{reason(r)}</Badge>{/each}
								</div>
							{/if}
							{#if info.state === 'stopped' || !info.accepting}
								<Button variant="outline" size="sm" onclick={() => act('resume', () => api.resumePrinter(info!.name).then(refreshPrinter), 'Printer resumed')}>
									Resume printer
								</Button>
							{/if}
						{:else}
							<Skeleton class="h-4 w-full" />
							<Skeleton class="h-4 w-2/3" />
						{/if}
					</div>

					<Separator />

					<Field.Set class="gap-2">
						<div class="flex items-center justify-between">
							<Field.Legend variant="label" class="mb-0">Queue</Field.Legend>
							{#if info?.jobs.length}
								<Button variant="ghost" size="xs" onclick={() => act('cancel', () => api.cancelAll(info!.name).then(refreshPrinter))}>Cancel all</Button>
							{/if}
						</div>
						{#if info?.jobs.length}
							{#each info.jobs as j (j.id)}
								<div class="flex items-center gap-2 text-sm">
									<span class="min-w-0 flex-1 truncate tabular-nums" title={j.id}>{j.id}</span>
									<span class="text-xs text-muted-foreground">{kb(j.size)}</span>
									<Button variant="ghost" size="icon-xs" aria-label="Cancel job" title="Cancel job" onclick={() => act('cancel', () => api.cancelJob(j.id).then(refreshPrinter))}>
										<IconX />
									</Button>
								</div>
							{/each}
						{:else}
							<Field.Description>Empty</Field.Description>
						{/if}
					</Field.Set>

					<Separator />

					<Field.Set class="gap-3">
						<Field.Legend variant="label" class="mb-0">Maintenance</Field.Legend>
						{#if epson}
							<div class="grid grid-cols-2 gap-2">
								<Button variant="outline" disabled={!!busy || !device?.available} onclick={() => (confirm = 'nozzle')}>
									{#if busy === 'nozzle'}<Spinner data-icon="inline-start" />{/if} Nozzle check
								</Button>
								<Button variant="outline" disabled={!!busy || !device?.available} onclick={() => (confirm = 'clean')}>
									{#if busy === 'clean'}<Spinner data-icon="inline-start" />{/if} Head cleaning
								</Button>
							</div>
							{#if device && !device.available}
								<Field.Description>{device.detail}</Field.Description>
							{/if}
						{/if}
						<div class="flex flex-wrap gap-2">
							{#if caps.value.epsonUtility && epson}
								<Button variant="ghost" size="sm" onclick={() => act('', api.openEpsonUtility)}>
									Epson Printer Utility <Hint text="Epson's own tool: ink levels, alignment, more." />
								</Button>
							{/if}
							<Button variant="ghost" size="sm" onclick={() => api.openUrl(`http://localhost:631/printers/${encodeURIComponent(settings.printer)}`).catch((e) => toast.error(String(e)))}>
								CUPS page
							</Button>
						</div>
					</Field.Set>
				</Field.Group>
			</Tabs.Content>

			<Tabs.Content value="duplex" class="min-h-0 overflow-y-auto overflow-x-hidden">
				<Field.Group class="gap-5">
					<Field.Field orientation="horizontal">
						<Field.Label for="cal-up">
							Pages come out face up
							<Hint text={`Detected: face ${printer.detectedFaceUp ? 'up' : 'down'}. Face up stacks end with the last page on top, so NBSP can print in reverse.`} />
						</Field.Label>
						<Switch id="cal-up" checked={printer.faceUp} onCheckedChange={(v) => settings.setCalibration(settings.printer, { faceUp: v })} />
					</Field.Field>
					{#if !printer.duplex}
						<Separator />
						<Field.Set class="gap-5">
							<Field.Legend variant="label" class="mb-0">Double-sided</Field.Legend>
							<Field.Description>
								Print the test and flip the stack when asked. Each back should match its front, with the arrows pointing the same way.
							</Field.Description>
							<Field.Field orientation="horizontal">
								<Field.Label for="cal-order">Backs in same order <Hint text="Turn off if backs land on the wrong sheets." /></Field.Label>
								<Switch id="cal-order" checked={cal.sameOrder} onCheckedChange={(v) => settings.setCalibration(settings.printer, { sameOrder: v })} />
							</Field.Field>
							<Field.Field orientation="horizontal">
								<Field.Label for="cal-rot">Rotate backs 180° <Hint text="Turn on if backs come out upside down." /></Field.Label>
								<Switch id="cal-rot" checked={cal.rotateBacks} onCheckedChange={(v) => settings.setCalibration(settings.printer, { rotateBacks: v })} />
							</Field.Field>
							<Button variant="outline" onclick={test} disabled={!!busy || !settings.printer || output.phase !== 'idle'}>
								{#if busy === 'test'}<Spinner data-icon="inline-start" />{:else}<IconPrinter data-icon="inline-start" />{/if}
								Print 2-sheet test
							</Button>
						</Field.Set>
					{/if}
				</Field.Group>
			</Tabs.Content>
		</Tabs.Root>
	</Dialog.Content>
</Dialog.Root>

<AlertDialog.Root open={confirm !== null} onOpenChange={(o) => !o && (confirm = null)}>
	<AlertDialog.Content>
		<AlertDialog.Header>
			<AlertDialog.Title>{confirm === 'nozzle' ? 'Print nozzle check?' : 'Clean print head?'}</AlertDialog.Title>
			<AlertDialog.Description>
				{confirm === 'nozzle'
					? 'Prints a test pattern on one sheet of plain A4. Gaps in the lines mean the head needs cleaning.'
					: 'Uses some ink and takes about a minute. Run a nozzle check afterwards.'}
			</AlertDialog.Description>
		</AlertDialog.Header>
		<AlertDialog.Footer>
			<AlertDialog.Cancel>Cancel</AlertDialog.Cancel>
			<AlertDialog.Action
				onclick={() => {
					const a = confirm!;
					confirm = null;
					maintenance(a);
				}}
			>
				{confirm === 'nozzle' ? 'Print' : 'Clean'}
			</AlertDialog.Action>
		</AlertDialog.Footer>
	</AlertDialog.Content>
</AlertDialog.Root>
