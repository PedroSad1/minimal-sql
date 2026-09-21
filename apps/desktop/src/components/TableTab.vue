<template>
  <div class="table-tab">
    <form class="table-filter" @submit.prevent="applyFilter">
      <div v-for="(row, index) in drafts" :key="index" class="table-filter-row">
        <button
          v-if="index > 0"
          class="table-filter-and"
          type="button"
          :title="row.join === 'or' ? 'Usar AND' : 'Usar OR'"
          @click="toggleJoin(index)"
        >
          {{ row.join === "or" ? "OR" : "AND" }}
        </button>
        <select v-model="row.field" class="form-control" aria-label="Campo">
          <option v-for="field in fields" :key="field" :value="field">{{ field }}</option>
        </select>
        <select v-model="row.op" class="form-control" aria-label="Cláusula">
          <option v-for="item in operators" :key="item.value" :value="item.value">{{ item.label }}</option>
        </select>
        <input
          v-model="row.value"
          class="form-control"
          :disabled="isNullOp(row.op)"
          :placeholder="row.op === 'in' || row.op === 'not in' ? 'a, b, c' : 'Valor'"
          aria-label="Valor"
          @keydown.enter.prevent="applyFilter"
        />
        <button
          v-if="drafts.length > 1"
          class="btn btn-fab"
          type="button"
          title="Remover condição"
          @click="removeRow(index)"
        >
          <i class="material-icons">close</i>
        </button>
        <button
          v-if="index === drafts.length - 1 && fields.length > 0"
          class="btn btn-fab"
          type="button"
          title="AND"
          @click="addRow"
        >
          <i class="material-icons">add</i>
        </button>
        <button
          v-if="index === drafts.length - 1"
          class="btn btn-primary btn-small"
          type="submit"
          :disabled="busy"
        >
          Filter
        </button>
      </div>
    </form>
    <p v-if="error" class="error-alert">{{ error }}</p>
    <ResultGrid
      ref="grid"
      :active="active"
      :result="result"
      :sync-mode="syncMode"
      :can-load-more="canLoadMore"
      :editable="editable"
      :table-name="tableName"
      :schema="schema"
      :primary-keys="primaryKeys"
      @select-json="$emit('select-json', $event)"
      @pending-count="$emit('pending-count', $event)"
      @near-end="$emit('load-more')"
    />
    <div v-if="loadingMore" class="load-more-bar" role="status">
      <i class="load-spin" aria-hidden="true"></i>
      <span>Carregando a próxima página</span>
    </div>
    <div class="statusbar">
      <span class="status-text">{{ status }}</span>
      <label class="page-size">
        <input
          :value="pageSize"
          type="number"
          min="1"
          max="1000"
          aria-label="Itens por página"
          @change="commitPageSize"
          @keydown.enter.prevent="commitPageSize"
        />
        <span>por página</span>
      </label>
      <span v-if="!editable" class="hint">sem PK: edição desligada</span>
      <div class="statusbar-actions">
        <button
          class="btn btn-flat btn-fab"
          type="button"
          title="Atualizar tabela (F5)"
          :disabled="busy"
          @click="$emit('refresh')"
        >
          <i class="material-icons">refresh</i>
        </button>
        <button class="btn btn-flat btn-small" type="button" :disabled="pendingCount === 0" @click="discard">
          Reset
        </button>
        <button
          class="btn btn-primary btn-small btn-badge"
          type="button"
          :disabled="busy || pendingCount === 0"
          @click="$emit('save-changes')"
        >
          <span v-if="pendingCount > 0" class="badge">{{ pendingCount }}</span>
          Apply
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import type { NgQueryResult, TableChanges } from "../ipc";
import type { TableFilterDraft } from "../tabs";
import ResultGrid from "./ResultGrid.vue";

const operators = [
  { value: "=", label: "EQUALS" },
  { value: "!=", label: "NOT" },
  { value: "in", label: "IN" },
  { value: "not in", label: "NOT IN" },
  { value: "like", label: "LIKE" },
  { value: ">", label: "GREATER THAN" },
  { value: ">=", label: "GREATER OR EQUAL" },
  { value: "<", label: "LESS THAN" },
  { value: "<=", label: "LESS OR EQUAL" },
  { value: "is null", label: "IS NULL" },
  { value: "is not null", label: "IS NOT NULL" },
];

