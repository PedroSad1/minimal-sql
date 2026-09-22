<template>
  <div class="graphite-wrapper">
    <Titlebar
      :title="windowTitle"
      :connected="Boolean(sessionId)"
      :json-sidebar-open="jsonSidebarOpen"
      :update-version="updateVersion"
      :updating="updating"
      :update-error="updateError"
      @cycle-theme="cycleTheme"
      @toggle-json="jsonSidebarOpen = !jsonSidebarOpen"
      @open-update="openUpdate"
    />
    <CoreInterface
      ref="core"
      :tables="tables"
      :views="views"
      :tabs="tabs"
      :active-tab-id="activeTabId"
      :json-value="jsonValue"
      :json-sidebar-open="jsonSidebarOpen"
      :hidden-keys="hiddenKeys"
      :read-only="form.readOnly"
      :saved="saved"
      :catalogs="catalogs"
      :prepare-connection="prepareConnection"
      :active-connection-id="selectedSavedId"
      :active-connection-name="form.name || form.host || 'connection'"
      :connection-error="error"
      :connection-busy="busy"
      :connected="Boolean(sessionId)"
      @select-connection="(item) => revealConnection(item, 'existing')"
      @add-connection="openEditor()"
      @edit-connection="openEditor"
      @remove-connection="removeConnection"
      @apply-filter="applyTableFilter"
      :page-size="pageSize"
      @page-size="setPageSize"
      @load-more="loadMoreTable"
      @refresh="refreshTable"
      @run="runQuery"
      @open-table="openTable"
      @update:sql="setActiveSql"
      @copy-sql="copySql"
      @save-changes="saveChanges"
      @save-structure="saveStructure"
      @select-json="onSelectJson"
      @entity-action="onEntityAction"
      @show-hidden="hiddenKeys = []"
      @activate-tab="activateTab"
      @close-tab="closeTab"
      @add-query="addQueryTab()"
      @tab-action="onTabAction"
    />
    <div v-if="editor.open" class="modal-backdrop" @click.self="editor.open = false">
      <div class="connection-modal" role="dialog" aria-modal="true">
        <div class="connection-modal-head">
          <h3>{{ editor.mode === "edit" ? "Edit connection" : "New connection" }}</h3>
          <button class="btn btn-fab" type="button" title="Close" @click="editor.open = false">
            <i class="material-icons">close</i>
          </button>
        </div>
        <ConnectionInterface
          bare
          :form="form"
          :saved="saved"
          :selected-id="editor.id"
          :error="error"
          :busy="busy"
          @connect="connectFromEditor"
          @save="saveFromEditor"
          @pick-sqlite="pickSqlite"
        />
      </div>
    </div>
    <PromptModal
      :open="prompt.open"
      :title="prompt.title"
      :message="prompt.message"
      :with-input="prompt.withInput"
      :value="prompt.value"
      :confirm-label="prompt.confirmLabel"
      @cancel="finishPrompt(null)"
      @confirm="finishPrompt"
    />
  </div>
</template>

<script setup lang="ts">
import { relaunch } from "@tauri-apps/plugin-process";
import { check } from "@tauri-apps/plugin-updater";
import { computed, onMounted, onUnmounted, reactive, ref, watch } from "vue";
import Titlebar from "./components/Titlebar.vue";
import ConnectionInterface from "./components/ConnectionInterface.vue";
import type { ConnectionForm } from "./components/ConnectionInterface.vue";
import CoreInterface from "./components/CoreInterface.vue";
import PromptModal from "./components/PromptModal.vue";
import {
  ipc,
  setSessionId,
  type SavedConnection,
  type TableChanges,
  type TableColumn,
  type TableOrView,
} from "./ipc";
import { copyText, onOpenSqlite, pickOpenFile, pickSavePath, pickSqliteFile } from "./shell";
import {
  createTableSql,
  dropSchemaSql,
  dropSql,
  duplicateSql,
  firstCell,
  qualifyTable,
  renameSchemaSql,
  renameSql,
  selectTopSql,
  structureChangeSql,
  truncateSql,
  viewCreateSql,
} from "./sql";
import type { StructureEdits } from "./sql";
import {
  createQueryTab,
  createStructureTab,
  createTableTab,
  tableTabKey,
  type WorkspaceTab,
} from "./tabs";
import { readTablePageSize, writeTablePageSize } from "./table-page";

