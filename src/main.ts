import { createApp } from "vue";
import { createPinia } from "pinia";
import App from "./App.vue";
import { applyLocale } from "./i18n";
import { router } from "./router";
import "./style.css";

applyLocale("system");

const app = createApp(App);
app.use(createPinia());
app.use(router);
app.mount("#app");
