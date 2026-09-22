<template>
  <aside class="sidebar secondary-sidebar json-viewer-sidebar" :style="{ width: `${width}px` }">
    <div
      class="json-resize"
      role="separator"
      aria-orientation="vertical"
      aria-label="Resize JSON viewer"
      @pointerdown="startResize"
    ></div>
    <div class="sidebar-heading">
      <span class="sub">JSON Viewer</span>
    </div>
    <div v-if="!empty" class="filter-wrap">
      <input
        v-model="filter"
        class="form-control"
        type="search"
        placeholder="Filter keys"
      />
    </div>
    <div v-show="empty" class="empty-text">
      Click a JSON or JSONB column to open the value.
    </div>
    <div v-show="!empty" ref="host" class="json-editor"></div>
  </aside>
</template>

<script setup lang="ts">
import { EditorView, lineNumbers } from "@codemirror/view";
import { EditorState } from "@codemirror/state";
import { json } from "@codemirror/lang-json";
import { computed, nextTick, onBeforeUnmount, ref, watch } from "vue";
import { graphiteEditor } from "../editor-theme";

const props = defineProps<{
  value: unknown;
  open?: boolean;
}>();

const host = ref<HTMLElement | null>(null);
const filter = ref("");
const width = ref(360);
let view: EditorView | null = null;

const empty = computed(() => props.value == null);

const text = computed(() => stringifyFiltered(props.value, filter.value.trim()));

function rebuild() {
  view?.destroy();
  view = null;
  if (!host.value || empty.value) return;
  view = new EditorView({
    parent: host.value,
    state: EditorState.create({
      doc: text.value,
      extensions: [
        lineNumbers(),
        json(),
        graphiteEditor,
        EditorView.lineWrapping,
        EditorView.editable.of(false),
      ],
    }),
  });
}

watch(width, () => view?.requestMeasure());

function startResize(event: PointerEvent) {
  event.preventDefault();
  const startX = event.clientX;
  const startWidth = width.value;
  const target = event.currentTarget;
  if (target instanceof HTMLElement) target.setPointerCapture(event.pointerId);
  const move = (next: PointerEvent) => {
    const max = Math.floor(window.innerWidth * 0.7);
    width.value = Math.min(max, Math.max(240, startWidth + startX - next.clientX));
  };
  const stop = () => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", stop);
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", stop);
}

watch(
  () => [props.value, filter.value, empty.value, props.open] as const,
  async () => {
    await nextTick();
    requestAnimationFrame(() => rebuild());
  },
  { deep: true, immediate: true },
);

onBeforeUnmount(() => {
  view?.destroy();
});

function parseValue(value: unknown) {
  if (typeof value !== "string") return value;
  const trimmed = value.trim();
  if (!(trimmed.startsWith("{") || trimmed.startsWith("["))) return value;
  try {
    return JSON.parse(trimmed);
  } catch {
    return value;
  }
}

function stringifyFiltered(value: unknown, needle: string) {
  const parsed = needle
    ? filterValue(parseValue(value), needle.toLowerCase())
    : parseValue(value);
  try {
    return JSON.stringify(parsed ?? {}, null, 2);
  } catch {
    return String(value);
  }
}

function filterValue(value: unknown, needle: string): unknown {
  if (!needle) return value;
  if (Array.isArray(value)) {
    return value.map((item) => filterValue(item, needle)).filter((item) => item !== undefined);
  }
  if (value && typeof value === "object") {
    const out: Record<string, unknown> = {};
    for (const [key, child] of Object.entries(value as Record<string, unknown>)) {
      if (key.toLowerCase().includes(needle)) {
        out[key] = child;
        continue;
      }
      const nested = filterValue(child, needle);
      if (nested && typeof nested === "object" && Object.keys(nested as object).length) {
        out[key] = nested;
      }
    }
    return out;
  }
  return undefined;
}
</script>