const themes = ["dark", "light", "system"] as const;
const theme = ref<(typeof themes)[number]>("dark");
const updateVersion = ref<string | null>(null);
const updating = ref(false);
const updateError = ref<string | null>(null);
let availableUpdate: Awaited<ReturnType<typeof check>> = null;
const sessionId = ref<string | null>(null);
const sessions = new Map<string, string>();
const tablesWithKeys = new WeakSet<WorkspaceTab>();
const pageSize = ref(readTablePageSize());
const loadingMore = new Set<string>();
const tables = ref<TableOrView[]>([]);
const views = ref<TableOrView[]>([]);
const catalogs = ref<Record<string, { tables: TableOrView[]; views: TableOrView[] }>>({});
const PASSWORD_MASK = "********";
const editor = reactive({
  open: false,
  mode: "create" as "create" | "edit",
  id: null as string | null,
});
const error = ref("");
const busy = ref(false);
const saved = ref<SavedConnection[]>([]);
const selectedSavedId = ref<string | null>(null);
const jsonValue = ref<unknown>(null);
const jsonSidebarOpen = ref(false);
const hiddenKeys = ref<string[]>([]);
const tabs = ref<WorkspaceTab[]>([]);
const activeTabId = ref<string | null>(null);
const queryCount = ref(0);
const core = ref<{ buildChanges: () => TableChanges | undefined } | null>(null);
const prompt = reactive({
  open: false,
  title: "",
  message: "",
  withInput: false,
  value: "",
  confirmLabel: "OK",
  resolve: null as ((value: string | null) => void) | null,
});
const form = reactive<ConnectionForm>({
  connectionType: "postgresql",
  name: "alluoffice-local",
  host: "127.0.0.1",
  port: 5432,
  user: "postgres",
  password: "postgres",
  defaultDatabase: "allu",
  filename: "",
  ssl: false,
  readOnly: false,
  projectId: "",
  dataset: "",
  serviceAccountJson: "",
});

const windowTitle = computed(() =>
  sessionId.value ? `${form.name || form.defaultDatabase} — Minimal SQL` : "Minimal SQL",
);
const activeTab = computed(() => tabs.value.find((tab) => tab.id === activeTabId.value) ?? null);

let unlistenMenu: (() => void) | undefined;

onMounted(async () => {
  applyTheme();
  void checkUpdate();
  saved.value = await ipc.savedFind();
  unlistenMenu = await onOpenSqlite(() => {
    void pickSqlite();
  });
});

async function checkUpdate() {
  try {
    const update = await check();
    if (!update) return;
    availableUpdate = update;
    updateVersion.value = update.version;
    updateError.value = null;
  } catch {
    return;
  }
}

async function openUpdate() {
  const update = availableUpdate;
  if (!update || updating.value) return;
  updating.value = true;
  updateError.value = null;
  try {
    await update.downloadAndInstall();
    await relaunch();
  } catch (error) {
    updating.value = false;
    updateError.value = error instanceof Error ? error.message : "Update failed";
  }
}

onUnmounted(() => {
  unlistenMenu?.();
});

function applyTheme() {
  document.body.className = `theme-${theme.value}`;
}

function cycleTheme() {
  const index = themes.indexOf(theme.value);
  theme.value = themes[(index + 1) % themes.length];
  applyTheme();
}

watch(sessionId, (id) => {
  if (id) return;
  jsonSidebarOpen.value = false;
  jsonValue.value = null;
});

function onSelectJson(value: unknown) {
  jsonValue.value = value;
  if (value != null) jsonSidebarOpen.value = true;
}

function storedPassword(id: string | null) {
  if (!id) return "";
  const item = saved.value.find((entry) => entry.id === id);
  const payload = item?.payload as { password?: string } | undefined;
  return payload?.password ?? "";
}

function payloadFromForm(connectionId: string | null = editor.id) {
  const keepStored =
    editor.open &&
    editor.mode === "edit" &&
    (form.password === PASSWORD_MASK || form.password === "");
  return {
    connectionType: form.connectionType,
    name: form.name,
    host: form.host,
    port: form.port,
    user: form.user,
    password: keepStored ? storedPassword(connectionId) : form.password,
    defaultDatabase: form.defaultDatabase,
    filename: form.filename || undefined,
    ssl: form.ssl,
    readOnly: form.readOnly,
    projectId: form.projectId || undefined,
    dataset: form.dataset || undefined,
    serviceAccountJson: form.serviceAccountJson || undefined,
  };
}

