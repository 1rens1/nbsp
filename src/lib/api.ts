import { invoke } from "@tauri-apps/api/core";

export interface SourceInfo {
	id: number;
	name: string;
	path: string;
	/** Displayed page sizes in points. */
	pages: [number, number][];
}

export interface Cell {
	src: number;
	index: number;
	rot: number;
	/** Points, top-left origin. */
	x: number;
	y: number;
	w: number;
	h: number;
	border?: boolean;
}

/** Text stamped on a sheet (page numbers). Points, top-left origin; (x, y) is the baseline anchor. */
export interface Label {
	text: string;
	x: number;
	y: number;
	size: number;
	/** Clockwise degrees around the anchor. */
	angle: number;
	align: "left" | "center" | "right";
}

export interface Sheet {
	w: number;
	h: number;
	cells: Cell[];
	lines?: [number, number, number, number][];
	labels?: Label[];
}

export interface Printer {
	name: string;
	isDefault: boolean;
	state: string;
}

export interface PrinterOption {
	key: string;
	label: string;
	choices: string[];
	/** The driver's names for `choices`, same order. */
	labels: string[];
	default: string;
}

export interface PrinterPaper {
	/** PageSize choice. */
	id: string;
	label: string;
	/** Portrait, mm. */
	w: number;
	h: number;
}

export interface PrinterOptions {
	options: PrinterOption[];
	/** [key1, choice1, key2, choice2]: combinations the driver rejects. */
	constraints: [string, string, string, string][];
	papers: PrinterPaper[];
	/** Custom size range in mm: [min width, max width, min height, max height]. */
	custom: [number, number, number, number] | null;
	/** Prints both sides by itself. */
	duplex: boolean;
	/** Sheets land printed side up (last page on top). */
	faceUp: boolean;
}

export interface Job {
	id: string;
	user: string;
	size: number;
	time: string;
}

export interface PrinterInfo {
	name: string;
	model: string;
	uri: string;
	location: string;
	state: string;
	reasons: string[];
	accepting: boolean;
	jobs: Job[];
	completed: Job[];
}

export interface DeviceStatus {
	available: boolean;
	status: string;
	detail: string;
}

export interface PrintRequest {
	printer: string;
	title: string;
	copies: number;
	collate: boolean;
	options: [string, string][];
}

export interface Scanner {
	id: string;
	name: string;
}

export interface ScanOptions {
	device: string;
	mode: string;
	dpi: number;
	/** mm from the glass's top-left: [x, y, w, h]. */
	area: [number, number, number, number];
	brightness: number;
	contrast: number;
}

/** What the command line asked for (see launch.rs). */
export interface Launch {
	files: string[];
	inbox: string[];
	print: boolean;
	scan: boolean;
	quitAfter: boolean;
	preset: string | null;
	printer: string | null;
	settings: Record<string, unknown>;
	ppd: Record<string, string>;
}

export const api = {
	openFiles: (paths: string[], consume = false) =>
		invoke<{ sources: SourceInfo[]; errors: string[] }>("open_files", { paths, consume }),
	/** Stores pasted bytes as a temp file; returns its path. */
	savePasted: (name: string, data: ArrayBuffer) =>
		invoke<string>("save_pasted", data, { headers: { "x-name": encodeURIComponent(name) } }),
	/** Re-lays out a DOCX/ODT for a paper size (mm, portrait). */
	reflowOffice: (path: string, paper: [number, number]) => invoke<SourceInfo>("reflow_office", { path, paper }),
	closeSource: (id: number) => invoke<void>("close_source", { id }),
	exportPdf: (sheets: Sheet[], path: string) => invoke<void>("export_pdf", { sheets, path }),
	print: (sheets: Sheet[], request: PrintRequest) => invoke<string>("print_sheets", { sheets, request }),
	printers: () => invoke<Printer[]>("printers"),
	printerOptions: (printer: string) => invoke<PrinterOptions>("printer_options", { printer }),
	printerInfo: (printer: string) => invoke<PrinterInfo>("printer_info", { printer }),
	deviceStatus: (model: string) => invoke<DeviceStatus>("printer_device_status", { model }),
	maintenance: (printer: string, action: "nozzle" | "clean") => invoke<void>("printer_maintenance", { printer, action }),
	resumePrinter: (printer: string) => invoke<void>("printer_resume", { printer }),
	cancelAll: (printer: string) => invoke<void>("cancel_all_jobs", { printer }),
	openEpsonUtility: () => invoke<void>("open_epson_utility"),
	openUrl: (url: string) => invoke<void>("open_url", { url }),
	jobActive: (printer: string, job: string) => invoke<boolean>("job_active", { printer, job }),
	cancelJob: (job: string) => invoke<void>("cancel_job", { job }),
	scanners: () => invoke<Scanner[]>("scanners"),
	/** Width and height of the scanner's glass, mm. */
	scannerGlass: (device: string) => invoke<[number, number]>("scanner_glass", { device }),
	scan: (options: ScanOptions) => invoke<SourceInfo>("scan_page", { options }),
	scanPreview: (options: ScanOptions) => invoke<ArrayBuffer>("scan_preview", { options }),
	cancelScan: () => invoke<void>("cancel_scan"),
	takeLaunch: () => invoke<Launch | null>("take_launch"),
	thumbBase: () => invoke<string>("thumb_base"),
	restartApp: () => invoke<void>("restart_app"),
	capabilities: () =>
		invoke<{
			office: boolean;
			virtualPrinter: boolean;
			epsonUtility: boolean;
			installer: boolean;
			scan: boolean;
			missing: string[];
		}>("capabilities"),
	diagnostics: (printer: string | null) => invoke<string>("diagnostics", { printer }),
	setupVirtualPrinter: (remove: boolean) => invoke<void>("setup_virtual_printer", { remove }),
	testPages: () => invoke<SourceInfo>("duplex_test_pages"),
};

// Up to the server's 4000 px cap, so high zoom stays sharp (only visible sheets load these).
const BUCKETS = [160, 320, 640, 1280, 2000, 3000, 4000];

/** Loopback thumbnail server (see thumbs.rs); set once at startup by `initThumbs`. */
let thumbBase = "";

export async function initThumbs() {
	thumbBase = await api.thumbBase();
}

/** Thumbnail URL; width is snapped to a few buckets so the render cache gets hits. */
export function pageUrl(src: number, index: number, rot: number, px: number, gray = false) {
	const w = BUCKETS.find((b) => b >= px) ?? BUCKETS[BUCKETS.length - 1];
	return `${thumbBase}/page/${src}/${index}?w=${w}&r=${rot}${gray ? "&g=1" : ""}`;
}