const props = withDefaults(
  defineProps<{
    result: NgQueryResult;
    error: string;
    busy: boolean;
    tableName: string;
    schema: string | null;
    primaryKeys: string[];
    pendingCount: number;
    active?: boolean;
    filters?: TableFilterDraft[] | null;
    pageSize?: number;
    syncMode?: "replace" | "append";
  }>(),
  { active: true, filters: null, pageSize: 100, syncMode: "replace" },
);

const emit = defineEmits<{
  (event: "save-changes"): void;
  (event: "select-json", value: unknown): void;
  (event: "pending-count", count: number): void;
  (event: "apply-filter", filters: TableFilterDraft[]): void;
  (event: "page-size", value: number): void;
  (event: "load-more"): void;
  (event: "refresh"): void;
}>();

const grid = ref<{ buildChanges: () => TableChanges; discard: () => void } | null>(null);
const drafts = ref<TableFilterDraft[]>([{ field: "", op: "=", value: "", join: "and" }]);
const fields = computed(() => (props.result.fields ?? []).map((field) => field.name).filter(Boolean));

watch(
  fields,
  (names) => {
    for (const row of drafts.value) {
      if (!row.field && names.length) row.field = names[0];
    }
  },
  { immediate: true },
);

watch(
  () => props.filters,
  (value) => {
    drafts.value = value?.length
      ? value.map((row) => ({ ...row, join: row.join === "or" ? "or" : "and" }))
      : [{ field: fields.value[0] ?? "", op: "=", value: "", join: "and" }];
  },
);

function isNullOp(op: string) {
  return op === "is null" || op === "is not null";
}

function toggleJoin(index: number) {
  const row = drafts.value[index];
  if (!row) return;
  row.join = row.join === "or" ? "and" : "or";
}

function addRow() {
  drafts.value.push({ field: fields.value[0] ?? "", op: "=", value: "", join: "and" });
}

function removeRow(index: number) {
  drafts.value.splice(index, 1);
}

function applyFilter() {
  emit(
    "apply-filter",
    drafts.value.map((row) => ({ ...row })),
  );
}
const editable = computed(() => props.primaryKeys.length > 0);
const loadedCount = computed(() => props.result.rows?.length ?? 0);
const totalCount = computed(() => props.result.totalRowCount ?? props.result.rowCount ?? loadedCount.value);
const canLoadMore = computed(() => !props.busy && loadedCount.value < totalCount.value);
const loadingMore = computed(() => props.busy && loadedCount.value > 0);
const status = computed(() => {
  if (props.busy && loadedCount.value === 0) return "running";
  if (props.error) return "error";
  if (props.pendingCount) return `${props.pendingCount} alterações pendentes`;
  if (loadedCount.value < totalCount.value) return `${loadedCount.value} de ${totalCount.value} linhas`;
  return `${totalCount.value} linhas`;
});

function commitPageSize(event: Event) {
  const input = event.target;
  if (!(input instanceof HTMLInputElement)) return;
  const value = Number(input.value);
  if (!Number.isFinite(value) || value < 1) {
    input.value = String(props.pageSize);
    return;
  }
  emit("page-size", value);
}

function discard() {
  grid.value?.discard();
}

function typingTarget(target: EventTarget | null) {
  return target instanceof Element && Boolean(target.closest("input, textarea, select, [contenteditable='true'], .cm-content"));
}

function onRefreshKey(event: KeyboardEvent) {
  if (!props.active || props.busy || event.repeat) return;
  if (typingTarget(event.target)) return;
  const key = event.key.toLowerCase();
  const combo = (event.metaKey || event.ctrlKey) && key === "r" && !event.shiftKey && !event.altKey;
  if (key !== "f5" && !combo) return;
  event.preventDefault();
  emit("refresh");
}

onMounted(() => window.addEventListener("keydown", onRefreshKey));
onBeforeUnmount(() => window.removeEventListener("keydown", onRefreshKey));

function buildChanges() {
  return grid.value?.buildChanges();
}

defineExpose({ buildChanges, discard });
</script>