function configFromSaved(item: SavedConnection) {
  const payload = item.payload as Partial<ConnectionForm>;
  return {
    connectionType: payload.connectionType ?? "postgresql",
    name: item.name,
    host: payload.host ?? "127.0.0.1",
    port: payload.port ?? 5432,
    user: payload.user ?? "",
    password: payload.password ?? "",
    defaultDatabase: payload.defaultDatabase ?? "",
    filename: payload.filename || undefined,
    ssl: Boolean(payload.ssl),
    readOnly: Boolean(payload.readOnly),
    projectId: payload.projectId || undefined,
    dataset: payload.dataset || undefined,
    serviceAccountJson: payload.serviceAccountJson || undefined,
  };
}

function blankForm() {
  Object.assign(form, {
    connectionType: "postgresql",
    name: "",
    host: "127.0.0.1",
    port: 5432,
    user: "",
    password: "",
    defaultDatabase: "",
    filename: "",
    ssl: false,
    readOnly: false,
    projectId: "",
    dataset: "",
    serviceAccountJson: "",
  });
}

function openEditor(item?: SavedConnection) {
  error.value = "";
  if (item) {
    applySaved(item);
    form.password = PASSWORD_MASK;
    editor.mode = "edit";
    editor.id = item.id;
  } else {
    blankForm();
    editor.mode = "create";
    editor.id = null;
  }
  editor.open = true;
}

async function refreshEntities(connectionId = selectedSavedId.value) {
  const nextTables = await ipc.tables();
  const nextViews = await ipc.views();
  tables.value = nextTables;
  views.value = nextViews;
  if (connectionId) {
    catalogs.value = {
      ...catalogs.value,
      [connectionId]: { tables: nextTables, views: nextViews },
    };
  }
}

async function persistEditor() {
  const id = editor.mode === "edit" && editor.id ? editor.id : crypto.randomUUID();
  const record = await ipc.savedSave({
    id,
    name: form.name || form.host || "connection",
    payload: payloadFromForm(id),
  });
  editor.id = record.id;
  editor.mode = "edit";
  for (const tab of tabs.value) {
    if (tab.connectionId === record.id) tab.connectionName = record.name;
  }
  saved.value = await ipc.savedFind();
  form.password = PASSWORD_MASK;
  return record;
}

async function saveFromEditor() {
  error.value = "";
  busy.value = true;
  try {
    await persistEditor();
  } catch (err) {
    error.value = String(err);
  } finally {
    busy.value = false;
  }
}

async function connectFromEditor() {
  error.value = "";
  try {
    const record = await persistEditor();
    editor.open = false;
    await revealConnection(record, "existing");
  } catch (err) {
    error.value = String(err);
  }
}

function applySaved(item: SavedConnection) {
  const payload = item.payload as Partial<ConnectionForm>;
  Object.assign(form, {
    connectionType: payload.connectionType ?? "postgresql",
    name: item.name,
    host: payload.host ?? "127.0.0.1",
    port: payload.port ?? 5432,
    user: payload.user ?? "",
    password: payload.password ?? "",
    defaultDatabase: payload.defaultDatabase ?? "",
    filename: payload.filename ?? "",
    ssl: Boolean(payload.ssl),
    readOnly: Boolean(payload.readOnly),
    projectId: payload.projectId ?? "",
    dataset: payload.dataset ?? "",
    serviceAccountJson: payload.serviceAccountJson ?? "",
  });
}

async function useConnection(item: SavedConnection) {
  if (!editor.open) applySaved(item);
  const cached = sessions.get(item.id);
  if (cached) {
    sessionId.value = cached;
    setSessionId(cached);
    try {
      await refreshEntities(item.id);
      return;
    } catch {
      sessions.delete(item.id);
      setSessionId(cached);
      await ipc.disconnect().catch(() => undefined);
    }
  }
  const id = await ipc.create(configFromSaved(item));
  sessions.set(item.id, id);
  sessionId.value = id;
  setSessionId(id);
  await ipc.connect();
  await refreshEntities(item.id);
}

function focusExistingTab(id: string) {
  const existing = [...tabs.value].reverse().find((tab) => tab.connectionId === id);
  activeTabId.value = existing?.id ?? null;
}

