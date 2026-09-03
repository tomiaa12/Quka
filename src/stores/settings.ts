import { defineStore } from "pinia";
import { applyLocale, localeLabel, nextLocale } from "../i18n";
import {
  changeGlobalShortcut,
  getSettings,
  nextShortcut,
  shortcutLabel,
  updateSettings,
} from "../services/settings";
import { applyTheme, nextTheme, themeLabel } from "../services/theme";
import type { LocaleMode, SettingsState, ThemeMode, TrayIconStyle } from "../types/settings";

function asTrayIcon(value: unknown): TrayIconStyle {
  if (value === "mono" || value === "search" || value === "bolt") return value;
  return "color";
}

function asLocale(value: unknown): LocaleMode {
  if (value === "zh-CN" || value === "en" || value === "system") return value;
  return "system";
}

const defaultSettings: SettingsState = {
  globalShortcut: "DoubleCtrl",
  launchAtStartup: false,
  resultLimit: 8,
  enableUsageRanking: true,
  theme: "system",
  locale: "system",
  disableOnFullscreen: true,
  trayIcon: "color",
};

function snapshot(state: SettingsState): SettingsState {
  return {
    globalShortcut: state.globalShortcut,
    launchAtStartup: state.launchAtStartup,
    resultLimit: state.resultLimit,
    enableUsageRanking: state.enableUsageRanking,
    theme: state.theme,
    locale: state.locale,
    disableOnFullscreen: state.disableOnFullscreen,
    trayIcon: state.trayIcon,
  };
}

export const useSettingsStore = defineStore("settings", {
  state: (): SettingsState => ({ ...defaultSettings }),
  getters: {
    shortcutLabel: (state) => {
      void state.locale;
      return shortcutLabel(state.globalShortcut);
    },
    themeLabel: (state) => {
      void state.locale;
      return themeLabel(state.theme);
    },
    localeLabel: (state) => localeLabel(state.locale),
  },
  actions: {
    hydrate(remote: SettingsState) {
      this.globalShortcut = remote.globalShortcut;
      this.launchAtStartup = remote.launchAtStartup;
      this.resultLimit = remote.resultLimit;
      this.enableUsageRanking = remote.enableUsageRanking;
      this.theme = remote.theme;
      this.locale = asLocale(remote.locale);
      this.disableOnFullscreen = remote.disableOnFullscreen !== false;
      this.trayIcon = asTrayIcon(remote.trayIcon);
      applyTheme(this.theme);
      applyLocale(this.locale);
    },
    async load() {
      try {
        const remote = await getSettings();
        if (!remote) {
          applyLocale(this.locale);
          return;
        }
        this.hydrate(remote);
      } catch (error) {
        applyLocale(this.locale);
        console.error(error);
      }
    },
    async persist() {
      try {
        await updateSettings(snapshot(this));
      } catch (error) {
        console.error(error);
      }
    },
    async setTheme(theme: ThemeMode) {
      this.theme = theme;
      applyTheme(theme);
      await this.persist();
    },
    async cycleTheme() {
      await this.setTheme(nextTheme(this.theme));
    },
    async setLocale(locale: LocaleMode) {
      this.locale = locale;
      applyLocale(locale);
      await this.persist();
    },
    async cycleLocale() {
      await this.setLocale(nextLocale(this.locale));
    },
    async cycleShortcut() {
      const status = await changeGlobalShortcut(nextShortcut(this.globalShortcut));
      this.globalShortcut = status.shortcut;
      return status;
    },
    async setResultLimit(limit: number) {
      this.resultLimit = Math.min(12, Math.max(4, limit));
      await this.persist();
    },
    async setUsageRanking(enabled: boolean) {
      this.enableUsageRanking = enabled;
      await this.persist();
    },
    async setTrayIcon(style: TrayIconStyle) {
      this.trayIcon = style;
      await this.persist();
    },
    async setDisableOnFullscreen(enabled: boolean) {
      this.disableOnFullscreen = enabled;
      await this.persist();
    },
    async setLaunchAtStartup(enabled: boolean) {
      const previous = this.launchAtStartup;
      this.launchAtStartup = enabled;
      try {
        await updateSettings(snapshot(this));
      } catch (error) {
        this.launchAtStartup = previous;
        throw error;
      }
    },
  },
});
