<template>
  <teleport to="body">
    <div v-if="open" class="modal-backdrop" @click.self="$emit('cancel')">
      <div class="card-flat padding prompt-modal" role="dialog">
        <h3>{{ title }}</h3>
        <p v-if="message">{{ message }}</p>
        <input
          v-if="withInput"
          ref="input"
          v-model="draft"
          class="form-control"
          type="text"
          @keydown.enter="$emit('confirm', draft)"
          @keydown.escape="$emit('cancel')"
        />
        <div class="actions">
          <button class="btn btn-flat" type="button" @click="$emit('cancel')">Cancel</button>
          <button class="btn btn-primary" type="button" @click="$emit('confirm', draft)">
            {{ confirmLabel }}
          </button>
        </div>
      </div>
    </div>
  </teleport>
</template>

<script setup lang="ts">
import { nextTick, ref, watch } from "vue";

const props = defineProps<{
  open: boolean;
  title: string;
  message?: string;
  withInput?: boolean;
  value?: string;
  confirmLabel?: string;
}>();

defineEmits<{
  (event: "cancel"): void;
  (event: "confirm", value: string): void;
}>();

const draft = ref(props.value ?? "");
const input = ref<HTMLInputElement | null>(null);

watch(
  () => [props.open, props.value] as const,
  async ([open, value]) => {
    draft.value = value ?? "";
    if (open && props.withInput) {
      await nextTick();
      input.value?.focus();
      input.value?.select();
    }
  },
);
</script>
