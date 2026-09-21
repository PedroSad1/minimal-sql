<template>
  <div ref="header" class="tabs-header">
    <ul class="nav-tabs tab-measure" aria-hidden="true">
      <li v-for="tab in tabs" :key="tab.id" class="nav-item">
        <span class="nav-link">
          <i class="material-icons">{{ icon(tab.kind) }}</i>
          <span class="tab-title truncate">
            {{ tab.title }}
            <span v-if="showConnection(tab)" class="tab-connection">{{ tab.connectionName }}</span>
          </span>
          <span class="tab-close"><i class="material-icons">close</i></span>
        </span>
      </li>
    </ul>
    <div ref="strip" class="tab-strip">
      <ul class="nav-tabs">
        <li v-for="tab in visibleTabs" :key="tab.id" class="nav-item">
          <a
            class="nav-link"
            :class="{ active: tab.id === activeId }"
            @click="$emit('activate', tab.id)"
            @click.middle.prevent="$emit('close', tab.id)"
            @contextmenu.prevent="openMenu($event, tab)"
          >
            <i class="material-icons">{{ icon(tab.kind) }}</i>
            <span class="tab-title truncate">
              {{ tab.title }}
              <span v-if="showConnection(tab)" class="tab-connection">{{ tab.connectionName }}</span>
            </span>
            <span class="tab-close" @click.stop.prevent="$emit('close', tab.id)">
              <i class="material-icons">close</i>
            </span>
          </a>
        </li>
      </ul>
    </div>
    <div v-if="stackedTabs.length" class="tab-stack">
      <button
        class="nav-link tab-stack-link"
        type="button"
        :title="`${stackedTabs.length} tabs`"
        @click="stackOpen = !stackOpen"
      >
        <span class="tab-stack-sheets" aria-hidden="true">
          <span></span>
          <span></span>
          <span></span>
        </span>
        <span class="tab-stack-count">{{ stackedTabs.length }}</span>
      </button>
      <div v-if="stackOpen" class="tab-stack-menu">
        <button
          v-for="tab in stackedTabs"
          :key="tab.id"
          class="tab-stack-item"
          type="button"
          @click="pickStacked(tab.id)"
          @click.middle.prevent="$emit('close', tab.id)"
          @contextmenu.prevent="openMenu($event, tab)"
        >
          <i class="material-icons">{{ icon(tab.kind) }}</i>
          <span class="tab-title truncate">
            {{ tab.title }}
            <span v-if="showConnection(tab)" class="tab-connection">{{ tab.connectionName }}</span>
          </span>
        </button>
      </div>
    </div>
    <button
      v-if="connected"
      ref="addButton"
      class="btn btn-fab add-query"
      type="button"
      title="New query"
      @click="$emit('add-query')"
    >
      <i class="material-icons">add</i>
    </button>
    <ContextMenu
      :open="menu.open"
      :x="menu.x"
      :y="menu.y"
      :options="menuOptions"
      @close="menu.open = false"
      @pick="onPick"
    />
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, reactive, ref, watch } from "vue";
import type { TabKind, WorkspaceTab } from "../tabs";
import ContextMenu, { type MenuOption } from "./ContextMenu.vue";

const props = defineProps<{
  tabs: WorkspaceTab[];
  activeId: string | null;
  connected: boolean;
}>();

const emit = defineEmits<{
  (event: "activate", id: string): void;
  (event: "close", id: string): void;
  (event: "add-query"): void;
  (event: "tab-action", payload: { slug: string; id: string }): void;
}>();

const header = ref<HTMLElement | null>(null);
const strip = ref<HTMLElement | null>(null);
const addButton = ref<HTMLElement | null>(null);
const room = ref(0);
const widths = ref<number[]>([]);
const stackOpen = ref(false);
const menu = reactive({ open: false, x: 0, y: 0, tabId: "" });

const STACK_WIDTH = 88
const GAP = 4;