async function revealConnection(item: SavedConnection, focus: "existing" | "keep" = "keep") {
  const cached = sessions.get(item.id);
  if (cached && item.id === selectedSavedId.value && catalogs.value[item.id]) {
    sessionId.value = cached;
    setSessionId(cached);
    if (focus === "existing") focusExistingTab(item.id);
    return true;
  }
  if (busy.value) return false;
  const previousId = selectedSavedId.value;
  const previousSession = sessionId.value;
  if (previousId && previousSession) sessions.set(previousId, previousSession);
  error.value = "";
  busy.value = true;
  try {
    await useConnection(item);
    selectedSavedId.value = item.id;
    if (focus === "existing") focusExistingTab(item.id);
    return true;
  } catch (err) {
    error.value = String(err);
    sessionId.value = previousSession;
    setSessionId(previousSession);
    selectedSavedId.value = previousId;
    const previous = saved.value.find((entry) => entry.id === previousId);
    if (previous && !editor.open) applySaved(previous);
    return false;
  } finally {
    busy.value = false;
  }
}

async function switchConnection(item: SavedConnection) {
  return revealConnection(item, "keep");
}

async function prepareConnection(id: string) {
  const item = saved.value.find((entry) => entry.id === id);
  if (!item) return;
  const cached = sessions.get(id);
  if (cached && selectedSavedId.value === id) {
    sessionId.value = cached;
    setSessionId(cached);
    return;
  }
  await revealConnection(item, "keep");
}

async function removeConnection(item: SavedConnection) {
  const ok = await ask({
    title: "Remove connection",
    message: `Remove ${item.name}?`,
    confirmLabel: "Remove",
  });
  if (ok == null) return;
  await ipc.savedRemove(item.id);
  const session = sessions.get(item.id);
  if (session) {
    const current = sessionId.value;
    setSessionId(session);
    await ipc.disconnect().catch(() => undefined);
    sessions.delete(item.id);
    if (current && current !== session) {
      sessionId.value = current;
      setSessionId(current);
    } else {
      sessionId.value = null;
      setSessionId(null);
    }
  }
  const nextCatalogs = { ...catalogs.value };
  delete nextCatalogs[item.id];
  catalogs.value = nextCatalogs;
  const removed = new Set(tabs.value.filter((tab) => tab.connectionId === item.id).map((tab) => tab.id));
  tabs.value = tabs.value.filter((tab) => tab.connectionId !== item.id);
  if (activeTabId.value && removed.has(activeTabId.value)) {
    activeTabId.value = tabs.value[0]?.id ?? null;
  }
  if (selectedSavedId.value === item.id) {
    selectedSavedId.value = null;
    sessionId.value = null;
    tables.value = [];
    views.value = [];
    const next = tabs.value.find((tab) => tab.connectionId && sessions.has(tab.connectionId));
    if (next?.connectionId) {
      const replacement = saved.value.find((entry) => entry.id === next.connectionId);
      if (replacement) await revealConnection(replacement, "keep");
      activeTabId.value = next.id;
    }
  }
  saved.value = await ipc.savedFind();
}

async function pickSqlite() {
  const path = await pickSqliteFile();
  if (typeof path === "string") {
    form.connectionType = "sqlite";
    form.filename = path;
    form.name = path.split("/").pop() ?? "sqlite";
  }
}

function quoteIdent(name: string) {
  if (form.connectionType === "mysql" || form.connectionType === "mariadb") {
    return `\`${name.replace(/`/g, "``")}\``;
  }
  if (form.connectionType === "sqlserver") {
    return `[${name.replace(/]/g, "]]")}]`;
  }
  return `"${name.replace(/"/g, '""')}"`;
}

function ask(options: {
  title: string;
  message?: string;
  withInput?: boolean;
  value?: string;
  confirmLabel?: string;
}): Promise<string | null> {
  return new Promise((resolve) => {
    prompt.open = true;
    prompt.title = options.title;
    prompt.message = options.message ?? "";
    prompt.withInput = Boolean(options.withInput);
    prompt.value = options.value ?? "";
    prompt.confirmLabel = options.confirmLabel ?? "OK";
    prompt.resolve = resolve;
  });
}

function finishPrompt(value: string | null) {
  prompt.open = false;
  const resolve = prompt.resolve;
  prompt.resolve = null;
  resolve?.(value);
}

function adopt(tab: WorkspaceTab, afterId?: string) {
  if (!tabs.value.some((item) => item.id === tab.id)) {
    const index = afterId ? tabs.value.findIndex((item) => item.id === afterId) : -1;
    if (index >= 0) tabs.value.splice(index + 1, 0, tab);
    else tabs.value.push(tab);
  }
  return tabs.value.find((item) => item.id === tab.id) ?? tab;
}

function connectionContext() {
  return {
    id: selectedSavedId.value,
    name: form.name || form.host || "connection",
  };
}

