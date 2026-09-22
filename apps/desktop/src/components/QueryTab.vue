<template>
  <div class="query-editor">
    <div class="top-panel">
      <SqlEditor
        :active="active"
        :model-value="sql"
        :entities="entities"
        :connection-type="connectionType"
        :connection-id="connectionId"
        :prepare-connection="prepareConnection"
        @update:model-value="$emit('update:sql', $event)"
        @run="$emit('run')"
      />
      <div class="toolbar">
        <button class="btn btn-flat" type="button" @click="$emit('copy-sql')">
          <i class="material-icons">content_copy</i>
          Copy
        </button>
        <span class="expand" />
        <button
          class="btn btn-primary btn-small"
          type="button"
          data-testid="run-query"
          :disabled="busy"
          @click="$emit('run')"
        >
          <i class="material-icons">play_arrow</i>
          Run
        </button>
      </div>
    </div>
    <div class="bottom-panel">
      <p v-if="error" class="error-alert">{{ error }}</p>
      <div v-if="!hasResult && !error" class="result-frame empty-results">Execute a query to see results.</div>
      <ResultGrid v-else :active="active" :result="result" @select-json="$emit('select-json', $event)" />
    </div>
    <div class="statusbar">
      <span>{{ status }}</span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import type { NgQueryResult, TableOrView } from "../ipc";
import SqlEditor from "./SqlEditor.vue";
import ResultGrid from "./ResultGrid.vue";

const props = withDefaults(
  defineProps<{
    sql: string;
    result: NgQueryResult;
    error: string;
    busy: boolean;
    active?: boolean;
    entities?: TableOrView[];
    connectionType?: string;
    connectionId?: string | null;
    prepareConnection?: (id: string) => Promise<void>;
  }>(),
  { active: true, entities: () => [], connectionType: "postgresql", connectionId: null },
);

defineEmits<{
  (event: "run"): void;
  (event: "update:sql", value: string): void;
  (event: "copy-sql"): void;
  (event: "select-json", value: unknown): void;
}>();

const hasResult = computed(
  () => (props.result.fields?.length ?? 0) > 0 || (props.result.rowCount ?? 0) > 0,
);
const status = computed(() => {
  if (props.busy) return "running";
  if (props.error) return "error";
  if (!hasResult.value) return "ready";
  return `${props.result.rowCount ?? props.result.rows?.length ?? 0} rows`;
});
</script>
