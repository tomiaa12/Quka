<script setup lang="ts">
import { onMounted, onUnmounted } from "vue";
import { RouterView, useRouter } from "vue-router";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { applyLocale } from "./i18n";
import { applyTheme } from "./services/theme";
import { isTauri } from "./services/window";
import { useSettingsStore } from "./stores/settings";
import type { SettingsState } from "./types/settings";

const settings = useSettingsStore();
const router = useRouter();
let unlistenSettings: UnlistenFn | undefined;

onMounted(() => {
  if (!isTauri()) document.documentElement.classList.add("is-web");
  if (isTauri() && getCurrentWindow().label === "settings") {
    document.documentElement.classList.add("is-settings");
    if (router.currentRoute.value.name !== "settings") {
      void router.replace({ name: "settings" });
    }
  }
  void settings.load().then(() => {
    applyTheme(settings.theme);
    applyLocale(settings.locale);
  });
  if (!isTauri()) return;
  void listen<SettingsState>("settings-updated", (event) => {
    settings.hydrate(event.payload);
  }).then((unlisten) => {
    unlistenSettings = unlisten;
  });
});

onUnmounted(() => {
  void unlistenSettings?.();
});
</script>

<template>
  <RouterView />
</template>
