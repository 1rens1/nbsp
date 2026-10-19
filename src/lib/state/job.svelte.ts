import { SvelteMap, SvelteSet } from "svelte/reactivity";
import { toast } from "svelte-sonner";
import { api, type SourceInfo } from "$lib/api";
import type { LayoutPage } from "$lib/layout";
import { settings } from "$lib/state/settings.svelte";

export type JobPage =
	| { id: string; kind: "page"; src: number; index: number; rot: number }
	| { id: string; kind: "blank"; w: number; h: number; rot: number };

let seq = 0;
const newId = () => `p${++seq}`;

const A4: [number, number] = [595.28, 841.89];

/** Formats whose text can be laid out for another paper size (see reflow.rs). */
const reflowable = (path: string) => /\.(docx|odt)$/i.test(path);

class Job {
	sources = new SvelteMap<number, SourceInfo>();
	pages = $state<JobPage[]>([]);
	selected = new SvelteSet<string>();
	/** Anchor for shift-click range selection, and the keyboard cursor. */
	anchor = $state<string | null>(null);
	/** Last page picked; a fresh object each time so picking the same page again still re-scrolls the preview. */
	reveal = $state<{ id: string } | null>(null);
	loading = $state(0);
	/** Paper each reflowable source was laid out for ("" = as authored). */
	#laidOut = new Map<number, string>();
	#reflowing = false;
	#reflowAgain = false;

	/** Some open source is a document that can be re-laid out. */
	canReflow = $derived([...this.sources.values()].some((s) => reflowable(s.path)));

	#undo: JobPage[][] = [];
	#redo: JobPage[][] = [];
	canUndo = $state(false);
	canRedo = $state(false);

	title = $derived.by(() => {
		const names = [...new Set(this.pages.filter((p) => p.kind === "page").map((p) => (p as { src: number }).src))]
			.map((id) => this.sources.get(id)?.name)
			.filter(Boolean);
		return names.length === 0 ? "Untitled" : names.length === 1 ? names[0]! : `${names[0]} +${names.length - 1}`;
	});

	size(p: JobPage): [number, number] {
		const [w, h] = p.kind === "page" ? (this.sources.get(p.src)?.pages[p.index] ?? A4) : [p.w, p.h];
		return p.rot % 180 === 0 ? [w, h] : [h, w];
	}

	layoutPage(p: JobPage): LayoutPage {
		const [w, h] = this.size(p);
		return p.kind === "page" ? { src: p.src, index: p.index, rot: p.rot, w, h } : { index: 0, rot: 0, w, h };
	}

	/** Records the current page list for undo, then applies `next`. */
	commit(next: JobPage[]) {
		this.#undo.push(this.pages);
		if (this.#undo.length > 200) this.#undo.shift();
		this.#redo = [];
		this.pages = next;
		this.#sync();
	}

	undo() {
		const prev = this.#undo.pop();
		if (!prev) return;
		this.#redo.push(this.pages);
		this.pages = prev;
		this.#sync();
	}

	redo() {
		const next = this.#redo.pop();
		if (!next) return;
		this.#undo.push(this.pages);
		this.pages = next;
		this.#sync();
	}

	#sync() {
		this.canUndo = this.#undo.length > 0;
		this.canRedo = this.#redo.length > 0;
		const ids = new Set(this.pages.map((p) => p.id));
		for (const id of this.selected) if (!ids.has(id)) this.selected.delete(id);
		this.#release();
	}

