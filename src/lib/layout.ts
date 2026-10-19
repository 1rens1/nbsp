import type { Cell, Label, Sheet } from "./api";

export const MM = 72 / 25.4;

/** A page as the planner sees it: displayed size in points, after the user's rotation. */
export interface LayoutPage {
	/** Undefined for blank pages. */
	src?: number;
	index: number;
	rot: number;
	w: number;
	h: number;
}

export type Align = `${"top" | "middle" | "bottom"}-${"left" | "center" | "right"}`;

export type Duplex = "off" | "long" | "short";

export interface LayoutSettings {
	/** Paper in mm, portrait (w <= h). */
	paper: { w: number; h: number };
	orientation: "auto" | "portrait" | "landscape";
	margins: { top: number; right: number; bottom: number; left: number };
	mode: "normal" | "booklet";
	perSheet: number;
	order: "rows" | "columns";
	gutter: number;
	scale: "fit" | "shrink" | "actual" | "custom";
	scalePct: number;
	/** What "custom" scale means: a percentage, or the printed width/height of each page. */
	scaleBy: "percent" | "width" | "height";
	/** Printed width/height in mm for scaleBy "width"/"height". */
	scaleLen: number;
	/** Where the pages sit on the sheet when they don't fill it. */
	align: Align;
	autoRotate: boolean;
	borders: boolean;
	foldLine: boolean;
	duplex: Duplex;
	pageNumbers: PageNumbers;
}

export type NumberPosition = `${"top" | "bottom"}-${"left" | "center" | "right"}`;

export interface PageNumbers {
	enabled: boolean;
	/** `{page}` = page number (`{n}` also works), `{total}` = last number. */
	format: string;
	position: NumberPosition;
	/** Font size in pt at 100% page scale. */
	size: number;
	/** Distance from the page edge, mm at 100% page scale. */
	offset: number;
	/** Number printed on the first numbered page. */
	start: number;
	/** Leading pages left unnumbered (e.g. a cover). */
	skip: number;
}

export const DEFAULT_PAGE_NUMBERS: PageNumbers = {
	enabled: false,
	format: "{page}",
	position: "bottom-center",
	size: 10,
	offset: 8,
	start: 1,
	skip: 0,
};

export interface Layout {
	/** Printed sides in reading order. With duplex, even = front, odd = back. */
	sides: Sheet[];
	duplex: Duplex;
	landscape: boolean;
	/** Set when the settings leave no room for pages (then `sides` is empty). */
	error?: string;
}

export const GRIDS: Record<number, [number, number]> = {
	1: [1, 1],
	2: [1, 2],
	4: [2, 2],
	6: [2, 3],
	8: [2, 4],
	9: [3, 3],
	16: [4, 4],
};

/**
 * Parses "1-3, 5, 8-" into 0-based indices (in order, duplicates kept). Empty → all. A part that
 * runs past the end is clamped (8-20 of 10 → 8-10); one entirely past it is an error.
 */
export function parseRange(expr: string, count: number): number[] | null {
	const parts = expr.split(",").map((p) => p.trim()).filter(Boolean);
	if (!parts.length) return Array.from({ length: count }, (_, i) => i);
	const out: number[] = [];
	for (const part of parts) {
		const m = /^(\d*)\s*(-?)\s*(\d*)$/.exec(part);
		if (!m || (!m[1] && !m[3])) return null;
		const a = m[1] ? +m[1] : 1;
		const b = m[2] ? (m[3] ? +m[3] : count) : a;
		if (a < 1 || b < 1 || Math.min(a, b) > count) return null;
		// Clamped first, so "1-99999999999" can't loop for minutes.
		const [from, to] = [Math.min(a, count), Math.min(b, count)];
		const step = from <= to ? 1 : -1;
		for (let i = from; step > 0 ? i <= to : i >= to; i += step) out.push(i - 1);
	}
	return out;
}

