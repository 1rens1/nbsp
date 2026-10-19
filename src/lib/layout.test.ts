import { describe, expect, test } from "bun:test";
import { DEFAULT_CALIBRATION, MM, DEFAULT_PAGE_NUMBERS, parseRange, summarizePages, passes, plan, type LayoutPage, type LayoutSettings } from "./layout";

const A4: [number, number] = [595.28, 841.89];
const pages = (n: number, [w, h] = A4): LayoutPage[] =>
	Array.from({ length: n }, (_, i) => ({ src: 1, index: i, rot: 0, w, h }));

const base: LayoutSettings = {
	paper: { w: 210, h: 297 },
	orientation: "auto",
	margins: { top: 0, right: 0, bottom: 0, left: 0 },
	mode: "normal",
	perSheet: 1,
	align: "middle-center",
	order: "rows",
	gutter: 0,
	scale: "fit",
	scalePct: 100,
	scaleBy: "percent",
	scaleLen: 100,
	autoRotate: true,
	borders: false,
	foldLine: false,
	duplex: "off",
	pageNumbers: { ...DEFAULT_PAGE_NUMBERS },
};

const order = (l: { cells: { index: number }[] }[]) => l.map((s) => s.cells.map((c) => c.index));

describe("parseRange", () => {
	test("ranges", () => {
		expect(parseRange("", 3)).toEqual([0, 1, 2]);
		expect(parseRange("1-3, 5, 8-", 9)).toEqual([0, 1, 2, 4, 7, 8]);
		expect(parseRange("3-1", 5)).toEqual([2, 1, 0]);
		expect(parseRange("x", 5)).toBeNull();
	});

	test("edges", () => {
		expect(parseRange(" , ", 3)).toEqual([0, 1, 2]);
		expect(parseRange("50", 10)).toBeNull();
		expect(parseRange("8-20", 10)).toEqual([7, 8, 9]);
		expect(parseRange("20-9", 10)).toEqual([9, 8]);
		const t = performance.now();
		expect(parseRange("1-99999999999", 5)).toEqual([0, 1, 2, 3, 4]);
		expect(performance.now() - t).toBeLessThan(50);
	});
});

describe("infeasible settings", () => {
	test("margins wider than the paper", () => {
		const l = plan(pages(2), { ...base, margins: { top: 200, right: 200, bottom: 200, left: 200 } });
		expect(l.sides).toEqual([]);
		expect(l.error).toBeTruthy();
	});
	test("gaps too large for 16 up", () => {
		expect(plan(pages(16), { ...base, perSheet: 16, gutter: 100 }).error).toBeTruthy();
	});
	test("booklet with huge binding gap", () => {
		expect(plan(pages(4), { ...base, mode: "booklet", gutter: 400 }).error).toBeTruthy();
	});
	test("no cell ever has a negative size", () => {
		for (const gutter of [0, 20, 50])
			for (const perSheet of [1, 2, 4, 6, 9, 16])
				for (const c of plan(pages(16), { ...base, perSheet, gutter }).sides.flatMap((x) => x.cells))
					expect(c.w > 0 && c.h > 0).toBe(true);
	});
});

describe("orientation", () => {
	test("follows the majority page shape", () => {
		const land: [number, number] = [A4[1], A4[0]];
		const mixed = [...pages(1), ...pages(3, land)];
		expect(plan(mixed, { ...base, autoRotate: false }).landscape).toBe(true);
	});
});

describe("summarizePages", () => {
	test("runs and truncation", () => {
		expect(summarizePages([0, 1, 2, 3])).toBe("1–4");
		expect(summarizePages([1, 3, 5, 7])).toBe("2, 4, 6, 8");
		expect(summarizePages([1, 3, 5, 7, 9, 11, 13, 15])).toBe("2, 4, 6, …, 16");
		expect(summarizePages([0, 1, 4])).toBe("1, 2, 5");
	});
});

