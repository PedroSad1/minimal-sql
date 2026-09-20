<template>
  <div class="graphite-shell">
    <header class="titlebar">
      <strong>GRAPHITE</strong>
      <div class="row">
        <button @click="cycleTheme">tema: {{ theme }}</button>
        <button v-if="sessionId" @click="disconnect">desconectar</button>
      </div>
    </header>

    <section v-if="!sessionId" class="connection">
      <h2>Nova conexão</h2>
      <p>Community databases. Sem license key. Sem upsell.</p>
      <label>tipo</label>
      <select v-model="form.connectionType">
        <option v-for="item in types" :key="item.value" :value="item.value">
          {{ item.label }}
        </option>
      </select>
      <template v-if="form.connectionType === 'sqlite'">
        <label>arquivo</label>
        <input v-model="form.filename" placeholder="/tmp/app.db" />
      </template>
      <template v-else-if="form.connectionType === 'bigquery'">
        <label>project id</label>
        <input v-model="form.projectId" />
        <label>dataset</label>
        <input v-model="form.dataset" />
        <label>service account json</label>
        <input v-model="form.serviceAccountJson" />
      </template>
      <template v-else>
        <label>host</label>
        <input v-model="form.host" />
        <label>port</label>
        <input v-model.number="form.port" type="number" />
        <label>user</label>
        <input v-model="form.user" />
        <label>password</label>
        <input v-model="form.password" type="password" />
        <label>database</label>
        <input v-model="form.defaultDatabase" />
      </template>
      <label>nome salvo</label>
      <input v-model="form.name" />
      <div class="row">
        <button class="primary" @click="connect">conectar</button>
        <button @click="save">salvar</button>
      </div>
      <p v-if="error" class="error">{{ error }}</p>
      <div class="saved" v-if="saved.length">
        <h3>salvas</h3>
        <div v-for="item in saved" :key="item.id" class="entity" @click="loadSaved(item)">
          {{ item.name }}
        </div>
      </div>
    </section>

    <div v-else class="layout">
      <aside class="sidebar">
        <p class="ok">{{ version }}</p>
        <h3>tabelas</h3>
        <div
          v-for="table in tables"
          :key="table.name"
          class="entity"
          @click="openTable(table)"
        >
          {{ table.schema ? table.schema + "." : "" }}{{ table.name }}
        </div>
        <h3>views</h3>
        <div v-for="view in views" :key="view.name" class="entity" @click="openTable(view)">
          {{ view.name }}
        </div>
      </aside>
      <main class="main">
        <div style="padding: 8px">
          <div v-for="(filter, index) in filters" :key="index" class="filter-row">
            <input v-model="filter.field" placeholder="campo" />
            <select v-model="filter.op">
              <option value="=">=</option>
              <option value="!=">!=</option>
              <option value=">">&gt;</option>
              <option value="<">&lt;</option>
              <option value="like">like</option>
              <option value="in">in</option>
            </select>
            <input v-model="filter.value" placeholder="valor" />
            <button @click="filters.splice(index, 1)">x</button>
          </div>
          <div class="row">
            <button @click="filters.push({ field: '', op: '=', value: '' })">+ filtro</button>
            <button @click="runSelect">aplicar filtros</button>
            <button @click="runQuery">executar sql</button>
            <button @click="exportCsv">export csv</button>
            <button @click="exportJson">export json</button>
            <button @click="importCsv">import csv</button>
            <button @click="saveEdits">salvar edições</button>
            <button @click="backup">backup tabela</button>
          </div>
          <textarea v-model="sql" spellcheck="false" />
        </div>
        <div class="table-wrap">
          <table>
            <thead>
              <tr>
                <th v-for="col in result.columns" :key="col">{{ col }}</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="(row, r) in result.rows" :key="r" @click="selectedRow = row">
                <td
                  v-for="(cell, c) in row"
                  :key="c"
                  contenteditable
                  @blur="onEdit(r, c, $event)"
                >
                  {{ display(cell) }}
                </td>
              </tr>
            </tbody>
          </table>
        </div>
        <footer class="statusbar">
          <span>{{ result.rowCount || result.total || 0 }} linhas</span>
          <span v-if="error" class="error">{{ error }}</span>
        </footer>
      </main>
      <aside class="json-pane">
        <h3>JSON</h3>
        <pre>{{ jsonText }}</pre>
      </aside>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import {
  COMMUNITY_TYPES,
  ipc,
  type QueryResult,
  type SavedConnection,
  type TableOrView,
  type TableResult,
} from "./ipc";