/** "1, 3, 5, …, 15" style summary of 0-based indices (shown 1-based); runs collapse to "1–8". */
export function summarizePages(indices: number[], max = 4): string {
	if (!indices.length) return "none";
	const parts: string[] = [];
	for (let i = 0; i < indices.length; ) {
		let j = i;
		while (j + 1 < indices.length && indices[j + 1] === indices[j] + 1) j++;
		parts.push(j - i >= 2 ? `${indices[i] + 1}–${indices[j] + 1}` : String(indices[i] + 1));
		if (j - i === 1) parts.push(String(indices[j] + 1));
		i = j + 1;
	}
	return parts.length <= max ? parts.join(", ") : [...parts.slice(0, max - 1), "…", parts[parts.length - 1]].join(", ");
}

interface Placement {
	rot: number;
	/** Extra clockwise turn added by auto-rotate (0 or 270). */
	turn: number;
	/** Page scale: placed size / displayed size. */
	k: number;
	x: number;
	y: number;
	w: number;
	h: number;
}

function place(
	p: LayoutPage,
	cx: number,
	cy: number,
	cw: number,
	ch: number,
	s: LayoutSettings,
): Placement {
	let { w, h, rot } = p;
	let turn = 0;
	if (s.autoRotate && cw > 0 && ch > 0) {
		const straight = Math.min(cw / w, ch / h);
		const turned = Math.min(cw / h, ch / w);
		if (turned > straight * 1.01) {
			[w, h] = [h, w];
			rot = (rot + 270) % 360;
			turn = 270;
		}
	}
	const fit = Math.min(cw / w, ch / h);
	const k =
		s.scale === "fit"
			? fit
			: s.scale === "shrink"
				? Math.min(1, fit)
				: s.scale === "actual"
					? 1
					: s.scaleBy === "width"
						? (s.scaleLen * MM) / w
						: s.scaleBy === "height"
							? (s.scaleLen * MM) / h
							: s.scalePct / 100;
	const pw = w * k;
	const ph = h * k;
	return { rot, turn, k, x: cx + (cw - pw) / 2, y: cy + (ch - ph) / 2, w: pw, h: ph };
}

function fitScore(p: LayoutPage, cw: number, ch: number, autoRotate: boolean) {
	const a = Math.min(cw / p.w, ch / p.h);
	return autoRotate ? Math.max(a, Math.min(cw / p.h, ch / p.w)) : a;
}

function cell(p: LayoutPage, at: Placement, border: boolean): Cell | null {
	if (p.src === undefined) return null;
	return { src: p.src, index: p.index, rot: at.rot, x: at.x, y: at.y, w: at.w, h: at.h, border };
}

/** Page number for the `i`-th printed page, positioned in the page's own (upright) frame. */
function pageLabel(p: LayoutPage, at: Placement, i: number, count: number, n: PageNumbers): Label | null {
	if (!n.enabled || p.src === undefined || i < n.skip) return null;
	const num = n.start + i - n.skip;
	const last = n.start + count - n.skip - 1;
	const text = n.format.replaceAll("{page}", String(num)).replaceAll("{n}", String(num)).replaceAll("{total}", String(last));
	const size = n.size * at.k;
	const off = n.offset * MM * at.k;
	// The page's upright frame: before auto-rotate's turn.
	const [lw, lh] = at.turn ? [at.h, at.w] : [at.w, at.h];
	const [v, hz] = n.position.split("-") as ["top" | "bottom", "left" | "center" | "right"];
	const lx = hz === "left" ? off : hz === "right" ? lw - off : lw / 2;
	// Baseline: Helvetica cap height is ~0.72 em.
	const ly = v === "top" ? off + size * 0.72 : lh - off;
	// Clockwise 270° turn maps (x, y) in a w×h box to (y, w − x).
	const [x, y] = at.turn ? [ly, lw - lx] : [lx, ly];
	return { text, x: at.x + x, y: at.y + y, size, angle: at.turn, align: hz };
}

