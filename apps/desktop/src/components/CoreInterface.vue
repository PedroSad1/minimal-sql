<template>
  <div class="interface core-interface">
    <div class="interface-wrap">
      <aside class="sidebar primary-sidebar">
        <div class="sidebar-heading">
          <span class="sub">connections</span>
          <button class="btn btn-fab" type="button" title="New connection" @click="$emit('add-connection')">
            <i class="material-icons">add</i>
          </button>
        </div>
        <div class="advanced-filter">
          <input
            v-model="filter"
            class="form-control"
            type="search"
            placeholder="Filter"
          />
          <button
            v-if="hiddenKeys.length"
            class="btn btn-flat btn-small"
            type="button"
            @click="$emit('show-hidden')"
          >
            Show hidden ({{ hiddenKeys.length }})
          </button>
        </div>
        <p v-if="connectionError" class="error-alert">{{ connectionError }}</p>
        <div class="sidebar-list table-list">
          <div v-for="item in roots" :key="item.id" class="tree-connection">
            <a
              class="list-item-btn schema-btn"
              :class="{
                open: isConnectionOpen(item.id),
                active: isActive(item),
                busy: connectionBusy && !isActive(item),
              }"
              role="button"
              @click="onConnection(item)"
              @contextmenu.prevent.stop="openConnectionMenu($event, item)"
            >
              <span class="btn-fab open-close">
                <i class="dropdown-icon material-icons">keyboard_arrow_right</i>
              </span>
              <i class="material-icons table-icon">storage</i>
              <span class="table-name truncate">{{ item.name }}</span>
            </a>
            <template v-if="isConnectionOpen(item.id)">
              <p v-if="connectionBusy && isActive(item) && !catalogs[item.id]" class="hint">Connecting…</p>
              <div
                v-for="group in groupsFor(item.id)"
                :key="item.id + (group.schema || 'default')"
                class="tree-schema"
              >
                <a
                  v-if="group.schema"
                  class="list-item-btn schema-btn"
                  :class="{ open: isSchemaOpen(item.id, group.schema) }"
                  role="button"
                  @click="toggleSchema(item.id, group.schema)"
                  @contextmenu.prevent.stop="openSchemaMenu($event, group.schema)"
                >
                  <span class="btn-fab open-close">
                    <i class="dropdown-icon material-icons">keyboard_arrow_right</i>
                  </span>
                  <span class="table-name truncate">{{ group.schema }}</span>
                </a>
                <div v-if="!group.schema || isSchemaOpen(item.id, group.schema)" class="tree-entity">
                    <template v-for="node in group.nodes" :key="node.entity.entityType + qualify(node.entity)">
                      <div v-if="node.partitions.length" class="tree-partition">
                        <a
                          class="list-item-btn schema-btn"
                          :class="{ open: isPartitionOpen(item.id, node.entity) }"
                          role="button"
                          @click="togglePartition(item.id, node.entity)"
                          @dblclick.prevent="openEntity(item.id, node.entity)"
                          @contextmenu.prevent.stop="openTableMenu($event, node.entity)"
                        >
                          <span class="btn-fab open-close">
                            <i class="dropdown-icon material-icons">keyboard_arrow_right</i>
                          </span>
                          <i class="material-icons table-icon">grid_on</i>
                          <span class="table-name truncate">{{ node.entity.name }}</span>
                        </a>
                        <div v-if="isPartitionOpen(item.id, node.entity)" class="tree-entity">
                          <EntityListItem
                            v-for="part in node.partitions"
                            :key="qualify(part)"
                            :entity="part"
                            :label="part.name"
                            icon="grid_on"
                            :active="selectedKey === qualify(part)"
                            :load-columns="() => loadColumns(item.id, part)"
                            @open="openEntity(item.id, part)"
                            @select="selectEntity(part)"
                            @menu="openTableMenu($event, part)"
                            @column-menu="openColumnMenu"
                          />
                        </div>
                      </div>
                      <EntityListItem
                        v-else
                        :entity="node.entity"
                        :label="node.entity.name"
                        :icon="entityIcon(node.entity)"
                        :active="selectedKey === qualify(node.entity)"
                        :load-columns="() => loadColumns(item.id, node.entity)"
                        @open="openEntity(item.id, node.entity)"
                        @select="selectEntity(node.entity)"
                        @menu="openTableMenu($event, node.entity)"
                        @column-menu="openColumnMenu"
                      />
                    </template>
                  </div>
              </div>
            </template>
          </div>
          <p v-if="!roots.length" class="hint">No saved connections.</p>
        </div>
      </aside>
      <div class="page-content main-content">
        <TabBar
          :tabs="tabs"
          :active-id="activeTabId"
          :connected="connected"
          @activate="$emit('activate-tab', $event)"
          @close="$emit('close-tab', $event)"
          @add-query="$emit('add-query')"
          @tab-action="$emit('tab-action', $event)"
        />
        <div class="tab-content">
          <div v-if="!tabs.length" class="empty-editor-group">
            <span class="empty-text">Open a query tab or double-click a table.</span>
          </div>
          <div
            v-for="tab in tabs"
            :key="tab.id"
            class="tab-pane"
            :class="{ active: tab.id === activeTabId }"
          >
            <QueryTab
              v-if="tab.kind === 'query'"
              :active="tab.id === activeTabId"
              :sql="tab.sql"
              :result="tab.result"
              :error="tab.error"
              :busy="tab.busy"
              :entities="entitiesFor(tab.connectionId)"
              :connection-type="connectionTypeFor(tab.connectionId)"
              :connection-id="tab.connectionId"
              :prepare-connection="prepareConnection"
              @run="$emit('run')"
              @update:sql="$emit('update:sql', $event)"
              @copy-sql="$emit('copy-sql')"
              @select-json="$emit('select-json', $event)"
            />
            <TableTab
              v-else-if="tab.kind === 'table' && tab.table"
              :ref="(el) => setPane(tab.id, el)"
              :active="tab.id === activeTabId"
              :result="tab.result"
              :error="tab.error"
              :busy="tab.busy"
              :table-name="tab.table.name"
              :schema="tab.table.schema ?? null"
              :primary-keys="tab.primaryKeys"
              :pending-count="pendingByTab[tab.id] || 0"
              @save-changes="$emit('save-changes')"
              @select-json="$emit('select-json', $event)"
              @pending-count="pendingByTab[tab.id] = $event"
              :filters="tab.tableFilter"
              :page-size="pageSize ?? 100"
              :sync-mode="tab.resultMode"
              @apply-filter="$emit('apply-filter', tab.id, $event)"
              @page-size="$emit('page-size', $event)"
              @load-more="$emit('load-more', tab.id)"
              @refresh="$emit('refresh', tab.id)"
            />
            <div v-else-if="tab.kind === 'structure' && tab.table" class="table-tab">
              <p v-if="tab.error" class="error-alert">{{ tab.error }}</p>
              <StructurePanel
                v-if="tab.structure"
                :title="tab.structure.title"
                :columns="tab.structure.columns"
                :indexes="tab.structure.indexes"
                :triggers="tab.structure.triggers"
                :editable="!readOnly && tab.table.entityType === 'table'"
                :busy="tab.busy"
                @save="$emit('save-structure', { table: tab.table, edits: $event })"
              />
            </div>
          </div>
        </div>
      </div>
      <JsonViewer
        v-if="connected"
        v-show="jsonSidebarOpen"
        :value="jsonValue"
        :open="jsonSidebarOpen"
      />
    </div>
    <ContextMenu
      :open="menu.open"
      :x="menu.x"
      :y="menu.y"
      :options="menu.options"
      @close="menu.open = false"
      @pick="onMenuPick"
    />
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, reactive, ref, watch } from "vue";
import { ipc, type SavedConnection, type TableColumn, type TableOrView } from "../ipc";
import type { TableFilterDraft, WorkspaceTab } from "../tabs";
import { tableTabKey } from "../tabs";
import JsonViewer from "./JsonViewer.vue";
import EntityListItem from "./EntityListItem.vue";
import StructurePanel from "./StructurePanel.vue";
import ContextMenu, { type MenuOption } from "./ContextMenu.vue";
import TabBar from "./TabBar.vue";
import QueryTab from "./QueryTab.vue";
import TableTab from "./TableTab.vue";
import { columnMenuOptions, schemaMenuOptions, tableMenuOptions } from "../entity-menu";
import type { StructureEdits } from "../sql";