const menuTab = computed(() => props.tabs.find((tab) => tab.id === menu.tabId) ?? null);
const menuOptions = computed<MenuOption[]>(() => {
  const tab = menuTab.value;
  if (!tab) return [];
  const index = props.tabs.findIndex((item) => item.id === tab.id);
  return [
    { name: "Close", slug: "close" },
    { name: "Close Others", slug: "close-others", disabled: props.tabs.length < 2 },
    { name: "Close All", slug: "close-all" },
    { name: "Close Tabs to the Right", slug: "close-right", disabled: index < 0 || index === props.tabs.length - 1 },
    { divider: true, slug: "divider" },
    { name: "Duplicate", slug: "duplicate" },
    { name: "Copy Entity Name", slug: "copy-name", disabled: tab.kind === "query" },
  ];
});

const split = computed(() => {
  const tabs = props.tabs;
  const known = widths.value;
  if (!tabs.length || known.length !== tabs.length || room.value <= 0) {
    return { visible: tabs, stacked: [] as WorkspaceTab[] };
  }
  const total = known.reduce((sum, width, index) => sum + width + (index ? GAP : 0), 0);
  if (total <= room.value) return { visible: tabs, stacked: [] as WorkspaceTab[] };
  const budget = Math.max(0, room.value - STACK_WIDTH - GAP);
  const visible: WorkspaceTab[] = [];
  let used = 0;
  for (let index = 0; index < tabs.length; index += 1) {
    const width = known[index] ?? 160;
    const next = used + (visible.length ? GAP : 0) + width;
    if (next > budget && visible.length > 0) break;
    visible.push(tabs[index]);
    used = next;
  }
  let stacked = tabs.slice(visible.length);
  const active = tabs.find((tab) => tab.id === props.activeId);
  if (active && stacked.some((tab) => tab.id === active.id)) {
    const displaced = visible.pop();
    if (displaced) stacked = [displaced, ...stacked];
    visible.push(active);
    stacked = stacked.filter((tab) => tab.id !== active.id);
  }
  return { visible, stacked };
});

const visibleTabs = computed(() => split.value.visible);
const stackedTabs = computed(() => split.value.stacked);

watch(stackedTabs, (tabs) => {
  if (!tabs.length) stackOpen.value = false;
});

function showConnection(tab: WorkspaceTab) {
  const ids = new Set(props.tabs.map((item) => item.connectionId));
  return ids.size > 1 && tab.connectionName.length > 0;
}

function icon(kind: TabKind) {
  if (kind === "table") return "grid_on";
  if (kind === "structure") return "view_column";
  return "code";
}

function measure() {
  const bar = header.value;
  if (!bar) return;
  const addWidth = addButton.value?.offsetWidth ?? 28;
  room.value = Math.max(0, bar.clientWidth - addWidth - 12);
  const links = bar.querySelectorAll(".tab-measure .nav-link");
  widths.value = Array.from(links).map((node) => Math.ceil(node.getBoundingClientRect().width));
}

function openMenu(event: MouseEvent, tab: WorkspaceTab) {
  stackOpen.value = false;
  const menuWidth = 240;
  const menuHeight = 250;
  menu.tabId = tab.id;
  menu.x = Math.max(8, Math.min(event.clientX, window.innerWidth - menuWidth));
  menu.y = Math.max(8, Math.min(event.clientY, window.innerHeight - menuHeight));
  menu.open = true;
}

function onPick(option: MenuOption) {
  const id = menu.tabId;
  menu.open = false;
  if (!id || !option.slug || option.slug === "divider") return;
  if (option.slug === "close") {
    emit("close", id);
    return;
  }
  emit("tab-action", { slug: option.slug, id });
}

function pickStacked(id: string) {
  stackOpen.value = false;
  emit("activate", id);
}

function onPointerDown(event: PointerEvent) {
  const target = event.target;
  if (!(target instanceof Node)) return;
  if (!header.value?.querySelector(".tab-stack")?.contains(target)) stackOpen.value = false;
}

let observer: ResizeObserver | null = null;

onMounted(() => {
  observer = new ResizeObserver(() => measure());
  if (header.value) observer.observe(header.value);
  window.addEventListener("pointerdown", onPointerDown);
  void nextTick(measure);
});

onUnmounted(() => {
  observer?.disconnect();
  window.removeEventListener("pointerdown", onPointerDown);
});

watch(
  () => props.tabs.map((tab) => `${tab.id}:${tab.title}:${tab.connectionName}`).join("|"),
  () => void nextTick(measure),
);
</script>