function addQueryTab(sql = "") {
  queryCount.value += 1;
  const tab = adopt(createQueryTab(`Query #${queryCount.value}`, sql, connectionContext()));
  activeTabId.value = tab.id;
  return tab;
}

function focusOrCreateQuery(sql?: string) {
  const current = activeTab.value;
  if (current?.kind === "query") {
    if (sql != null) current.sql = sql;
    return current;
  }
  return addQueryTab(sql ?? "");
}

function closeTab(id: string) {
  const index = tabs.value.findIndex((tab) => tab.id === id);
  if (index < 0) return;
  const closing = tabs.value[index];
  tabs.value.splice(index, 1);
  if (activeTabId.value !== id) return;
  const next = tabs.value[index] ?? tabs.value[index - 1] ?? null;
  if (next?.connectionId && next.connectionId !== closing.connectionId) {
    const item = saved.value.find((entry) => entry.id === next.connectionId);
    if (item) {
      void switchConnection(item).then(() => {
        if (selectedSavedId.value === item.id) activeTabId.value = next.id;
      });
      return;
    }
  }
  activeTabId.value = next?.id ?? null;
}

function closeTabs(ids: Set<string>, nextId: string | null) {
  if (!ids.size) return;
  const closingActive = activeTabId.value != null && ids.has(activeTabId.value);
  tabs.value = tabs.value.filter((tab) => !ids.has(tab.id));
  if (!closingActive) return;
  if (nextId && tabs.value.some((tab) => tab.id === nextId)) {
    void activateTab(nextId);
    return;
  }
  const fallback = tabs.value[0];
  if (fallback) void activateTab(fallback.id);
  else activeTabId.value = null;
}

function onTabAction(payload: { slug: string; id: string }) {
  if (payload.slug === "close-others") {
    closeTabs(new Set(tabs.value.filter((tab) => tab.id !== payload.id).map((tab) => tab.id)), payload.id);
    return;
  }
  if (payload.slug === "close-all") {
    tabs.value = [];
    activeTabId.value = null;
    return;
  }
  if (payload.slug === "close-right") {
    const index = tabs.value.findIndex((tab) => tab.id === payload.id);
    if (index < 0) return;
    const ids = new Set(tabs.value.slice(index + 1).map((tab) => tab.id));
    const next = activeTabId.value && ids.has(activeTabId.value) ? payload.id : null;
    closeTabs(ids, next);
    return;
  }
  if (payload.slug === "duplicate") {
    void duplicateTab(payload.id);
    return;
  }
  if (payload.slug === "copy-name") void copyTabName(payload.id);
}

async function duplicateTab(id: string) {
  const source = tabs.value.find((tab) => tab.id === id);
  if (!source) return;
  if (source.connectionId && source.connectionId !== selectedSavedId.value) {
    const item = saved.value.find((entry) => entry.id === source.connectionId);
    if (!item) return;
    await switchConnection(item);
    if (selectedSavedId.value !== item.id) return;
  }
  const connection = { id: source.connectionId, name: source.connectionName };
  if (source.kind === "query") {
    queryCount.value += 1;
    const copy = adopt(createQueryTab(`Query #${queryCount.value}`, source.sql, connection), source.id);
    activeTabId.value = copy.id;
    return;
  }
  if (!source.table) return;
  if (source.kind === "table") {
    const copy = adopt(createTableTab(source.table, connection), source.id);
    activeTabId.value = copy.id;
    await loadTableData(copy);
    return;
  }
  const copy = adopt(createStructureTab(source.table, connection), source.id);
  activeTabId.value = copy.id;
  await loadStructure(copy);
}

async function copyTabName(id: string) {
  const tab = tabs.value.find((item) => item.id === id);
  if (!tab || tab.kind === "query") return;
  await copyText(tab.table?.name || tab.title);
}

async function activateTab(id: string) {
  const tab = tabs.value.find((item) => item.id === id);
  if (!tab) return;
  if (tab.connectionId && tab.connectionId !== selectedSavedId.value) {
    const item = saved.value.find((entry) => entry.id === tab.connectionId);
    if (item) {
      await switchConnection(item);
      if (selectedSavedId.value === item.id) activeTabId.value = id;
      return;
    }
  }
  activeTabId.value = id;
}

function setActiveSql(value: string) {
  if (activeTab.value?.kind === "query") activeTab.value.sql = value;
}

function findTableTab(table: TableOrView, kind: WorkspaceTab["kind"]) {
  const key = tableTabKey(table);
  return tabs.value.find(
    (tab) =>
      tab.kind === kind &&
      tab.connectionId === selectedSavedId.value &&
      tab.table &&
      tableTabKey(tab.table) === key,
  );
}