export type { StructureView } from "../tabs";

const props = defineProps<{
  tables: TableOrView[];
  views: TableOrView[];
  tabs: WorkspaceTab[];
  activeTabId: string | null;
  jsonValue: unknown;
  jsonSidebarOpen: boolean;
  hiddenKeys: string[];
  readOnly: boolean;
  saved: SavedConnection[];
  activeConnectionId: string | null;
  connectionError: string;
  connectionBusy: boolean;
  activeConnectionName: string;
  catalogs: Record<string, { tables: TableOrView[]; views: TableOrView[] }>;
  prepareConnection?: (id: string) => Promise<void>;
  pageSize?: number;
  connected: boolean;
}>();

const emit = defineEmits<{
  (event: "run"): void;
  (event: "open-table", table: TableOrView): void;
  (event: "update:sql", value: string): void;
  (event: "copy-sql"): void;
  (event: "save-changes"): void;
  (event: "save-structure", payload: { table: TableOrView; edits: StructureEdits }): void;
  (event: "select-json", value: unknown): void;
  (event: "entity-action", payload: { slug: string; table?: TableOrView; schema?: string; column?: TableColumn }): void;
  (event: "show-hidden"): void;
  (event: "activate-tab", id: string): void;
  (event: "close-tab", id: string): void;
  (event: "add-query"): void;
  (event: "tab-action", payload: { slug: string; id: string }): void;
  (event: "select-connection", item: SavedConnection): void;
  (event: "add-connection"): void;
  (event: "edit-connection", item: SavedConnection): void;
  (event: "remove-connection", item: SavedConnection): void;
  (event: "apply-filter", tabId: string, filters: TableFilterDraft[]): void;
  (event: "page-size", value: number): void;
  (event: "load-more", tabId: string): void;
  (event: "refresh", tabId: string): void;
}>();

