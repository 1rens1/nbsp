import { save } from "@tauri-apps/plugin-dialog";
import { toast } from "svelte-sonner";
import { api, type PrintRequest, type Sheet } from "$lib/api";
import { parseRange, passes, plan, type Layout } from "$lib/layout";
import { job, type JobPage } from "./job.svelte";
import { printer } from "./printer.svelte";
import { DEFAULTS, settings } from "./settings.svelte";

export type PrintPhase = "idle" | "sending" | "fronts" | "flip" | "backs";

const SIDES = { off: "one-sided", long: "two-sided-long-edge", short: "two-sided-short-edge" } as const;

/** Sheets (times copies) above which Print asks first. */
export const LARGE_JOB = 50;

class Output {
	range = $derived(settings.s.subset === "custom" ? parseRange(settings.s.range, job.pages.length) : []);
	rangeError = $derived(this.range === null);

	/** 0-based positions (in the job) of the pages that will be printed, in print order. */
	picked = $derived.by<number[]>(() => {
		const s = settings.s;
		const all = job.pages.map((_, i) => i);
		switch (s.subset) {
			case "selected":
				return all.filter((i) => job.selected.has(job.pages[i].id));
			case "odd":
				return all.filter((i) => i % 2 === 0);
			case "even":
				return all.filter((i) => i % 2 === 1);
			case "custom":
				return this.range ?? [];
			default:
				return all;
		}
	});

	pages = $derived<JobPage[]>(this.picked.map((i) => job.pages[i]));

	layout = $derived<Layout>(plan(this.pages.map((p) => job.layoutPage(p)), settings.s));
	sheets = $derived(this.layout.duplex === "off" ? this.layout.sides.length : Math.ceil(this.layout.sides.length / 2));

	phase = $state<PrintPhase>("idle");
	/** Sheets in the stack to flip, while a double-sided job waits for its backs. */
	flipSheets = $state(0);
	/** A large job waiting for the user's go-ahead (LargeJobDialog). */
	confirming = $state(false);
	/** Everything the backs pass needs, captured when the fronts were sent. */
	#backs: { sheets: Sheet[]; request: PrintRequest; done?: () => void } | null = null;
	#job = "";
	/** Bumped by every print and abort, so a stale wait can't advance a newer job. */
	#run = 0;

	ppdOptions(duplex = this.layout.duplex): [string, string][] {
		const s = settings.s;
		// Saved choices may come from another driver (a preset, the command line): use this one's names.
		const chosen = new Map<string, string>();
		for (const [k, v] of Object.entries(s.ppd)) {
			const t = k === "PageSize" ? null : printer.translate(k, v);
			if (t && !chosen.has(t[0])) chosen.set(...t);
		}
		const current = (k: string) => (k === "PageSize" ? s.paperId : (chosen.get(k) ?? printer.option(k)?.default ?? ""));
		const out: [string, string][] = [["PageSize", s.paperId]];
		// Choices the driver rejects in this combination (Borderless on A4, Draft on photo paper) stay out.
		for (const [k, v] of chosen) if (!printer.blocked(k, v, current)) out.push([k, v]);
		// The printer's own default could be two-sided; NBSP decides.
		if (printer.duplex) out.push(["sides", SIDES[duplex]]);
		return out;
	}

