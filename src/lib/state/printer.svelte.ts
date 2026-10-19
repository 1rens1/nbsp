import { api, type Printer, type PrinterOption, type PrinterOptions } from "$lib/api";
import { KNOWN_PAPERS, settings, type Paper } from "./settings.svelte";

/** One setting under the names different drivers give it. */
const ALIASES = [
	["Color", "ColorModel", "print-color-mode"],
	["PrintQuality", "cupsPrintQuality", "print-quality"],
	["MediaType", "media-type"],
];
/** Choices that mean the same across drivers (checked in order: "Gray" before "Color"). */
const FAMILIES = [/gr[ae]y|mono|black/i, /^(colou?r|rgb|cmyk)$/i, /draft|fast|eco/i, /standard|normal/i, /high|best|fine/i, /plain|stationery/i];

class PrinterState {
	list = $state<Printer[]>([]);
	options = $state<PrinterOption[]>([]);
	constraints = $state<[string, string, string, string][]>([]);
	#ppdPapers = $state<PrinterOptions["papers"]>([]);
	/** Custom size range in mm, or null when the driver takes fixed sizes only. */
	custom = $state<{ minW: number; maxW: number; minH: number; maxH: number } | null>(null);
	/** Prints both sides by itself (else the flip-the-stack flow). */
	duplex = $state(false);
	#faceUp = $state(true);
	error = $state("");

	current = $derived(this.list.find((p) => p.name === settings.printer));

	/** Output lands face up, so "first page on top" means printing in reverse. A saved choice wins. */
	faceUp = $derived(settings.calibrationFor(settings.printer).faceUp ?? this.#faceUp);
	/** What was detected, before any saved choice. */
	detectedFaceUp = $derived(this.#faceUp);

	option(key: string) {
		return this.options.find((o) => o.key === key);
	}

	/** The key this printer uses for a setting known under several names (Color, ColorModel, ...). */
	key(name: string) {
		const group = ALIASES.find((g) => g.includes(name)) ?? [name];
		return group.find((k) => this.option(k));
	}

	/** The driver's name for a choice. */
	label(key: string, choice: string) {
		const o = this.option(key);
		return o?.labels[o.choices.indexOf(choice)] ?? choice;
	}

	/** A saved option (maybe from another printer or the command line) as this printer's equivalent. */
	translate(key: string, value: string): [string, string] | null {
		const k = this.key(key);
		const opt = k && this.option(k);
		if (!opt) return null;
		if (opt.choices.includes(value)) return [k, value];
		const family = FAMILIES.find((f) => f.test(value));
		const match = family && opt.choices.find((c) => family.test(c));
		return match ? [k, match] : null;
	}

	/** True when `key=choice` conflicts with the current value of another option. */
	blocked(key: string, choice: string, current: (key: string) => string) {
		return this.constraints.some(
			([k1, v1, k2, v2]) =>
				(k1 === key && matches(choice, v1) && matches(current(k2), v2)) || (k2 === key && matches(choice, v2) && matches(current(k1), v1))
		);
	}

	/** The driver's sizes, then the user's custom sizes that fit this printer. */
	papers = $derived.by<Paper[]>(() => {
		const c = this.custom;
		// Drivers list sizes the hardware can't feed (A3 on an A4 printer); the custom range tells.
		const ppd = this.#ppdPapers.filter((p) => !c || p.w <= c.maxW + 0.5);
		// Until the driver answers, offer every size we know.
		const ids = ppd.length ? new Set(ppd.map((p) => p.id)) : new Set(Object.keys(KNOWN_PAPERS));
		// Known sizes first, in KNOWN_PAPERS order (common sizes first, grouped by kind), then the rest.
		const known = Object.entries(KNOWN_PAPERS)
			.filter(([id]) => ids.has(id))
			.map(([id, [name, w, h, kind]]) => ({ id, name, w, h, kind }));
		const other = ppd
			.filter((p) => !(p.id in KNOWN_PAPERS))
			.map(({ id, label, w, h }) => ({ id, name: label, w, h, kind: /env/i.test(id) ? ("envelope" as const) : ("document" as const) }));
		const fits = (p: Paper) => {
			const [w, h] = [Math.min(p.w, p.h), Math.max(p.w, p.h)];
			return !!c && w >= c.minW && w <= c.maxW && h >= c.minH && h <= c.maxH;
		};
		const custom = settings.customPapers.filter(fits);
		return [...known, ...other, ...custom];
	});

	async refresh() {
		try {
			const list = await api.printers();
			// Polled every few seconds: only touch state (and everything derived from it) on change.
			if (JSON.stringify(list) !== JSON.stringify(this.list)) this.list = list;
			this.error = "";
			if (!this.list.some((p) => p.name === settings.printer)) {
				const real = this.list.filter((p) => p.name !== "NBSP");
				settings.printer = (real.find((p) => p.isDefault) ?? real[0])?.name ?? "";
			}
		} catch (e) {
			this.error = String(e);
		}
	}

	async loadOptions() {
		if (!settings.printer) return;
		try {
			const res = await api.printerOptions(settings.printer);
			this.options = res.options;
			this.constraints = res.constraints;
			this.#ppdPapers = res.papers;
			this.custom = res.custom && { minW: res.custom[0], maxW: res.custom[1], minH: res.custom[2], maxH: res.custom[3] };
			this.duplex = res.duplex;
			this.#faceUp = res.faceUp;
		} catch (e) {
			this.options = [];
			this.constraints = [];
			this.#ppdPapers = [];
			this.custom = null;
			this.duplex = false;
			this.#faceUp = true;
			this.error = String(e);
		}
	}
}

/** A constraint choice of "*" stands for any value except None/False/Off (see cups.rs). */
export function matches(value: string, choice: string) {
	return choice === "*" ? !["", "None", "False", "Off"].includes(value) : value === choice;
}

export const printer = new PrinterState();