const filter = ref("");
const sidebarSelected = ref("");
const collapsed = ref<Set<string>>(new Set());
const openConnections = ref<Set<string>>(new Set());
const openPartitions = ref<Set<string>>(new Set());
const pendingByTab = reactive<Record<string, number>>({});
const panes = new Map<string, { buildChanges: () => unknown; discard: () => void }>();
const menu = reactive({
  open: false,
  x: 0,
  y: 0,
  options: [] as MenuOption[],
  table: undefined as TableOrView | undefined,
  schema: undefined as string | undefined,
  column: undefined as TableColumn | undefined,
  connection: undefined as SavedConnection | undefined,
});

const activeTab = computed(() => props.tabs.find((tab) => tab.id === props.activeTabId) ?? null);
const selectedKey = computed(() => {
  if (activeTab.value?.table) return tableTabKey(activeTab.value.table);
  return sidebarSelected.value;
});
const hidden = computed(() => new Set(props.hiddenKeys));
const roots = computed(() => {
  const items = [...props.saved];
  const activeId = props.activeConnectionId;
  if (activeId && !items.some((item) => item.id === activeId)) {
    items.unshift({ id: activeId, name: props.activeConnectionName || "connection", payload: {} });
  }
  return items;
});

watch(
  () => props.activeConnectionId,
  (id) => {
    if (!id) return;
    if (openConnections.value.has(id)) return;
    openConnections.value = new Set([...openConnections.value, id]);
  },
);