export function plan(pages: LayoutPage[], s: LayoutSettings): Layout {
	const short = Math.min(s.paper.w, s.paper.h) * MM;
	const long = Math.max(s.paper.w, s.paper.h) * MM;
	const m = {
		top: s.margins.top * MM,
		right: s.margins.right * MM,
		bottom: s.margins.bottom * MM,
		left: s.margins.left * MM,
	};
	const gutter = s.gutter * MM;
	// Orientation follows the most common page shape, not just whatever page 1 is.
	const real = pages.filter((p) => p.w > 0);
	const wide = real.filter((p) => p.w > p.h).length * 2 > real.length;
	const first = real.find((p) => p.w > p.h === wide) ?? { index: 0, rot: 0, w: short, h: long };
	const NO_ROOM = "Margins and gaps leave no room for pages on this paper";

	if (s.mode === "booklet") {
		const W = long;
		const H = short;
		const halfW = (W - m.left - m.right - gutter) / 2;
		const ch = H - m.top - m.bottom;
		if (halfW < 1 || ch < 1) return { sides: [], duplex: "short", landscape: true, error: NO_ROOM };
		const n = Math.ceil(pages.length / 4) * 4;
		const blank: LayoutPage = { index: 0, rot: 0, w: first.w, h: first.h };
		const at = (i: number) => (i < pages.length ? pages[i] : blank);
		const sides: Sheet[] = [];
		const side = (l: number, r: number): Sheet => {
			const pl = place(at(l), m.left, m.top, halfW, ch, s);
			const pr = place(at(r), m.left + halfW + gutter, m.top, halfW, ch, s);
			const cells = [cell(at(l), pl, s.borders), cell(at(r), pr, s.borders)].filter((c): c is Cell => c !== null);
			const labels = [
				pageLabel(at(l), pl, l, pages.length, s.pageNumbers),
				pageLabel(at(r), pr, r, pages.length, s.pageNumbers),
			].filter((x): x is Label => x !== null);
			return { w: W, h: H, cells, lines: s.foldLine ? [[W / 2, 0, W / 2, H]] : [], labels };
		};
		for (let i = 0; i < n / 4; i++) {
			sides.push(side(n - 1 - 2 * i, 2 * i));
			sides.push(side(2 * i + 1, n - 2 - 2 * i));
		}
		return { sides, duplex: "short", landscape: true };
	}

	const [ga, gb] = GRIDS[s.perSheet] ?? [1, 1];
	type Option = { landscape: boolean; cols: number; rows: number; score: number };
	const options: Option[] = [];
	for (const landscape of [false, true]) {
		if (s.orientation === "portrait" && landscape) continue;
		if (s.orientation === "landscape" && !landscape) continue;
		for (const [cols, rows] of ga === gb ? [[ga, gb]] : [[ga, gb], [gb, ga]]) {
			const W = landscape ? long : short;
			const H = landscape ? short : long;
			const cw = (W - m.left - m.right - gutter * (cols - 1)) / cols;
			const ch = (H - m.top - m.bottom - gutter * (rows - 1)) / rows;
			options.push({ landscape, cols, rows, score: fitScore(first, cw, ch, s.autoRotate && s.perSheet === 1) });
		}
	}
	// Prefer portrait on ties.
	const best = options.reduce((a, b) => (b.score > a.score * 1.001 ? b : a));
	const { landscape, cols, rows } = best;
	const W = landscape ? long : short;
	const H = landscape ? short : long;
	const cw = (W - m.left - m.right - gutter * (cols - 1)) / cols;
	const ch = (H - m.top - m.bottom - gutter * (rows - 1)) / rows;
	if (cw < 1 || ch < 1) return { sides: [], duplex: s.duplex, landscape, error: NO_ROOM };
	const per = cols * rows;
	const sides: Sheet[] = [];
	for (let start = 0; start < Math.max(pages.length, 1); start += per) {
		const cells: Cell[] = [];
		const labels: Label[] = [];
		const items: { p: LayoutPage; col: number; row: number; at: Placement; i: number }[] = [];
		const colW: number[] = Array(cols).fill(0);
		const rowH: number[] = Array(rows).fill(0);
		for (let k = 0; k < per && start + k < pages.length; k++) {
			const col = s.order === "rows" ? k % cols : Math.floor(k / rows);
			const row = s.order === "rows" ? Math.floor(k / cols) : k % rows;
			const p = pages[start + k];
			const at = place(p, 0, 0, cw, ch, s);
			colW[col] = Math.max(colW[col], at.w);
			rowH[row] = Math.max(rowH[row], at.h);
			items.push({ p, col, row, at, i: start + k });
		}
		// Pack the used columns and rows, then anchor the block on the sheet.
		const [v, hz] = s.align.split("-") as ["top" | "middle" | "bottom", "left" | "center" | "right"];
		const stops = (sizes: number[], room: number, edge: "start" | "mid" | "end", from: number) => {
			const used = sizes.filter((x) => x > 0).length;
			const total = sizes.reduce((a, b) => a + b, 0) + gutter * Math.max(used - 1, 0);
			let at = from + (edge === "start" ? 0 : edge === "end" ? room - total : (room - total) / 2);
			return sizes.map((x) => {
				const o = at;
				if (x > 0) at += x + gutter;
				return o;
			});
		};
		const xs = stops(colW, W - m.left - m.right, hz === "left" ? "start" : hz === "right" ? "end" : "mid", m.left);
		const ys = stops(rowH, H - m.top - m.bottom, v === "top" ? "start" : v === "bottom" ? "end" : "mid", m.top);
		for (const { p, col, row, at: a, i } of items) {
			const at = {
				...a,
				x: xs[col] + (hz === "left" ? 0 : hz === "right" ? colW[col] - a.w : (colW[col] - a.w) / 2),
				y: ys[row] + (v === "top" ? 0 : v === "bottom" ? rowH[row] - a.h : (rowH[row] - a.h) / 2),
			};
			const c = cell(p, at, s.borders && per > 1);
			if (c) cells.push(c);
			const l = pageLabel(p, at, i, pages.length, s.pageNumbers);
			if (l) labels.push(l);
		}
		sides.push({ w: W, h: H, cells, lines: [], labels });
	}
	return { sides, duplex: s.duplex, landscape };
}

