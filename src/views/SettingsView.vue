<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getDatabaseInfo, rescanApplications, type ScanResult } from "../services/app";
import { getShortcutStatus } from "../services/settings";
import { checkForUpdate, currentVersion, installUpdate, type AppUpdate } from "../services/update";
import { useI18n } from "../i18n/use-i18n";
import { isTauri } from "../services/window";
import { useSettingsStore } from "../stores/settings";

type SettingsTab = "general" | "search" | "apps";

const settings = useSettingsStore();
const { t } = useI18n();
const tab = ref<SettingsTab>("general");
const appCount = ref(0);
const scanning = ref(false);
const message = ref("");
const error = ref("");
const version = ref("");
const updateInfo = ref<AppUpdate | null>(null);
const checkingUpdate = ref(false);
const installingUpdate = ref(false);

let unlistenRescan: UnlistenFn | undefined;

function setTab(next: SettingsTab): void {
  tab.value = next;
}

async function refreshCount(): Promise<void> {
  try {
    const info = await getDatabaseInfo();
    appCount.value = info.applicationCount;
  } catch (item) {
    error.value = item instanceof Error ? item.message : String(item);
  }
}

async function toggleStartup(): Promise<void> {
  try {
    const next = !settings.launchAtStartup;
    await settings.setLaunchAtStartup(next);
    error.value = "";
    message.value = next ? t("settings.startupOn") : t("settings.startupOff");
  } catch (item) {
    error.value = item instanceof Error ? item.message : String(item);
  }
}

async function cycleShortcut(): Promise<void> {
  try {
    const status = await settings.cycleShortcut();
    if (status.error || !status.registered) {
      error.value = status.error || t("search.shortcutFailed");
      return;
    }
    error.value = "";
    message.value = t("search.shortcutChanged", { label: settings.shortcutLabel });
  } catch (item) {
    error.value = item instanceof Error ? item.message : String(item);
  }
}

async function applyScan(result: ScanResult): Promise<void> {
  appCount.value = result.applicationCount;
  message.value = t("settings.scanned", {
    n: result.applicationCount,
    inserted: result.inserted,
    updated: result.updated,
  });
}

function updateStatus(): string {
  if (installingUpdate.value) return t("settings.installingUpdate");
  if (checkingUpdate.value) return t("settings.checkingUpdates");
  if (updateInfo.value) return t("settings.updateFound", { version: updateInfo.value.version });
  if (version.value) return t("settings.currentVersion", { version: version.value });
  return t("settings.checkGithub");
}

async function checkUpdate(silent = false): Promise<void> {
  checkingUpdate.value = true;
  try {
    const next = await checkForUpdate();
    updateInfo.value = next;
    if (!silent) {
      error.value = "";
      message.value = next ? t("settings.updateFound", { version: next.version }) : t("settings.latest");
    }
  } catch (item) {
    updateInfo.value = null;
    if (!silent) {
      const text = item instanceof Error ? item.message : String(item);
      error.value = text.includes("Could not fetch") || text.includes("error sending request")
        ? t("settings.updateFailed")
        : text;
    }
  } finally {
    checkingUpdate.value = false;
  }
}

async function applyUpdate(): Promise<void> {
  if (!updateInfo.value) return;
  installingUpdate.value = true;
  error.value = "";
  try {
    await installUpdate(updateInfo.value);
  } catch (item) {
    error.value = item instanceof Error ? item.message : String(item);
    installingUpdate.value = false;
  }
}

async function rescan(): Promise<void> {
  scanning.value = true;
  error.value = "";
  try {
    await applyScan(await rescanApplications());
  } catch (item) {
    error.value = item instanceof Error ? item.message : String(item);
  } finally {
    scanning.value = false;
  }
}

onMounted(() => {
  void settings.load();
  void refreshCount();
  void currentVersion().then((value) => {
    version.value = value;
  });
  if (!isTauri()) return;
  void getShortcutStatus().then((status) => {
    if (status.error) error.value = status.error;
  });
  void checkUpdate(true);
  void listen<ScanResult>("apps-rescanned", (event) => {
    appCount.value = event.payload.applicationCount;
    if (!event.payload.silent) {
      void applyScan(event.payload);
    }
  }).then((unlisten) => {
    unlistenRescan = unlisten;
  });
});

onUnmounted(() => {
  void unlistenRescan?.();
});
</script>

