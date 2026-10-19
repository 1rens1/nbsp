<script lang="ts">
	import {
		IconChevronDown,
		IconDeviceFloppy,
		IconLink,
		IconLinkOff,
		IconPlus,
		IconTrash
	} from '@tabler/icons-svelte';
	import { Button } from '$lib/components/ui/button';
	import * as Collapsible from '$lib/components/ui/collapsible';
	import * as DropdownMenu from '$lib/components/ui/dropdown-menu';
	import * as Field from '$lib/components/ui/field';
	import { Input } from '$lib/components/ui/input';
	import * as Select from '$lib/components/ui/select';
	import { Separator } from '$lib/components/ui/separator';
	import { Slider } from '$lib/components/ui/slider';
	import { Spinner } from '$lib/components/ui/spinner';
	import { Switch } from '$lib/components/ui/switch';
	import * as Tabs from '$lib/components/ui/tabs';
	import { scrollFade } from '$lib/actions/scroll-fade';
	import { summarizePages, type Align, type NumberPosition } from '$lib/layout';
	import { job } from '$lib/state/job.svelte';
	import { output } from '$lib/state/output.svelte';
	import { matches, printer } from '$lib/state/printer.svelte';
	import { settings, type PaperKind, type PrintSettings } from '$lib/state/settings.svelte';
	import { cn } from '$lib/utils';
	import Hint from './Hint.svelte';
	import IconButton from './IconButton.svelte';
	import NumberInput from './NumberInput.svelte';
	import Segmented from './Segmented.svelte';

	let { onprompt }: { onprompt: (kind: 'preset' | 'paper') => void } = $props();

	const CUSTOM = '__custom';
	const ALIGNS: Align[] = ['top-left', 'top-center', 'top-right', 'middle-left', 'middle-center', 'middle-right', 'bottom-left', 'bottom-center', 'bottom-right'];
	const s = $derived(settings.s);
	let linked = $state(true);

	const SUBSETS: Record<PrintSettings['subset'], string> = {
		all: 'All pages',
		odd: 'Odd pages',
		even: 'Even pages',
		selected: 'Selected pages',
		custom: 'Custom range'
	};
	const DUPLEX = { off: 'Single-sided', long: 'Flip on long edge', short: 'Flip on short edge' };
	const ORIENTATION = { auto: 'Automatic', portrait: 'Portrait', landscape: 'Landscape' };
	const ORDER = { rows: 'Across, then down', columns: 'Down, then across' };
	const SCALE = { fit: 'Fit to page', shrink: 'Shrink to fit', actual: 'Actual size', custom: 'Custom' };
	/** Driver choice names that read badly as they are. */
	const NICER: Record<string, string> = { PLAIN: 'Plain paper' };
	const label = (key: string, c: string) => NICER[c] ?? printer.label(key, c);
	const FORMATS = ['{page}', '{page} / {total}', 'Page {page}', 'Page {page} of {total}', '- {page} -'];
	const POSITIONS: Record<NumberPosition, string> = {
		'bottom-center': 'Bottom center',
		'bottom-right': 'Bottom right',
		'bottom-left': 'Bottom left',
		'top-center': 'Top center',
		'top-right': 'Top right',
		'top-left': 'Top left'
	};
	let customFormat = $state(false);
	const isCustom = $derived(customFormat || !FORMATS.includes(s.pageNumbers.format));
	/** Format rendered for the first numbered page, e.g. "Page 1 of 9". */
	function example(f: string) {
		const n = s.pageNumbers;
		const last = n.start + Math.max(output.pages.length - n.skip, 1) - 1;
		return f.replaceAll('{page}', String(n.start)).replaceAll('{n}', String(n.start)).replaceAll('{total}', String(last));
	}

	const ADJUST = ['BrightnessValue', 'ContrastValue', 'SaturationValue', 'CyanValue', 'MagentaValue', 'YellowValue'];

	/** Current value of a PPD option: the user's choice (maybe saved under another driver's name), else the printer default. */
	function ppd(key: string) {
		if (printer.option(key)?.choices.includes(s.ppd[key])) return s.ppd[key];
		for (const [k, v] of Object.entries(s.ppd)) {
			const t = printer.translate(k, v);
			if (t?.[0] === key) return t[1];
		}
		return printer.option(key)?.default ?? '';
	}

	/** Sets a PPD option; any other option it now conflicts with falls back to its first allowed choice. */
	function setPpd(key: string, v: string) {
		const next = { ...s.ppd, [key]: v };
		// The same setting saved under another driver's name would compete with this choice.
		for (const k of Object.keys(next)) if (k !== key && printer.key(k) === key) delete next[k];
		const get = (k: string) => next[k] ?? printer.option(k)?.default ?? (k === 'PageSize' ? s.paperId : '');
		for (const [k1, v1, k2, v2] of printer.constraints) {
			const other = k1 === key && v1 === v ? k2 : k2 === key && v2 === v ? k1 : null;
			if (!other || other === 'PageSize' || !matches(get(other), other === k2 ? v2 : v1)) continue;
			const ok = printer.option(other)?.choices.find((c) => !printer.blocked(other, c, get));
			if (ok) next[other] = ok;
		}
		s.ppd = next;
	}
	const current = (k: string) => (k === 'PageSize' ? s.paperId : ppd(k));
	/** A setting worth showing: the printer has it, with something to choose. */
	const choosable = (name: string) => {
		const k = printer.key(name);
		return k && printer.option(k)!.choices.length > 1 ? k : undefined;
	};
	const colorKey = $derived(choosable('Color'));
	const mediaKey = $derived(choosable('MediaType'));
	const qualityKey = $derived(choosable('PrintQuality'));
	const blocked = (key: string, choice: string) => printer.blocked(key, choice, current);

	const paper = $derived(printer.papers.find((p) => p.id === s.paperId));
	const customPapers = $derived(printer.papers.filter((p) => p.custom));
	const nup = $derived(s.mode === 'normal' && s.perSheet > 1);
	const booklet = $derived(s.mode === 'booklet');
	/** Which pages a non-"all" choice resolves to; "All pages" needs no echo. */
	const pagesSummary = $derived(job.pages.length && s.subset !== 'all' ? summarizePages(output.picked) : '');
	const adjusted = $derived(ADJUST.some((k) => printer.option(k) && ppd(k) !== printer.option(k)!.default));

	function setMargin(side: keyof typeof s.margins, v: number) {
		s.margins = linked ? { top: v, right: v, bottom: v, left: v } : { ...s.margins, [side]: v };
	}

	/** Margins from before Borderless zeroed them, restored when it's turned off again. */
	let marginsBefore: typeof s.margins | null = null;
	function setBorderless(on: boolean) {
		setPpd('Borderless', on ? 'On' : 'Off');
		if (on) {
			marginsBefore = { ...s.margins };
			s.margins = { top: 0, right: 0, bottom: 0, left: 0 };
		} else if (marginsBefore) {
			s.margins = marginsBefore;
			marginsBefore = null;
		}
	}

	const PAPER_GROUPS: [PaperKind, string][] = [
		['document', ''],
		['photo', 'Photo'],
		['envelope', 'Envelopes']
	];

	function resetAdjustments() {
		const next = { ...s.ppd };
		for (const k of [...ADJUST, 'GammaValue', 'CorrectionColor']) delete next[k];
		s.ppd = next;
	}