export function rotate180(sheet: Sheet): Sheet {
	return {
		w: sheet.w,
		h: sheet.h,
		cells: sheet.cells.map((c) => ({
			...c,
			x: sheet.w - c.x - c.w,
			y: sheet.h - c.y - c.h,
			rot: (c.rot + 180) % 360,
		})),
		lines: sheet.lines?.map(([x1, y1, x2, y2]) => [sheet.w - x1, sheet.h - y1, sheet.w - x2, sheet.h - y2]),
		labels: sheet.labels?.map((l) => ({ ...l, x: sheet.w - l.x, y: sheet.h - l.y, angle: (l.angle + 180) % 360 })),
	};
}

export interface DuplexCalibration {
	/** Backs must be printed in the same order the fronts were printed. */
	sameOrder: boolean;
	/** Extra 180° turn of the backs, on top of what the flip edge needs. */
	rotateBacks: boolean;
}

export const DEFAULT_CALIBRATION: DuplexCalibration = { sameOrder: true, rotateBacks: false };

/**
 * Splits a layout into print passes (one job each), in the order sheets must be fed. `reverse`
 * prints the last sheet first (first page on top of a face-up stack). A printer with a duplexer
 * takes everything in one pass and turns the backs itself.
 */
export function passes(layout: Layout, reverse: boolean, cal: DuplexCalibration, hardware = false): Sheet[][] {
	const order = <T>(list: T[], reverse: boolean) => (reverse ? [...list].reverse() : list);
	if (layout.duplex === "off") return [order(layout.sides, reverse)];
	const fronts = layout.sides.filter((_, i) => i % 2 === 0);
	const backs = layout.sides.filter((_, i) => i % 2 === 1);
	if (backs.length < fronts.length) {
		const last = fronts[fronts.length - 1];
		backs.push({ w: last.w, h: last.h, cells: [], lines: [] });
	}
	if (hardware) return [order(fronts.map((f, i) => [f, backs[i]]), reverse).flat()];
	// The stack is always turned over on the paper's long edge, which is the page's long edge in
	// portrait and in landscape alike. Documents flipping on their short edge need the backs upside down.
	const turn = (layout.duplex === "short") !== cal.rotateBacks;
	const backOrder = cal.sameOrder ? reverse : !reverse;
	return [order(fronts, reverse), order(turn ? backs.map(rotate180) : backs, backOrder)];
}
