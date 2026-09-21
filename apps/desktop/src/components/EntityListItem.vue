<template>
  <div class="list-item entity-list-item">
    <a
      class="list-item-btn"
      role="button"
      :class="{ active, open: expanded }"
      @contextmenu.prevent.stop="$emit('menu', $event)"
    >
      <span class="btn-fab open-close" @click.stop.prevent="toggle">
        <i class="dropdown-icon material-icons">keyboard_arrow_right</i>
      </span>
      <span class="item-wrapper flex flex-middle expand" @click="$emit('select')" @dblclick.prevent="$emit('open')">
        <i class="material-icons table-icon">{{ icon }}</i>
        <span class="table-name truncate" :title="label">{{ label }}</span>
      </span>
    </a>
    <div v-if="expanded" class="sub-items">
      <span v-if="loading" class="sub-item">Loading…</span>
      <span v-else-if="error" class="sub-item">{{ error }}</span>
      <span v-else-if="columns.length === 0" class="sub-item">No Columns</span>
      <span
        v-for="column in columns"
        v-else
        :key="column.columnName"
        class="sub-item"
        @contextmenu.prevent.stop="$emit('column-menu', { column, event: $event })"
      >
        <span class="title truncate" :title="column.columnName">{{ column.columnName }}</span>
        <span class="badge" :class="column.dataType">
          <span>{{ column.dataType }}</span>
        </span>
      </span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from "vue";
import { ipc, type TableColumn, type TableOrView } from "../ipc";

const props = defineProps<{
  entity: TableOrView;
  label: string;
  icon: string;
  active?: boolean;
  loadColumns?: () => Promise<TableColumn[]>;
}>();

defineEmits<{
  (event: "open"): void;
  (event: "select"): void;
  (event: "menu", event: MouseEvent): void;
  (event: "column-menu", payload: { column: TableColumn; event: MouseEvent }): void;
}>();

const expanded = ref(false);
const loading = ref(false);
const error = ref("");
const columns = ref<TableColumn[]>([]);

async function toggle() {
  expanded.value = !expanded.value;
  if (!expanded.value || columns.value.length) return;
  loading.value = true;
  error.value = "";
  try {
    columns.value = props.loadColumns
      ? await props.loadColumns()
      : await ipc.columns(props.entity.name, props.entity.schema || undefined);
  } catch (err) {
    error.value = String(err);
  } finally {
    loading.value = false;
  }
}
</script>