</script>

{#snippet choice<T extends string>(value: T, labels: Record<T, string>, onchange: (v: T) => void, disabled = false)}
	<Select.Root type="single" {value} onValueChange={(v) => onchange(v as T)} {disabled}>
		<Select.Trigger class="w-full min-w-0 @md/field-group:w-44!"><span class="truncate">{labels[value]}</span></Select.Trigger>
		<Select.Content>
			<Select.Group>
				{#each Object.entries(labels) as [k, label] (k)}
					<Select.Item value={k} label={label as string}>{label}</Select.Item>
				{/each}
			</Select.Group>
		</Select.Content>
	</Select.Root>
{/snippet}

<aside class="flex h-full min-w-0 flex-col">
	<header class="flex h-11 shrink-0 items-center gap-1 border-b pr-1.5 pl-3">
		<span class="text-sm font-medium">Print</span>
		<DropdownMenu.Root>
			<DropdownMenu.Trigger>
				{#snippet child({ props })}
					<Button {...props} variant="ghost" size="sm" class="ml-auto min-w-0">
						<IconDeviceFloppy data-icon="inline-start" />
						<span class="truncate">Presets</span>
					</Button>
				{/snippet}
			</DropdownMenu.Trigger>
			<DropdownMenu.Content align="end" class="w-56">
				{#if settings.presets.length}
					<DropdownMenu.Group>
						{#each settings.presets as p (p.name)}
							<DropdownMenu.Item onclick={() => settings.applyPreset(p)} title={p.name}>
								<span class="flex-1 truncate">{p.name}</span>
								<button
									class="text-muted-foreground hover:text-destructive"
									aria-label="Delete preset"
									title="Delete preset"
									onclick={(e) => {
										e.stopPropagation();
										settings.deletePreset(p.name);
									}}
								>
									<IconTrash />
								</button>
							</DropdownMenu.Item>
						{/each}
					</DropdownMenu.Group>
					<DropdownMenu.Separator />
				{/if}
				<DropdownMenu.Group>
					<DropdownMenu.Item onclick={() => onprompt('preset')}><IconPlus /> Save current as preset…</DropdownMenu.Item>
				</DropdownMenu.Group>
			</DropdownMenu.Content>
		</DropdownMenu.Root>
	</header>

	<div class="scroll-fade min-h-0 flex-1 overflow-y-auto" use:scrollFade>
		<Field.Group class="gap-4 p-4">
			<Field.Field orientation="responsive" class="gap-2">
				<Field.Label class="min-w-0">
					Pages
					{#if pagesSummary}
						<span class="truncate font-normal text-muted-foreground tabular-nums" title={pagesSummary}>({pagesSummary})</span>
					{/if}
				</Field.Label>
				{@render choice(s.subset, SUBSETS, (v) => (s.subset = v))}
			</Field.Field>
			{#if s.subset === 'custom'}
				<Input placeholder="1-3, 5, 8-" bind:value={s.range} aria-invalid={output.rangeError} aria-label="Page range" />
			{/if}

			<Field.Field orientation="horizontal">
				<Field.Label for="copies">Copies</Field.Label>
				<NumberInput id="copies" bind:value={s.copies} min={1} max={999} />
			</Field.Field>
			{#if s.copies > 1}
				<Field.Field orientation="horizontal">
					<Field.Label for="collate">
						Collate
						<Hint text="On: whole sets in order (1 2 3, 1 2 3). Off: each page repeated (1 1, 2 2, 3 3)." />
					</Field.Label>
					<Switch id="collate" bind:checked={s.collate} />
				</Field.Field>
			{/if}
		</Field.Group>

		<Separator />

		<Tabs.Root value="layout" class="gap-0">
			<Tabs.List class="mx-4 mt-3 w-[calc(100%-2rem)]">
				<Tabs.Trigger value="layout">Layout</Tabs.Trigger>
				<Tabs.Trigger value="paper">Paper</Tabs.Trigger>
				<Tabs.Trigger value="quality">Quality</Tabs.Trigger>
			</Tabs.List>

			<Tabs.Content value="layout">
				<Field.Group class="gap-4 p-4">
					<Field.Field orientation="horizontal">
						<Field.Label for="booklet">Booklet <Hint text="Fold the stack in half and staple. Always double-sided." /></Field.Label>
						<Switch
							id="booklet"
							checked={booklet}
							onCheckedChange={(v) => (s.mode = v ? 'booklet' : 'normal')}
						/>
					</Field.Field>

					{#if !booklet}
						<Field.Field class="gap-2">
							<Field.Label>Pages per sheet</Field.Label>
							<Segmented bind:value={s.perSheet} options={[1, 2, 4, 6, 9, 16].map((n) => ({ value: n, label: String(n) }))} />
						</Field.Field>
						{#if nup}
							<Field.Field orientation="responsive" class="gap-2">
								<Field.Label>Page order</Field.Label>
								{@render choice(s.order, ORDER, (v) => (s.order = v))}
							</Field.Field>
						{/if}
						<Field.Field orientation="responsive" class="gap-2">
							<Field.Label>
								Double-sided
								{#if !printer.duplex}<Hint text="Fronts print first, then you flip the stack and the backs print." />{/if}
							</Field.Label>
							{@render choice(s.duplex, DUPLEX, (v) => (s.duplex = v))}
						</Field.Field>
					{/if}

					{#if printer.faceUp}
						<Field.Field orientation="horizontal">
							<Field.Label for="ontop">First page on top <Hint text="Prints last page first, so the face-up stack is in order." /></Field.Label>
							<Switch id="ontop" bind:checked={s.firstOnTop} />
						</Field.Field>
					{/if}

					<Separator />

					<Field.Field orientation="responsive" class="gap-2">
						<Field.Label>Scale</Field.Label>
						{@render choice(s.scale, SCALE, (v) => (s.scale = v))}
					</Field.Field>
					{#if s.scale === 'custom'}
						<Segmented
							bind:value={s.scaleBy}
							options={[
								{ value: 'percent', label: 'Percent' },
								{ value: 'width', label: 'Width' },
								{ value: 'height', label: 'Height' }
							]}
						/>
						<Field.Field orientation="horizontal">
							{#if s.scaleBy === 'percent'}
								<Field.Label for="scale-pct" class="font-normal text-muted-foreground">Percent</Field.Label>
								<NumberInput id="scale-pct" bind:value={s.scalePct} min={5} max={400} step={5} unit="%" />
							{:else}
								<Field.Label for="scale-len" class="font-normal text-muted-foreground">
									Printed {s.scaleBy}
								</Field.Label>
								<NumberInput id="scale-len" bind:value={s.scaleLen} min={5} max={2000} step={1} length />
							{/if}
						</Field.Field>
					{/if}

					{#if !booklet && (nup || s.scale !== 'fit')}
						<Field.Field orientation="horizontal">
							<Field.Label>Position on sheet</Field.Label>
							<div class="grid grid-cols-3 gap-1" role="radiogroup" aria-label="Position on sheet">
								{#each ALIGNS as a (a)}
									<button
										type="button"
										role="radio"
										aria-checked={s.align === a}
										aria-label={a.replace('-', ' ')}
										title={a.replace('-', ' ')}
										class={cn(
											'size-5 rounded-sm border transition-colors hover:bg-accent',
											s.align === a && 'border-primary bg-primary hover:bg-primary'
										)}
										onclick={() => (s.align = a)}
									></button>
								{/each}
							</div>
						</Field.Field>
					{/if}

					{#if nup || booklet}
						<Field.Field orientation="horizontal">
							<Field.Label for="gutter">{booklet ? 'Binding gap' : 'Gap between pages'}</Field.Label>
							<NumberInput id="gutter" bind:value={s.gutter} max={50} step={0.5} length />
						</Field.Field>
					{/if}

					<Field.Field orientation="horizontal">
						<Field.Label for="autorot">Auto-rotate pages</Field.Label>
						<Switch id="autorot" bind:checked={s.autoRotate} />
					</Field.Field>
					{#if nup}
						<Field.Field orientation="horizontal">
							<Field.Label for="borders">Page borders</Field.Label>
							<Switch id="borders" bind:checked={s.borders} />
						</Field.Field>
					{/if}
					{#if booklet}
						<Field.Field orientation="horizontal">
							<Field.Label for="fold">Fold line</Field.Label>
							<Switch id="fold" bind:checked={s.foldLine} />
						</Field.Field>
					{/if}

					<Separator />

					<Field.Field orientation="horizontal">
						<Field.Label for="numbers">Page numbers</Field.Label>
						<Switch id="numbers" bind:checked={s.pageNumbers.enabled} />
					</Field.Field>
					{#if s.pageNumbers.enabled}
						<Field.Field orientation="responsive" class="gap-2">
							<Field.Label>Format</Field.Label>
							<Select.Root
								type="single"
								value={isCustom ? 'custom' : s.pageNumbers.format}
								onValueChange={(v) => {
									customFormat = v === 'custom';
									if (v !== 'custom') s.pageNumbers.format = v;
								}}
							>
								<Select.Trigger class="w-full min-w-0 @md/field-group:w-44!">
									<span class="truncate">{isCustom ? 'Custom' : example(s.pageNumbers.format)}</span>
								</Select.Trigger>
								<Select.Content>
									<Select.Group>
										{#each FORMATS as f (f)}
											<Select.Item value={f} label={example(f)}>{example(f)}</Select.Item>
										{/each}
										<Select.Item value="custom" label="Custom">Custom</Select.Item>
									</Select.Group>
								</Select.Content>
							</Select.Root>
						</Field.Field>
						{#if isCustom}
							<Input bind:value={s.pageNumbers.format} placeholder="Page {'{page}'} of {'{total}'}" aria-label="Custom format" />
							<Field.Description>{'{page}'} is the page number, {'{total}'} the last one.</Field.Description>
						{/if}
						<Field.Field orientation="responsive" class="gap-2">
							<Field.Label>Position</Field.Label>
							{@render choice(s.pageNumbers.position, POSITIONS, (v) => (s.pageNumbers.position = v))}
						</Field.Field>
						<div class="grid gap-4 @md/field-group:grid-cols-2 @md/field-group:gap-x-8">
						{#each [
							['size', 'Size', 6, 36, 1, 'pt'],
							['offset', 'From edge', 2, 40, 1, 'mm'],
							['start', 'Start at', 0, 9999, 1, ''],
							['skip', 'Skip first', 0, 999, 1, '']
						] as const as [key, label, min, max, stepBy, unit] (key)}
							<Field.Field orientation="horizontal" class="gap-2">
								<Field.Label class="w-24 shrink-0 font-normal whitespace-nowrap text-muted-foreground">
									{label}
									{#if key === 'skip'}<Hint text="Pages at the start left unnumbered, like a cover." />{/if}
								</Field.Label>
								<NumberInput bind:value={s.pageNumbers[key]} {min} {max} step={stepBy} unit={unit === 'mm' ? undefined : unit || undefined} length={unit === 'mm'} label={label} />
							</Field.Field>
						{/each}
						</div>
					{/if}
				</Field.Group>
			</Tabs.Content>

			<Tabs.Content value="paper">
				<Field.Group class="gap-4 p-4">
					<Field.Field orientation="responsive" class="gap-2">
						<Field.Label>Paper size</Field.Label>
						<!-- A function binding reads back s.paperId, so the menu never sticks on "Custom size…"
						     (picking it again after Cancel would otherwise do nothing). -->
						<Select.Root
							type="single"
							bind:value={
								() => s.paperId,
								(v) => {
									if (v === CUSTOM) return onprompt('paper');
									const p = printer.papers.find((x) => x.id === v);
									if (p) settings.setPaper(p);
								}
							}
						>
							<Select.Trigger class="w-full min-w-0 @md/field-group:w-44!" title={paper?.name}>
								<span class="truncate">{paper?.name ?? s.paperId}</span>
							</Select.Trigger>
							<Select.Content class="max-h-80">
								{#each PAPER_GROUPS as [kind, heading], g (kind)}
									{@const list = printer.papers.filter((p) => !p.custom && p.kind === kind)}
									{#if list.length}
										{#if g > 0}<Select.Separator />{/if}
										<Select.Group>
											{#if heading}<Select.GroupHeading>{heading}</Select.GroupHeading>{/if}
											{#each list as p (p.id)}
												<Select.Item value={p.id} label={p.name}>
													<span class="flex-1 truncate">{p.name}</span>
													<span class="text-xs text-muted-foreground tabular-nums">{p.w}×{p.h}</span>
												</Select.Item>
											{/each}
										</Select.Group>
									{/if}
								{/each}
								{#if customPapers.length}
									<Select.Separator />
									<Select.Group>
										<Select.GroupHeading>Custom</Select.GroupHeading>
										{#each customPapers as p (p.id)}
											<Select.Item value={p.id} label={p.name}>
												<span class="flex-1 truncate">{p.name}</span>
												<span class="text-xs text-muted-foreground tabular-nums">{p.w}×{p.h}</span>
											</Select.Item>
										{/each}
									</Select.Group>
								{/if}
								{#if printer.custom}
									<Select.Separator />
									<Select.Item value={CUSTOM} label="Custom size">
										<IconPlus class="size-4" /> Custom size…
									</Select.Item>
								{/if}
							</Select.Content>
						</Select.Root>
					</Field.Field>

					{#if job.canReflow}
						<Field.Field orientation="horizontal">
							<Field.Label for="reflow">
								Reflow text
								<Hint text="Word and ODT documents are laid out again for this paper size instead of scaling the pages." />
							</Field.Label>
							<Switch id="reflow" bind:checked={s.reflow} />
						</Field.Field>
					{/if}

					{#if !booklet}
						<Field.Field orientation="responsive" class="gap-2">
							<Field.Label>Orientation</Field.Label>
							{@render choice(s.orientation, ORIENTATION, (v) => (s.orientation = v))}
						</Field.Field>
					{/if}

					<Field.Field class="gap-2">
						<div class="flex items-center justify-between gap-2">
							<div class="flex items-center gap-1">
								<Field.Label>Margins</Field.Label>
								<IconButton
									icon={linked ? IconLink : IconLinkOff}
									label={linked ? 'Set each side separately' : 'Same on all sides'}
									size="icon-xs"
									onclick={() => (linked = !linked)}
								/>
							</div>
							{#if linked}
								<NumberInput bind:value={() => s.margins.top, (v) => setMargin('top', v)} label="All margins" max={60} step={0.5} length />
							{/if}
						</div>
						{#if !linked}
							<div class="grid gap-2 @md/field-group:grid-cols-2 @md/field-group:gap-x-8">
							{#each ['top', 'bottom', 'left', 'right'] as const as side (side)}
								<Field.Field orientation="horizontal" class="gap-2">
									<Field.Label class="font-normal text-muted-foreground capitalize">{side}</Field.Label>
									<NumberInput bind:value={() => s.margins[side], (v) => setMargin(side, v)} label="{side} margin" max={60} step={0.5} length />
								</Field.Field>
							{/each}
							</div>
						{/if}
					</Field.Field>

					{#if printer.option('Borderless')}
						{@const off = blocked('Borderless', 'On')}
						<Field.Field orientation="horizontal" data-disabled={off}>
							<Field.Label for="borderless">
								Borderless
								<Hint text={off ? 'Only for photo sizes (3R, 4R, 16:9, postcard).' : 'Photo paper only. Slightly enlarges the image.'} />
							</Field.Label>
							<Switch
								id="borderless"
								disabled={off}
								checked={ppd('Borderless') === 'On' && !off}
								onCheckedChange={setBorderless}
							/>
						</Field.Field>
					{/if}
				</Field.Group>
			</Tabs.Content>

			<Tabs.Content value="quality">
				<Field.Group class="gap-4 p-4">
					{#if colorKey}
						{@const k = colorKey}
						<Field.Field class="gap-2">
							<Segmented
								bind:value={() => ppd(k), (v) => setPpd(k, v)}
								options={printer.option(k)!.choices.map((c) => ({ value: c, label: label(k, c) }))}
							/>
						</Field.Field>
					{/if}
					{#if mediaKey}
						{@const k = mediaKey}
						<Field.Field orientation="responsive" class="gap-2">
							<Field.Label>Paper type</Field.Label>
							<Select.Root type="single" value={ppd(k)} onValueChange={(v) => setPpd(k, v)}>
								<Select.Trigger class="w-full min-w-0 @md/field-group:w-44!"><span class="truncate">{label(k, ppd(k))}</span></Select.Trigger>
								<Select.Content>
									<Select.Group>
										{#each printer.option(k)!.choices as c (c)}
											<Select.Item value={c} label={label(k, c)}>{label(k, c)}</Select.Item>
										{/each}
									</Select.Group>
								</Select.Content>
							</Select.Root>
						</Field.Field>
					{/if}
					{#if qualityKey}
						{@const k = qualityKey}
						<Field.Field class="gap-2">
							<Field.Label>
								Quality
								{#if printer.constraints.some((c) => c[0] === k || c[2] === k)}<Hint text="Some paper types limit the quality." />{/if}
							</Field.Label>
							<Segmented
								bind:value={() => ppd(k), (v) => setPpd(k, v)}
								options={printer.option(k)!.choices.map((c) => ({ value: c, label: label(k, c), disabled: blocked(k, c) }))}
							/>
						</Field.Field>
					{/if}
					{#if printer.option('CorrectionColor')}
						<Field.Field orientation="responsive" class="gap-2">
							<Field.Label>
								Color mode
								<Hint text="Epson Vivid is the driver's punchier default (the Windows driver's “Vivid” modes). Adobe RGB is for color-managed photos." />
							</Field.Label>
							<Select.Root type="single" value={ppd('CorrectionColor')} onValueChange={(v) => setPpd('CorrectionColor', v)}>
								<Select.Trigger class="w-full min-w-0 @md/field-group:w-44!"><span class="truncate">{label('CorrectionColor', ppd('CorrectionColor'))}</span></Select.Trigger>
								<Select.Content>
									<Select.Group>
										{#each printer.option('CorrectionColor')!.choices as c (c)}
											<Select.Item value={c} label={label('CorrectionColor', c)}>{label('CorrectionColor', c)}</Select.Item>
										{/each}
									</Select.Group>
								</Select.Content>
							</Select.Root>
						</Field.Field>
					{/if}

					{#if ADJUST.some((k) => printer.option(k))}
						<Collapsible.Root>
							<div class="flex items-center gap-1">
								<Collapsible.Trigger class="group flex flex-1 items-center gap-1 text-sm font-medium">
									<IconChevronDown class="size-4 transition-transform group-data-[state=closed]:-rotate-90" />
									Adjustments
									{#if adjusted}<span class="size-1.5 rounded-full bg-primary" aria-label="Changed"></span>{/if}
								</Collapsible.Trigger>
								{#if adjusted}
									<Button variant="ghost" size="xs" onclick={resetAdjustments}>Reset</Button>
								{/if}
							</div>
							<Collapsible.Content>
								<Field.Group class="gap-4 pt-4">
									{#each ADJUST.filter((k) => printer.option(k)) as key (key)}
										{@const opt = printer.option(key)!}
										{@const nums = opt.choices.map(Number)}
										<Field.Field class="gap-2">
											<div class="flex items-center justify-between">
												<Field.Label class="font-normal">{opt.label}</Field.Label>
												<span class="text-xs text-muted-foreground tabular-nums">{ppd(key)}</span>
											</div>
											<Slider
												type="single"
												min={Math.min(...nums)}
												max={Math.max(...nums)}
												step={1}
												value={Number(ppd(key))}
												onValueChange={(v) => setPpd(key, String(v))}
											/>
										</Field.Field>
									{/each}
									{#if printer.option('GammaValue')}
										<Field.Field orientation="horizontal">
											<Field.Label class="font-normal">Gamma</Field.Label>
											<Segmented
												class="w-auto"
												bind:value={() => ppd('GammaValue'), (v) => setPpd('GammaValue', v)}
												options={printer.option('GammaValue')!.choices.map((c) => ({ value: c, label: c }))}
											/>
										</Field.Field>
									{/if}
									{#if printer.option('MirrorImage')}
										<Field.Field orientation="horizontal">
											<Field.Label for="mirror" class="font-normal">Mirror <Hint text="For iron-on transfers." /></Field.Label>
											<Switch id="mirror" checked={ppd('MirrorImage') === 'On'} onCheckedChange={(v) => setPpd('MirrorImage', v ? 'On' : 'Off')} />
										</Field.Field>
									{/if}
								</Field.Group>
							</Collapsible.Content>
						</Collapsible.Root>
					{/if}

					{#if !printer.options.length}
						<Field.Description>No printer options available.</Field.Description>
					{/if}
				</Field.Group>
			</Tabs.Content>
		</Tabs.Root>
	</div>

	<footer class="shrink-0 border-t p-3">
		<Button
			size="lg"
			class={cn('w-full', output.phase === 'sending' && 'pointer-events-none')}
			disabled={!output.pages.length || !settings.printer || output.phase !== 'idle' || output.rangeError || !!output.layout.error}
			onclick={() => output.print()}
			title="Print (Ctrl P)"
		>
			{#if output.phase === 'sending'}<Spinner data-icon="inline-start" />{/if}
			Print
			{#if output.sheets}
				<span class="font-normal tabular-nums opacity-70">
					({output.sheets * s.copies} sheet{output.sheets * s.copies === 1 ? '' : 's'})
				</span>
			{/if}
		</Button>
	</footer>
</aside>
