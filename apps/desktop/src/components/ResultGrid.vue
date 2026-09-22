<template>
  <div class="result-wrap" @copy="onCopyEvent" @contextmenu="onGridMenu" @dblclick="onGridDblClick">
    <div ref="host" class="tabulator-host" data-testid="result-grid" tabindex="0"></div>
    <ContextMenu
      :open="menu.open"
      :x="menu.x"
      :y="menu.y"
      :options="menu.options"
      @close="menu.open = false"
      @pick="onMenuPick"
    />
  </div>
</template>

<script setup lang="ts">
import { TabulatorFull as Tabulator } from "tabulator-tables";
import type { CellComponent, ColumnComponent, RangeComponent } from "tabulator-tables";
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref, watch } from "vue";
import { formatCopy, hideLabel, type CopyBlock, type CopyKind } from "../column-menu";
import type { NgQueryResult, RowChange, TableChanges } from "../ipc";
import { copyText } from "../shell";
import ContextMenu, { type MenuOption } from "./ContextMenu.vue";

const ROW_HEADER = "--row-header--";
const NEW_ROW = "--new-row--";

const props = withDefaults(
  defineProps<{
    result: NgQueryResult;
    editable?: boolean;
    tableName?: string;
    schema?: string | null;
    primaryKeys?: string[];
    active?: boolean;
    syncMode?: "replace" | "append";
    canLoadMore?: boolean;
  }>(),
  { active: true, syncMode: "replace", canLoadMore: false },
);

const emit = defineEmits<{
  (event: "select-json", value: unknown): void;
  (event: "pending-count", count: number): void;
  (event: "selection-count", count: number): void;
  (event: "near-end"): void;
}>();

const host = ref<HTMLElement | null>(null);
let table: Tabulator | null = null;
let generation = 0;
let sizeObserver: ResizeObserver | null = null;
let layoutObserver: ResizeObserver | null = null;
let bootObserver: ResizeObserver | null = null;
let cancelLayoutWait: (() => void) | null = null;
const originals = new Map<string, Record<string, unknown>>();
const pending = new Map<string, RowChange>();
const inserts = new Map<string, RowChange>();
const deletes = new Map<string, RowChange>();
let insertSeq = 0;

const pkSet = computed(() => new Set(props.primaryKeys ?? []));
let structureKey = "";

function rowKey(row: Record<string, unknown>) {
  if (typeof row[NEW_ROW] === "string") return row[NEW_ROW];
  const keys = props.primaryKeys ?? [];
  if (!keys.length) return JSON.stringify(row);
  return JSON.stringify(keys.map((key) => [key, row[key]]));
}

function coerce(original: unknown, edited: unknown) {
  if (edited === original) return original;
  if (typeof original === "number") {
    const number = Number(edited);
    return Number.isNaN(number) ? edited : number;
  }
  if (typeof original === "boolean") return edited === true || edited === "true";
  if (original === null && (edited === "" || edited === "null")) return null;
  return edited;
}

function isJsonish(value: unknown) {
  if (value && typeof value === "object") return true;
  if (typeof value !== "string") return false;
  const trimmed = value.trim();
  return trimmed.startsWith("{") || trimmed.startsWith("[");
}

function asJson(value: unknown) {
  if (value && typeof value === "object") return value;
  if (typeof value !== "string") return value;
  const trimmed = value.trim();
  if (!(trimmed.startsWith("{") || trimmed.startsWith("["))) return value;
  try {
    return JSON.parse(trimmed);
  } catch {
    return value;
  }
}

function notifyPending() {
  emit("pending-count", pending.size + inserts.size + deletes.size);
}

function clearLocalChanges() {
  pending.clear();
  inserts.clear();
  deletes.clear();
  notifyPending();
  emit("selection-count", 0);
}

function typedValue(fieldId: string, value: unknown) {
  if (value === null || value === undefined || value === "") return null;
  if (typeof value === "number" || typeof value === "boolean") return value;
  const sample = (props.result.rows ?? []).find((row) => row[fieldId] != null)?.[fieldId];
  if (typeof sample === "number") {
    const number = Number(value);
    return Number.isNaN(number) ? value : number;
  }
  if (typeof sample === "boolean") return value === true || value === "true" || value === "t";
  return value;
}