	/** Closes sources no page can reach any more, in the job or its undo/redo history. */
	#release() {
		const used = new Set<number>();
		for (const list of [this.pages, ...this.#undo, ...this.#redo])
			for (const p of list) if (p.kind === "page") used.add(p.src);
		for (const id of this.sources.keys())
			if (!used.has(id)) {
				this.sources.delete(id);
				this.#laidOut.delete(id);
				api.closeSource(id);
			}
	}

	addSource(s: SourceInfo, at = this.pages.length) {
		this.sources.set(s.id, s);
		const added: JobPage[] = s.pages.map((_, index) => ({ id: newId(), kind: "page", src: s.id, index, rot: 0 }));
		this.commit([...this.pages.slice(0, at), ...added, ...this.pages.slice(at)]);
		return added;
	}

	async open(paths: string[], { at, consume }: { at?: number; consume?: boolean } = {}) {
		if (!paths.length) return;
		this.loading++;
		try {
			let pos = at ?? this.pages.length;
			for (const p of paths) {
				const key = this.#paperKey();
				const res = key && reflowable(p) ? await this.#reflowOne(p) : await api.openFiles([p], consume);
				for (const s of res.sources) {
					this.#laidOut.set(s.id, key && reflowable(p) ? key : "");
					pos += this.addSource(s, pos).length;
				}
				for (const e of res.errors) toast.error(e);
			}
		} catch (e) {
			toast.error(String(e));
		} finally {
			this.loading--;
		}
	}

	/** The paper to lay documents out for, or "" when reflow is off. */
	#paperKey() {
		const { reflow, paper } = settings.s;
		return reflow ? `${paper.w}x${paper.h}` : "";
	}