	#request(title: string, duplex = this.layout.duplex): PrintRequest {
		return { printer: settings.printer, title, copies: settings.s.copies, collate: settings.s.collate, options: this.ppdOptions(duplex) };
	}

	async #send(sheets: Sheet[], request: PrintRequest) {
		const id = await api.print(sheets, request);
		this.#job = id;
		return id;
	}

	/** Waits until CUPS stops listing the job. A failed query counts as still printing ("Skip waiting" covers that). */
	async #waitFor(printer: string, id: string, run: number) {
		while (run === this.#run && (await api.jobActive(printer, id).catch(() => true))) {
			await new Promise((r) => setTimeout(r, 1500));
		}
	}

	/** Resolves true once the job (or its first pass) was sent. `force` skips the large-job check. */
	async print(force = false): Promise<boolean> {
		if (!this.pages.length || this.phase !== "idle") return false;
		if (this.layout.error) {
			toast.error(this.layout.error);
			return false;
		}
		if (!settings.printer) {
			toast.error("No printer selected");
			return false;
		}
		if (!force && this.sheets * settings.s.copies > LARGE_JOB) {
			this.confirming = true;
			return false;
		}
		return this.#start(this.layout, this.#request(job.title));
	}

	/** Prints the 2-sheet duplex test through the normal flow, with the current calibration. */
	async printTest(): Promise<boolean> {
		if (this.phase !== "idle" || !settings.printer) return false;
		try {
			const src = await api.testPages();
			const pages = src.pages.map(([w, h], index) => ({ src: src.id, index, rot: 0, w, h }));
			const layout = plan(pages, { ...DEFAULTS, duplex: "long", paper: settings.s.paper });
			const request = { ...this.#request("NBSP duplex test", "long"), copies: 1, collate: true };
			return await this.#start(layout, request, () => api.closeSource(src.id));
		} catch (e) {
			toast.error(String(e));
			return false;
		}
	}

	async #start(layout: Layout, request: PrintRequest, done?: () => void): Promise<boolean> {
		// A booklet comes out ready to fold: after one flip of the stack, sheet 1 is on top. A face-down
		// stack is in order as printed.
		const reverse = settings.s.firstOnTop && settings.s.mode !== "booklet" && printer.faceUp;
		const [first, second] = passes(layout, reverse, settings.calibrationFor(request.printer), printer.duplex);
		const run = ++this.#run;
		this.phase = "sending";
		try {
			const id = await this.#send(first, second ? { ...request, title: `${request.title} (fronts)` } : request);
			if (!second) {
				this.phase = "idle";
				done?.();
				const n = first.length * request.copies;
				toast.success(`Sent ${n} sheet${n === 1 ? "" : "s"} to ${request.printer}`, {
					action: { label: "Cancel", onClick: () => api.cancelJob(id) },
				});
				return true;
			}
			this.#backs = { sheets: second, request: { ...request, title: `${request.title} (backs)` }, done };
			this.flipSheets = first.length * request.copies;
			this.phase = "fronts";
			await this.#waitFor(request.printer, id, run);
			if (run === this.#run && this.phase === "fronts") this.phase = "flip";
			return true;
		} catch (e) {
			if (run === this.#run) this.phase = "idle";
			done?.();
			toast.error(String(e));
			return false;
		}
	}

	async printBacks() {
		const backs = this.#backs;
		if (!backs) return;
		this.phase = "backs";
		try {
			await this.#send(backs.sheets, backs.request);
			toast.success("Backs sent");
		} catch (e) {
			toast.error(String(e));
		}
		this.#backs = null;
		backs.done?.();
		this.phase = "idle";
	}

	async abort() {
		this.#run++;
		if (this.#job && this.phase === "fronts") await api.cancelJob(this.#job).catch(() => {});
		this.#backs?.done?.();
		this.#backs = null;
		this.phase = "idle";
	}

	/** Saves the job's pages as a plain PDF (one page per page, no imposition). */
	async exportPages() {
		if (!job.pages.length) return;
		const path = await save({ defaultPath: `${job.title.replace(/\.[^.]+$/, "")}.pdf`, filters: [{ name: "PDF", extensions: ["pdf"] }] });
		if (!path) return;
		const sheets: Sheet[] = job.pages.map((p) => {
			const lp = job.layoutPage(p);
			return { w: lp.w, h: lp.h, cells: lp.src === undefined ? [] : [{ src: lp.src, index: lp.index, rot: lp.rot, x: 0, y: 0, w: lp.w, h: lp.h }] };
		});
		await this.#export(sheets, path);
	}

	/** Saves the imposed sheets exactly as they would print. */
	async exportLayout() {
		if (!this.layout.sides.length) return;
		const path = await save({ defaultPath: `${job.title.replace(/\.[^.]+$/, "")}-print.pdf`, filters: [{ name: "PDF", extensions: ["pdf"] }] });
		if (!path) return;
		await this.#export(this.layout.sides, path);
	}

	async #export(sheets: Sheet[], path: string) {
		try {
			await api.exportPdf(sheets, path);
			toast.success("Saved", { description: path });
		} catch (e) {
			toast.error(String(e));
		}
	}
}

export const output = new Output();
