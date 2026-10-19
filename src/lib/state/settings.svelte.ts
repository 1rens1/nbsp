import { load, type Store } from "@tauri-apps/plugin-store";
import type { Unit } from "$lib/units";
import { DEFAULT_CALIBRATION, DEFAULT_PAGE_NUMBERS, type DuplexCalibration, type LayoutSettings } from "$lib/layout";

export interface Paper {
	/** CUPS PageSize value. */
	id: string;
	name: string;
	w: number;
	h: number;
	custom?: boolean;
	kind?: PaperKind;
}

export type PaperKind = "document" | "photo" | "envelope";

export interface PrinterCalibration extends DuplexCalibration {
	/** Overrides the detected output tray. */
	faceUp?: boolean;
}

export interface PrintSettings extends LayoutSettings {
	paperId: string;
	/** DOCX/ODT: lay the text out for the paper size instead of scaling the finished page. */
	reflow: boolean;
	range: string;
	subset: "all" | "odd" | "even" | "selected" | "custom";
	copies: number;
	collate: boolean;
	firstOnTop: boolean;
	/** PPD options by key (PrintQuality, MediaType, Color, Borderless, ...). */
	ppd: Record<string, string>;
}

export interface Preset {
	name: string;
	settings: Partial<PrintSettings>;
}

/** Friendlier names, kinds and menu order for common PageSize names; drivers name the rest. */
export const KNOWN_PAPERS: Record<string, [string, number, number, PaperKind]> = {
	A4: ["A4", 210, 297, "document"],
	Letter: ["Letter", 215.9, 279.4, "document"],
	A5: ["A5", 148, 210, "document"],
	A6: ["A6", 105, 148, "document"],
	B5: ["B5", 182, 257, "document"],
	B6: ["B6", 128, 182, "document"],
	Legal: ["Legal", 215.9, 355.6, "document"],
	"8.5x13in": ["F4 / Folio", 215.9, 330.2, "document"],
	IndianLegal: ["Indian Legal", 215, 345, "document"],
	"16K": ["16K", 195, 270, "document"],
	"5x8in": ["5×8in", 127, 203.2, "document"],
	"4x6in": ["4R photo (4×6in)", 101.6, 152.4, "photo"],
	"3.5x5in": ["3R photo (3.5×5in)", 88.9, 127, "photo"],
	"5x7in": ["5R photo (5×7in)", 127, 177.8, "photo"],
	"8x10in": ["8R photo (8×10in)", 203.2, 254, "photo"],
	"169widesize": ["16:9 wide (4×7.1in)", 101.6, 180.6, "photo"],
	"100x148mm": ["Postcard (100×148mm)", 100, 148, "photo"],
	ENVELOPE_10: ["Envelope #10", 104.8, 241.3, "envelope"],
	ENVELOPEDL: ["Envelope DL", 110, 220, "envelope"],
	ENVELOPEC6: ["Envelope C6", 114, 162, "envelope"],
};

export const DEFAULTS: PrintSettings = {
	paper: { w: 210, h: 297 },
	paperId: "A4",
	reflow: false,
	orientation: "auto",
	margins: { top: 5, right: 5, bottom: 5, left: 5 },
	mode: "normal",
	perSheet: 1,
	order: "rows",
	gutter: 4,
	scale: "fit",
	scalePct: 100,
	scaleBy: "percent",
	scaleLen: 100,
	align: "middle-center",
	autoRotate: true,
	borders: false,
	foldLine: false,
	duplex: "off",
	pageNumbers: { ...DEFAULT_PAGE_NUMBERS },
	range: "",
	subset: "all",
	copies: 1,
	collate: true,
	firstOnTop: true,
	ppd: {},
};

class Settings {
	s = $state<PrintSettings>(structuredClone(DEFAULTS));
	presets = $state<Preset[]>([]);
	customPapers = $state<Paper[]>([]);
	/** Per printer: how its manual double-sided stack behaves, and any output tray override. */
	calibrations = $state<Record<string, PrinterCalibration>>({});
	printer = $state("");
	/** Native (KDE/GTK) title bar: the only way to get the system's shadow, corners and window rules. */
	systemTitleBar = $state(true);
	/** What the window was actually opened with; changing the setting only takes effect on restart. */
	titleBarApplied = $state(true);
	/** How lengths are shown and typed; everything is stored in mm. */
	unit = $state<Unit>("mm");
	#store: Store | null = null;
	#timer: ReturnType<typeof setTimeout> | undefined;
	#pending: (() => Promise<void>) | null = null;

