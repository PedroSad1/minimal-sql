import { createApp } from "vue";
import App from "./App.vue";
import { util } from "./tauri-util";
import "tabulator-tables/dist/css/tabulator.min.css";
import "./styles/main.scss";

const app = createApp(App);
app.config.globalProperties.$util = util;
app.mount("#app");
