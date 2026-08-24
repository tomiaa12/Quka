<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { type ScanResult } from "../services/app";
import { useI18n } from "../i18n/use-i18n";
import { getShortcutStatus } from "../services/settings";
import { hideSearchWindow, isTauri, onWindowFocusChange, resizeSearchWindow } from "../services/window";
import { useSearchStore } from "../stores/search";
import { useSettingsStore } from "../stores/settings";
import AppList from "./AppList.vue";
import EmptyState from "./EmptyState.vue";
import SearchInput from "./SearchInput.vue";

const WINDOW_WIDTH = 640;
const OUTER = 0;
const INPUT_H = 56;
const DIVIDER = 1;
const LIST_PAD = 12;
const ITEM_H = 52;
const FOOTER_H = 36;
const EMPTY_H = 96;
const BANNER_H = 48;

const search = useSearchStore();
const settings = useSettingsStore();
const { t } = useI18n();
const inputRef = ref<{ focus: () => void } | null>(null);

const emptyVariant = computed(() => {
  if (search.scanning) return "scanning" as const;
  if (search.loading) return "loading" as const;
  if (!search.isEmptyKeyword) return "empty" as const;
  return "hint" as const;
});

const footerHint = computed(() => {
  if (search.scanning && search.scanner === "windows") return t("search.scannerWindows");
  if (search.scanning && search.scanner === "macos") return t("search.scannerMac");
  if (search.scanning) return t("search.scanner");
  if (search.loading) return t("search.loading");
  if (search.results.length === 0 && !search.isEmptyKeyword) return t("search.noResults");
  if (!search.isEmptyKeyword && search.searchElapsedMs > 0) {
    return t("search.fastMs", { ms: search.searchElapsedMs });
  }
  if (search.isEmptyKeyword && search.results.length > 0) return t("search.recent");
  return t("search.fast");
});

function windowHeight(): number {
  let height = OUTER * 2 + INPUT_H + DIVIDER + FOOTER_H;
  if (search.error || search.notice) height += BANNER_H;
  if (search.results.length === 0) height += EMPTY_H;
  else height += LIST_PAD + search.results.length * ITEM_H;
  return height;
}

function onKeydown(event: KeyboardEvent): void {
  if (event.isComposing) return;

  if (event.key === "ArrowDown") {
    event.preventDefault();
    search.moveDown();
    return;
  }
  if (event.key === "ArrowUp") {
    event.preventDefault();
    search.moveUp();
    return;
  }
  if (event.key === "Enter") {
    event.preventDefault();
    void search.launchSelected();
    return;
  }
  if (event.key === "Escape") {
    event.preventDefault();
    search.clear();
    void hideSearchWindow();
  }
}

watch(
  () => [search.results.length, search.loading, search.scanning, search.error, search.notice],
  () => {
    void resizeSearchWindow(WINDOW_WIDTH, windowHeight());
  },
  { immediate: true },
);

const media = window.matchMedia("(prefers-color-scheme: dark)");
const onSchemeChange = () => {
  if (settings.theme === "system") settings.setTheme("system");
};

function focusInput(): void {
  inputRef.value?.focus();
}

async function cycleShortcut(): Promise<void> {
  try {
    const status = await settings.cycleShortcut();
    if (status.error || !status.registered) {
      search.error = status.error || t("search.shortcutFailed");
    } else {
      search.error = "";
      search.notice = t("search.shortcutChanged", { label: settings.shortcutLabel });
    }
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    search.error = message.includes("快捷键注册失败") || message.includes("shortcut")
      ? message
      : `${t("search.shortcutFailed")}：${message}`;
  }
}

let unlistenShown: UnlistenFn | undefined;
let unlistenFocus: UnlistenFn | undefined;
let unlistenRescan: UnlistenFn | undefined;
let unlistenScanFailed: UnlistenFn | undefined;
let ignoreBlurUntil = 0;

function armIgnoreBlur(): void {
  ignoreBlurUntil = Date.now() + 400;
}

function hideIfUnfocused(): void {
  if (Date.now() < ignoreBlurUntil) return;
  search.clear();
  void hideSearchWindow();
}

function hideIfWindowLostFocus(): void {
  window.setTimeout(() => {
    if (document.hasFocus()) return;
    hideIfUnfocused();
  }, 0);
}

onMounted(() => {
  void (async () => {
    await settings.load();
    await search.bootstrap();
    try {
      const status = await getShortcutStatus();
      if (status.shortcut) settings.globalShortcut = status.shortcut;
      if (!status.registered) {
        search.error = status.error || t("search.shortcutFailed");
      } else if (status.error) {
        search.error = status.error;
      }
    } catch (error) {
      search.error = error instanceof Error ? error.message : String(error);
    }
    armIgnoreBlur();
    focusInput();
    if (isTauri()) {
      unlistenShown = await listen("search-shown", () => {
        armIgnoreBlur();
        focusInput();
        void settings.load();
      });
      unlistenFocus = await onWindowFocusChange((focused) => {
        if (focused) {
          armIgnoreBlur();
          focusInput();
          return;
        }
        hideIfUnfocused();
      });
      unlistenRescan = await listen<ScanResult>("apps-rescanned", (event) => {
        search.notice = t("search.foundApps", { n: event.payload.applicationCount });
        void search.search();
      });
      unlistenScanFailed = await listen<string>("scan-failed", (event) => {
        search.error = event.payload;
      });
    }
  })();
  media.addEventListener("change", onSchemeChange);
  window.addEventListener("focus", focusInput);
  window.addEventListener("blur", hideIfWindowLostFocus);
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible") focusInput();
    else hideIfWindowLostFocus();
  });
});

onUnmounted(() => {
  media.removeEventListener("change", onSchemeChange);
  window.removeEventListener("focus", focusInput);
  window.removeEventListener("blur", hideIfWindowLostFocus);
  void unlistenShown?.();
  void unlistenFocus?.();
  void unlistenRescan?.();
  void unlistenScanFailed?.();
});
</script>

<template>
  <section class="search-win">
    <SearchInput
      ref="inputRef"
      :model-value="search.keyword"
      :placeholder="t('search.placeholder')"
      @update:model-value="search.setKeyword"
      @keydown="onKeydown"
    />
    <div class="search-divider"></div>
    <div v-if="search.error" class="banner banner-error">{{ search.error }}</div>
    <div v-else-if="search.notice" class="banner banner-ok">{{ search.notice }}</div>
    <AppList
      v-if="search.results.length > 0"
      :apps="search.results"
      :selected-index="search.selectedIndex"
      :keyword="search.keyword"
      :recent="search.isEmptyKeyword"
      :elapsed-ms="search.searchElapsedMs"
      @select="search.selectIndex"
      @launch="search.launchSelected"
    />
    <EmptyState v-else :variant="emptyVariant" :scanner="search.scanner" />
    <div class="search-footer">
      <div class="hint-keys">
        <span><kbd>↑</kbd><kbd>↓</kbd> {{ t("search.select") }}</span>
        <span><kbd>Enter</kbd> {{ t("search.launch") }}</span>
        <span><kbd>Esc</kbd> {{ t("search.close") }}</span>
      </div>
      <div class="footer-actions">
        <button type="button" class="shortcut-chip" :title="t('search.changeShortcut')" @click="cycleShortcut">
          {{ settings.shortcutLabel }}
        </button>
        <button type="button" class="theme-toggle" @click="settings.cycleTheme()">
          {{ settings.themeLabel }} · {{ footerHint }}
        </button>
      </div>
    </div>
  </section>
</template>