function syncInsert(data: Record<string, unknown>) {
  const id = data[NEW_ROW];
  if (typeof id !== "string" || !props.tableName) return false;
  const values: [string, unknown][] = [];
  for (const field of props.result.fields ?? []) {
    const value = typedValue(field.id, data[field.id]);
    if (value === null) continue;
    values.push([field.name, value]);
  }
  if (!values.length) inserts.delete(id);
  else {
    inserts.set(id, {
      table: props.tableName,
      schema: props.schema ?? null,
      primaryKeys: [],
      values,
    });
  }
  return true;
}

function buildChanges(): TableChanges {
  return {
    inserts: [...inserts.values()],
    updates: [...pending.values()],
    deletes: [...deletes.values()],
  };
}

function discard() {
  clearLocalChanges();
  void rebuild();
}

function paintRow(row: { getData: () => unknown; getElement: () => HTMLElement }) {
  const data = row.getData() as Record<string, unknown>;
  const element = row.getElement();
  const fresh = typeof data[NEW_ROW] === "string";
  element.classList.toggle("row-new", fresh);
  element.classList.toggle("row-deleted", deletes.has(fresh ? String(data[NEW_ROW]) : rowKey(data)));
}

function selectedRows() {
  if (!table) return [];
  const rows = [];
  const seen = new Set<string>();
  for (const range of table.getRanges()) {
    for (const row of range.getRows()) {
      const data = row.getData() as Record<string, unknown>;
      const key = rowKey(data);
      if (seen.has(key)) continue;
      seen.add(key);
      rows.push(row);
    }
  }
  return rows;
}

function refreshSelection() {
  emit("selection-count", selectedRows().length);
}

async function addDataRow() {
  if (!table || !props.tableName || !props.editable) return;
  const id = `new-${++insertSeq}`;
  const record: Record<string, unknown> = { [NEW_ROW]: id };
  for (const field of props.result.fields ?? []) record[field.id] = null;
  originals.set(id, { ...record });
  const row = await table.addRow(record);
  paintRow(row);
  try {
    await row.scrollTo();
  } catch {
    // The new row stays in the grid when the scroll fails.
  }
  const cell = row.getCells().find((item) => String(item.getField()) !== ROW_HEADER);
  cell?.edit();
}

function deleteDataRows() {
  if (!table || !props.tableName || !props.editable) return;
  for (const row of selectedRows()) {
    const data = row.getData() as Record<string, unknown>;
    if (typeof data[NEW_ROW] === "string") {
      inserts.delete(data[NEW_ROW]);
      originals.delete(data[NEW_ROW]);
      void row.delete();
      continue;
    }
    const key = rowKey(data);
    if (deletes.has(key)) {
      deletes.delete(key);
      paintRow(row);
      continue;
    }
    const original = originals.get(key);
    if (!original) continue;
    const primaryKeys = (props.primaryKeys ?? []).map((column) => [column, original[column]] as [string, unknown]);
    if (!primaryKeys.length || primaryKeys.some(([, value]) => value === null || value === undefined)) continue;
    pending.delete(key);
    deletes.set(key, {
      table: props.tableName,
      schema: props.schema ?? null,
      primaryKeys,
      values: [],
    });
    paintRow(row);
  }
  notifyPending();
  refreshSelection();
}

function stopSizing() {
  sizeObserver?.disconnect();
  sizeObserver = null;
}

function stopLayoutWait() {
  const cancel = cancelLayoutWait;
  cancelLayoutWait = null;
  layoutObserver?.disconnect();
  layoutObserver = null;
  cancel?.();
}

