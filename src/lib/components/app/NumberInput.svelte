<script lang="ts">
	import { IconMinus, IconPlus } from '@tabler/icons-svelte';
	import { cn } from '$lib/utils';
	import * as InputGroup from '$lib/components/ui/input-group';
	import { settings } from '$lib/state/settings.svelte';
	import { fromUnit, toUnit } from '$lib/units';

	let {
		value = $bindable(),
		id,
		min = 0,
		max = 100,
		step = 1,
		unit,
		length = false,
		disabled = false,
		label,
		class: className
	}: {
		value: number;
		id?: string;
		min?: number;
		max?: number;
		step?: number;
		unit?: string;
		/** A length: value, min, max and step are mm, shown in the chosen unit (click the unit to switch). */
		length?: boolean;
		disabled?: boolean;
		label?: string;
		class?: string;
	} = $props();

	const inch = $derived(length && settings.unit === 'in');
	// Buttons step by a rounder amount in inches (0.05 in is about 1 mm).
	const stepBy = $derived(inch ? (step >= 1 ? 0.05 : 0.02) : step);
	const shown = $derived(length ? toUnit(value, settings.unit) : value);
	const lo = $derived(length ? toUnit(min, settings.unit) : min);
	const hi = $derived(length ? toUnit(max, settings.unit) : max);

	// Snap to a grid in the displayed unit, then clamp in mm; avoids float noise like 4.499999.
	function set(v: number, grid: number) {
		if (!Number.isFinite(v)) return (value = min);
		const snapped = Math.round(v / grid) * grid;
		const mm = length ? fromUnit(snapped, settings.unit) : snapped;
		value = +Math.min(max, Math.max(min, mm)).toFixed(4);
	}
	// Typing keeps full precision (0.125 in must not jump to 0.15); the buttons snap to the step.
	const typed = (v: number) => set(v, inch ? 0.01 : step);

	/**
	 * What's being typed (undefined when not editing). Clamping every keystroke would turn "12"
	 * into "112" (empty snaps to 1) and make 100 untypeable with a minimum of 54, so a value is
	 * applied live only once it's valid, and clamped when the field is left.
	 */
	let draft = $state<number | null | undefined>(undefined);
	function input(v: number | null) {
		draft = v;
		if (v !== null && Number.isFinite(v) && v >= lo && v <= hi) typed(v);
	}
	function commit() {
		const v = draft;
		draft = undefined;
		if (v !== null && v !== undefined && Number.isFinite(v)) typed(v);
	}
	function bump(dir: 1 | -1) {
		draft = undefined;
		set(shown + dir * stepBy, stepBy);
	}
</script>

<InputGroup.Root data-disabled={disabled || undefined} class={cn('w-32 shrink-0', className)}>
	<InputGroup.Addon align="inline-start">
		<InputGroup.Button size="icon-xs" aria-label="Decrease" disabled={disabled || value <= min} onclick={() => bump(-1)}>
			<IconMinus />
		</InputGroup.Button>
	</InputGroup.Addon>
	<InputGroup.Input
		{id}
		type="number"
		inputmode="decimal"
		min={lo}
		max={hi}
		step={stepBy}
		{disabled}
		aria-label={label}
		class="text-center tabular-nums [appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none"
		bind:value={() => (draft === undefined ? shown : draft), input}
		onblur={commit}
		onkeydown={(e) => e.key === 'Enter' && commit()}
	/>
	{#if length}
		<InputGroup.Addon align="inline-end" class="pr-0">
			<InputGroup.Button
				class="w-6 justify-center px-0 text-muted-foreground"
				title={inch ? 'Switch to millimetres' : 'Switch to inches'}
				onclick={() => (settings.unit = inch ? 'mm' : 'in')}
			>
				{settings.unit}
			</InputGroup.Button>
		</InputGroup.Addon>
	{:else if unit}
		<InputGroup.Addon align="inline-end" class="pr-0"><InputGroup.Text>{unit}</InputGroup.Text></InputGroup.Addon>
	{/if}
	<InputGroup.Addon align="inline-end">
		<InputGroup.Button size="icon-xs" aria-label="Increase" disabled={disabled || value >= max} onclick={() => bump(1)}>
			<IconPlus />
		</InputGroup.Button>
	</InputGroup.Addon>
</InputGroup.Root>
