import { createRouter, createWebHistory } from "vue-router";
import { getCurrentWindow } from "@tauri-apps/api/window";
import SearchView from "../views/SearchView.vue";
import SettingsView from "../views/SettingsView.vue";
import { isTauri } from "../services/window";

export const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: "/", name: "search", component: SearchView },
    { path: "/settings", name: "settings", component: SettingsView },
  ],
});

router.beforeEach((to) => {
  if (isTauri() && getCurrentWindow().label === "settings" && to.name !== "settings") {
    return { name: "settings" };
  }
  return true;
});