async function openTable(table: TableOrView) {
  const existing = findTableTab(table, "table");
  if (existing) {
    activeTabId.value = existing.id;
    await loadTableData(existing);
    return;
  }
  const tab = adopt(createTableTab(table, connectionContext()));
  activeTabId.value = tab.id;
  await loadTableData(tab);
}

function filterValue(filter: { op: string; value: string }) {
  if (filter.op === "in" || filter.op === "not in") {
    return filter.value
      .split(",")
      .map((item) => item.trim())
      .filter(Boolean);
  }
  return filter.value;
}

function setPageSize(value: number) {
  pageSize.value = writeTablePageSize(value);
  const tab = activeTab.value;
  if (tab?.kind === "table") void loadTableData(tab);
}

function tableFilters(tab: WorkspaceTab) {
  return (tab.tableFilter ?? []).map((filter) => ({
    field: filter.field,
    op: filter.op,
    value: filterValue(filter),
    join: filter.join === "or" ? "or" : "and",
  }));
}

function tableOrder(tab: WorkspaceTab) {
  return tab.primaryKeys.map((field) => ({ field, dir: "asc" }));
}

async function fetchTablePage(tab: WorkspaceTab, offset: number, skipCount: boolean) {
  if (!tab.table) throw new Error("missing table");
  return ipc.selectTop({
    table: tab.table.name,
    schema: tab.table.schema ?? null,
    offset,
    limit: pageSize.value,
    orderBy: tableOrder(tab),
    filters: tableFilters(tab),
    selects: null,
    skipCount,
  });
}

async function applyTableFilter(tabId: string, filters: { field: string; op: string; value: string; join?: "and" | "or" }[]) {
  const tab = tabs.value.find((item) => item.id === tabId);
  if (!tab || tab.kind !== "table") return;
  const active = filters.filter(
    (filter) => filter.field && (filter.op === "is null" || filter.op === "is not null" || filter.value.trim()),
  );
  tab.tableFilter = active.length ? active.map((filter) => ({ ...filter, join: filter.join === "or" ? "or" : "and" })) : null;
  await loadTableData(tab);
}

function ensurePageSize() {
  const stored = readTablePageSize();
  if (pageSize.value !== stored && !localStorage.getItem("minimal-sql.tablePageSize")) {
    pageSize.value = stored;
  }
}

async function loadTableData(tab: WorkspaceTab) {
  if (!tab.table) return;
  ensurePageSize();
  const gen = tab.loadGen + 1;
  tab.loadGen = gen;
  if (tab.connectionId) await prepareConnection(tab.connectionId);
  if (tab.loadGen !== gen) return;
  tab.busy = true;
  tab.error = "";
  tab.sql = selectTopSql(quoteIdent, tab.table, pageSize.value);
  try {
    if (!tablesWithKeys.has(tab)) {
      tab.primaryKeys = await ipc.primaryKeys(tab.table.name, tab.table.schema).catch(() => []);
      tablesWithKeys.add(tab);
    }
    const page = await fetchTablePage(tab, 0, false);
    if (tab.loadGen !== gen) return;
    tab.resultMode = "replace";
    tab.result = page;
  } catch (err) {
    if (tab.loadGen !== gen) return;
    tab.error = String(err);
    tab.resultMode = "replace";
    tab.result = { fields: [], rows: [], rowCount: 0, totalRowCount: 0 };
  } finally {
    if (tab.loadGen === gen) tab.busy = false;
  }
}

function refreshTable(tabId: string) {
  const tab = tabs.value.find((item) => item.id === tabId);
  if (!tab || tab.kind !== "table" || tab.busy) return;
  void loadTableData(tab);
}

async function loadMoreTable(tabId: string) {
  const tab = tabs.value.find((item) => item.id === tabId);
  if (!tab || tab.kind !== "table" || !tab.table || tab.busy || loadingMore.has(tab.id)) return;
  const loaded = tab.result.rows?.length ?? 0;
  const total = tab.result.totalRowCount ?? tab.result.rowCount ?? loaded;
  if (loaded >= total) return;
  const gen = tab.loadGen;
  loadingMore.add(tab.id);
  tab.busy = true;
  try {
    const page = await fetchTablePage(tab, loaded, true);
    if (tab.loadGen !== gen) return;
    const extra = page.rows ?? [];
    if (!extra.length) {
      tab.result = { ...tab.result, rowCount: loaded, totalRowCount: loaded };
      return;
    }
    const rows = [...(tab.result.rows ?? []), ...extra];
    tab.resultMode = "append";
    tab.result = {
      ...page,
      rows,
      rowCount: total,
      totalRowCount: total,
    };
  } catch (err) {
    if (tab.loadGen !== gen) return;
    tab.error = String(err);
  } finally {
    loadingMore.delete(tab.id);
    if (tab.loadGen === gen) tab.busy = false;
  }
}

