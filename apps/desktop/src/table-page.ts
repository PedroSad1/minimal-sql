export const DEFAULT_TABLE_PAGE_SIZE = 100;
export const MAX_TABLE_PAGE_SIZE = 1000;

const STORAGE_KEY = "minimal-sql.tablePageSize";

export function clampTablePageSize(value: number) {
  if (!Number.isFinite(value)) return DEFAULT_TABLE_PAGE_SIZE;
  return Math.min(MAX_TABLE_PAGE_SIZE, Math.max(1, Math.round(value)));
}

export function readTablePageSize() {
  const stored = localStorage.getItem(STORAGE_KEY);
  if (!stored) return DEFAULT_TABLE_PAGE_SIZE;
  return clampTablePageSize(Number(stored));
}

export function writeTablePageSize(value: number) {
  const next = clampTablePageSize(value);
  localStorage.setItem(STORAGE_KEY, String(next));
  return next;
}
