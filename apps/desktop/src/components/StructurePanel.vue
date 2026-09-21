<template>
  <div class="structure-panel">
    <div class="structure-scroll">
      <div class="structure-heading">{{ title }}</div>
      <p v-if="localError" class="error-alert">{{ localError }}</p>
      <section>
        <h4>Columns</h4>
        <table>
          <thead>
            <tr>
              <th>Name</th>
              <th>Type</th>
              <th>Nullable</th>
              <th v-if="editable"></th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="column in columnsDraft" :key="column.key" :class="{ dropped: column.dropped }">
              <td>
                <input v-model="column.name" class="form-control" :disabled="!editable || column.dropped" />
              </td>
              <td>
                <input v-model="column.dataType" class="form-control" :disabled="!editable || column.dropped" />
              </td>
              <td>
                <input v-model="column.nullable" type="checkbox" :disabled="!editable || column.dropped" />
              </td>
              <td v-if="editable" class="structure-action">
                <button class="btn btn-small structure-drop" type="button" @click="toggleColumn(column)">
                  {{ column.dropped ? "Undo" : "Drop" }}
                </button>
              </td>
            </tr>
            <tr v-if="!columnsDraft.length">
              <td :colspan="editable ? 4 : 3">No Columns</td>
            </tr>
          </tbody>
        </table>
        <button v-if="editable" class="btn btn-small" type="button" @click="addColumn">Add column</button>
      </section>
      <section>
        <h4>Indexes</h4>
        <table>
          <thead>
            <tr>
              <th>Name</th>
              <th>Unique</th>
              <th>Primary</th>
              <th v-if="editable"></th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="index in indexDraft" :key="index.name" :class="{ dropped: index.dropped }">
              <td>{{ index.name }}</td>
              <td>{{ index.unique ? "YES" : "NO" }}</td>
              <td>{{ index.primary ? "YES" : "NO" }}</td>
              <td v-if="editable" class="structure-action">
                <button
                  class="btn btn-small structure-drop"
                  type="button"
                  :disabled="index.primary"
                  @click="index.dropped = !index.dropped"
                >
                  {{ index.dropped ? "Undo" : "Drop" }}
                </button>
              </td>
            </tr>
            <tr v-if="!indexDraft.length">
              <td :colspan="editable ? 4 : 3">No Indexes</td>
            </tr>
          </tbody>
        </table>
      </section>
      <section>
        <h4>Triggers</h4>
        <table>
          <thead>
            <tr>
              <th>Name</th>
              <th>Timing</th>
              <th>Event</th>
              <th v-if="editable"></th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="trigger in triggerDraft" :key="trigger.name" :class="{ dropped: trigger.dropped }">
              <td>{{ trigger.name }}</td>
              <td>{{ trigger.timing || "—" }}</td>
              <td>{{ trigger.manipulation || "—" }}</td>
              <td v-if="editable" class="structure-action">
                <button class="btn btn-small structure-drop" type="button" @click="trigger.dropped = !trigger.dropped">
                  {{ trigger.dropped ? "Undo" : "Drop" }}
                </button>
              </td>
            </tr>
            <tr v-if="!triggerDraft.length">
              <td :colspan="editable ? 4 : 3">No Triggers</td>
            </tr>
          </tbody>
        </table>
      </section>
    </div>
    <div v-if="editable" class="structure-actions">
      <button class="btn btn-small" type="button" :disabled="!dirty || busy" @click="reset">Reset</button>
      <button class="btn btn-primary btn-small" type="button" :disabled="!dirty || busy" @click="save">Save</button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watch } from "vue";
import type { TableColumn, TableIndex, TableTrigger } from "../ipc";
import type { ColumnEdit, StructureEdits } from "../sql";

const props = defineProps<{
  title: string;
  columns: TableColumn[];
  indexes: TableIndex[];
  triggers: TableTrigger[];
  editable?: boolean;
  busy?: boolean;
}>();

const emit = defineEmits<{
  (event: "save", edits: StructureEdits): void;
}>();

interface ColumnDraft extends ColumnEdit {
  key: string;
}

interface IndexDraft {
  name: string;
  unique: boolean;
  primary: boolean;
  dropped: boolean;
}

interface TriggerDraft {
  name: string;
  timing?: string | null;
  manipulation?: string | null;
  dropped: boolean;
}

const columnsDraft = ref<ColumnDraft[]>([]);
const indexDraft = ref<IndexDraft[]>([]);
const triggerDraft = ref<TriggerDraft[]>([]);
const localError = ref("");
let nextKey = 1;

function reset() {
  localError.value = "";
  columnsDraft.value = props.columns.map((column) => ({
    key: `col-${column.columnName}`,
    originalName: column.columnName,
    name: column.columnName,
    dataType: column.dataType,
    nullable: column.nullable,
    originalType: column.dataType,
    originalNullable: column.nullable,
    dropped: false,
  }));
  indexDraft.value = props.indexes.map((index) => ({
    name: index.name,
    unique: index.unique,
    primary: index.primary,
    dropped: false,
  }));
  triggerDraft.value = props.triggers.map((trigger) => ({
    name: trigger.name,
    timing: trigger.timing,
    manipulation: trigger.manipulation,
    dropped: false,
  }));
}

watch(() => [props.columns, props.indexes, props.triggers], reset, { immediate: true });

const dirty = computed(() => {
  if (indexDraft.value.some((index) => index.dropped)) return true;
  if (triggerDraft.value.some((trigger) => trigger.dropped)) return true;
  return columnsDraft.value.some((column) => {
    if (!column.originalName) return !column.dropped && column.name.trim().length > 0;
    if (column.dropped) return true;
    return (
      column.name.trim() !== column.originalName ||
      column.dataType.trim() !== column.originalType ||
      column.nullable !== column.originalNullable
    );
  });
});

function addColumn() {
  columnsDraft.value.push({
    key: `new-${nextKey}`,
    originalName: null,
    name: "",
    dataType: "text",
    nullable: true,
    originalType: "text",
    originalNullable: true,
    dropped: false,
  });
  nextKey += 1;
}

function toggleColumn(column: ColumnDraft) {
  if (!column.originalName && !column.dropped) {
    columnsDraft.value = columnsDraft.value.filter((item) => item.key !== column.key);
    return;
  }
  column.dropped = !column.dropped;
}

function save() {
  localError.value = "";
  const missing = columnsDraft.value.some((column) => !column.dropped && !column.name.trim());
  if (missing) {
    localError.value = "Every column needs a name.";
    return;
  }
  const missingType = columnsDraft.value.some((column) => !column.dropped && !column.dataType.trim());
  if (missingType) {
    localError.value = "Every column needs a type.";
    return;
  }
  emit("save", {
    columns: columnsDraft.value.map(({ key: _key, ...column }) => column),
    dropIndexes: indexDraft.value.filter((index) => index.dropped).map((index) => index.name),
    dropTriggers: triggerDraft.value.filter((trigger) => trigger.dropped).map((trigger) => trigger.name),
  });
}
</script>
