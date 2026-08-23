<script setup lang="ts">
import { onMounted } from "vue";
import { RouterView, useRouter } from "vue-router";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { applyTheme } from "./services/theme";
import { isTauri } from "./services/window";
import { useSettingsStore } from "./stores/settings";

const settings = useSettingsStore();
const router = useRouter();

onMounted(() => {
  if (!isTauri()) document.documentElement.classList.add("is-web");
  if (isTauri() && getCurrentWindow().label === "settings") {
    document.documentElement.classList.add("is-settings");
    if (router.currentRoute.value.name !== "settings") {
      void router.replace({ name: "settings" });
    }
  }
  void settings.load().then(() => applyTheme(settings.theme));
});
</script>

<template>
  <RouterView />
</template>