<template>
  <section class="settings-win">
    <div class="settings-body">
      <nav class="settings-nav">
        <button type="button" class="nav-item" :class="{ 'is-active': tab === 'general' }" @click="setTab('general')">
          {{ t("settings.general") }}
        </button>
        <button type="button" class="nav-item" :class="{ 'is-active': tab === 'search' }" @click="setTab('search')">
          {{ t("settings.search") }}
        </button>
        <button type="button" class="nav-item" :class="{ 'is-active': tab === 'apps' }" @click="setTab('apps')">
          {{ t("settings.apps") }}
        </button>
      </nav>
      <main class="settings-main">
        <div v-if="error" class="banner banner-error">{{ error }}</div>
        <div v-else-if="message" class="banner banner-ok">{{ message }}</div>

        <template v-if="tab === 'general'">
          <h2>{{ t("settings.general") }}</h2>
          <div class="row">
            <div>
              <div class="row-title">{{ t("settings.startup") }}</div>
              <div class="row-desc">{{ t("settings.startupDesc") }}</div>
            </div>
            <button
              type="button"
              class="toggle"
              :class="{ on: settings.launchAtStartup }"
              @click="toggleStartup"
            >
              <i></i>
            </button>
          </div>
          <div class="row">
            <div>
              <div class="row-title">{{ t("settings.shortcut") }}</div>
              <div class="row-desc">{{ t("settings.shortcutDesc") }}</div>
            </div>
            <button type="button" class="shortcut-box" @click="cycleShortcut">{{ settings.shortcutLabel }}</button>
          </div>
          <div class="row">
            <div>
              <div class="row-title">{{ t("settings.language") }}</div>
              <div class="row-desc">{{ t("settings.languageDesc") }}</div>
            </div>
            <button type="button" class="shortcut-box" @click="settings.cycleLocale()">
              {{ settings.localeLabel }}
            </button>
          </div>
          <div class="row">
            <div>
              <div class="row-title">{{ t("settings.theme") }}</div>
              <div class="row-desc">{{ t("settings.themeDesc") }}</div>
            </div>
            <button type="button" class="shortcut-box" @click="settings.cycleTheme()">
              {{ settings.themeLabel }}
            </button>
          </div>
          <div class="row">
            <div>
              <div class="row-title">{{ t("settings.updates") }}</div>
              <div class="row-desc">{{ updateStatus() }}</div>
            </div>
            <button
              v-if="updateInfo"
              type="button"
              class="btn"
              :disabled="installingUpdate"
              @click="applyUpdate"
            >
              {{ installingUpdate ? t("settings.updating") : t("settings.updateNow") }}
            </button>
            <button
              v-else
              type="button"
              class="btn"
              :disabled="checkingUpdate"
              @click="checkUpdate(false)"
            >
              {{ checkingUpdate ? t("settings.checking") : t("settings.checkUpdate") }}
            </button>
          </div>
        </template>

        <template v-else-if="tab === 'search'">
          <h2>{{ t("settings.search") }}</h2>
          <div class="row">
            <div>
              <div class="row-title">{{ t("settings.maxResults") }}</div>
              <div class="row-desc">{{ t("settings.maxResultsDesc") }}</div>
            </div>
            <div class="stepper">
              <button type="button" class="btn" @click="settings.setResultLimit(settings.resultLimit - 1)">−</button>
              <b>{{ settings.resultLimit }}</b>
              <button type="button" class="btn" @click="settings.setResultLimit(settings.resultLimit + 1)">+</button>
            </div>
          </div>
          <div class="row">
            <div>
              <div class="row-title">{{ t("settings.usageRanking") }}</div>
              <div class="row-desc">{{ t("settings.usageRankingDesc") }}</div>
            </div>
            <button
              type="button"
              class="toggle"
              :class="{ on: settings.enableUsageRanking }"
              @click="settings.setUsageRanking(!settings.enableUsageRanking)"
            >
              <i></i>
            </button>
          </div>
        </template>

        <template v-else>
          <h2>{{ t("settings.apps") }}</h2>
          <div class="row">
            <div>
              <div class="row-title">{{ t("settings.rescan") }}</div>
              <div class="row-desc">{{ t("settings.rescanDesc") }}</div>
            </div>
            <button type="button" class="btn" :disabled="scanning" @click="rescan">
              {{ scanning ? t("settings.rescanning") : t("settings.rescan") }}
            </button>
          </div>
          <div class="row">
            <div>
              <div class="row-title">{{ t("settings.library") }}</div>
              <div class="row-desc">{{ t("settings.libraryDesc", { n: appCount }) }}</div>
            </div>
            <span class="app-meta">SQLite</span>
          </div>
        </template>
      </main>
    </div>
  </section>
</template>
