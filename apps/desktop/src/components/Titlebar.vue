<template>
  <div class="titlebar">
    <div class="titlebar-title">{{ title }}</div>
    <div class="titlebar-actions">
      <button
        v-if="updateVersion"
        class="btn btn-flat btn-small titlebar-update"
        type="button"
        :disabled="updating"
        :title="updateError || `Update to ${updateVersion}`"
        @click="$emit('open-update')"
      >
        {{ updating ? "Updating…" : "Update" }}
      </button>
      <button
        v-if="connected"
        class="btn btn-link btn-icon"
        type="button"
        :title="jsonSidebarOpen ? 'Close JSON viewer' : 'Open JSON viewer'"
        @click="$emit('toggle-json')"
      >
        <i class="material-icons">{{ jsonSidebarOpen ? "view_sidebar" : "code" }}</i>
      </button>
      <button class="btn btn-link btn-icon" type="button" title="Switch theme" @click="$emit('cycle-theme')">
        <i class="material-icons">brightness_6</i>
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
defineProps<{
  title: string;
  connected: boolean;
  jsonSidebarOpen?: boolean;
  updateVersion?: string | null;
  updating?: boolean;
  updateError?: string | null;
}>();
defineEmits<{
  (event: "cycle-theme"): void;
  (event: "toggle-json"): void;
  (event: "open-update"): void;
}>();
</script>