function isActive(item: SavedConnection) {
  return Boolean(item.id) && item.id === props.activeConnectionId;
}

function isConnectionOpen(id: string) {
  return openConnections.value.has(id);
}

function onConnection(item: SavedConnection) {
  if (!item.id) return;
  if (props.connectionBusy && !isActive(item)) return;
  const next = new Set(openConnections.value);
  const opening = !next.has(item.id);
  if (opening) next.add(item.id);
  else next.delete(item.id);
  openConnections.value = next;
  if (opening) emit("select-connection", item);
}

function entityIcon(entity: TableOrView) {
  if (entity.entityType === "view" || entity.entityType === "materialized-view") return "visibility";
  return "grid_on";
}

function qualify(entity: TableOrView) {
  return entity.schema ? `${entity.schema}.${entity.name}` : entity.name;
}

function selectEntity(entity: TableOrView) {
  sidebarSelected.value = qualify(entity);
}

function filterEntities(list: TableOrView[], text: string) {
  const needle = text.trim().toLowerCase();
  return list.filter((item) => {
    if (hidden.value.has(qualify(item))) return false;
    if (item.schema && hidden.value.has(`schema:${item.schema}`)) return false;
    if (!needle) return true;
    return qualify(item).toLowerCase().includes(needle);
  });
}

function schemaKey(id: string, schema: string) {
  return `${id}:${schema}`;
}

function isSchemaOpen(id: string, schema: string) {
  return !collapsed.value.has(schemaKey(id, schema));
}

function toggleSchema(id: string, schema: string) {
  const key = schemaKey(id, schema);
  const next = new Set(collapsed.value);
  if (next.has(key)) next.delete(key);
  else next.add(key);
  collapsed.value = next;
}

function entitiesFor(id: string | null) {
  if (!id) return [];
  const catalog = props.catalogs[id];
  if (catalog) return [...catalog.tables, ...catalog.views];
  if (id === props.activeConnectionId) return [...props.tables, ...props.views];
  return [];
}

function connectionTypeFor(id: string | null) {
  const item = props.saved.find((entry) => entry.id === id);
  const payload = item?.payload;
  if (payload && typeof payload === "object" && "connectionType" in payload) {
    const value = (payload as { connectionType?: unknown }).connectionType;
    if (typeof value === "string" && value) return value;
  }
  return "postgresql";
}

function catalogEntities(id: string) {
  const catalog = props.catalogs[id];
  const source = catalog
    ? [...catalog.tables, ...catalog.views]
    : id === props.activeConnectionId
      ? [...props.tables, ...props.views]
      : [];
  return filterEntities(source, "");
}

function nestPartitions(items: TableOrView[]) {
  const children = new Map<string, TableOrView[]>();
  for (const item of items) {
    if (!item.parent || item.entityType !== "table") continue;
    const key = `${item.schema || ""}:${item.parent}`;
    const list = children.get(key) ?? [];
    list.push(item);
    children.set(key, list);
  }
  const nodes: { entity: TableOrView; partitions: TableOrView[] }[] = [];
  const seen = new Set<string>();
  for (const item of items) {
    if (item.parent && item.entityType === "table") continue;
    const key = `${item.schema || ""}:${item.name}`;
    seen.add(key);
    nodes.push({ entity: item, partitions: children.get(key) ?? [] });
  }
  for (const item of items) {
    if (!item.parent || item.entityType !== "table") continue;
    const key = `${item.schema || ""}:${item.parent}`;
    if (seen.has(key)) continue;
    seen.add(key);
    nodes.push({
      entity: { name: item.parent, schema: item.schema, entityType: "table" },
      partitions: children.get(key) ?? [],
    });
  }
  return nodes;
}