	async #reflowOne(path: string) {
		const { paper } = settings.s;
		try {
			return { sources: [await api.reflowOffice(path, [paper.w, paper.h])], errors: [] as string[] };
		} catch (e) {
			return { sources: [], errors: [String(e)] };
		}
	}

	/**
	 * Re-lays out every document for the current paper (or back to as authored when reflow is off).
	 * Page edits on a re-laid-out document are dropped: its pages are different ones now.
	 */
	async reflow() {
		if (this.#reflowing) {
			this.#reflowAgain = true;
			return;
		}
		this.#reflowing = true;
		this.loading++;
		try {
			do {
				this.#reflowAgain = false;
				const key = this.#paperKey();
				// Only sources still in the job: superseded ones stay open for undo.
				const live = new Set(this.pages.flatMap((p) => (p.kind === "page" ? [p.src] : [])));
				const stale = [...this.sources.values()].filter(
					(s) => live.has(s.id) && reflowable(s.path) && (this.#laidOut.get(s.id) ?? "") !== key
				);
				for (const old of stale) {
					const res = key ? await this.#reflowOne(old.path) : await api.openFiles([old.path]);
					for (const e of res.errors) toast.error(e);
					const fresh = res.sources[0];
					if (fresh) this.#swap(old, fresh, key);
				}
			} while (this.#reflowAgain);
		} finally {
			this.loading--;
			this.#reflowing = false;
		}
	}

	/** Puts `fresh`'s pages where `old`'s were. */
	#swap(old: SourceInfo, fresh: SourceInfo, key: string) {
		this.#laidOut.set(fresh.id, key);
		this.sources.set(fresh.id, fresh);
		const mine = this.pages.filter((p) => p.kind === "page" && p.src === old.id);
		const edited = mine.length !== old.pages.length || mine.some((p) => p.rot !== 0);
		if (edited) toast.info(`Page edits to ${old.name} were reset`);
		const first = this.pages.findIndex((p) => p.kind === "page" && p.src === old.id);
		const added: JobPage[] = fresh.pages.map((_, index) => ({ id: newId(), kind: "page", src: fresh.id, index, rot: 0 }));
		const next = this.pages.filter((p) => !(p.kind === "page" && p.src === old.id));
		next.splice(Math.max(0, first), 0, ...added);
		this.commit(next);
	}

	/** Empties the job (undoable: sources stay open while the history can bring them back). */
	clear() {
		this.selected.clear();
		this.commit([]);
	}

	// Selection

	select(id: string, mode: "set" | "toggle" | "range") {
		this.reveal = { id };
		const ids = this.pages.map((p) => p.id);
		if (mode === "range" && this.anchor && ids.includes(this.anchor)) {
			const a = ids.indexOf(this.anchor);
			const b = ids.indexOf(id);
			this.selected.clear();
			for (const x of ids.slice(Math.min(a, b), Math.max(a, b) + 1)) this.selected.add(x);
			return;
		}
		if (mode === "toggle") {
			if (this.selected.has(id)) this.selected.delete(id);
			else this.selected.add(id);
		} else {
			this.selected.clear();
			this.selected.add(id);
		}
		this.anchor = id;
	}

	/** Arrow keys: moves the cursor one page; `extend` grows the selection like shift-click. */
	step(dir: 1 | -1, extend: boolean) {
		if (!this.pages.length) return;
		const ids = this.pages.map((p) => p.id);
		const cur = this.anchor ? ids.indexOf(this.anchor) : -1;
		const next = ids[cur < 0 ? (dir > 0 ? 0 : ids.length - 1) : Math.max(0, Math.min(ids.length - 1, cur + dir))];
		if (extend && this.anchor) {
			// Keep the anchor where the range started; move only the far end.
			const start = this.anchor;
			this.select(next, "range");
			this.anchor = start;
			this.cursor = next;
		} else {
			this.select(next, "set");
			this.cursor = next;
		}
	}

	/** The page the keyboard last moved to (scrolled into view by the pages panel). */
	cursor = $state<string | null>(null);

	/** Alt+arrows: moves the selected pages one place. */
	nudge(dir: 1 | -1) {
		if (!this.selected.size) return;
		const idx = this.pages.flatMap((p, i) => (this.selected.has(p.id) ? [i] : []));
		const to = dir < 0 ? idx[0] - 1 : idx[idx.length - 1] + 2;
		if (to < 0 || to > this.pages.length) return;
		this.move(new Set(this.selected), to);
		this.cursor = dir < 0 ? this.pages[idx[0] - 1].id : this.pages[idx[idx.length - 1] + 1].id;
	}

	selectAll() {
		for (const p of this.pages) this.selected.add(p.id);
	}

	/** Selected ids, or the given one when the selection doesn't include it (context menus). */
	targets(id?: string) {
		return id && !this.selected.has(id) ? new Set([id]) : new Set(this.selected);
	}

	// Edits

	rotate(ids: Set<string>, by: number) {
		if (!ids.size) return;
		this.commit(this.pages.map((p) => (ids.has(p.id) ? { ...p, rot: (p.rot + by + 360) % 360 } : p)));
	}

	remove(ids: Set<string>) {
		if (!ids.size) return;
		this.commit(this.pages.filter((p) => !ids.has(p.id)));
	}

	duplicate(ids: Set<string>) {
		if (!ids.size) return;
		this.commit(this.pages.flatMap((p) => (ids.has(p.id) ? [p, { ...p, id: newId() }] : [p])));
	}

	insertBlank(after?: string) {
		const i = after ? this.pages.findIndex((p) => p.id === after) : this.pages.length - 1;
		const ref = this.pages[i];
		const [w, h] = ref ? this.size(ref) : A4;
		const blank: JobPage = { id: newId(), kind: "blank", w, h, rot: 0 };
		this.commit([...this.pages.slice(0, i + 1), blank, ...this.pages.slice(i + 1)]);
	}

	reverse(ids: Set<string>) {
		const all = ids.size < 2;
		const positions = this.pages.map((p, i) => (all || ids.has(p.id) ? i : -1)).filter((i) => i >= 0);
		const picked = positions.map((i) => this.pages[i]).reverse();
		const next = [...this.pages];
		positions.forEach((pos, k) => (next[pos] = picked[k]));
		this.commit(next);
	}

	/** Moves the given pages so they sit before index `to` of the current list. */
	move(ids: Set<string>, to: number) {
		const moving = this.pages.filter((p) => ids.has(p.id));
		const before = this.pages.slice(0, to).filter((p) => !ids.has(p.id));
		const after = this.pages.slice(to).filter((p) => !ids.has(p.id));
		this.commit([...before, ...moving, ...after]);
	}
}

export const job = new Job();
