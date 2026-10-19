export type Unit = "mm" | "in";

export const MM_PER_IN = 25.4;

/** Millimetres to a unit's number, rounded for display (inches get 2 decimals). */
export const toUnit = (mm: number, unit: Unit) => (unit === "in" ? +(mm / MM_PER_IN).toFixed(2) : +mm.toFixed(2));

export const fromUnit = (v: number, unit: Unit) => (unit === "in" ? v * MM_PER_IN : v);

/** "210 mm" or "8.27 in". */
export const fmtLen = (mm: number, unit: Unit) => `${toUnit(mm, unit)} ${unit}`;

/** "210 × 297 mm" or "8.27 × 11.69 in". */
export const fmtSize = (w: number, h: number, unit: Unit) => `${toUnit(w, unit)} × ${toUnit(h, unit)} ${unit}`;