const types = COMMUNITY_TYPES;
const theme = ref("dark");
const sessionId = ref<string | null>(null);
const version = ref("");
const tables = ref<TableOrView[]>([]);
const views = ref<TableOrView[]>([]);
const sql = ref("select 1");
const error = ref("");
const saved = ref<SavedConnection[]>([]);
const filters = ref<{ field: string; op: string; value: string }[]>([]);
const activeTable = ref<TableOrView | null>(null);
const selectedRow = ref<unknown[] | null>(null);
const edits = ref<{ row: number; column: number; value: string }[]>([]);
const result = reactive<QueryResult & TableResult>({
  columns: [],
  rows: [],
  rowCount: 0,
  truncated: false,
  total: 0,
});

const form = reactive({
  connectionType: "sqlite",
  filename: "",
  host: "127.0.0.1",
  port: 5432,
  user: "",
  password: "",
  defaultDatabase: "",
  name: "local",
  projectId: "",
  dataset: "",
  serviceAccountJson: "",
});

const jsonText = computed(() => {
  if (!selectedRow.value || !result.columns.length) return "{}";
  const obj: Record<string, unknown> = {};
  result.columns.forEach((col, i) => {
    obj[col] = selectedRow.value?.[i];
  });
  return JSON.stringify(obj, null, 2);
});

function display(cell: unknown) {
  if (cell === null || cell === undefined) return "";
  if (typeof cell === "object") return JSON.stringify(cell);
  return String(cell);
}

function applyTheme(value: string) {
  theme.value = value;
  document.body.className = `theme-${value}`;
}

function cycleTheme() {
  const next = theme.value === "dark" ? "light" : theme.value === "light" ? "system" : "dark";
  applyTheme(next);
  void ipc.setSetting("theme", next);
}

async function connect() {
  error.value = "";
  try {
    const id = await ipc.create({ ...form });
    sessionId.value = id;
    version.value = await ipc.version(id);
    tables.value = await ipc.tables(id);
    views.value = await ipc.views(id);
  } catch (err) {
    error.value = String(err);
  }
}

async function disconnect() {
  if (sessionId.value) await ipc.disconnect(sessionId.value);
  sessionId.value = null;
}

async function save() {
  await ipc.savedSave({
    id: crypto.randomUUID(),
    name: form.name || form.connectionType,
    payload: { ...form },
  });
  saved.value = await ipc.savedFind();
}

function loadSaved(item: SavedConnection) {
  Object.assign(form, item.payload as object);
}

async function openTable(table: TableOrView) {
  activeTable.value = table;
  sql.value = `select * from ${table.schema ? table.schema + "." : ""}${table.name} limit 100`;
  await runSelect();
}

async function runQuery() {
  if (!sessionId.value) return;
  error.value = "";
  try {
    const out = await ipc.query(sessionId.value, sql.value);
    const first = out[0];
    if (first) Object.assign(result, first, { total: first.rowCount });
  } catch (err) {
    error.value = String(err);
  }
}

async function runSelect() {
  if (!sessionId.value || !activeTable.value) {
    await runQuery();
    return;
  }
  error.value = "";
  try {
    const out = await ipc.selectTop(sessionId.value, {
      table: activeTable.value.name,
      schema: activeTable.value.schema,
      offset: 0,
      limit: 200,
      orderBy: [],
      filters: filters.value
        .filter((f) => f.field)
        .map((f) => ({
          field: f.field,
          op: f.op,
          value: f.op === "in" ? f.value.split(",").map((s) => s.trim()) : f.value,
        })),
    });
    Object.assign(result, out, { rowCount: out.rows.length });
  } catch (err) {
    error.value = String(err);
  }
}

function onEdit(row: number, column: number, event: Event) {
  const value = (event.target as HTMLElement).innerText;
  edits.value.push({ row, column, value });
}

async function saveEdits() {
  if (!sessionId.value || !activeTable.value || !edits.value.length) return;
  const pk = result.columns[0];
  const updates = edits.value.map((edit) => ({
    table: activeTable.value!.name,
    schema: activeTable.value!.schema,
    primaryKeys: [[pk, result.rows[edit.row][0]]],
    values: [[result.columns[edit.column], edit.value]],
  }));
  await ipc.applyChanges(sessionId.value, { inserts: [], updates, deletes: [] });
  edits.value = [];
  await runSelect();
}

async function exportCsv() {
  await ipc.exportResult(result, `/tmp/graphite-export.csv`, "csv");
}

async function exportJson() {
  await ipc.exportResult(result, `/tmp/graphite-export.json`, "json");
}

async function importCsv() {
  const table = await ipc.importFile("/tmp/graphite-import.csv", "csv");
  result.columns = table.columns;
  result.rows = table.rows;
  result.rowCount = table.rows.length;
}

async function backup() {
  if (!sessionId.value || !activeTable.value) return;
  await ipc.backup(sessionId.value, activeTable.value.name, `/tmp/graphite-${activeTable.value.name}.sql`);
}

onMounted(async () => {
  const stored = await ipc.getSetting("theme").catch(() => null);
  applyTheme(stored || "dark");
  saved.value = await ipc.savedFind().catch(() => []);
});
</script>
