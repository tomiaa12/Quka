<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getDatabaseInfo, rescanApplications, type ScanResult } from "../services/app";
import { getShortcutStatus } from "../services/settings";
import { checkForUpdate, currentVersion, installUpdate, type AppUpdate } from "../services/update";
import { useI18n } from "../i18n/use-i18n";
import { isTauri } from "../services/window";
import { useSettingsStore } from "../stores/settings";
import type { TrayIconStyle } from "../types/settings";
import colorIcon from "../assets/tray-color.png";

const trayIcons: TrayIconStyle[] = ["color", "mono", "search", "bolt"];

function trayIconLabel(style: TrayIconStyle): string {
  if (style === "mono") return t("trayIcon.mono");
  if (style === "search") return t("trayIcon.search");
  if (style === "bolt") return t("trayIcon.bolt");
  return t("trayIcon.color");
}

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
const updatePercent = ref(0);
const updateHasTotal = ref(false);

let unlistenRescan: UnlistenFn | undefined;
let toastTimer: number | undefined;

function showToast(nextError = "", nextMessage = ""): void {
  error.value = nextError;
  message.value = nextMessage;
  window.clearTimeout(toastTimer);
  if (!nextError && !nextMessage) return;
  toastTimer = window.setTimeout(() => {
    error.value = "";
    message.value = "";
  }, 2800);
}

function setTab(next: SettingsTab): void {
  tab.value = next;
}

async function refreshCount(): Promise<void> {
  try {
    const info = await getDatabaseInfo();
    appCount.value = info.applicationCount;
  } catch (item) {
    showToast(item instanceof Error ? item.message : String(item));
  }
}

async function toggleStartup(): Promise<void> {
  try {
    const next = !settings.launchAtStartup;
    await settings.setLaunchAtStartup(next);
    showToast("", next ? t("settings.startupOn") : t("settings.startupOff"));
  } catch (item) {
    showToast(item instanceof Error ? item.message : String(item));
  }
}

async function toggleFullscreen(): Promise<void> {
  try {
    await settings.setDisableOnFullscreen(!settings.disableOnFullscreen);
    showToast("");
  } catch (item) {
    showToast(item instanceof Error ? item.message : String(item));
  }
}

async function cycleShortcut(): Promise<void> {
  try {
    const status = await settings.cycleShortcut();
    if (status.error || !status.registered) {
      showToast(status.error || t("search.shortcutFailed"));
      return;
    }
    showToast("", t("search.shortcutChanged", { label: settings.shortcutLabel }));
  } catch (item) {
    showToast(item instanceof Error ? item.message : String(item));
  }
}

async function applyScan(result: ScanResult): Promise<void> {
  appCount.value = result.applicationCount;
  showToast(
    "",
    t("settings.scanned", {
      n: result.applicationCount,
      inserted: result.inserted,
      updated: result.updated,
    }),
  );
}

function updateStatus(): string {
  if (installingUpdate.value) {
    if (updateHasTotal.value) {
      return `${t("settings.installingUpdate")} ${t("settings.updateProgress", { percent: updatePercent.value })}`;
    }
    return t("settings.installingUpdate");
  }
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
      showToast("", next ? t("settings.updateFound", { version: next.version }) : t("settings.latest"));
    }
  } catch (item) {
    updateInfo.value = null;
    if (!silent) {
      const text = item instanceof Error ? item.message : String(item);
      showToast(
        text.includes("Could not fetch") || text.includes("error sending request")
          ? t("settings.updateFailed")
          : text,
      );
    }
  } finally {
    checkingUpdate.value = false;
  }
}

async function applyUpdate(): Promise<void> {
  if (!updateInfo.value) return;
  installingUpdate.value = true;
  updatePercent.value = 0;
  updateHasTotal.value = false;
  showToast("");
  try {
    await installUpdate(updateInfo.value, (progress) => {
      updateHasTotal.value = progress.total > 0;
      updatePercent.value = progress.percent;
    });
  } catch (item) {
    showToast(item instanceof Error ? item.message : String(item));
    installingUpdate.value = false;
  }
}

async function rescan(): Promise<void> {
  scanning.value = true;
  showToast("");
  try {
    await applyScan(await rescanApplications());
  } catch (item) {
    showToast(item instanceof Error ? item.message : String(item));
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
    if (status.error) showToast(status.error);
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
  window.clearTimeout(toastTimer);
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
              <div class="row-title">{{ t("settings.fullscreen") }}</div>
              <div class="row-desc">{{ t("settings.fullscreenDesc") }}</div>
            </div>
            <button
              type="button"
              class="toggle"
              :class="{ on: settings.disableOnFullscreen }"
              @click="toggleFullscreen"
            >
              <i></i>
            </button>
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
              <div class="row-title">{{ t("settings.trayIcon") }}</div>
              <div class="row-desc">{{ t("settings.trayIconDesc") }}</div>
            </div>
            <div class="icon-picks">
              <button
                v-for="style in trayIcons"
                :key="style"
                type="button"
                class="icon-pick"
                :class="{ on: settings.trayIcon === style }"
                :title="trayIconLabel(style)"
                @click="settings.setTrayIcon(style)"
              >
                <img v-if="style === 'color'" :src="colorIcon" alt="" />
                <svg v-else-if="style === 'mono'" viewBox="0 0 24 24" aria-hidden="true">
                  <circle cx="12" cy="12" r="7.2" fill="none" stroke="currentColor" stroke-width="3.2" />
                  <path d="M15.2 15.2 19.4 19.4" fill="none" stroke="currentColor" stroke-width="3.2" stroke-linecap="round" />
                </svg>
                <svg v-else-if="style === 'search'" viewBox="0 0 24 24" aria-hidden="true">
                  <circle cx="10.4" cy="10.4" r="5.4" fill="none" stroke="currentColor" stroke-width="2.6" />
                  <path d="M14.6 14.6 19 19" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" />
                </svg>
                <svg v-else viewBox="0 0 24 24" aria-hidden="true">
                  <path d="M13.6 3.2 7.4 12.6h4.2L9.6 20.8l7.6-10.4h-4.2z" fill="currentColor" />
                </svg>
              </button>
            </div>
          </div>
          <div class="row">
            <div class="update-copy">
              <div class="row-title">{{ t("settings.updates") }}</div>
              <div class="row-desc">{{ updateStatus() }}</div>
              <div
                v-if="installingUpdate"
                class="update-progress"
                :class="{ 'is-indeterminate': !updateHasTotal }"
              >
                <i
                  class="update-progress-bar"
                  :style="updateHasTotal ? { width: `${updatePercent}%` } : undefined"
                ></i>
              </div>
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
    <div v-if="error" class="settings-toast banner banner-error">{{ error }}</div>
    <div v-else-if="message" class="settings-toast banner banner-ok">{{ message }}</div>
  </section>
</template>