	async init() {
		try {
			this.#store = await load("settings.json", { autoSave: false, defaults: {} });
			const saved = await this.#store.get<Partial<PrintSettings>>("last");
			// Per-job choices that shouldn't stick between sessions.
			if (saved)
				this.s = {
					...structuredClone(DEFAULTS),
					...saved,
					pageNumbers: { ...DEFAULT_PAGE_NUMBERS, ...saved.pageNumbers },
					range: "",
					subset: "all",
					copies: 1,
				};
			this.presets = (await this.#store.get<Preset[]>("presets")) ?? [];
			this.customPapers = (await this.#store.get<Paper[]>("papers")) ?? [];
			this.printer = (await this.#store.get<string>("printer")) ?? "";
			this.calibrations = (await this.#store.get<Record<string, PrinterCalibration>>("calibrations")) ?? {};
			// Before calibration was per printer, it belonged to the selected one.
			const single = await this.#store.get<DuplexCalibration>("calibration");
			if (single && this.printer && !this.calibrations[this.printer]) this.calibrations[this.printer] = single;
			this.systemTitleBar = (await this.#store.get<boolean>("systemTitleBar")) ?? true;
			this.titleBarApplied = this.systemTitleBar;
			this.unit = (await this.#store.get<Unit>("unit")) ?? "mm";
		} catch (e) {
			console.error("settings load failed", e);
		}
	}

	/** Values set from the command line, and what they replaced, so they're never saved as defaults. */
	#cli: { applied: Partial<PrintSettings>; before: Partial<PrintSettings> } | null = null;

	/** Applies one-off overrides (from `nbsp print …`) on top of the current settings. */
	applyOverrides(over: Partial<PrintSettings>, ppd: Record<string, string>) {
		const applied: Partial<PrintSettings> = { ...structuredClone(over), ppd: { ...$state.snapshot(this.s.ppd), ...ppd } };
		if (over.pageNumbers) applied.pageNumbers = { ...$state.snapshot(this.s.pageNumbers), ...over.pageNumbers };
		const before: Partial<PrintSettings> = {};
		for (const k of Object.keys(applied) as (keyof PrintSettings)[]) (before as Record<string, unknown>)[k] = $state.snapshot(this.s[k]);
		this.#cli = { applied, before };
		this.s = { ...this.s, ...structuredClone(applied) };
	}

	/** Current settings minus untouched command-line overrides. */
	#persistable() {
		const s = $state.snapshot(this.s) as PrintSettings;
		if (!this.#cli) return s;
		const out = s as unknown as Record<string, unknown>;
		const { applied, before } = this.#cli as { applied: Record<string, unknown>; before: Record<string, unknown> };
		for (const k of Object.keys(applied)) {
			if (JSON.stringify(out[k]) === JSON.stringify(applied[k])) out[k] = before[k];
		}
		return s;
	}

	/** Debounced persist; call from an effect that reads everything worth saving. */
	save() {
		clearTimeout(this.#timer);
		const snapshot = {
			last: this.#persistable(),
			presets: $state.snapshot(this.presets),
			papers: $state.snapshot(this.customPapers),
			calibrations: $state.snapshot(this.calibrations),
			printer: this.printer,
			systemTitleBar: this.systemTitleBar,
			unit: this.unit,
		};
		this.#pending = async () => {
			this.#pending = null;
			if (!this.#store) return;
			for (const [k, v] of Object.entries(snapshot)) await this.#store.set(k, v);
			await this.#store.save();
		};
		this.#timer = setTimeout(() => this.#pending?.(), 400);
	}

	/** Writes a pending save now (the window is closing). */
	async flush() {
		clearTimeout(this.#timer);
		await this.#pending?.();
	}

	calibrationFor(printer: string): PrinterCalibration {
		return { ...DEFAULT_CALIBRATION, ...this.calibrations[printer] };
	}

	setCalibration(printer: string, change: Partial<PrinterCalibration>) {
		this.calibrations = { ...this.calibrations, [printer]: { ...this.calibrationFor(printer), ...change } };
	}

	setPaper(p: Paper) {
		this.s.paperId = p.id;
		this.s.paper = { w: Math.min(p.w, p.h), h: Math.max(p.w, p.h) };
	}

	savePreset(name: string) {
		const { range, subset, copies, ...rest } = $state.snapshot(this.s);
		void range, subset, copies;
		this.presets = [...this.presets.filter((p) => p.name !== name), { name, settings: rest }];
	}

	applyPreset(p: Preset) {
		this.s = { ...this.s, ...structuredClone(p.settings) };
	}

	deletePreset(name: string) {
		this.presets = this.presets.filter((p) => p.name !== name);
	}

	addPaper(name: string, w: number, h: number) {
		const id = `Custom.${w}x${h}mm`;
		this.customPapers = [...this.customPapers.filter((p) => p.id !== id), { id, name, w, h, custom: true }];
		return this.customPapers[this.customPapers.length - 1];
	}

	deletePaper(id: string) {
		this.customPapers = this.customPapers.filter((p) => p.id !== id);
	}
}

export const settings = new Settings();
