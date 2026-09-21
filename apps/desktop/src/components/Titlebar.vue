<template>
  <div class="titlebar">
    <div class="titlebar-title">{{ title }}</div>
    <div class="titlebar-actions">
      <button
        v-if="connected"
        class="btn btn-link btn-icon"
        type="button"
        :title="jsonSidebarOpen ? 'Fechar JSON viewer' : 'Abrir JSON viewer'"
        @click="$emit('toggle-json')"
      >
        <i class="material-icons">{{ jsonSidebarOpen ? "view_sidebar" : "code" }}</i>
      </button>
      <button class="btn btn-link btn-icon" type="button" @click="$emit('cycle-theme')">
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
}>();
defineEmits<{
  (event: "cycle-theme"): void;
  (event: "toggle-json"): void;
}>();
</script>