function whenLaidOut(el: HTMLElement, token: number) {
  stopLayoutWait();
  return new Promise<boolean>((resolve) => {
    let frames = 0;
    let settled = false;
    const finish = (ok: boolean) => {
      if (settled) return;
      settled = true;
      layoutObserver?.disconnect();
      layoutObserver = null;
      cancelLayoutWait = null;
      resolve(ok && token === generation);
    };
    cancelLayoutWait = () => finish(false);
    const ready = () =>
      token === generation && el.isConnected && props.active !== false && el.clientHeight > 0 && el.clientWidth > 0;
    const watchBox = () => {
      layoutObserver?.disconnect();
      layoutObserver = new ResizeObserver(() => {
        if (token !== generation || !el.isConnected) return finish(false);
        if (ready()) finish(true);
      });
      layoutObserver.observe(el);
      if (el.parentElement) layoutObserver.observe(el.parentElement);
    };
    const tick = () => {
      if (token !== generation || !el.isConnected) return finish(false);
      if (ready()) return finish(true);
      frames += 1;
      if (frames < 45) {
        requestAnimationFrame(tick);
        return;
      }
      watchBox();
    };
    tick();
  });
}

function paintUntilVisible(el: HTMLElement, token: number) {
  let frames = 0;
  const tick = () => {
    if (token !== generation || !table) return;
    const row = el.querySelector(".tabulator-row") as HTMLElement | null;
    const holder = el.querySelector(".tabulator-tableholder") as HTMLElement | null;
    const visible =
      props.active !== false && el.clientHeight > 0 && (holder?.clientHeight ?? 0) > 0 && (row?.offsetHeight ?? 0) > 0;
    if (visible || table.destroyed) return;
    frames += 1;
    if (el.clientHeight > 0 && el.querySelector(".tabulator")) table.redraw(true);
    if (frames < 60) requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
}

function watchSize(el: HTMLElement, token: number) {
  stopSizing();
  let height = 0;
  let painting = false;
  sizeObserver = new ResizeObserver(() => {
    if (painting || token !== generation || !table || !props.active) return;
    const next = el.clientHeight;
    if (next <= 0 || next === height) return;
    height = next;
    painting = true;
    if (el.querySelector(".tabulator")) table.redraw(true);
    requestAnimationFrame(() => {
      painting = false;
    });
  });
  sizeObserver.observe(el);
}

function cloneRows() {
  return (props.result.rows ?? []).map((row) => ({ ...row }));
}

function remember(rows: Record<string, unknown>[]) {
  originals.clear();
  for (const row of rows) originals.set(rowKey(row), { ...row });
}

function currentStructureKey() {
  const fields = (props.result.fields ?? [])
    .map((field) => `${field.id}:${(field.enumValues ?? []).join("\u001f")}`)
    .join(",");
  const keys = (props.primaryKeys ?? []).join(",");
  return `${fields}|${props.tableName ?? ""}|${props.editable}|${keys}`;
}

function canEditCell(cell: CellComponent, isPk: boolean) {
  const data = cell.getRow().getData() as Record<string, unknown>;
  if (typeof data[NEW_ROW] === "string") return true;
  return !isPk;
}

function columnEditor(values: string[] | null | undefined, canEdit: boolean, isPk: boolean) {
  if (!canEdit) return { editor: false as const, editable: false as const };
  const editable = (cell: CellComponent) => canEditCell(cell, isPk);
  if (values?.length) return { editor: false as const, editable };
  return { editor: "input" as const, editable };
}

function cellText(value: unknown) {
  if (value == null) return "";
  const text = typeof value === "object" ? JSON.stringify(value) : String(value);
  if (/[\t\n\r"]/.test(text)) return `"${text.replace(/"/g, '""')}"`;
  return text;
}

function selectionText() {
  if (!table) return "";
  const ranges = table.getRanges();
  if (!ranges.length) return "";
  return ranges
    .map((range) => {
      const columns = range.getColumns().filter((column) => String(column.getField()) !== ROW_HEADER);
      const rows = range.getRows();
      return rows
        .map((row) => {
          const data = row.getData() as Record<string, unknown>;
          return columns.map((column) => cellText(data[column.getField()])).join("\t");
        })
        .join("\n");
    })
    .filter(Boolean)
    .join("\n\n");
}

function typingTarget(target: EventTarget | null) {
  return target instanceof Element && Boolean(target.closest("input, textarea, select, [contenteditable='true'], .cm-content"));
}

async function writeSelection(text: string) {
  if (!text) return;
  try {
    await copyText(text);
  } catch {
    await navigator.clipboard.writeText(text);
  }
}

const menu = reactive({
  open: false,
  x: 0,
  y: 0,
  kind: "column" as "column" | "enum",
  options: [] as MenuOption[],
});
let menuColumn: ColumnComponent | null = null;
let menuCell: CellComponent | null = null;

function isRowHeader(column: { getField: () => unknown }) {
  return String(column.getField()) === ROW_HEADER;
}

function columnTitle(column: ColumnComponent) {
  return column.getDefinition().title || String(column.getField());
}

function columnEditable(column: ColumnComponent) {
  return column.getDefinition().editable !== false;
}

function rangeIncludesCell(range: RangeComponent, cell: CellComponent) {
  const field = String(cell.getField());
  if (!range.getColumns().some((column) => String(column.getField()) === field)) return false;
  const data = cell.getRow().getData();
  return range.getRows().some((row) => row.getData() === data);
}

function toBlock(range: RangeComponent): CopyBlock {
  return {
    columns: range
      .getColumns()
      .filter((column) => !isRowHeader(column))
      .map((column) => ({ field: String(column.getField()), title: columnTitle(column) })),
    rows: range.getRows().map((row) => row.getData() as Record<string, unknown>),
  };
}

function targetBlocks() {
  const ranges = (table?.getRanges() ?? []).filter((range) => range.getColumns().length && range.getRows().length);
  if (menuCell) {
    const hit = ranges.filter((range) => rangeIncludesCell(range, menuCell as CellComponent));
    if (hit.length) return hit.map(toBlock);
    const column = menuCell.getColumn();
    return [{ columns: [{ field: String(column.getField()), title: columnTitle(column) }], rows: [menuCell.getRow().getData() as Record<string, unknown>] }];
  }
  if (!menuColumn) return [] as CopyBlock[];
  const field = String(menuColumn.getField());
  const hit = ranges.filter((range) => range.getColumns().some((column) => String(column.getField()) === field));
  if (hit.length) return hit.map(toBlock);
  const rows = (table?.getRows() ?? []).map((row) => row.getData() as Record<string, unknown>);
  return [{ columns: [{ field, title: columnTitle(menuColumn) }], rows }];
}

function nullableCells() {
  const ranges = table?.getRanges() ?? [];
  const cells: CellComponent[] = [];
  if (menuCell && (!ranges.length || ranges.every((range) => !rangeIncludesCell(range, menuCell as CellComponent)))) {
    if (columnEditable(menuCell.getColumn())) cells.push(menuCell);
    return cells;
  }
  for (const range of ranges) {
    for (const row of range.getRows()) {
      for (const column of range.getColumns()) {
        if (!columnEditable(column)) continue;
        const cell = row.getCell(column.getField());
        if (cell) cells.push(cell);
      }
    }
  }
  return cells;
}

const copyShortcut = navigator.platform.toLowerCase().includes("mac") ? "⌘C" : "Ctrl+C";

function menuOptions() {
  const blocks = targetBlocks();
  const oneColumn = blocks.length > 0 && blocks.every((block) => block.columns.length === 1);
  const title = menuColumn ? columnTitle(menuColumn) : "column";
  const canNull = nullableCells().length > 0;
  const options: MenuOption[] = [
    { name: "Set as NULL", slug: "null", disabled: !canNull },
    { divider: true, slug: "d1" },
    { name: "Copy", slug: "plain", shortcut: copyShortcut },
    { name: "Copy Column Name", slug: "columnName" },
    { name: "Copy as TSV for Excel", slug: "tsv" },
    { name: "Copy as JSON", slug: "json" },
    { name: "Copy as Markdown", slug: "markdown" },
    { name: "Copy as SQL", slug: "sql", disabled: !props.tableName },
  ];
  if (oneColumn) options.push({ name: "Copy for IN statement", slug: "in" });
  options.push(
    { divider: true, slug: "d2" },
    { name: "Sort ascending", slug: "sort-asc" },
    { name: "Sort descending", slug: "sort-desc" },
    { divider: true, slug: "d3" },
    { name: "Resize all columns to match", slug: "resize-match" },
    { name: "Resize all columns to fit content", slug: "resize-fit" },
    { name: "Resize all columns to fixed width", slug: "resize-fixed" },
    { divider: true, slug: "d4" },
    { name: hideLabel(title), slug: "hide" },
    { name: "Reset layout", slug: "reset" },
  );
  return options;
}

function columnFromEvent(target: EventTarget | null) {
  if (!(target instanceof Element) || !table) return null;
  const header = target.closest(".tabulator-col");
  if (!header) return null;
  return table.getColumns().find((column) => column.getElement() === header) ?? null;
}

function cellFromEvent(target: EventTarget | null) {
  if (!(target instanceof Element) || !table) return null;
  const node = target.closest(".tabulator-cell");
  if (!node) return null;
  for (const row of table.getRows()) {
    for (const cell of row.getCells()) {
      if (cell.getElement() === node) return cell;
    }
  }
  return null;
}

function enumValuesFor(field: string) {
  return (props.result.fields ?? []).find((item) => item.id === field || item.name === field)?.enumValues ?? [];
}

function onGridDblClick(event: MouseEvent) {
  if (props.active === false || !table) return;
  const cell = cellFromEvent(event.target);
  if (!cell || isRowHeader(cell.getColumn())) return;
  const column = cell.getColumn();
  const editable = column.getDefinition().editable;
  if (typeof editable === "function" ? !editable(cell) : !editable) return;
  const values = enumValuesFor(String(cell.getField()));
  if (!values.length) return;
  event.preventDefault();
  event.stopPropagation();
  menu.kind = "enum";
  menuColumn = column;
  menuCell = cell;
  placeMenu(event, enumOptions(cell, values));
}

function onGridMenu(event: MouseEvent) {
  if (props.active === false || !table) return;
  const column = columnFromEvent(event.target);
  const cell = cellFromEvent(event.target);
  if (!column && !cell) return;
  event.preventDefault();
  menuColumn = column ?? cell?.getColumn() ?? null;
  if (menuColumn && isRowHeader(menuColumn)) {
    menuColumn = null;
    return;
  }
  menuCell = cell;
  if (!menuColumn) return;
  menu.kind = "column";
  placeMenu(event, menuOptions());
}

function resizeAll(width: number | boolean) {
  if (!table) return;
  table.blockRedraw();
  try {
    for (const column of table.getColumns()) {
      if (isRowHeader(column)) continue;
      column.setWidth(width as number);
    }
  } finally {
    table.restoreRedraw();
  }
}

function placeMenu(event: MouseEvent, options: MenuOption[]) {
  const width = 320;
  const height = Math.min(options.length * 32, Math.round(window.innerHeight * 0.7));
  menu.options = options;
  menu.x = Math.max(8, Math.min(event.clientX, window.innerWidth - width));
  menu.y = Math.max(8, Math.min(event.clientY, window.innerHeight - height));
  menu.open = true;
}

function enumOptions(cell: CellComponent, values: string[]) {
  const current = cell.getValue();
  return values.map((value) => ({
    name: value,
    slug: value,
    icon: current === value ? "check" : undefined,
  }));
}

function onMenuPick(option: MenuOption) {
  menu.open = false;
  if (menu.kind === "enum") {
    menu.kind = "column";
    if (option.name != null && menuCell) menuCell.setValue(option.name, true);
    return;
  }
  const slug = option.slug;
  if (!slug || !menuColumn || !table) return;
  if (slug === "null") {
    for (const cell of nullableCells()) cell.setValue(null);
    return;
  }
  if (slug === "plain" || slug === "columnName" || slug === "tsv" || slug === "json" || slug === "markdown" || slug === "sql" || slug === "in") {
    const text = formatCopy(targetBlocks(), slug as CopyKind, {
      table: props.tableName,
      schema: props.schema,
    });
    void writeSelection(text);
    return;
  }
  if (slug === "sort-asc" || slug === "sort-desc") {
    table.setSort(menuColumn.getField(), slug === "sort-asc" ? "asc" : "desc");
    return;
  }
  if (slug === "resize-match") {
    resizeAll(menuColumn.getWidth());
    return;
  }
  if (slug === "resize-fit") {
    resizeAll(true);
    return;
  }
  if (slug === "resize-fixed") {
    resizeAll(200);
    return;
  }
  if (slug === "hide") {
    menuColumn.hide();
    return;
  }
  if (slug === "reset") {
    table.blockRedraw();
    try {
      for (const column of table.getColumns()) {
        if (isRowHeader(column)) continue;
        column.show();
        column.setWidth(true as unknown as number);
      }
    } finally {
      table.restoreRedraw();
    }
  }
}

function onCopyEvent(event: ClipboardEvent) {
  if (props.active === false || typingTarget(event.target)) return;
  const text = selectionText();
  if (!text || !event.clipboardData) return;
  event.preventDefault();
  event.clipboardData.setData("text/plain", text);
}

function onCopyKey(event: KeyboardEvent) {
  if (props.active === false) return;
  if (!(event.metaKey || event.ctrlKey) || event.key.toLowerCase() !== "c" || event.altKey) return;
  if (typingTarget(event.target)) return;
  const text = selectionText();
  if (!text) return;
  event.preventDefault();
  void writeSelection(text);
}

let lastScrollTop = 0;

function checkEnd() {
  if (props.active === false) return;
  const holder = host.value?.querySelector(".tabulator-tableholder") as HTMLElement | null;
  if (!holder || holder.clientHeight < 40) return;
  const top = holder.scrollTop;
  const scrollingDown = top > lastScrollTop + 2;
  lastScrollTop = top;
  const gap = holder.scrollHeight - top - holder.clientHeight;
  if (!scrollingDown || gap > 64 || holder.scrollHeight <= holder.clientHeight + 8) return;
  if (!props.canLoadMore) return;
  emit("near-end");
}

async function refreshRows() {
  const current = table;
  if (!current) return;
  const rows = cloneRows();
  remember(rows);
  clearLocalChanges();
  try {
    await current.replaceData(rows);
  } catch {
    if (table === current) void rebuild();
  }
}

async function appendRows(extra: Record<string, unknown>[]) {
  const current = table;
  if (!current || !extra.length) return;
  const copies = extra.map((row) => ({ ...row }));
  for (const row of copies) originals.set(rowKey(row), { ...row });
  try {
    await current.addData(copies);
  } catch {
    if (table === current) void rebuild();
  }
}

function useCommandAsControl(event: MouseEvent) {
  if (!event.metaKey || event.ctrlKey || event.button !== 0) return;
  event.preventDefault();
  event.stopPropagation();
  event.target?.dispatchEvent(
    new MouseEvent(event.type, {
      bubbles: true,
      cancelable: true,
      clientX: event.clientX,
      clientY: event.clientY,
      ctrlKey: true,
      shiftKey: event.shiftKey,
      button: event.button,
    }),
  );
}

async function rebuild() {
  const token = ++generation;
  stopLayoutWait();
  stopSizing();
  table?.destroy();
  table = null;
  structureKey = "";
  originals.clear();
  clearLocalChanges();
  const fieldsReady = props.result.fields ?? [];
  if (!fieldsReady.length) return;
  await nextTick();
  const el = host.value;
  if (token !== generation || !el) return;
  const ready = await whenLaidOut(el, token);
  if (!ready || token !== generation || host.value !== el) return;
  const fields = props.result.fields ?? [];
  const rows = cloneRows();
  remember(rows);
  const canEdit = Boolean(props.editable && (props.primaryKeys ?? []).length && props.tableName);
  table = new Tabulator(el, {
    data: rows,
    columns: fields.map((field) => {
      const isPk = pkSet.value.has(field.id) || pkSet.value.has(field.name);
      return {
        title: field.name,
        field: field.id,
        headerSort: true,
        ...columnEditor(field.enumValues, canEdit, isPk),
      };
    }),
    layout: "fitDataFill",
    height: "100%",
    renderVertical: "basic",
    rowHeader: {
      field: ROW_HEADER,
      title: "",
      resizable: false,
      frozen: true,
      headerSort: false,
      editor: false,
      editable: false,
      minWidth: 52,
      width: 52,
      hozAlign: "center",
      cssClass: "row-index",
      formatter: "rownum",
    },
    selectableRange: true,
    selectableRangeRows: true,
    selectableRangeColumns: true,
    selectableRangeClearCells: false,
    selectableRangeAutoFocus: false,
    selectableRangeInitializeDefault: false,
    headerSortClickElement: "icon",
    editTriggerEvent: "dblclick",
    placeholder: "0 linhas. Esta tabela está vazia.",
    rowFormatter: (row) => paintRow(row),
    clipboard: "copy",
    clipboardCopyRowRange: "range",
    clipboardCopyConfig: { columnHeaders: false, rowHeaders: false },
  });
  table.on("tableBuilt", () => {
    if (token !== generation || host.value !== el) return;
    structureKey = currentStructureKey();
    el.addEventListener("mousedown", useCommandAsControl, true);
    watchSize(el, token);
    paintUntilVisible(el, token);
    table?.on("scrollVertical", () => checkEnd());
    table?.on("rangeChanged", () => refreshSelection());
    refreshSelection();
  });
  table.on("cellClick", (_event, cell) => {
    if (isRowHeader(cell.getColumn())) return;
    const raw = cell.getValue();
    if (!isJsonish(raw)) {
      emit("select-json", null);
      return;
    }
    emit("select-json", asJson(raw));
  });
  table.on("cellEdited", (cell) => {
    if (!props.tableName) return;
    const data = cell.getRow().getData() as Record<string, unknown>;
    if (syncInsert(data)) {
      cell.getElement().classList.toggle("cell-edited", cell.getValue() != null && cell.getValue() !== "");
      notifyPending();
      return;
    }
    const key = rowKey(data);
    if (deletes.has(key)) return;
    const original = originals.get(key);
    if (!original) return;
    const field = String(cell.getField());
    const next = coerce(original[field], cell.getValue());
    data[field] = next;
    const values: [string, unknown][] = [];
    for (const [col, orig] of Object.entries(original)) {
      if (pkSet.value.has(col)) continue;
      if (JSON.stringify(data[col]) !== JSON.stringify(orig)) {
        values.push([col, data[col]]);
      }
    }
    if (!values.length) {
      pending.delete(key);
    } else {
      pending.set(key, {
        table: props.tableName,
        schema: props.schema ?? null,
        primaryKeys: (props.primaryKeys ?? []).map((col) => [col, original[col]]),
        values,
      });
    }
    cell
      .getElement()
      .classList.toggle("cell-edited", JSON.stringify(next) !== JSON.stringify(original[field]));
    notifyPending();
  });
}

watch(
  () => [props.result, props.editable, props.tableName, (props.primaryKeys ?? []).join("|"), props.syncMode] as const,
  () => {
    const fields = props.result.fields ?? [];
    const rows = props.result.rows ?? [];
    if (table && fields.length && currentStructureKey() === structureKey) {
      const have = table.getDataCount();
      if (props.syncMode === "append" && rows.length > have) {
        void appendRows(rows.slice(have));
        return;
      }
      void refreshRows();
      return;
    }
    void rebuild();
  },
  { immediate: true },
);

watch(
  () => props.active,
  (active) => {
    if (!active) return;
    const el = host.value;
    if (!table || !el) {
      void rebuild();
      return;
    }
    paintUntilVisible(el, generation);
  },
);

let bootQueued = false;

function watchUntilBox() {
  bootObserver?.disconnect();
  const el = host.value;
  const parent = el?.parentElement;
  if (!el || !parent) return;
  bootObserver = new ResizeObserver(() => {
    const node = host.value;
    if (bootQueued || !node || table) return;
    if (node.clientHeight <= 0 || node.clientWidth <= 0) return;
    if (!(props.result.fields ?? []).length) return;
    bootQueued = true;
    void rebuild().finally(() => {
      bootQueued = false;
    });
  });
  bootObserver.observe(parent);
  bootObserver.observe(el);
}

onMounted(() => {
  window.addEventListener("keydown", onCopyKey);
  watchUntilBox();
});
onBeforeUnmount(() => {
  window.removeEventListener("keydown", onCopyKey);
  generation += 1;
  bootObserver?.disconnect();
  bootObserver = null;
  stopLayoutWait();
  stopSizing();
  table?.destroy();
});

defineExpose({ buildChanges, discard, addDataRow, deleteDataRows });
</script>
