<template>
  <teleport to="body">
    <div
      v-if="open"
      class="context-menu-backdrop"
      @click="$emit('close')"
      @contextmenu.prevent="$emit('close')"
    >
      <ul
        class="context-menu"
        :style="{ left: `${x}px`, top: `${y}px` }"
        @click.stop
      >
        <li
          v-for="(option, index) in options"
          :key="option.slug || `d-${index}`"
          :class="{ divider: option.divider, disabled: option.disabled }"
          :title="option.title"
          @click="pick(option)"
        >
          <template v-if="!option.divider">
            <span>{{ option.name }}</span>
            <span v-if="option.shortcut" class="shortcut">{{ option.shortcut }}</span>
            <i v-if="option.icon" class="material-icons">{{ option.icon }}</i>
          </template>
        </li>
      </ul>
    </div>
  </teleport>
</template>

<script setup lang="ts">
export interface MenuOption {
  name?: string;
  slug?: string;
  divider?: boolean;
  disabled?: boolean;
  title?: string;
  icon?: string;
  shortcut?: string;
}

defineProps<{
  open: boolean;
  x: number;
  y: number;
  options: MenuOption[];
}>();

const emit = defineEmits<{
  (event: "close"): void;
  (event: "pick", option: MenuOption): void;
}>();

function pick(option: MenuOption) {
  if (option.divider || option.disabled) return;
  emit("pick", option);
}
</script>