function groupsFor(id: string) {
  const groups = new Map<string, TableOrView[]>();
  const term = filter.value.trim().toLowerCase();
  for (const entity of catalogEntities(id)) {
    if (term && !qualify(entity).toLowerCase().includes(term)) continue;
    const schema = entity.schema || "";
    const list = groups.get(schema) ?? [];
    list.push(entity);
    groups.set(schema, list);
  }
  return [...groups.entries()]
    .sort((left, right) => left[0].localeCompare(right[0]))
    .map(([schema, items]) => ({ schema, nodes: nestPartitions(items) }));
}

function partitionKey(id: string, entity: TableOrView) {
  return `${id}:${entity.schema || ""}:${entity.name}`;
}

function isPartitionOpen(id: string, entity: TableOrView) {
  return openPartitions.value.has(partitionKey(id, entity));
}

function togglePartition(id: string, entity: TableOrView) {
  const key = partitionKey(id, entity);
  const next = new Set(openPartitions.value);
  if (next.has(key)) next.delete(key);
  else next.add(key);
  openPartitions.value = next;
}

async function loadColumns(id: string, entity: TableOrView) {
  if (props.prepareConnection) await props.prepareConnection(id);
  return ipc.columns(entity.name, entity.schema || undefined);
}

async function openEntity(id: string, entity: TableOrView) {
  if (props.prepareConnection) await props.prepareConnection(id);
  emit("open-table", entity);
}

function openConnectionMenu(event: MouseEvent, item: SavedConnection) {
  menu.table = undefined;
  menu.schema = undefined;
  menu.column = undefined;
  menu.connection = item;
  placeMenu(event, [
    { name: "Edit", slug: "edit-connection" },
    { name: "Remove", slug: "remove-connection" },
  ]);
}

function placeMenu(event: MouseEvent, options: MenuOption[]) {
  menu.open = true;
  menu.x = Math.min(event.clientX, window.innerWidth - 260);
  menu.y = Math.min(event.clientY, window.innerHeight - 420);
  menu.options = options;
}

function openTableMenu(event: MouseEvent, table: TableOrView) {
  menu.table = table;
  menu.schema = undefined;
  menu.column = undefined;
  menu.connection = undefined;
  const isView = table.entityType === "view" || table.entityType === "materialized-view";
  placeMenu(event, tableMenuOptions(props.readOnly, isView));
}

function openSchemaMenu(event: MouseEvent, schema: string) {
  menu.table = undefined;
  menu.schema = schema;
  menu.column = undefined;
  placeMenu(event, schemaMenuOptions(props.readOnly));
}

function openColumnMenu(payload: { column: TableColumn; event: MouseEvent }) {
  menu.table = undefined;
  menu.schema = undefined;
  menu.column = payload.column;
  placeMenu(payload.event, columnMenuOptions);
}

function onMenuPick(option: MenuOption) {
  menu.open = false;
  if (!option.slug) return;
  if (option.slug === "edit-connection" && menu.connection) {
    emit("edit-connection", menu.connection);
    return;
  }
  if (option.slug === "remove-connection" && menu.connection) {
    emit("remove-connection", menu.connection);
    return;
  }
  emit("entity-action", {
    slug: option.slug,
    table: menu.table,
    schema: menu.schema,
    column: menu.column,
  });
}

function setPane(id: string, el: unknown) {
  const pane = el as { buildChanges: () => unknown; discard: () => void } | null;
  if (pane && typeof pane.buildChanges === "function") panes.set(id, pane);
  else panes.delete(id);
}

function resetEdits() {
  if (props.activeTabId) panes.get(props.activeTabId)?.discard();
}

function buildChanges() {
  if (!props.activeTabId) return undefined;
  return panes.get(props.activeTabId)?.buildChanges();
}

function onKey(event: KeyboardEvent) {
  if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "s") {
    event.preventDefault();
    emit("save-changes");
  }
}

onMounted(() => window.addEventListener("keydown", onKey));
onUnmounted(() => window.removeEventListener("keydown", onKey));

defineExpose({ buildChanges, resetEdits });
</script>
