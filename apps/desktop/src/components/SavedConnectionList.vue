<template>
  <div class="saved-connection-list">
    <button
      v-for="item in saved"
      :key="item.id"
      type="button"
      class="saved-connection"
      :class="{ active: item.id === selectedId }"
      :disabled="disabled"
      @click="$emit('select', item)"
    >
      <span class="saved-connection-mark">
        <i class="material-icons">storage</i>
      </span>
      <span class="saved-connection-copy">
        <span class="saved-connection-name truncate">{{ item.name }}</span>
        <span class="saved-connection-meta truncate">{{ detail(item) }}</span>
      </span>
    </button>
    <p v-if="!saved.length" class="hint">Nenhuma conexão salva.</p>
  </div>
</template>

<script setup lang="ts">
import { COMMUNITY_TYPES, type SavedConnection } from "../ipc";

defineProps<{
  saved: SavedConnection[];
  selectedId: string | null;
  disabled?: boolean;
}>();

defineEmits<{
  (event: "select", item: SavedConnection): void;
}>();

function detail(item: SavedConnection) {
  const payload = item.payload as {
    connectionType?: string;
    host?: string;
    port?: number;
    defaultDatabase?: string;
    filename?: string;
    projectId?: string;
    dataset?: string;
  };
  const type = COMMUNITY_TYPES.find((entry) => entry.value === payload.connectionType)?.label ?? "SQL";
  if (payload.connectionType === "sqlite") return `${type} · ${payload.filename || "arquivo"}`;
  if (payload.connectionType === "bigquery") {
    return `${type} · ${[payload.projectId, payload.dataset].filter(Boolean).join(" / ") || "BigQuery"}`;
  }
  const host = [payload.host, payload.port].filter((part) => part !== undefined && part !== "").join(":");
  const database = payload.defaultDatabase ? ` / ${payload.defaultDatabase}` : "";
  const target = `${host}${database}`.trim();
  return target ? `${type} · ${target}` : type;
}
</script>