async function loadStructure(tab: WorkspaceTab) {
  if (!tab.table) return;
  const table = tab.table;
  tab.busy = true;
  tab.error = "";
  try {
    const [columns, indexes, triggers] = await Promise.all([
      ipc.columns(table.name, table.schema ?? undefined),
      ipc.indexes(table.name, table.schema ?? undefined).catch(() => []),
      ipc.triggers(table.name, table.schema ?? undefined).catch(() => []),
    ]);
    tab.structure = {
      title: qualifyTable(quoteIdent, table),
      columns,
      indexes,
      triggers,
    };
  } catch (err) {
    tab.error = String(err);
  } finally {
    tab.busy = false;
  }
}

async function viewStructure(table: TableOrView) {
  const existing = findTableTab(table, "structure");
  const tab = adopt(existing ?? createStructureTab(table, connectionContext()));
  activeTabId.value = tab.id;
  await loadStructure(tab);
}

async function saveStructure(payload: { table: TableOrView; edits: StructureEdits }) {
  const tab = findTableTab(payload.table, "structure");
  const statements = structureChangeSql(form.connectionType, quoteIdent, payload.table, payload.edits);
  if (!statements.length) {
    if (tab) tab.error = "This database cannot apply these structure changes.";
    return;
  }
  const ok = await ask({
    title: "Save structure",
    message: statements.join("\n"),
    confirmLabel: "Save",
  });
  if (ok == null) return;
  if (tab) {
    tab.busy = true;
    tab.error = "";
  }
  try {
    for (const sql of statements) {
      await ipc.query(sql);
    }
    await viewStructure(payload.table);
    await refreshEntities();
  } catch (err) {
    if (tab) tab.error = String(err);
  } finally {
    if (tab) tab.busy = false;
  }
}

async function runSql(text: string) {
  const tab = focusOrCreateQuery(text);
  activeTabId.value = tab.id;
  await runQuery();
}

async function onEntityAction(payload: {
  slug: string;
  table?: TableOrView;
  schema?: string;
  column?: TableColumn;
}) {
  const table = payload.table;
  try {
    if (payload.slug === "copy-column" && payload.column) {
      await copyText(payload.column.columnName);
      return;
    }
    if (payload.slug === "hide-schema" && payload.schema) {
      hiddenKeys.value = [...hiddenKeys.value, `schema:${payload.schema}`];
      return;
    }
    if (payload.slug === "rename-schema" && payload.schema) {
      const next = await ask({
        title: "Rename schema",
        withInput: true,
        value: payload.schema,
        confirmLabel: "Rename",
      });
      if (!next || next === payload.schema) return;
      await runSql(renameSchemaSql(quoteIdent, payload.schema, next));
      await refreshEntities();
      return;
    }
    if (payload.slug === "drop-schema" && payload.schema) {
      const ok = await ask({
        title: "Drop schema",
        message: `Drop schema ${payload.schema}? This cannot be undone.`,
        confirmLabel: "Drop",
      });
      if (ok == null) return;
      await runSql(dropSchemaSql(quoteIdent, payload.schema));
      await refreshEntities();
      return;
    }
    if (!table) return;
    if (payload.slug === "view-data") {
      await openTable(table);
      return;
    }
    if (payload.slug === "view-structure") {
      await viewStructure(table);
      return;
    }
    if (payload.slug === "export") {
      await exportTable(table);
      return;
    }
    if (payload.slug === "import") {
      await importIntoTable(table);
      return;
    }
    if (payload.slug === "copy-name") {
      await copyText(table.name);
      return;
    }
    if (payload.slug === "hide-entity") {
      hiddenKeys.value = [...hiddenKeys.value, tableTabKey(table)];
      return;
    }
    if (payload.slug === "sql-create") {
      await loadCreateSql(table);
      return;
    }
    if (payload.slug === "select-top") {
      focusOrCreateQuery(selectTopSql(quoteIdent, table));
      return;
    }
    if (payload.slug === "rename") {
      const next = await ask({
        title: "Rename",
        withInput: true,
        value: table.name,
        confirmLabel: "Rename",
      });
      if (!next || next === table.name) return;
      await runSql(renameSql(form.connectionType, quoteIdent, table, next));
      await refreshEntities();
      return;
    }
    if (payload.slug === "sql-drop") {
      const ok = await ask({
        title: "Drop",
        message: `Drop ${table.entityType} ${table.name}? This cannot be undone.`,
        confirmLabel: "Drop",
      });
      if (ok == null) return;
      await runSql(dropSql(quoteIdent, table));
      await refreshEntities();
      return;
    }
    if (payload.slug === "sql-truncate") {
      const ok = await ask({
        title: "Truncate",
        message: `Truncate table ${table.name}? This deletes all rows.`,
        confirmLabel: "Truncate",
      });
      if (ok == null) return;
      await runSql(truncateSql(form.connectionType, quoteIdent, table));
      return;
    }
    if (payload.slug === "sql-duplicate") {
      const next = await ask({
        title: "Duplicate",
        withInput: true,
        value: `${table.name}_copy`,
        confirmLabel: "Duplicate",
      });
      if (!next) return;
      await runSql(duplicateSql(quoteIdent, table, next));
      await refreshEntities();
    }
  } catch (err) {
    error.value = String(err);
  }
}

