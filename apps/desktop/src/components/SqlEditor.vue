<template>
  <div class="sql-editor" ref="host"></div>
</template>

<script setup lang="ts">
import { EditorView, keymap, lineNumbers, placeholder } from "@codemirror/view";
import { EditorState } from "@codemirror/state";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { sql } from "@codemirror/lang-sql";
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { graphiteEditor } from "../editor-theme";

const props = withDefaults(
  defineProps<{
    modelValue: string;
    active?: boolean;
  }>(),
  { active: true },
);
const emit = defineEmits<{
  (event: "update:modelValue", value: string): void;
  (event: "run"): void;
}>();

const host = ref<HTMLElement | null>(null);
let view: EditorView | null = null;

onMounted(() => {
  if (!host.value) return;
  view = new EditorView({
    parent: host.value,
    state: EditorState.create({
      doc: props.modelValue,
      extensions: [
        lineNumbers(),
        history(),
        sql(),
        graphiteEditor,
        placeholder("escreva SQL. Cmd+Enter executa."),
        keymap.of([
          {
            key: "Mod-Enter",
            run: () => {
              emit("run");
              return true;
            },
          },
          ...defaultKeymap,
          ...historyKeymap,
          indentWithTab,
        ]),
        EditorView.updateListener.of((update) => {
          if (update.docChanged) {
            emit("update:modelValue", update.state.doc.toString());
          }
        }),
      ],
    }),
  });
});

watch(
  () => props.active,
  async (active) => {
    if (!active || !view) return;
    await nextTick();
    requestAnimationFrame(() => view?.requestMeasure());
  },
);

watch(
  () => props.modelValue,
  (value) => {
    if (!view) return;
    if (value === view.state.doc.toString()) return;
    view.dispatch({
      changes: { from: 0, to: view.state.doc.length, insert: value },
    });
  },
);

onBeforeUnmount(() => {
  view?.destroy();
  view = null;
});
</script>
