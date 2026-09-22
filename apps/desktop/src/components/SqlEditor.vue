<template>
  <div class="sql-editor" ref="host"></div>
</template>

<script setup lang="ts">
import { acceptCompletion, autocompletion } from "@codemirror/autocomplete";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { Compartment, EditorState, Prec } from "@codemirror/state";
import { EditorView, keymap, lineNumbers, placeholder, tooltips } from "@codemirror/view";
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { graphiteEditor } from "../editor-theme";
import { ipc, type TableOrView } from "../ipc";
import { createSqlCompletion, dialectFor } from "../sql-complete";
import { sqlTone, sqlToneRefresh } from "../sql-tone";

const props = withDefaults(
  defineProps<{
    modelValue: string;
    active?: boolean;
    entities?: TableOrView[];
    connectionType?: string;
    connectionId?: string | null;
    prepareConnection?: (id: string) => Promise<void>;
  }>(),
  { active: true, entities: () => [], connectionType: "postgresql", connectionId: null },
);
const emit = defineEmits<{
  (event: "update:modelValue", value: string): void;
  (event: "run"): void;
}>();

const host = ref<HTMLElement | null>(null);
const sqlLang = new Compartment();
const tone = new Compartment();
let view: EditorView | null = null;

const completion = createSqlCompletion({
  getConnectionType: () => props.connectionType || "postgresql",
  getEntities: () => props.entities ?? [],
  loadColumns: async (table, schema) => {
    if (!props.connectionId) return [];
    if (props.prepareConnection) await props.prepareConnection(props.connectionId);
    const columns = await ipc.columns(table, schema || undefined);
    return columns.map((column) => ({ name: column.columnName, dataType: column.dataType }));
  },
});

function sqlLanguage(type: string) {
  const dialect = dialectFor(type || "postgresql");
  return [dialect.language, dialect.language.data.of({ autocomplete: completion })];
}

function entityKey() {
  return (props.entities ?? []).map((entity) => `${entity.schema ?? ""}.${entity.name}`).join("\n");
}

function toneExtension() {
  return sqlTone({
    getConnectionType: () => props.connectionType || "postgresql",
    getEntities: () => props.entities ?? [],
    loadColumns: async (table, schema) => {
      if (!props.connectionId) return null;
      if (props.prepareConnection) await props.prepareConnection(props.connectionId);
      const columns = await ipc.columns(table, schema || undefined);
      return columns.map((column) => ({ name: column.columnName }));
    },
  });
}

onMounted(() => {
  if (!host.value) return;
  view = new EditorView({
    parent: host.value,
    state: EditorState.create({
      doc: props.modelValue,
      extensions: [
        lineNumbers(),
        history(),
        sqlLang.of(sqlLanguage(props.connectionType)),
        tone.of(toneExtension()),
        graphiteEditor,
        tooltips({ parent: document.body }),
        autocompletion({
          icons: false,
          activateOnCompletion: (item) => typeof item.apply === "string" && item.apply.endsWith("."),
        }),
        placeholder("escreva SQL. Cmd+Enter executa."),
        Prec.highest(
          keymap.of([
            {
              key: "Mod-Enter",
              run: () => {
                emit("run");
                return true;
              },
            },
            { key: "Tab", run: acceptCompletion },
          ]),
        ),
        keymap.of([...defaultKeymap, ...historyKeymap, indentWithTab]),
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
  () => props.connectionType,
  (type) => {
    if (!view) return;
    view.dispatch({
      effects: [sqlLang.reconfigure(sqlLanguage(type)), tone.reconfigure(toneExtension())],
    });
  },
);

watch(
  () => props.connectionId,
  () => {
    if (!view) return;
    view.dispatch({ effects: tone.reconfigure(toneExtension()) });
  },
);

watch(entityKey, () => {
  if (!view) return;
  view.dispatch({ effects: sqlToneRefresh.of(null) });
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
