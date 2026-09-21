/// <reference types="vite/client" />

declare module "*.vue" {
  import type { DefineComponent } from "vue";
  const component: DefineComponent<object, object, unknown>;
  export default component;
}

declare module "vue" {
  interface ComponentCustomProperties {
    $util: {
      send: <T = unknown>(channel: string, payload?: Record<string, unknown>) => Promise<T>;
    };
  }
}