describe("plan", () => {
	test("1-up fills A4 portrait", () => {
		const l = plan(pages(2), base);
		expect(l.landscape).toBe(false);
		expect(l.sides[0].cells[0].w).toBeCloseTo(595.28, 0);
	});

	test("2-up goes landscape side by side", () => {
		const l = plan(pages(3), { ...base, perSheet: 2 });
		expect(l.landscape).toBe(true);
		expect(order(l.sides)).toEqual([[0, 1], [2]]);
		expect(l.sides[0].cells[1].x).toBeGreaterThan(l.sides[0].cells[0].x);
	});

	test("actual-size 2-up packs into the top-left corner", () => {
		const l = plan(pages(2, [297.64, 419.53]), { ...base, perSheet: 2, orientation: "landscape", scale: "actual", align: "top-left" });
		const [a, b] = l.sides[0].cells;
		expect([a.x, a.y]).toEqual([0, 0]);
		expect(b.x).toBeCloseTo(a.w, 5);
		expect(b.y).toBe(0);
	});

	test("custom scale by real width", () => {
		const l = plan(pages(1, [297.64, 419.53]), { ...base, scale: "custom", scaleBy: "width", scaleLen: 50 });
		expect(l.sides[0].cells[0].w).toBeCloseTo(50 * MM, 5);
	});

	test("landscape page auto-rotates on portrait sheet", () => {
		const l = plan(pages(1, [841.89, 595.28]), { ...base, orientation: "portrait" });
		expect(l.sides[0].cells[0].rot).toBe(270);
	});

	test("booklet order pads to multiple of 4", () => {
		const l = plan(pages(6), { ...base, mode: "booklet" });
		// 8 slots: 2 sheets. Blank slots 6,7 have no cells.
		expect(order(l.sides)).toEqual([[0], [1], [5, 2], [3, 4]]);
		const l8 = plan(pages(8), { ...base, mode: "booklet" });
		expect(order(l8.sides)).toEqual([[7, 0], [1, 6], [5, 2], [3, 4]]);
	});
});

describe("passes", () => {
	test("simplex first page on top prints reversed", () => {
		const [p] = passes(plan(pages(3), base), true, DEFAULT_CALIBRATION);
		expect(order(p)).toEqual([[2], [1], [0]]);
	});

	test("manual duplex with odd count pads a blank back", () => {
		const [f, b] = passes(plan(pages(3), { ...base, duplex: "long" }), true, DEFAULT_CALIBRATION);
		expect(order(f)).toEqual([[2], [0]]);
		expect(order(b)).toEqual([[], [1]]);
		expect(b[1].cells[0].rot).toBe(0);
	});

	test("short-edge flip turns backs", () => {
		const [, b] = passes(plan(pages(2), { ...base, duplex: "short" }), false, DEFAULT_CALIBRATION);
		expect(b[0].cells[0].rot).toBe(180);
	});
});

describe("passes with a duplexer", () => {
	test("one pass, sheets reversed but each front before its back", () => {
		const r = passes(plan(pages(3), { ...base, duplex: "long" }), true, DEFAULT_CALIBRATION, true);
		expect(r.length).toBe(1);
		expect(order(r[0])).toEqual([[2], [], [0], [1]]);
	});

	test("short edge leaves the turning to the printer", () => {
		const [p] = passes(plan(pages(2), { ...base, duplex: "short" }), false, DEFAULT_CALIBRATION, true);
		expect(order(p)).toEqual([[0], [1]]);
		expect(p[1].cells[0].rot).toBe(0);
	});
});

describe("passes in landscape", () => {
	test("booklet turns backs, long-edge 2-up does not", () => {
		const [, bb] = passes(plan(pages(4), { ...base, mode: "booklet" }), false, DEFAULT_CALIBRATION);
		expect(bb[0].cells[0].rot).toBe(180);
		const [, bl] = passes(plan(pages(4), { ...base, perSheet: 2, duplex: "long" }), false, DEFAULT_CALIBRATION);
		expect(bl[0].cells[0].rot).toBe(0);
	});
});

describe("page numbers", () => {
	const numbered = { ...base, pageNumbers: { ...DEFAULT_PAGE_NUMBERS, enabled: true, format: "{page} / {total}", skip: 1 } };
	const texts = (l: ReturnType<typeof plan>) => l.sides.map((s) => (s.labels ?? []).map((x) => x.text));

	test("skip cover, count the rest", () => {
		expect(texts(plan(pages(3), numbered))).toEqual([[], ["1 / 2"], ["2 / 2"]]);
	});

	test("bottom center sits inside the page", () => {
		const [l] = plan(pages(1), { ...numbered, pageNumbers: { ...numbered.pageNumbers, skip: 0 } }).sides[0].labels!;
		expect(l.x).toBeCloseTo(595.28 / 2, 0);
		expect(l.y).toBeCloseTo(841.89 - 8 * (72 / 25.4), 0);
		expect(l.angle).toBe(0);
	});

	test("2-up scales the number with the page", () => {
		const l = plan(pages(2), { ...numbered, perSheet: 2, pageNumbers: { ...numbered.pageNumbers, skip: 0 } }).sides[0].labels!;
		expect(l.length).toBe(2);
		expect(l[0].size).toBeLessThan(10);
	});

	test("auto-rotated page turns its number too", () => {
		const s = { ...numbered, orientation: "portrait" as const, pageNumbers: { ...numbered.pageNumbers, skip: 0 } };
		const [l] = plan(pages(1, [841.89, 595.28]), s).sides[0].labels!;
		expect(l.angle).toBe(270);
	});

	test("booklet padding stays unnumbered", () => {
		const s = { ...numbered, mode: "booklet" as const, pageNumbers: { ...numbered.pageNumbers, skip: 0, format: "{page}" } };
		expect(texts(plan(pages(3), s)).flat().sort()).toEqual(["1", "2", "3"]);
	});
});
