<template>
  <div class="interface connection-interface" :class="{ bare }">
      <div class="interface-wrap">
      <aside v-if="!bare" class="sidebar connection-sidebar">
        <div class="sidebar-heading">
          <span class="sub">conexões salvas</span>
          <button class="btn btn-fab" type="button" title="nova conexão" @click="createBlank">
            <i class="material-icons">add</i>
          </button>
        </div>
        <div class="sidebar-list">
          <SavedConnectionList :saved="saved" :selected-id="selectedId" @select="$emit('select-saved', $event)" />
        </div>
      </aside>
      <div class="connection-main page-content flex-col">
        <div class="small-wrap">
          <div v-if="!form.connectionType" class="empty-state">
            <h3>Welcome to Minimal SQL</h3>
            <p>Start by adding new connection.</p>
            <div class="actions">
              <button class="btn btn-primary" type="button" @click="createBlank">
                <i class="material-icons">add</i>
                New Connection
              </button>
            </div>
          </div>
          <div v-else :class="bare ? 'connection-form' : 'card-flat padding'">
            <div v-if="!bare" class="connection-heading">
              <h3 class="card-title">{{ form.name || friendlyType }}</h3>
            </div>
            <form @submit.prevent="$emit('connect')">
              <label class="label">Connection Type</label>
              <select v-model="form.connectionType" class="form-control">
                <option v-for="item in types" :key="item.value" :value="item.value">
                  {{ item.label }}
                </option>
              </select>
              <template v-if="form.connectionType === 'sqlite'">
                <label class="label">arquivo</label>
                <div class="row">
                  <input v-model="form.filename" class="form-control" placeholder="/tmp/app.db" />
                  <button class="btn btn-flat" type="button" @click="$emit('pick-sqlite')">
                    Browse
                  </button>
                </div>
              </template>
              <template v-else-if="form.connectionType === 'bigquery'">
                <label class="label">project id</label>
                <input v-model="form.projectId" class="form-control" />
                <label class="label">dataset</label>
                <input v-model="form.dataset" class="form-control" />
                <label class="label">service account json</label>
                <input v-model="form.serviceAccountJson" class="form-control" />
              </template>
              <template v-else>
                <label class="label">Host</label>
                <input v-model="form.host" class="form-control" />
                <label class="label">Port</label>
                <input v-model.number="form.port" class="form-control" type="number" />
                <label class="label">User</label>
                <input v-model="form.user" class="form-control" />
                <label class="label">Password</label>
                <input v-model="form.password" class="form-control" type="password" />
                <label class="label">Default Database</label>
                <input v-model="form.defaultDatabase" class="form-control" />
                <label class="checkbox">
                  <input v-model="form.ssl" type="checkbox" />
                  Enable SSL
                </label>
              </template>
              <label class="label">Connection Name</label>
              <input v-model="form.name" class="form-control" />
              <p v-if="error" class="error-alert">{{ error }}</p>
              <div class="row actions">
                <button class="btn btn-primary" type="submit" :disabled="busy">Connect</button>
                <button class="btn btn-flat" type="button" @click="$emit('save')">Save</button>
              </div>
            </form>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { COMMUNITY_TYPES, type SavedConnection } from "../ipc";
import SavedConnectionList from "./SavedConnectionList.vue";

export interface ConnectionForm {
  connectionType: string;
  name: string;
  host: string;
  port: number;
  user: string;
  password: string;
  defaultDatabase: string;
  filename: string;
  ssl: boolean;
  readOnly: boolean;
  projectId: string;
  dataset: string;
  serviceAccountJson: string;
}

const props = defineProps<{
  form: ConnectionForm;
  saved: SavedConnection[];
  selectedId: string | null;
  error: string;
  busy: boolean;
  bare?: boolean;
}>();

defineEmits<{
  (event: "connect"): void;
  (event: "save"): void;
  (event: "select-saved", item: SavedConnection): void;
  (event: "pick-sqlite"): void;
}>();

const types = COMMUNITY_TYPES;
const friendlyType = computed(
  () => types.find((item) => item.value === props.form.connectionType)?.label ?? "Connection",
);

function createBlank() {
  props.form.connectionType = "postgresql";
  props.form.name = "";
}
</script>