async function loadCreateSql(table: TableOrView) {
  const tab = focusOrCreateQuery();
  if (table.entityType === "view" || table.entityType === "materialized-view") {
    const lookup = viewCreateSql(form.connectionType, quoteIdent, table);
    const results = await ipc.query(lookup);
    tab.sql = firstCell(results[0]) || `-- no create script for ${table.name}`;
    tab.result = { fields: [], rows: [], rowCount: 0 };
    return;
  }
  const columns = await ipc.columns(table.name, table.schema ?? undefined);
  tab.sql = createTableSql(quoteIdent, table, columns);
  tab.result = { fields: [], rows: [], rowCount: 0 };
}

async function exportTable(table: TableOrView) {
  const path = await pickSavePath(`${table.name}.csv`);
  if (typeof path !== "string") return;
  const format = path.endsWith(".json") ? "json" : "csv";
  try {
    await ipc.exportQuery(`SELECT * FROM ${qualifyTable(quoteIdent, table)}`, path, format);
  } catch (err) {
    markError(String(err));
  }
}

async function importIntoTable(table: TableOrView) {
  const path = await pickOpenFile();
  if (typeof path !== "string") return;
  const format = path.endsWith(".json") ? "json" : path.endsWith(".xlsx") ? "xlsx" : "csv";
  try {
    const imported = await ipc.importFile(path, format);
    const inserts = imported.rows.map((row) => ({
      table: table.name,
      schema: table.schema ?? null,
      primaryKeys: [] as [string, unknown][],
      values: imported.columns.map((column, index) => [column, row[index]] as [string, unknown]),
    }));
    const chunk = 80;
    for (let i = 0; i < inserts.length; i += chunk) {
      await ipc.applyChanges({
        inserts: inserts.slice(i, i + chunk),
        updates: [],
        deletes: [],
      });
    }
    await openTable(table);
  } catch (err) {
    markError(String(err));
  }
}

function markError(message: string) {
  error.value = message;
  if (activeTab.value) activeTab.value.error = message;
}

async function saveChanges() {
  const tab = activeTab.value;
  if (!tab || tab.kind !== "table") return;
  const changes = core.value?.buildChanges();
  const count = (changes?.inserts.length ?? 0) + (changes?.updates.length ?? 0) + (changes?.deletes.length ?? 0);
  if (!changes || !count) return;
  tab.busy = true;
  tab.error = "";
  try {
    await ipc.applyChanges(changes);
    await loadTableData(tab);
  } catch (err) {
    tab.error = String(err);
  } finally {
    tab.busy = false;
  }
}

async function copySql() {
  const tab = activeTab.value;
  if (tab?.kind === "query") await copyText(tab.sql);
}

async function runQuery() {
  const tab = activeTab.value;
  if (!sessionId.value || !tab || tab.kind !== "query") return;
  tab.error = "";
  tab.busy = true;
  try {
    const results = await ipc.query(tab.sql);
    tab.result = results[0] ?? { fields: [], rows: [], rowCount: 0 };
  } catch (err) {
    tab.error = String(err);
    tab.result = { fields: [], rows: [], rowCount: 0 };
  } finally {
    tab.busy = false;
  }
}
</script>
